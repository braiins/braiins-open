#![feature(await_macro, async_await)]
use futures::stream::StreamExt;
use std::fmt::Debug;
use std::str;
use tokio::prelude::*;
use tokio::runtime::current_thread as runtime;

use super::*;
use stratum::test_utils;
use stratum::v1;
use stratum::v2;
use wire::utils::CompatFix;
use wire::{self, Message, Payload};
use wire::{tokio, ProtocolBase};

//       F::Error: From<E>,
//        M: TryInto<F::Send, Error = E>,

/// Simulates incoming message by converting it into a TxFrame and running the deserialization
/// chain from that point on
fn v2_simulate_incoming_message<M>(translation: &mut V2ToV1Translation, message: M)
where
    M: TryInto<TxFrame, Error = stratum::error::Error>,
{
    // create a tx frame, we won't send it but only extract the pure data (as it implements the deref trait)
    let frame: wire::TxFrame = message.try_into().expect("Could not serialize message");

    let msg = v2::deserialize_message(&frame).expect("Deserialization failed");
    msg.accept(translation);
}

fn v1_simulate_incoming_message<M>(translation: &mut V2ToV1Translation, message: M)
where
    M: TryInto<TxFrame, Error = stratum::error::Error>,
{
    // create a tx frame, we won't send it but only extract the pure data (as it implements the deref trait) as if it arrived to translation
    let frame: wire::TxFrame = message.try_into().expect("Could not serialize message");

    let msg = v1::deserialize_message(
        std::str::from_utf8(&frame).expect("Cannot convert frame to utf-8 str"),
    )
    .expect("Deserialization failed");
    msg.accept(translation);
}

async fn v2_verify_generated_response_message(v2_rx: &mut mpsc::Receiver<TxFrame>) {
    // Pickup the response and verify it
    let v2_response_tx_frame = await!(v2_rx.next()).expect("At least 1 message was expected");

    // This is specific for the unit test only: Instead of sending the message via some
    // connection, the test case will deserialize it and inspect it using the identity
    // handler from test utils
    let v2_response =
        v2::deserialize_message(&v2_response_tx_frame).expect("Deserialization failed");
    // verify the response using testing identity handler
    v2_response.accept(&mut test_utils::v2::TestIdentityHandler);
}

//fn verify_message_from_frame<F, T, P: ProtocolBase, H>(
//    frame: TxFrame,
//    deserialize_message: F,
//    &mut handler: P::Handler,
//) where
//    F: Fn(T) -> Message<P>,
//{
//    let message = deserialize_message(&frame).expect("Deserialization failed");
//    // verify the response using testing identity handler
//    message.accept(handler);
//}

async fn v1_verify_generated_response_message(v1_rx: &mut mpsc::Receiver<TxFrame>) {
    // Pickup the response and verify it
    // TODO add timeout
    let frame = await!(v1_rx.next()).expect("At least 1 message was expected");

    let msg = v1::deserialize_message(
        std::str::from_utf8(&frame).expect("Cannot convert frame to utf-8 str"),
    )
    .expect("Deserialization failed");
    msg.accept(&mut test_utils::v1::TestIdentityHandler);
}

/// This test simulates incoming connection to the translation and verifies that the translation
/// emits corresponding V1 or V2 messages
/// TODO we need a way to detect that translation is not responding and the entire test should fail
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
            // Setup mining connection should result into: mining.configure
            await!(v1_verify_generated_response_message(&mut v1_rx));
            v1_simulate_incoming_message(
                &mut translation,
                test_utils::v1::build_configure_ok_response_message(),
            );
            await!(v2_verify_generated_response_message(&mut v2_rx));

            // Opening a channel should result into: V1 generating a subscribe request
            v2_simulate_incoming_message(&mut translation, test_utils::v2::build_open_channel());
            // Opening a channel should result into: V1 generating a subscribe and authorize requests
            await!(v1_verify_generated_response_message(&mut v1_rx));
            await!(v1_verify_generated_response_message(&mut v1_rx));

            // Subscribe response
            v1_simulate_incoming_message(
                &mut translation,
                test_utils::v1::build_subscribe_ok_response_frame(),
            );
            // Authorize response
            v1_simulate_incoming_message(
                &mut translation,
                test_utils::v1::build_authorize_ok_response_message(),
            );

            // SetDifficulty notification before completion
            v1_simulate_incoming_message(
                &mut translation,
                test_utils::v1::build_set_difficulty_request_message(),
            );
            // Now we should have a successfully open channel
            await!(v2_verify_generated_response_message(&mut v2_rx));

            v1_simulate_incoming_message(
                &mut translation,
                test_utils::v1::build_mining_notify_request_message(),
            );
            // Expect NewMiningJob
            await!(v2_verify_generated_response_message(&mut v2_rx));
            // Expect SetNewPrevHash
            await!(v2_verify_generated_response_message(&mut v2_rx));
            // Ensure that the V1 job has been registered
            let submit_template = V1SubmitTemplate {
                job_id: v1::messages::JobId::from_slice(&test_utils::v1::MINING_NOTIFY_JOB_ID),
                time: test_utils::v1::MINING_NOTIFY_NTIME,
                version: test_utils::common::MINING_WORK_VERSION,
            };

            let registered_submit_template = translation
                .v2_to_v1_job_map
                .get(&0)
                .expect("No mining job with V2 ID 0");
            assert_eq!(
                submit_template,
                registered_submit_template.clone(),
                "New Mining Job ID not registered!"
            );

            // Send SubmitShares
            v2_simulate_incoming_message(&mut translation, test_utils::v2::build_submit_shares());
            // Expect mining.submit to be generated
            await!(v1_verify_generated_response_message(&mut v1_rx));
            // Simulate mining.submit response (true)
            v1_simulate_incoming_message(
                &mut translation,
                test_utils::v1::build_mining_submit_ok_response_message(),
            );
            // Expect SubmitSharesSuccess to be generated
            await!(v2_verify_generated_response_message(&mut v2_rx));
        }
            .compat_fix(),
    );
}

#[test]
fn test_diff_1_bitcoin_target() {
    // Difficulty 1 target in big-endian format
    let difficulty_1_target_bytes: [u8; 32] = [
        0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ];
    let expected_difficulty_1_target_uint256 =
        uint::U256::from_big_endian(&difficulty_1_target_bytes);

    assert_eq!(
        expected_difficulty_1_target_uint256,
        V2ToV1Translation::DIFF1_TARGET,
        "Bitcoin difficulty 1 targets don't match exp: {:x?}, actual:{:x?}",
        expected_difficulty_1_target_uint256,
        V2ToV1Translation::DIFF1_TARGET
    );
}
