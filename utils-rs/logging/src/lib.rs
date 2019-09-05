// Copyright (C) 2019  Braiins Systems s.r.o.
//
// This file is part of Braiins Open-Source Initiative (BOSI).
//
// BOSI is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// Please, keep in mind that we may also license BOSI or any part thereof
// under a proprietary license. For more information on the terms and conditions
// of such proprietary license or if you have any other questions, please
// contact us at opensource@braiins.com.

//! Logging boilerplate and configuration
//!
//! This crate takes care of a few shortcommings of `slog` and `slog_async`:
//! - A global shared `Logger` instance using `slog_async`
//! - Configuration of the global instance
//! - Logging macros that operate on the shared instance
//! - Flushing of logs on application exit
//!
//! It also re-exports `slog` - this is a way to provide common `slog`
//! dependency.
//!
//! The global instance is created using `lazy_static`.
//! This means it's configured and created the first time
//! it's accessed. Once created, the global instance cannot
//! be re-configured. To configure it, use `set_logger_config()`
//! or one of the convenience functions `setup()` or `setup_for_app()`.
//! Make sure this is done before the global logger is actually used,
//! otherwise these functions panic.
//!
//! The global logger is also configured with `slog_envlogger`,
//! that is, it applies filters set via the `RUST_LOG` env variable.
//! Refer to the [`env_logger` documentation](https://docs.rs/env_logger/0.6.2/env_logger/)
//! for more information.
//!
//! If no configuration is set with `set_logger_config()` et al.,
//! the global logger will by default use `LoggingConfig::for_testing()`,
//! ie. configuration suitable for testing. This is because as of now
//! there's no way to have common setup/teardown for tests, and so
//! it's best that the default is test-friendly.

use std::env;
use std::fmt;
use std::fs::OpenOptions;
use std::mem;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use lazy_static::lazy_static;
use slog::{o, Discard, Drain, FilterLevel, Logger};
use slog_async::{Async, AsyncGuard};
use slog_envlogger::EnvLogger;
use slog_term;

// Re-export slog things for easy access to slog by dependers
// and also because these are used by macros
pub use slog;
pub use slog::Level;

/// Logging target configuration: Where to log
#[derive(Clone, Debug)]
pub enum LoggingTarget {
    /// Log to standard error
    Stderr,
    /// Log to standard output
    Stdout,
    /// Log to a file
    File(PathBuf),
    /// Don't log anything anywhere
    None,
}

/// Describes logger configuration which can be set in runtime
#[derive(Clone, Debug)]
pub struct LoggingConfig {
    /// Where to log
    pub target: LoggingTarget,
    /// The default logging level,
    /// this may be altered with the RUST_LOG env var on startup.
    pub level: Level,
}

impl LoggingConfig {
    /// Logging configuration suitable for test harness,
    /// doesn't pollute terminal, logs to `test-log.txt` in system tmp location.
    pub fn for_testing() -> Self {
        Self {
            target: LoggingTarget::File(env::temp_dir().join("test-log.txt")),
            level: Level::Trace,
        }
    }

    /// Default setup for standalone programs
    ///
    /// The default level is `Debug` for debug builds
    /// and `Info` for release builds.
    pub fn for_app() -> Self {
        Self {
            target: LoggingTarget::Stderr,
            level: if cfg!(debug_assertions) {
                Level::Debug
            } else {
                Level::Info
            },
        }
    }

    /// Configuration where nothing is logged
    pub fn no_logging() -> Self {
        Self {
            target: LoggingTarget::None,
            level: Level::Error,
        }
    }
}

/// Default configuration for logger used for unit tests and integration tests
/// The default is to set up for testing because it's hard to perform
/// manual setup in tests (Rust test harness doesn't support setup/teardown yet).
impl Default for LoggingConfig {
    fn default() -> Self {
        Self::for_testing()
    }
}

/// Lock logger configuration with mutual exclusion
#[inline(always)]
fn lock_logger_config() -> MutexGuard<'static, Option<LoggingConfig>> {
    LOGGER_CONFIG
        .lock()
        .expect("Could not lock logger config mutex")
}

/// Set new logger configuration and return old one.
/// This is thread-safe, the global configuration is
/// protected by a mutex.
///
/// # Panics
///
/// Panics if `LOGGER` is already instantiated, ie. its configuration
/// can no longer be changed.
pub fn set_logger_config(config: LoggingConfig) -> LoggingConfig {
    lock_logger_config()
        .replace(config)
        .expect("Could not set logger config, LOGGER already instantiated")
}

/// Setup logger with configuration passed in `config`
/// and return a `FlushGuard`. Convenience function.
///
/// # Panics
///
/// Panics if `LOGGER` is already instantiated, ie. its configuration
/// can no longer be changed.
pub fn setup(config: LoggingConfig) -> FlushGuard {
    set_logger_config(config);
    LOGGER.take_guard()
}

/// Setup logger with default configuration suitable for application usage
/// (ie. in `main()`) and return a `FlushGuard`. Convenience function.
///
/// # Panics
///
/// Panics if `LOGGER` is already instantiated, ie. its configuration
/// can no longer be changed.
pub fn setup_for_app() -> FlushGuard {
    setup(LoggingConfig::for_app())
}

/// Sets up envlogger filter for a drain, with proper default settings
fn get_envlogger_drain<D: Drain>(drain: D, default_level: Level) -> EnvLogger<D> {
    let builder = slog_envlogger::LogBuilder::new(drain);
    match env::var("RUST_LOG") {
        Ok(ref rust_log) if !rust_log.is_empty() => {
            // Use the RUST_LOG env var
            builder.parse(rust_log).build()
        }
        _ => {
            // No RUST_LOG env var or empty, use the default level

            // For some reason there's no impl From<Level> for FilterLevel,
            // so we have go through usize here :-/
            let filter_level = FilterLevel::from_usize(default_level.as_usize())
                .expect("Internal error: Could not convert slog::Level to slog::FilterLevel");

            builder.filter(None, filter_level).build()
        }
    }
}

/// Create terminal drain for logger, logging to either stderr or stdout
fn get_terminal_drain(stderr: bool) -> impl Drain<Ok = (), Err = impl fmt::Debug> {
    let builder = slog_term::TermDecorator::new();
    let builder = if stderr {
        builder.stderr()
    } else {
        builder.stdout()
    };
    let terminal_decorator = builder.build();
    let terminal_drain = slog_term::FullFormat::new(terminal_decorator).build();
    terminal_drain
}

/// Create file drain for logger
fn get_file_drain(path: &Path) -> impl Drain<Ok = (), Err = impl fmt::Debug> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(true)
        .truncate(false)
        .open(path)
        .map_err(|e| {
            panic!(
                "Logging setup error: Could not open file `{}` for logging: {}",
                path.display(),
                e
            )
        })
        .unwrap();

    let file_decorator = slog_term::PlainDecorator::new(file);
    let file_drain = slog_term::FullFormat::new(file_decorator).build();
    file_drain
}

/// Logger flush RAII guard.
///
/// The guard ensures logs are flushed when it goes out of scope.
/// Due to the way `slog_async` works by default it can't ensure log flush
/// on application exit, this can only be done with the guard.
#[must_use = "When dropped, FlushGuard flushes and stops its associated logger instance"]
pub struct FlushGuard(Option<AsyncGuard>);

/// `GuardedLogger` holds both a `Logger` instance and a mutex
/// containing a `FlushGuard`. The `FlushGuard` can be
/// taken out and used as a RAII guard to ensure log flushing on scope exit.
/// Typically you want to use this in a `main()` function or similar.
/// Use `take_guard()` to obtain the `FlushGuard`.
pub struct GuardedLogger {
    pub logger: Logger,
    guard: Mutex<FlushGuard>,
}

impl GuardedLogger {
    fn new(config: &LoggingConfig) -> GuardedLogger {
        use LoggingTarget::*;

        match &config.target {
            None => Self::with_discard(),
            Stderr => Self::with_drain(config, get_terminal_drain(true)),
            Stdout => Self::with_drain(config, get_terminal_drain(false)),
            File(path) => Self::with_drain(config, get_file_drain(path)),
        }
    }

    fn with_drain<D, E>(config: &LoggingConfig, drain: D) -> Self
    where
        E: fmt::Debug,
        D: Drain<Ok = (), Err = E> + Send + 'static,
    {
        let drain = get_envlogger_drain(drain, config.level);
        let (drain, guard) = Async::new(drain.fuse()).build_with_guard();
        Self {
            logger: Logger::root(drain.fuse(), o!()),
            guard: Mutex::new(FlushGuard(Some(guard))),
        }
    }

    fn with_discard() -> Self {
        Self {
            logger: Logger::root(Discard, o!()),
            guard: Mutex::new(FlushGuard(None)),
        }
    }

    /// Get the `FlushGuard` associated with this `Logger`,
    /// note that if the guard has previously been taken,
    /// this will just return an empty (dummy) guard.
    pub fn take_guard(&self) -> FlushGuard {
        let mut locker = self
            .guard
            .lock()
            .expect("Could not lock GuardedLogger mutex");
        mem::replace(&mut *locker, FlushGuard(None))
    }

    /// Take a `FlushGuard` and drop it, effectivelly flushing
    /// the `Logger` immediately.
    ///
    /// **Warning**: This has no effect if the `FlushGuard`
    /// has already been taken and dropped or exists elsewhere.
    pub fn flush(&self) {
        drop(self.take_guard());
    }
}

impl Deref for GuardedLogger {
    type Target = Logger;

    fn deref(&self) -> &Logger {
        &self.logger
    }
}

lazy_static! {
    static ref LOGGER_CONFIG: Mutex<Option<LoggingConfig>> = Mutex::new(Some(LoggingConfig::default()));

    /// Static global reference to the logger that will be accessible from all crates
    pub static ref LOGGER: GuardedLogger = {
        // Take the configuration data
        let mut config_lock = lock_logger_config();
        let config = config_lock.take()
            .expect("Internal error: LOGGER_CONFIG empty in LOGGER initialization");

        GuardedLogger::new(&config)
    };
}

/// Log critical level record in the global logger
#[macro_export]
macro_rules! crit(
    (#$tag:expr, $($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Critical, $tag, $($args)+)
    };
    ($($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Critical, "", $($args)+)
    };
);

/// Log error level record in the global logger
#[macro_export]
macro_rules! error(
    (#$tag:expr, $($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Error, $tag, $($args)+)
    };
    ($($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Error, "", $($args)+)
    };
);

/// Log warning level record in the global logger
#[macro_export]
macro_rules! warn(
    (#$tag:expr, $($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Warning, $tag, $($args)+)
    };
    ($($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Warning, "", $($args)+)
    };
);

/// Log info level record in the global logger
#[macro_export]
macro_rules! info(
    (#$tag:expr, $($args:tt)*) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Info, $tag, $($args)*)
    };
    ($($args:tt)*) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Info, "", $($args)*)
    };
);

/// Log debug level record in the global logger
#[macro_export]
macro_rules! debug(
    (#$tag:expr, $($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Debug, $tag, $($args)+)
    };
    ($($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Debug, "", $($args)+)
    };
);

/// Log trace level record in the global logger
#[macro_export]
macro_rules! trace(
    (#$tag:expr, $($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Trace, $tag, $($args)+)
    };
    ($($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Trace, "", $($args)+)
    };
);

/// All logging macros are re-exported here for easy
/// inclusion in user code. Usage: `use logging::macros::*;`.
pub mod macros {
    pub use super::{crit, debug, error, info, trace, warn};
}
