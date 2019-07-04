use std::fs;

use logging::macros::*;
use logging::{self, Level, LoggingConfig, LOGGER};

use tempfile::NamedTempFile;

// Note: Each logging test needs to be in a separate files
// due to global LOGGER initialization

#[test]
fn test_logging_basic() {
    const LOG_MSG: &'static str = "Hello, World!";

    // Create configuration
    let temp_file = NamedTempFile::new().expect("Could not create temporary file");
    let config = LoggingConfig {
        file: Some((Level::Trace, temp_file.path().into())),
        term: None,
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
