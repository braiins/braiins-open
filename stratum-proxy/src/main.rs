//! Simple proxy that translates V2 protocol from clients to V1 protocol and connects to a
//! requested pool

#![feature(await_macro, async_await)]

use std::cell::RefCell;

use clap::{self, Arg};
use ctrlc;
use futures::future::FutureExt;
use slog::{error, info, trace};
use tokio::net::TcpListener;
use tokio::prelude::*;
use tokio::r#await;
use wire::utils::CompatFix;
use wire::{tokio, Framing};

use stratumproxy::server;

static V2_ADDR: &'static str = "127.0.0.1:3334";
static V1_ADDR: &'static str = "127.0.0.1:3335";

fn main() {
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

    let (server_task, quit) = server::run(v2_addr.to_string(), v1_addr.to_string());
    let quit = RefCell::new(quit);

    ctrlc::set_handler(move || {
        // Received SIGINT, tell the server taks to shut down:
        quit.try_borrow_mut().map(|mut quit| quit.try_send(()));
    });

    tokio::run(server_task.compat_fix());

    // FIXME: flush logs
}
