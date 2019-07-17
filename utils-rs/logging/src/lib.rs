use std::env;
use std::fs::OpenOptions;
use std::mem;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

use lazy_static::lazy_static;
use slog::{o, Discard, Drain, Duplicate, LevelFilter, Logger};
use slog_async::{Async, AsyncGuard};
use slog_term;
use slog_envlogger;

// Re-export slog things for easy access to slog by dependers
// and also because these are used by macros
pub use slog;
pub use slog::Level;

/// Describes logger configuration which can be set in runtime
#[derive(Clone, Debug)]
pub struct LoggingConfig {
    /// Logging level and filename associated with a file output
    pub file: Option<(Level, PathBuf)>,
    /// Logging level associated with a terminal output
    pub term: Option<Level>,
}

impl LoggingConfig {
    /// Logging configuration suitable for test harness,
    /// doesn't pollute terminal, logs to `test-log.txt` in system tmp location.
    pub fn for_testing() -> Self {
        Self {
            file: Some((Level::Trace, env::temp_dir().join("test-log.txt"))),
            term: None,
        }
    }

    /// Default setup for standalone programs
    pub fn for_app() -> Self {
        Self {
            file: None,
            term: Some(Level::Trace),
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
fn lock_logger_config() -> MutexGuard<'static, LoggingConfig> {
    LOGGER_CONFIG
        .lock()
        .expect("Could not lock logger config mutex")
}

/// Get copy of current logger configuration
pub fn get_logger_config() -> LoggingConfig {
    lock_logger_config().clone()
}

/// Set new logger configuration and return old one
pub fn set_logger_config(config: LoggingConfig) -> LoggingConfig {
    if LOGGER_INSTANTIATED.load(Ordering::SeqCst) {
        panic!("Cannot set logger config, LOGGER already instantiated");
    }

    let mut locker = lock_logger_config();
    mem::replace(&mut *locker, config)
}

/// Setup logger with configuration passed in `config`
/// and return a `FlushGuard`. Convenience function.
pub fn setup(config: LoggingConfig) -> FlushGuard {
    set_logger_config(config);
    LOGGER.take_guard()
}

/// Setup logger with default configuration suitable for application usage
/// (ie. in `main()`) and return a `FlushGuard`. Convenience function.
pub fn setup_for_app() -> FlushGuard {
    setup(LoggingConfig::for_app())
}

/// Create terminal drain for logger with logging level if requested
fn get_terminal_drain(config: &LoggingConfig) -> Option<impl Drain<Ok = (), Err = slog::Never>> {
    config.term.map(|level| {
        let terminal_decorator = slog_term::TermDecorator::new().build();
        let terminal_drain = slog_term::FullFormat::new(terminal_decorator).build();
        let terminal_drain = slog_envlogger::new(terminal_drain);
        LevelFilter::new(terminal_drain, level).fuse()
    })
}

/// Create file drain for logger with logging level if requested
fn get_file_drain(config: &LoggingConfig) -> Option<impl Drain<Ok = (), Err = slog::Never>> {
    config.file.as_ref().and_then(|(level, path)| {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(true)
            .truncate(false)
            .open(path.as_path())
            .map_err(|e| {
                eprintln!(
                    "Logging setup error: Could not open file `{}` for logging: {}",
                    path.display(),
                    e
                )
            })
            .ok()?;

        let file_decorator = slog_term::PlainDecorator::new(file);
        let file_drain = slog_term::FullFormat::new(file_decorator).build();
        let file_drain = slog_envlogger::new(file_drain);
        Some(LevelFilter::new(file_drain, *level).fuse())
    })
}

/// Wraps an optional `AsyncGuard`, newtype created to add `must_use`.
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
    fn from_drain<D>(drain: D) -> Self
    where
        D: Drain<Ok = (), Err = slog::Never> + Send + 'static,
    {
        let (drain, guard) = Async::new(drain).build_with_guard();
        Self {
            logger: Logger::root(drain.fuse(), o!()),
            guard: Mutex::new(FlushGuard(Some(guard))),
        }
    }

    fn new_discard() -> Self {
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

    /// Take a `FlushGuard` and drop is, effectivelly flushing
    /// the `Logger` immediately.
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

/// Flag to check for in set_logger_config() whether setting the config makes sense any more
static LOGGER_INSTANTIATED: AtomicBool = AtomicBool::new(false);

lazy_static! {
    static ref LOGGER_CONFIG: Mutex<LoggingConfig> = Mutex::new(LoggingConfig::default());

    /// Static reference to the logger that will be accessible from all crates
    pub static ref LOGGER: GuardedLogger = {
        LOGGER_INSTANTIATED.store(true, Ordering::SeqCst);

        // envlogger doesn't allow to set default log level, so this is a workaround
        if !env::var("RUST_LOG").is_ok() {
            env::set_var("RUST_LOG", "info");
        }

        // Read configuration and create drains as appropriate
        let config_lock = lock_logger_config();
        let terminal_drain = get_terminal_drain(&*config_lock);
        let file_drain = get_file_drain(&*config_lock);
        drop(config_lock);

        // Combine drains if both are set up, use just one if one is set up,
        // use a discard drain if none are set up
        match (terminal_drain, file_drain) {
            (Some(terminal_drain), Some(file_drain)) => {
                let composite_drain = Duplicate::new(terminal_drain, file_drain).fuse();
                GuardedLogger::from_drain(composite_drain)
            },
            (Some(terminal_drain), None) => GuardedLogger::from_drain(terminal_drain.fuse()),
            (None, Some(file_drain)) => GuardedLogger::from_drain(file_drain.fuse()),
            (None, None) => {
                GuardedLogger::new_discard()
            },
        }
    };
}

#[macro_export]
macro_rules! crit(
    (#$tag:expr, $($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Critical, $tag, $($args)+)
    };
    ($($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Critical, "", $($args)+)
    };
);

#[macro_export]
macro_rules! error(
    (#$tag:expr, $($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Error, $tag, $($args)+)
    };
    ($($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Error, "", $($args)+)
    };
);

#[macro_export]
macro_rules! warn(
    (#$tag:expr, $($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Warning, $tag, $($args)+)
    };
    ($($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Warning, "", $($args)+)
    };
);

#[macro_export]
macro_rules! info(
    (#$tag:expr, $($args:tt)*) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Info, $tag, $($args)*)
    };
    ($($args:tt)*) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Info, "", $($args)*)
    };
);

#[macro_export]
macro_rules! debug(
    (#$tag:expr, $($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Debug, $tag, $($args)+)
    };
    ($($args:tt)+) => {
        $crate::slog::slog_log!($crate::LOGGER, $crate::Level::Debug, "", $($args)+)
    };
);

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
