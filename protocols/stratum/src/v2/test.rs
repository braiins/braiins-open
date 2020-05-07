use super::*;
use crate::test_utils::v2::*;

use crate::error::Result;
use crate::v2::messages;
use crate::v2::telemetry;
use crate::v2::types::{Seq0_255, Uint256Bytes};

use ii_unvariant::{handler, GetId};

use std::convert::TryInto;

// TODO: Remove once async traits are removed
/// This test demonstrates an actual implementation of protocol handler (aka visitor to a set of
/// desired messsages
/// TODO refactor this test once we have a message dispatcher in place

#[tokio::test]
async fn test_build_message_from_frame() {
    let message_payload = build_setup_connection();
    let frame = message_payload
        .try_into()
        .expect("BUG: Cannot create test frame");

    let mut handler = TestIdentityHandler;
    handler.handle_v2(frame).await;
}

struct TelemetryHandler;

#[handler(async try framing::Frame suffix _v2)]
impl TelemetryHandler {
    async fn handle_open_telemetry_channel(
        &mut self,
        msg: Result<telemetry::messages::OpenTelemetryChannel>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_open_telemetry_channel_success(
        &mut self,
        msg: Result<telemetry::messages::OpenTelemetryChannelSuccess>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_open_telemetry_channel_error(
        &mut self,
        msg: Result<telemetry::messages::OpenTelemetryChannelError>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_submit_telemetry_data(
        &mut self,
        msg: Result<telemetry::messages::SubmitTelemetryData>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_submit_telemetry_data_success(
        &mut self,
        msg: Result<telemetry::messages::SubmitTelemetryDataSuccess>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_submit_telemetry_data_error(
        &mut self,
        msg: Result<telemetry::messages::SubmitTelemetryDataError>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!(": {:?}", e);
        }
        msg.map(|_m| {})
    }

    #[handle(_)]
    async fn handle_unknown(&mut self, frame: framing::Frame) -> Result<()> {
        Err(crate::error::Error::V2(error::Error::UnknownMessage(
            format!("BUG: Unimplemented handler for message {}", frame.get_id()),
        )))
    }
}

struct FullMiningHandler;

#[handler(async try framing::Frame suffix _v2)]
impl FullMiningHandler {
    async fn handle_setup_connection(
        &mut self,
        msg: Result<messages::SetupConnection>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_setup_connection_success(
        &mut self,
        msg: Result<messages::SetupConnectionSuccess>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_setup_connection_error(
        &mut self,
        msg: Result<messages::SetupConnectionError>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_channel_endpoint_changed(
        &mut self,
        msg: Result<messages::ChannelEndpointChanged>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_open_standard_mining_channel(
        &mut self,
        msg: Result<messages::OpenStandardMiningChannel>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_open_standard_mining_channel_success(
        &mut self,
        msg: Result<messages::OpenStandardMiningChannelSuccess>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_open_standard_mining_channel_error(
        &mut self,
        msg: Result<messages::OpenStandardMiningChannelError>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_update_channel(&mut self, msg: Result<messages::UpdateChannel>) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_update_channel_error(
        &mut self,
        msg: Result<messages::UpdateChannelError>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_close_channel(&mut self, msg: Result<messages::CloseChannel>) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_submit_shares_standard(
        &mut self,
        msg: Result<messages::SubmitSharesStandard>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_submit_shares_success(
        &mut self,
        msg: Result<messages::SubmitSharesSuccess>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_submit_shares_error(
        &mut self,
        msg: Result<messages::SubmitSharesError>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_new_mining_job(&mut self, msg: Result<messages::NewMiningJob>) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_new_extended_mining_job(
        &mut self,
        msg: Result<messages::NewExtendedMiningJob>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_set_new_prev_hash(
        &mut self,
        msg: Result<messages::SetNewPrevHash>,
    ) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_set_target(&mut self, msg: Result<messages::SetTarget>) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }
    async fn handle_reconnect(&mut self, msg: Result<messages::Reconnect>) -> Result<()> {
        if let Err(e) = &msg {
            panic!("BUG: Message parsing failed: {:?}", e);
        }
        msg.map(|_m| {})
    }

    #[handle(_)]
    async fn handle_unknown(&mut self, frame: framing::Frame) -> Result<()> {
        Err(crate::error::Error::V2(error::Error::UnknownMessage(
            format!(
                "BUG: Handler unimplemented handler for message {}",
                frame.get_id()
            ),
        )))
    }
}

#[tokio::test]
async fn test_telemetry_handler() {
    let open_ch: framing::Frame = build_open_telemetry_channel()
        .try_into()
        .expect("BUG: Cannot create test frame");
    let open_ch_s: framing::Frame = build_open_telemetry_channel_success()
        .try_into()
        .expect("BUG: Cannot create test frame");
    let open_ch_e: framing::Frame = build_open_telemetry_channel_error()
        .try_into()
        .expect("BUG: Cannot create test frame");
    let sub_d: framing::Frame = build_submit_telemetry_data()
        .try_into()
        .expect("BUG: Cannot create test frame");
    let sub_d_s: framing::Frame = build_submit_telemetry_data_success()
        .try_into()
        .expect("BUG: Cannot create test frame");
    let sub_d_e: framing::Frame = build_submit_telemetry_data_error()
        .try_into()
        .expect("BUG: Cannot create test frame");

    let mut handler = TelemetryHandler;

    handler
        .handle_v2(open_ch)
        .await
        .expect("BUG: message handling failed");
    handler
        .handle_v2(open_ch_s)
        .await
        .expect("BUG: message handling failed");
    handler
        .handle_v2(open_ch_e)
        .await
        .expect("BUG: message handling failed");
    handler
        .handle_v2(sub_d)
        .await
        .expect("BUG: message handling failed");
    handler
        .handle_v2(sub_d_s)
        .await
        .expect("BUG: message handling failed");
    handler
        .handle_v2(sub_d_e)
        .await
        .expect("BUG: message handling failed");
}

#[tokio::test]
async fn test_full_mining_handler() {
    let msg0: framing::Frame = build_setup_connection()
        .try_into()
        .expect("BUG: Cannot create test frame");
    let msg1: framing::Frame = build_setup_connection_success()
        .try_into()
        .expect("BUG: Cannot create test frame");
    let msg2: framing::Frame = messages::SetupConnectionError {
        flags: 0,
        code: Default::default(),
    }
    .try_into()
    .expect("BUG: Cannot create test frame");
    let msg3: framing::Frame = messages::ChannelEndpointChanged { channel_id: 0 }
        .try_into()
        .expect("BUG: Cannot create test frame");
    let msg4: framing::Frame = build_open_channel()
        .try_into()
        .expect("BUG: Cannot create test frame");
    let msg5: framing::Frame = build_open_channel_success()
        .try_into()
        .expect("BUG: Cannot create test frame");
    let msg6: framing::Frame = messages::OpenStandardMiningChannelError {
        req_id: 0,
        code: Default::default(),
    }
    .try_into()
    .expect("BUG: Cannot create test frame");
    let msg7: framing::Frame = messages::UpdateChannel {
        channel_id: 0,
        nominal_hash_rate: 0.0,
        maximum_target: ii_bitcoin::Target::default().into(),
    }
    .try_into()
    .expect("BUG: Cannot create test frame");
    let msg8: framing::Frame = messages::UpdateChannelError {
        channel_id: 0,
        error_code: Default::default(),
    }
    .try_into()
    .expect("BUG: Cannot create test frame");
    let msg9: framing::Frame = messages::CloseChannel {
        channel_id: 0,
        reason_code: Default::default(),
    }
    .try_into()
    .expect("BUG: Cannot create test frame");
    let msg10: framing::Frame = build_submit_shares()
        .try_into()
        .expect("BUG: Cannot create test frame");
    let msg11: framing::Frame = messages::SubmitSharesSuccess {
        channel_id: 0,
        last_seq_num: 0,
        new_submits_accepted_count: 0,
        new_shares_sum: 0,
    }
    .try_into()
    .expect("BUG: Cannot create test frame");
    let msg12: framing::Frame = messages::SubmitSharesError {
        channel_id: 0,
        seq_num: 0,
        code: Default::default(),
    }
    .try_into()
    .expect("BUG: Cannot create test frame");
    let msg13: framing::Frame = build_new_mining_job()
        .try_into()
        .expect("BUG: Cannot create test frame");
    let msg14: framing::Frame = messages::NewExtendedMiningJob {
        channel_id: 0,
        job_id: 0,
        future_job: false,
        version: 0,
        version_rolling_allowed: false,
        merkle_path: Seq0_255::<Uint256Bytes>::new(),
        coinbase_tx_prefix: Default::default(),
        coinbase_tx_suffix: Default::default(),
    }
    .try_into()
    .expect("BUG: Cannot create test frame");
    let msg15: framing::Frame = build_set_new_prev_hash()
        .try_into()
        .expect("BUG: Cannot create test frame");
    let msg16: framing::Frame = messages::SetTarget {
        channel_id: 0,
        max_target: ii_bitcoin::Target::default().into(),
    }
    .try_into()
    .expect("BUG: Cannot create test frame");
    let msg17: framing::Frame = build_reconnect()
        .try_into()
        .expect("BUG: Cannot create test frame");

    let mut handler = FullMiningHandler;
    handler
        .handle_v2(msg0)
        .await
        .expect("BUG: V2 frame handling failed");
    handler
        .handle_v2(msg1)
        .await
        .expect("BUG: V2 frame handling failed");
    handler
        .handle_v2(msg2)
        .await
        .expect("BUG: V2 frame handling failed");
    handler
        .handle_v2(msg3)
        .await
        .expect("BUG: V2 frame handling failed");
    handler
        .handle_v2(msg4)
        .await
        .expect("BUG: V2 frame handling failed");
    handler
        .handle_v2(msg5)
        .await
        .expect("BUG: V2 frame handling failed");
    handler
        .handle_v2(msg6)
        .await
        .expect("BUG: V2 frame handling failed");
    handler
        .handle_v2(msg7)
        .await
        .expect("BUG: V2 frame handling failed");
    handler
        .handle_v2(msg8)
        .await
        .expect("BUG: V2 frame handling failed");
    handler
        .handle_v2(msg9)
        .await
        .expect("BUG: V2 frame handling failed");
    handler
        .handle_v2(msg10)
        .await
        .expect("BUG: V2 frame handling failed");
    handler
        .handle_v2(msg11)
        .await
        .expect("BUG: V2 frame handling failed");
    handler
        .handle_v2(msg12)
        .await
        .expect("BUG: V2 frame handling failed");
    handler
        .handle_v2(msg13)
        .await
        .expect("BUG: V2 frame handling failed");
    handler
        .handle_v2(msg14)
        .await
        .expect("BUG: V2 frame handling failed");
    handler
        .handle_v2(msg15)
        .await
        .expect("BUG: V2 frame handling failed");
    handler
        .handle_v2(msg16)
        .await
        .expect("BUG: V2 frame handling failed");
    handler
        .handle_v2(msg17)
        .await
        .expect("BUG: V2 frame handling failed");
}

#[tokio::test]
async fn test_partially_implemented_mining_handler() {
    struct PartialMiningHandler;

    #[handler(async try framing::Frame suffix _v2)]
    impl PartialMiningHandler {
        async fn handle_setup_connection(
            &mut self,
            msg: Result<messages::SetupConnection>,
        ) -> Result<()> {
            if let Err(e) = &msg {
                panic!("BUG: Message parsing failed: {:?}", e);
            }
            msg.map(|_m| {})
        }

        #[handle(_)]
        async fn handle_non_implemented(&mut self, frame: framing::Frame) -> Result<()> {
            Err(crate::error::Error::V2(error::Error::UnknownMessage(
                format!(
                    "BUG: Handler unimplemented handler for message {}",
                    frame.get_id()
                ),
            )))
        }
    }

    let mut handler = PartialMiningHandler;

    let msg0: framing::Frame = build_setup_connection()
        .try_into()
        .expect("BUG: Cannot create test frame");
    let msg1: framing::Frame = build_setup_connection_success()
        .try_into()
        .expect("BUG: Cannot create test frame");

    handler
        .handle_v2(msg0)
        .await
        .expect("BUG: Handling message failed");
    handler
        .handle_v2(msg1)
        .await
        .expect_err("BUG: Handling message should've failed because handler was not implemented");
}
