//! Simple proxy that translates V2 protocol from clients to V1 protocol and connects to a
//! requested pool

#![feature(await_macro, async_await)]

use std::cell::RefCell;

use clap::{self, Arg};
use ctrlc;

use ii_logging::macros::*;
use ii_stratum_proxy::server;
use ii_wire::tokio;
use ii_wire::utils::CompatFix;

// TODO: defaults for listen & remote addrs?
// static V2_ADDR: &'static str = "127.0.0.1:3334";
// static V1_ADDR: &'static str = "127.0.0.1:3335";

fn main() {
    let _log_guard = ii_logging::setup_for_app();

    let args = clap::App::new("stratum-proxy")
        .arg(
            Arg::with_name("listen")
                .short("l")
                .long("listen")
                .value_name("ADDR")
                .help("Address the V2 end listen on")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("remote")
                .short("r")
                .long("remote")
                .value_name("ADDR")
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

    tokio::run(server.run().compat_fix());
}
