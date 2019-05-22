#![feature(await_macro, async_await)]

use tokio::r#await;
use tokio::net::TcpListener;
use tokio::prelude::*;

use stratum;
use stratum::v1::messages::SubscribeResponse;

use stratumproxy::protocol;
use stratumproxy::utils::CompatFix;

static LISTEN_ADDR: &'static str = "127.0.0.1:3000";

static STRATUM_ADDR: &'static str = "52.212.249.159:3333";


async fn run_client(addr: &'static str) {
    let (mut dispatcher, client) = protocol::Dispatcher::new_client();

    tokio::spawn(client.connect(addr).compat_fix());

    let subscribe_rpc: stratum::v1::rpc::Request = stratum::v1::messages::Subscribe {
        id: 0,
        agent_signature: "bOS".into(),
        extra_nonce1: None,
    }
        .into();
    // let resp: SubscribeResponse = await!(dispatcher.send(subscribe_rpc)).expect("Sending failed");
    let resp: stratum::v1::rpc::Response = await!(dispatcher.send(subscribe_rpc)).expect("Sending failed");
    //    let subscribe_rpc: stratum::v1::rpc::Request = stratum::v1::messages::Subscribe::create(
    //        0,
    //        "bOS",
    //        None,
    //    )
    //        .into();

    println!("response: {:?}", resp);
}

// fn run_server() {
//     let addr = LISTEN_ADDR.parse().unwrap();
//     let socket = TcpListener::bind(&addr).unwrap();

//     let mut incoming = socket.incoming();
//     let server = async move {
//         while let Some(Ok(socket)) = await!(incoming.next()) {
//             let (reader, writer) = socket.split();
//             let copy = tokio::io::copy(reader, writer);

//             let msg = async move {
//                 match await!(copy) {
//                     Ok((amount, _, _)) => eprintln!("wrote {} bytes", amount),
//                     Err(e) => eprintln!("error: {}", e),
//                 }
//             };

//             tokio::spawn_async(msg);
//         }
//     };

//     eprintln!("Server running on {}", LISTEN_ADDR);
//     tokio::run_async(server);
// }

fn main() {
    tokio::run(run_client(STRATUM_ADDR).compat_fix());
}
