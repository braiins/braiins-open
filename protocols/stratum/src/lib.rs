//!
use lazy_static::lazy_static;
use slog::{o, Drain, Level, LevelFilter, Logger};
use slog_async;
use slog_envlogger;
use slog_term;
use std::env;

// TODO: is it best practice to reexport this?
pub use packed_struct;
pub mod error;
pub mod v1;
pub mod v2;

/// Mask for allowed version bits that can be rolled based on BIP320
pub const BIP320_N_VERSION_MASK: u32 = 0x1fffe000;

/// Maximum number of bits allowed by BIP320_N_VERSION_MASK
pub const BIP320_N_VERSION_MAX_BITS: usize = 16;

lazy_static! {
    /// Build static reference to the logger that will be accessible from all crates
    pub static ref LOGGER: Logger = {
        let level = Level::Trace;

        // envlogger doesn't allow to set default log level, so this is a workaround
        if !env::var("RUST_LOG").is_ok() {
            env::set_var("RUST_LOG", "info");
        }

        // Setup drain for terminal output
        let terminal_decorator = slog_term::TermDecorator::new().build();
        let terminal_drain = slog_term::FullFormat::new(terminal_decorator)
            .build()
            .fuse();
        let terminal_drain = LevelFilter::new(terminal_drain, level).fuse();
        let terminal_drain = slog_envlogger::new(terminal_drain);
        let terminal_drain = slog_async::Async::new(terminal_drain).build().fuse();

        let log = Logger::root(terminal_drain, o!());
        log
    };
}
//pub static mut LOGGER: Option<Logger> = None;

#[inline]
pub fn logger() -> &'static Logger {
    //    LOGGER.unwrap_or_else( |_| {
    //        let level = Level::Trace;
    //        // Setup drain for terminal output
    //        let terminal_decorator = slog_term::TermDecorator::new().build();
    //        let terminal_drain = slog_term::FullFormat::new(terminal_decorator)
    //            .build()
    //            .fuse();
    //        let terminal_drain = LevelFilter::new(terminal_drain, level).fuse();
    //        let terminal_drain = slog_async::Async::new(terminal_drain).build().fuse();
    //
    //        let log = Logger::root(terminal_drain, o!());
    //
    //        LOGGER
    //        Some(log)
    //    })
    //
    &LOGGER
}
//
//pub fn init_logger(logger: slog::Logger) {
//    crate::LOGGER = logger;
//}
//
//fn main() {
//    let logger: Logger;
//
//    stratum::init_logger(logger.clone());
//    rurminer::init_logger(logger.clone());
//}
//

// This is here because some test utilities need to be shared between
// both unit and integration tests.
#[doc(hidden)]
pub mod test_utils;
