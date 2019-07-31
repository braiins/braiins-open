//! Test of logging basic setup and usage.
//!
//! **Warning**: Each logging test needs to be in a separate files
//! due to global LOGGER initialization

use std::env;
use std::fs;

use logging::macros::*;
use logging::{self, Level, LoggingConfig, LoggingTarget, LOGGER};

use tempfile::NamedTempFile;

#[test]
fn test_logging_basic() {
    const LOG_MSG: &'static str = "Hello, World!";

    // Set RUST_LOG to "": Don't let outer environment influence the test
    // and test the behaviour if RUST_LOG is empty
    env::set_var("RUST_LOG", "");

    // Create configuration
    let temp_file = NamedTempFile::new().expect("Could not create temporary file");
    let config = LoggingConfig {
        target: LoggingTarget::File(temp_file.path().into()),
        level: Level::Trace,
    };

    // Setup logger
    logging::set_logger_config(config);
    let flush_guard = LOGGER.take_guard();

    // Log a message and flush logs
    trace!("{}", LOG_MSG);
    drop(flush_guard);

    // Verify message
    let log_contents = fs::read_to_string(temp_file.path()).expect("Could not read back log file");
    assert!(log_contents.find(LOG_MSG).is_some());
}
