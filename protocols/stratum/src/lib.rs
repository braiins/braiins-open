//!
use lazy_static::lazy_static;
use slog::{o, Drain, Level, LevelFilter, Logger};
use slog_async;
use slog_term;
use std::panic::PanicInfo;

pub mod error;
pub mod v1;
pub mod v2;

lazy_static! {
    /// Build static reference to the logger that will be accessible from all crates
    pub static ref LOGGER: Logger = {
        let level = Level::Trace;
        // Setup drain for terminal output
        let terminal_decorator = slog_term::TermDecorator::new().build();
        let terminal_drain = slog_term::FullFormat::new(terminal_decorator)
            .build()
            .fuse();
        let terminal_drain = LevelFilter::new(terminal_drain, level).fuse();
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
