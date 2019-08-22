//! Test that setting logger config after using the logger
//! results in a panic.
//!
//! **Warning**: Each logging test needs to be in a separate files
//! due to global LOGGER initialization

use ii_logging::macros::*;
use ii_logging::{self, LoggingConfig};

#[test]
#[should_panic]
fn test_logging_config_too_late() {
    // Use silent config
    ii_logging::set_logger_config(LoggingConfig::no_logging());

    // Log something
    trace!("This will tirgger LOGGER instantiation");

    // This should now panic
    ii_logging::set_logger_config(LoggingConfig::default());
}
