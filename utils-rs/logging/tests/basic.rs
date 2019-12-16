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

//! Test of logging basic setup and usage.
//!
//! **Warning**: Each logging test needs to be in a separate files
//! due to global LOGGER initialization

use std::env;
use std::fs;

use ii_logging::macros::*;
use ii_logging::{self, Level, LoggingConfig, LoggingTarget, LOGGER};

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
        drain_channel_size: LoggingConfig::ASYNC_LOGGER_DRAIN_CHANNEL_SIZE,
    };

    // Setup logger
    ii_logging::set_logger_config(config);
    let flush_guard = LOGGER.take_guard();

    // Log a message and flush logs
    trace!("{}", LOG_MSG);
    drop(flush_guard);

    // Verify message
    let log_contents = fs::read_to_string(temp_file.path()).expect("Could not read back log file");
    assert!(log_contents.find(LOG_MSG).is_some());
}
