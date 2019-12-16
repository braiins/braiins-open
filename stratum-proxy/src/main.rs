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

//! Simple proxy that translates V2 protocol from clients to V1 protocol and connects to a
//! requested pool

use std::cell::RefCell;

use clap::{self, Arg};
use ctrlc;

use ii_async_compat::tokio;
use ii_logging::macros::*;
use ii_stratum_proxy::server;

// TODO: defaults for listen & remote addrs?
// static V2_ADDR: &'static str = "127.0.0.1:3334";
// static V1_ADDR: &'static str = "127.0.0.1:3335";

#[tokio::main]
async fn main() {
    ii_async_compat::setup_panic_handling();
    let _log_guard =
        ii_logging::setup_for_app(ii_logging::LoggingConfig::ASYNC_LOGGER_DRAIN_CHANNEL_SIZE);

    let args = clap::App::new("stratum-proxy")
        .arg(
            Arg::with_name("listen")
                .short("l")
                .long("listen")
                .value_name("HOSTNAME:PORT")
                .help("Address the V2 end listen on")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("remote")
                .short("r")
                .long("remote")
                .value_name("HOSTNAME:PORT")
                .help("Address the V1 end connects to")
                .required(true)
                .takes_value(true),
        )
        .get_matches();

    // Unwraps should be ok as long as the flags are required
    let v2_addr = args.value_of("listen").unwrap();
    let v1_addr = args.value_of("remote").unwrap();

    let server = match server::ProxyServer::listen(v2_addr.to_string(), v1_addr.to_string()) {
        Ok(task) => task,
        Err(err) => {
            error!("Can't bind the server: {}", err);
            return;
        }
    };

    let quit = RefCell::new(server.quit_channel());
    ctrlc::set_handler(move || {
        // Received SIGINT, tell the server task to shut down:
        let _ = quit.try_borrow_mut().map(|mut quit| quit.try_send(()));
    })
    .expect("Could not set SIGINT handler");

    server.run().await;
}
