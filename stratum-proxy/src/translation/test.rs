#![feature(await_macro, async_await)]
use futures::stream::StreamExt;
use std::fmt::Debug;
use tokio::prelude::*;
use tokio::runtime::current_thread as runtime;

use super::*;
use stratum::test_utils;
use stratum::v2;
use wire::tokio;
use wire::utils::CompatFix;
use wire::{self, Message, Payload};

//       F::Error: From<E>,
//        M: TryInto<F::Send, Error = E>,

/// Simulates
fn v2_simulate_incoming_message<M>(translation: &mut V2ToV1Translation, message: M)
where
    M: TryInto<TxFrame, Error = stratum::error::Error>,
{
    // create a tx frame, we won't send it but only extract the pure data (as it implements the deref trait)
    let frame: wire::TxFrame = message.try_into().expect("Could not serialize message");

    let msg = v2::deserialize_message(&frame).expect("Deserialization failed");
    msg.accept(translation);
}

async fn v2_verify_generated_response_message(v2_rx: &mut mpsc::Receiver<TxFrame>) {
    // Pickup the response and verify it
    let v2_response_tx_frame = await!(v2_rx.next()).expect("At least 1 message was expected");

    // This is specific for the unit test only: Instead of sending the message via some
    // connection, the test case will deserialize it and inspect it using the identity
    // handler from test utils
    let v2_response =
        v2::deserialize_message(&v2_response_tx_frame).expect("Deserialization frame");
    // verify the response using testing identity handler
    v2_response.accept(&mut test_utils::v2::TestIdentityHandler);
}

#[test]
fn test_setup_mining_connection_translate() {
    runtime::run(
        async {
            let (v1_tx, mut v1_rx) = mpsc::channel(1);
            let (v2_tx, mut v2_rx) = mpsc::channel(1);
            let mut translation = V2ToV1Translation::new(v1_tx, v2_tx);

            v2_simulate_incoming_message(
                &mut translation,
                test_utils::v2::build_setup_mining_connection(),
            );

            await!(v2_verify_generated_response_message(&mut v2_rx));
            //            // Pickup the response and verify it
            //            let v2_response_tx_frame =
            //                await!(v2_rx.next()).expect("At least 1 message was expected");
            //
            //            // This is specific for the unit test only: Instead of sending the message via some
            //            // connection, the test case will deserialize it and inspect it using the identity
            //            // handler from test utils
            //            let v2_response =
            //                v2::deserialize_message(&v2_response_tx_frame).expect("Deserialization frame");
            //            // verify the response using testing identity handler
            //            v2_response.accept(&mut test_utils::v2::TestIdentityHandler);

            v2_simulate_incoming_message(&mut translation, test_utils::v2::build_open_channel());
            await!(v1_verify_generated_response_message(&mut v1_rx));
            await!(v2_verify_generated_response_message(&mut v2_rx));
        }
            .compat_fix(),
    );
}
