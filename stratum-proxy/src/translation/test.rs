#![feature(await_macro, async_await)]
use futures::stream::StreamExt;
use tokio::prelude::*;
use tokio::runtime::current_thread as runtime;

use super::*;
use stratum::test_utils;
use stratum::v2;
use wire::tokio;
use wire::utils::CompatFix;
use wire::{self, Message, Payload};

#[test]
fn test_setup_mining_connection_translate() {
    runtime::run(
        async {
            let (v1_tx, mut v1_rx) = mpsc::channel(1);
            let (v2_tx, mut v2_rx) = mpsc::channel(1);
            let mut translation = V2ToV1Translation::new(v1_tx, v2_tx);

            // create a tx frame, we won't send it but only extract the pure data (as it implements the deref trait)
            let frame: wire::TxFrame = test_utils::v2::build_setup_mining_connection()
                .try_into()
                .expect("Could not serialize message");

            let msg = v2::deserialize_message(&frame).expect("Deserialization failed");
            msg.accept(&mut translation);

            // Pickup the response and verify it
            let v2_response_tx_frame =
                await!(v2_rx.next()).expect("At least 1 message was expected");

            // This is specific for the unit test only: Instead of sending the message via some
            // connection, the test case will deserialize it and inspect it using the identity
            // handler from test utils
            let v2_response =
                v2::deserialize_message(&v2_response_tx_frame).expect("Deserialization frame");
            // verify the response using testing identity handler
            v2_response.accept(&mut test_utils::v2::TestIdentityHandler);
        }
            .compat_fix(),
    );
}
