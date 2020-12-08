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

use std::collections::{HashMap, VecDeque};
use std::convert::From;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::fmt;
use std::mem::size_of;

use bytes::BytesMut;
use futures::channel::mpsc;
use primitive_types::U256;

use bitcoin_hashes::{sha256d, Hash, HashEngine};
use serde_json;
use serde_json::Value;

use ii_stratum::v1::{self, MessageId};
use ii_stratum::v2::{
    self,
    types::{Bytes0_32, Str0_255, Uint256Bytes},
};

use ii_unvariant::handler;

use ii_logging::macros::*;

use crate::error::{Error, Result, V2ProtocolError};
use crate::metrics::MetricsCollector;
use crate::util;
use std::sync::Arc;
use tokio::time::{Duration, Instant};

mod stratum {
    pub use ii_stratum::error::{Error, Result};
}

#[cfg(test)]
mod test;

/// Sequential ID to pair up messages, requests etc.
#[derive(Default, Debug)]
pub struct SeqId(u32);

impl SeqId {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a new ID, increment internal state
    pub fn next(&mut self) -> u32 {
        let current_value = self.0;
        self.0 = self.0.wrapping_add(1);
        current_value
    }
}

/// Compound struct for all translation options that can be tweaked in `V2ToV1Translation`
#[derive(Copy, Clone, Debug)]
pub struct V2ToV1TranslationOptions {
    /// Try to send `extranonce.subscribe` during handshake
    pub try_enable_xnsub: bool,
    /// Reconnect received from the upstream is translated and propagated to the v2 downstream
    /// connection. This can be useful for V2 clients that run this translation component locally
    pub propagate_reconnect_downstream: bool,
    // cannot use String here because of Copy trait requirement
    pub password: arrayvec::ArrayString<[u8; Self::MAX_V1_PASSWORD_SIZE]>,
}

impl V2ToV1TranslationOptions {
    const MAX_V1_PASSWORD_SIZE: usize = 128;

    pub fn new(
        try_enable_xnsub: bool,
        propagate_reconnect_downstream: bool,
        v1_password: &str,
    ) -> Self {
        let mut password = arrayvec::ArrayString::<[u8; Self::MAX_V1_PASSWORD_SIZE]>::new();
        password.push_str(v1_password);
        Self {
            try_enable_xnsub,
            propagate_reconnect_downstream,
            password,
        }
    }
}

impl Default for V2ToV1TranslationOptions {
    fn default() -> Self {
        Self {
            try_enable_xnsub: false,
            propagate_reconnect_downstream: false,
            password: arrayvec::ArrayString::new(),
        }
    }
}

/// States of the Translation setup
#[derive(PartialEq, Debug)]
enum V2ToV1TranslationState {
    /// No message received yet
    Init,
    /// Stratum V1 mining.configure is in progress
    V1Configure,
    /// Connection successfully setup, waiting for OpenStandardMiningChannel message
    ConnectionSetup,
    /// Channel now needs finalization of subscribe+authorize+set difficulty target with the
    /// upstream V1 server
    OpenStandardMiningChannelPending,
    /// Upstream subscribe/authorize failed state ensures sending OpenMiningChannelError only once
    V1SubscribeOrAuthorizeFail,
    /// Channel is operational
    Operational,
}

/// Represents a handler method that can process a particular ii_stratum result.
type V1StratumResultHandler =
    fn(&mut V2ToV1Translation, &v1::MessageId, &v1::rpc::StratumResult) -> Result<()>;

/// Represents a handler method that can process a particular ii_stratum error.
type V1StratumErrorHandler =
    fn(&mut V2ToV1Translation, &v1::MessageId, &v1::rpc::StratumError) -> Result<()>;

struct V1CompoundHandler {
    v1_stratum_result_handler: V1StratumResultHandler,
    v1_stratum_error_handler: V1StratumErrorHandler,
    /// Request method associated with these handlers is used for code instrumentation
    request_method: v1::rpc::Method,
    /// Time stamp when the request has been submitted
    timestamp_created: Instant,
}
impl V1CompoundHandler {
    fn new(
        v1_stratum_result_handler: V1StratumResultHandler,
        v1_stratum_error_handler: V1StratumErrorHandler,
        request_method: v1::rpc::Method,
    ) -> Self {
        Self {
            v1_stratum_result_handler,
            v1_stratum_error_handler,
            request_method,
            timestamp_created: Instant::now(),
        }
    }
    fn elapsed(&self) -> Duration {
        self.timestamp_created.elapsed()
    }
}

/// Custom mapping of V1 request id onto result/error handlers
type V1ReqMap = HashMap<u32, V1CompoundHandler>;

/// Helper template stored in V2->V1 job map
#[derive(Clone, PartialEq, Debug)]
struct V1SubmitTemplate {
    job_id: v1::messages::JobId,
    time: u32,
    version: u32,
}

enum V1ResultOrError<'a> {
    Result(&'a v1::rpc::StratumResult),
    Error(&'a v1::rpc::StratumError),
}

/// Maps V2 job ID to V1 job ID so that we can submit mining results upstream to V1 server
type JobMap = HashMap<u32, V1SubmitTemplate>;

//type V2ReqMap = HashMap<u32, FnMut(&mut V2ToV1Translation, &ii_stratum::Message<Protocol>, &v1::rpc::StratumResult)>;

enum SeqNum {
    V1(v1::MessageId),
    V2(u32),
}

/// Describes 2 variants of submitted shares
enum SubmitShare {
    /// Sequence number mapping between Stratum V1 and V2 SubmitShares/mining.submit resp.
    V1ToV2Mapping(u32, u32),
    /// Submit share error which proxy generates and can be faster than submitted shares to
    /// remote server
    SubmitSharesError(v2::messages::SubmitSharesError),
}

type SubmitShareQueue = VecDeque<SubmitShare>;

/// Object capable of translating stratm V2 header-only mining protocol that uses standard mining
/// channels into stratum V1 including extranonce 1 subscription
pub struct V2ToV1Translation {
    /// Statemachine tracking the translation setup
    state: V2ToV1TranslationState,

    /// Channel for sending out V1 responses
    v1_tx: mpsc::Sender<v1::Frame>,
    /// Unique request ID generator
    v1_req_id: SeqId,
    /// Mapping for pairing of incoming V1 message with original requests
    v1_req_map: V1ReqMap,

    v1_extra_nonce1: Option<v1::ExtraNonce1>,
    v1_extra_nonce2_size: usize,
    v1_authorized: bool,
    v1_xnsub_enabled: bool,

    /// Whether to force future jobs: might be handy for v1 pools which don't accept solutions with
    /// `ntime` less than specified on jobs they are solving (but greater than ntime on prevhash).
    v1_force_future_jobs: bool,

    /// Latest mining.notify payload that arrived before V1 authorize has completed.
    /// This allows immediate completion of channel open on V2.
    v1_deferred_notify: Option<v1::messages::Notify>,

    /// Channel for sending out V2 responses
    v2_tx: mpsc::Sender<v2::Frame>,
    #[allow(dead_code)] // TODO: unused as of now
    v2_req_id: SeqId,
    /// All connection details
    v2_conn_details: Option<v2::messages::SetupConnection>,
    /// Additional information about the pending channel being open
    v2_channel_details: Option<v2::messages::OpenStandardMiningChannel>,
    /// Target difficulty derived from mining.set_difficulty message
    /// The channel opening is not complete until the target is determined
    v2_target: Option<U256>,
    /// Unique job ID generator
    v2_job_id: SeqId,
    /// Translates V2 job ID to V1 job ID
    v2_to_v1_job_map: JobMap,
    /// Queue of submitted shares waiting for response processing
    v2_submit_share_queue: SubmitShareQueue,
    /// Options for translation
    options: V2ToV1TranslationOptions,
    v1_password: String,
    metrics: Option<Arc<MetricsCollector>>,
}

impl V2ToV1Translation {
    const PROTOCOL_VERSION: usize = 0;
    /// No support for the extended protocol yet, therefore, no extranonce advertised
    #[allow(dead_code)]
    const MAX_EXTRANONCE_SIZE: usize = 0;
    /// Currently, no support for multiple channels in the proxy
    const CHANNEL_ID: u32 = 0;
    /// Default group channel
    const DEFAULT_GROUP_CHANNEL_ID: u32 = 0;

    /// U256 in little endian
    /// TODO: consolidate into common part/generalize
    /// TODO: DIFF1 const target is broken, the last U64 word gets actually initialized to 0xffffffff, not sure why
    const DIFF1_TARGET: U256 = U256([0, 0, 0, 0xffff0000u64]);

    pub fn target_to_diff(target: U256) -> U256 {
        if target == U256::from(0) {
            U256::MAX
        } else {
            Self::DIFF1_TARGET / target
        }
    }

    pub fn diff_to_target<T: Into<U256>>(diff: T) -> U256 {
        let diff = diff.into();
        if diff == U256::from(0) {
            U256::MAX
        } else {
            Self::DIFF1_TARGET / diff
        }
    }

    pub fn new(
        v1_tx: mpsc::Sender<v1::Frame>,
        v2_tx: mpsc::Sender<v2::Frame>,
        options: V2ToV1TranslationOptions,
        metrics: Option<Arc<MetricsCollector>>,
    ) -> Self {
        let v1_password = options.password.to_string();
        Self {
            v2_conn_details: None,
            v2_channel_details: None,
            v2_target: None,
            state: V2ToV1TranslationState::Init,
            v1_tx,
            v1_req_id: SeqId::new(),
            v1_req_map: V1ReqMap::default(),
            v1_extra_nonce1: None,
            v1_extra_nonce2_size: 0,
            v1_authorized: false,
            v1_force_future_jobs: true,
            v1_xnsub_enabled: false,
            v1_deferred_notify: None,
            v2_tx,
            v2_req_id: SeqId::new(),
            v2_job_id: SeqId::new(),
            v2_to_v1_job_map: JobMap::default(),
            v2_submit_share_queue: SubmitShareQueue::default(),
            options,
            v1_password,
            metrics,
        }
    }

    /// Builds a V1 request from V1 method and assigns a unique identifier to it
    fn v1_method_into_message<M, E>(
        &mut self,
        method: M,
        result_handler: V1StratumResultHandler,
        error_handler: V1StratumErrorHandler,
    ) -> v1::rpc::Rpc
    where
        E: fmt::Debug,
        M: TryInto<v1::rpc::RequestPayload, Error = E>,
    {
        let payload = method
            .try_into()
            .expect("BUG: Cannot convert V1 method into a message");

        // TODO: decorate the request with a new unique ID -> this is the request ID
        let id = self.v1_req_id.next();
        trace!("Registering v1, request ID: {} method: {:?}", id, payload);
        if self
            .v1_req_map
            .insert(
                id,
                V1CompoundHandler::new(result_handler, error_handler, payload.method),
            )
            .is_some()
        {
            error!("BUG: V1 id {} already exists...", id);
            // TODO add graceful handling of this bug (shutdown?)
            panic!("V1 id already exists");
        }

        v1::rpc::Request {
            id: Some(id),
            payload,
        }
        .into()
    }

    /// Sets the current pending channel to operational state and submits success message
    fn finalize_open_channel(&mut self) -> Result<()> {
        trace!("finalize_open_channel()");
        let mut init_target: Uint256Bytes = Uint256Bytes([0; 32]);
        self.v2_target
            .expect("BUG: initial target still not defined when attempting to finalize OpenStandardMiningChannel")
            .to_little_endian(init_target.as_mut());

        // when V1 authorization has already taken place, report channel opening success
        if let Some(v2_channel_details) = self.v2_channel_details.as_ref() {
            self.state = V2ToV1TranslationState::Operational;
            let msg = v2::messages::OpenStandardMiningChannelSuccess {
                req_id: v2_channel_details.req_id,
                channel_id: Self::CHANNEL_ID,
                target: init_target.clone(),
                extranonce_prefix: Bytes0_32::new(),
                group_channel_id: Self::DEFAULT_GROUP_CHANNEL_ID,
            };
            util::submit_message(&mut self.v2_tx, msg)?;

            // If mining.notify is pending, process it now as part of open channel finalization
            if let Some(notify_payload) = self.v1_deferred_notify.take() {
                self.perform_notify(&notify_payload)?;
            }
            return Ok(());
        } else {
            Err(
                ii_stratum::error::Error::from(v2::error::Error::ChannelNotOperational(
                    "Channel details missing".to_string(),
                ))
                .into(),
            )
        }
    }

    /// Send new target
    /// TODO extend the translation unit test accordingly
    fn send_set_target(&mut self) -> Result<()> {
        trace!("send_set_target()");
        let max_target = Uint256Bytes::from(self.v2_target.expect(
            "BUG: initial target still not defined when attempting to finalize \
             OpenStandardMiningChannel",
        ));

        let msg = v2::messages::SetTarget {
            channel_id: Self::CHANNEL_ID,
            max_target,
        };

        util::submit_message(&mut self.v2_tx, msg)
    }

    /// Reports failure to open the channel and changes the translation state
    /// From this point on a new OpenStandardMiningChannel message is expected as an attempt to reopen the channel
    fn abort_open_channel(&mut self, err_msg: &str) {
        trace!(
            "abort_open_channel() - channel details: {:?}, msg: {}",
            self.v2_channel_details,
            err_msg
        );
        self.state = V2ToV1TranslationState::V1SubscribeOrAuthorizeFail;

        // Cleanup all parts associated with opening the channel
        self.v1_authorized = false;
        self.v1_extra_nonce1 = None;
        self.v1_extra_nonce2_size = 0;

        if let Some(v2_channel_details) = self.v2_channel_details.as_ref() {
            let msg = v2::messages::OpenMiningChannelError {
                req_id: v2_channel_details.req_id,
                code: err_msg.try_into().expect("BUG: incorrect error message"),
            };
            self.v2_channel_details = None;

            if let Err(submit_err) = util::submit_message(&mut self.v2_tx, msg) {
                info!(
                    "abort_open_channel() failed: {:?}, abort message: {}",
                    submit_err, err_msg
                );
            }
        } else {
            error!(
                "abort_open_channel(): no channel to abort, missing V2 channel details, message: \
                 {}",
                err_msg
            );
        }
    }

    /// Finalizes a pending SetupConnection upon successful negotiation of
    /// mining configuration of version rolling bits
    fn handle_configure_result(
        &mut self,
        id: &v1::MessageId,
        payload: &v1::rpc::StratumResult,
    ) -> Result<()> {
        trace!(
            "handle_configure_result() id={:?} state={:?} payload:{:?}",
            id,
            self.state,
            payload,
        );

        // TODO review the use of serde_json here, it may be possible to eliminate this dependency
        // Extract version mask and verify it matches the maximum possible value
        let proposed_version_mask: v1::messages::VersionMask =
            serde_json::from_value(payload.0["version-rolling.mask"].clone())?;

        trace!(
            "Evaluating: version-rolling state == {:?} && mask=={:x?}",
            payload.0["version-rolling"].as_bool(),
            proposed_version_mask
        );
        if payload.0["version-rolling"].as_bool() == Some(true)
            && (proposed_version_mask.0).0 == ii_stratum::BIP320_N_VERSION_MASK
        {
            self.state = V2ToV1TranslationState::ConnectionSetup;

            let success = v2::messages::SetupConnectionSuccess {
                used_version: Self::PROTOCOL_VERSION as u16,
                flags: 0,
            };
            util::submit_message(&mut self.v2_tx, success)
        } else {
            // TODO consolidate into abort_connection() + communicate shutdown of this
            // connection similarly everywhere in the code
            let response = v2::messages::SetupConnectionError {
                flags: 0, // TODO handle flags
                code: "Cannot negotiate upstream V1 version mask"
                    .try_into()
                    .expect("BUG: incorrect error message"),
            };
            util::submit_message(&mut self.v2_tx, response)
        }
    }

    fn handle_configure_error(
        &mut self,
        id: &v1::MessageId,
        payload: &v1::rpc::StratumError,
    ) -> Result<()> {
        trace!(
            "handle_configure_error() id={:?} state={:?} payload:{:?}",
            id,
            self.state,
            payload,
        );
        // TODO consolidate into abort_connection() + communicate shutdown of this
        // connection similarly everywhere in the code
        let response = v2::messages::SetupConnectionError {
            flags: 0, // TODO handle flags
            code: "Cannot negotiate upstream V1 version mask"
                .try_into()
                .expect("BUG: incorrect error message"),
        };
        util::submit_message(&mut self.v2_tx, response)
    }

    fn handle_extranonce_subscribe_result(
        &mut self,
        _id: &v1::MessageId,
        payload: &v1::rpc::StratumResult,
    ) -> Result<()> {
        v1::messages::BooleanResult::try_from(payload)
            // Convert potential stratum error to proxy error first
            .map_err(Into::into)
            // Handle the actual submission result
            .map(|bool_result| {
                if bool_result.0 {
                    info!("Support for #xnsub enabled");
                    self.v1_xnsub_enabled = true;
                } else {
                    error!("Pool refused to enable #xnsub");
                    self.v1_xnsub_enabled = false;
                }
                ()
            })
    }

    fn handle_extranonce_subscribe_error(
        &mut self,
        _id: &v1::MessageId,
        payload: &v1::rpc::StratumError,
    ) -> Result<()> {
        self.v1_xnsub_enabled = false;
        error!("Error when trying to enable #xnsub: {}", payload.1);
        Ok(())
    }

    fn handle_subscribe_result(
        &mut self,
        id: &v1::MessageId,
        payload: &v1::rpc::StratumResult,
    ) -> Result<()> {
        trace!(
            "handle_subscribe_result() id={:?} state={:?} payload:{:?}",
            id,
            self.state,
            payload,
        );
        let subscribe_result = v1::messages::SubscribeResult::try_from(payload).map_err(|e| {
            // Aborting channel failed, we can only log about it
            self.abort_open_channel("Upstream subscribe failed");
            e
        })?;

        self.v1_extra_nonce1 = Some(subscribe_result.extra_nonce_1().clone());
        self.v1_extra_nonce2_size = subscribe_result.extra_nonce_2_size().clone();

        // In order to finalize the opening procedure we need 3 items: authorization,
        // subscription and difficulty
        if self.v1_authorized && self.v2_target.is_some() {
            self.finalize_open_channel().map_err(|e| {
                self.abort_open_channel("Upstream subscribe failed");
                e
            })?
        }
        Ok(())
    }

    /// An authorize result should be true, any other problem results in aborting the channel
    fn handle_authorize_result(
        &mut self,
        id: &v1::MessageId,
        payload: &v1::rpc::StratumResult,
    ) -> Result<()> {
        trace!(
            "handle_authorize_result() id={:?} state={:?} payload:{:?}",
            id,
            self.state,
            payload,
        );
        // Authorize is expected as a plain boolean answer
        v1::messages::BooleanResult::try_from(payload)
            // Convert ii-stratum error to proxy error
            .map_err(Into::into)
            .and_then(|bool_result| {
                trace!("Authorize result: {:?}", bool_result);
                self.v1_authorized = bool_result.0;
                if self.v1_authorized {
                    // Subscribe result already received (since extra nonce 1 is present), let's
                    // finalize the open channel
                    if self.v1_extra_nonce1.is_some() && self.v2_target.is_some() {
                        self.finalize_open_channel()
                    }
                    // Channel opening will be finalized by subscribe result
                    else {
                        Ok(())
                    }
                } else {
                    Err(ii_stratum::error::Error::from(v1::error::Error::Subscribe(
                        "Authorize result is false".to_string(),
                    ))
                    .into())
                }
            })
            // any problem in parsing the response results in authorization failure
            .map_err(|e| {
                self.abort_open_channel("Not authorized");
                e
            })
    }

    fn handle_authorize_or_subscribe_error(
        &mut self,
        id: &v1::MessageId,
        payload: &v1::rpc::StratumError,
    ) -> Result<()> {
        trace!(
            "handle_authorize_or_subscribe_error() id={:?} state={:?} payload:{:?}",
            id,
            self.state,
            payload,
        );
        // Only the first of authorize or subscribe error issues the OpenMiningChannelError message
        if self.state != V2ToV1TranslationState::V1SubscribeOrAuthorizeFail {
            trace!(
                "Upstream connection init failed, dropping channel: {:?}",
                payload
            );
            self.abort_open_channel("Service not ready");
            Err(Error::from(ii_stratum::error::Error::from(
                v1::error::Error::Subscribe(format!("{:?}", payload)),
            )))
        } else {
            trace!("Ok, received the second of subscribe/authorize failures, channel is already closed: {:?}", payload);
            Ok(())
        }
    }

    fn handle_submit_result(
        &mut self,
        id: &v1::MessageId,
        payload: &v1::rpc::StratumResult,
    ) -> Result<()> {
        trace!(
            "handle_submit_result() id={:?} state={:?} payload:{:?}",
            id,
            self.state,
            payload,
        );
        // mining.submit response is expected as a plain boolean answer
        v1::messages::BooleanResult::try_from(payload)
            // Convert potential stratum error to proxy error first
            .map_err(Into::into)
            // Handle the actual submission result
            .and_then(|bool_result| {
                let v2_channel_details = self
                    .v2_channel_details
                    .as_ref()
                    .expect("BUG: V2 channel details missing");
                trace!(
                    "Submit result: {:?}, V2 channel: {:?}",
                    bool_result,
                    v2_channel_details
                );

                if bool_result.0 {
                    self.log_session_details("Share accepted");
                    if let Some(metrics) = self.metrics.as_ref() {
                        metrics.account_accepted_share(self.v2_target);
                    }
                    // TODO what if v2_target > 2**64 - 1?
                    self.accept_shares(
                        id,
                        self.v2_target.expect("BUG: difficulty missing").low_u64(),
                    )
                } else {
                    info!("Share rejected for {}", v2_channel_details.user.to_string());
                    self.reject_shares(
                        Self::CHANNEL_ID,
                        SeqNum::V1(*id),
                        format!("ShareRjct:{:?}", payload),
                    )
                }
            })
            // TODO what should be the behavior when the result is incorrectly passed, shall we
            // report it as a SubmitSharesError?
            .map_err(Into::into)
    }

    fn handle_submit_error(
        &mut self,
        id: &v1::MessageId,
        payload: &v1::rpc::StratumError,
    ) -> Result<()> {
        trace!(
            "handle_submit_error() id={:?} state={:?} payload:{:?}",
            id,
            self.state,
            payload,
        );
        self.reject_shares(
            Self::CHANNEL_ID,
            SeqNum::V1(*id),
            format!("ShareRjct:{:?}", payload),
        )
    }

    /// Iterates the merkle branches and calculates block merkle root using the extra nonce 1.
    /// Extra nonce 2 encodes the channel ID.
    /// TODO review, whether a Result has to be returned as missing enonce1 would be considered a bug
    fn calculate_merkle_root(
        &mut self,
        payload: &v1::messages::Notify,
    ) -> crate::error::Result<sha256d::Hash> {
        // TODO get rid of extra nonce 1 cloning
        if let Some(v1_extra_nonce1) = self.v1_extra_nonce1.clone() {
            // Build coin base transaction,
            let mut coin_base: BytesMut = BytesMut::with_capacity(
                payload.coin_base_1().len()
                    + (v1_extra_nonce1.0).len()
                    + self.v1_extra_nonce2_size
                    + payload.coin_base_2().len(),
            );
            coin_base.extend_from_slice(payload.coin_base_1());
            coin_base.extend_from_slice(v1_extra_nonce1.0.as_ref());
            coin_base.extend_from_slice(
                Self::channel_to_extra_nonce2_bytes(Self::CHANNEL_ID, self.v1_extra_nonce2_size)
                    .as_ref(),
            );
            coin_base.extend_from_slice(payload.coin_base_2());

            let mut engine = sha256d::Hash::engine();
            engine.input(&coin_base);

            let cb_tx_hash = sha256d::Hash::from_engine(engine);
            trace!("Coinbase TX hash: {:x?} {:x?}", cb_tx_hash, coin_base);

            let merkle_root =
                payload
                    .merkle_branch()
                    .iter()
                    .fold(cb_tx_hash, |curr_merkle_root, tx_hash| {
                        let mut engine = sha256d::Hash::engine();
                        engine.input(&curr_merkle_root.into_inner());
                        engine.input(tx_hash.as_ref().as_slice());
                        sha256d::Hash::from_engine(engine)
                    });
            trace!("Merkle root calculated: {:x?}", merkle_root);
            Ok(merkle_root)
        } else {
            Err(Error::General("Extra nonce 1 missing, cannot calculate merkle root".into()).into())
        }
    }

    /// Builds SetNewPrevHash for the specified v1 Notify `payload`
    ///
    /// The SetNewPrevHash has to reference the future job that the V2 downstream has
    /// previously received from us.
    ///
    /// TODO: how to and if at all shall we determine the ntime offset? The proxy, unless
    /// it has access to the bitcoin network, cannot determine precisly determine the median
    /// ntime. Using sys_time may not be suitable:
    /// max_ntime_offset = min(min_ntime + 7200, sys_time + 7200)- min_ntime =
    ///                  = 7200 - min(0, sys_time - min_ntime)
    ///
    /// To be safe, we won't go for 7200, and suggest something like 1/4 of the calculated
    /// interval
    fn build_set_new_prev_hash(
        &self,
        job_id: u32,
        payload: &v1::messages::Notify,
    ) -> crate::error::Result<v2::messages::SetNewPrevHash> {
        // TODO review how this can be prevented from failing. If this fails, it should result in
        // panic as it marks a software bug
        let prev_hash = sha256d::Hash::from_slice(payload.prev_hash())?;
        let prev_hash = Uint256Bytes(prev_hash.into_inner());

        Ok(v2::messages::SetNewPrevHash {
            channel_id: Self::CHANNEL_ID,
            prev_hash,
            min_ntime: payload.time(),
            nbits: payload.bits(),
            job_id,
        })
    }

    /// Converts specified `channel_id` into extra nonce 2 with a specified
    /// `v1_extra_nonce2_size`
    /// TODO review the implementation 'how to efficiently render a u32 into a byte array'
    #[inline]
    fn channel_to_extra_nonce2_bytes(channel_id: u32, v1_extra_nonce2_size: usize) -> BytesMut {
        let mut extra_nonce2: BytesMut = BytesMut::with_capacity(v1_extra_nonce2_size);

        let channel_id_bytes = u32::to_le_bytes(channel_id);
        if v1_extra_nonce2_size < size_of::<u32>() {
            // TODO: what to do when server deliberately sends small extranonce?
            if channel_id >= 1u32.wrapping_shl(8 * v1_extra_nonce2_size as u32) {
                error!("BUG: channel_id doesn't fit into extranonce");
            }
            // Write just part of channel_id
            extra_nonce2.extend_from_slice(&channel_id_bytes[0..v1_extra_nonce2_size]);
        } else {
            // Write full 32-bits of channel id and pad the rest
            extra_nonce2.extend_from_slice(&channel_id_bytes);
            let padding = v1_extra_nonce2_size - size_of::<u32>();
            extra_nonce2.extend_from_slice(&vec![0; padding]);
        }
        extra_nonce2
    }

    /// Scan the submit share queue if the front item is a `SubmitSharesError`, send out all
    /// shares error messages
    fn submit_queued_share_responses(&mut self) -> Result<()> {
        loop {
            match self.v2_submit_share_queue.front() {
                Some(SubmitShare::SubmitSharesError(_)) => {}
                _ => return Ok(()),
            }
            match self.v2_submit_share_queue.pop_front() {
                Some(SubmitShare::SubmitSharesError(submit_shares_error_msg)) => {
                    util::submit_message(&mut self.v2_tx, submit_shares_error_msg).map_err(
                        |e| {
                            info!("Cannot send 'SubmitSharesError': {:?}", e);
                            e
                        },
                    )?;
                }
                _ => panic!("BUG: unexpected submit share item"),
            }
        }
    }

    fn submit_share_response<T, E>(&mut self, msg: T, err_msg: &str) -> Result<()>
    where
        E: fmt::Debug,
        T: TryInto<v2::Frame, Error = E>,
    {
        util::submit_message(&mut self.v2_tx, msg).map_err(|e| {
            info!("Cannot send '{}': {:?}", err_msg, e);
            e
        })?;
        self.submit_queued_share_responses()
    }

    /// Helper that converts id of V1 mining.submit message to V2 sequence
    /// number of a submit shares message
    fn get_v2_submit_shares_seq_num(&mut self, id: &v1::MessageId) -> Result<u32> {
        let id = id.ok_or(Error::General(
            "Missing V1 message id for 'mining.submit' response".to_string(),
        ))?;
        match self.v2_submit_share_queue.pop_front() {
            Some(SubmitShare::V1ToV2Mapping(v1_seq_num, v2_seq_num)) if id == v1_seq_num => {
                Ok(v2_seq_num)
            }
            _ => Err(Error::General(format!(
                "Unexpected V1 message id ({}) in 'mining.submit' response",
                id
            ))),
        }
    }

    fn accept_shares(&mut self, id: &v1::MessageId, new_shares: u64) -> Result<()> {
        let success_msg = v2::messages::SubmitSharesSuccess {
            channel_id: Self::CHANNEL_ID,
            last_seq_num: self.get_v2_submit_shares_seq_num(id)?,
            new_submits_accepted_count: 1,
            new_shares_sum: new_shares as u32,
        };

        self.submit_share_response(success_msg, "SubmitSharesSuccess")
    }

    /// Generates log trace entry and reject shares error reply to the client
    ///
    /// `seq_num_variant` distinguishes share responses generated immediately in proxy (sequence
    /// number V2 is known) or responses received from remote server (sequence number V1
    /// must be remapped to V2)
    fn reject_shares(
        &mut self,
        channel_id: u32,
        seq_num_variant: SeqNum,
        err_msg: String,
    ) -> Result<()> {
        trace!("{}", err_msg);
        let (seq_num, submit) = match seq_num_variant {
            SeqNum::V1(id) => (self.get_v2_submit_shares_seq_num(&id)?, true),
            SeqNum::V2(value) => (value, self.v2_submit_share_queue.is_empty()),
        };
        if let Some(metrics) = self.metrics.as_ref() {
            metrics.account_rejected_share(self.v2_target);
        }
        let submit_shares_error_msg = v2::messages::SubmitSharesError {
            channel_id,
            seq_num,
            code: err_msg[..32].try_into().expect(
                format!(
                    "BUG: cannot convert error message to V2 format: {}",
                    err_msg
                )
                .as_str(),
            ),
        };

        if submit {
            self.submit_share_response(submit_shares_error_msg, "SubmitSharesError")
        } else {
            self.v2_submit_share_queue
                .push_back(SubmitShare::SubmitSharesError(submit_shares_error_msg));
            Ok(())
        }
    }

    fn perform_notify(&mut self, payload: &v1::messages::Notify) -> Result<()> {
        let merkle_root = self.calculate_merkle_root(payload)?;

        let v2_job = v2::messages::NewMiningJob {
            channel_id: Self::CHANNEL_ID,
            job_id: self.v2_job_id.next(),
            future_job: self.v2_to_v1_job_map.is_empty()
                || payload.clean_jobs()
                || self.v1_force_future_jobs,
            merkle_root: Uint256Bytes(merkle_root.into_inner()),
            version: payload.version(),
        };

        // Make sure we generate new prev hash. Empty JobMap means this is the first mining.notify
        // message and we also have to issue NewPrevHash. In addition to that, we also check the
        // clean jobs flag that indicates a must for new prev hash, too.
        let maybe_set_new_prev_hash = if v2_job.future_job {
            // Clean the job map only if V1 indicates new prev hash.
            if payload.clean_jobs() {
                self.v2_to_v1_job_map.clear();
            }
            // Any error means immediate termination
            // TODO write a unit test for such scenario, too
            Some(self.build_set_new_prev_hash(v2_job.job_id, payload)?)
        } else {
            None
        };
        trace!(
            "Registering V2 job ID {:x?} -> V1 job ID {:x?}",
            v2_job.job_id,
            payload.job_id(),
        );
        // TODO extract this duplicate code, turn the map into a new type with this
        // custom policy (attempt to insert with the same key is a bug)
        if self
            .v2_to_v1_job_map
            .insert(
                v2_job.job_id,
                V1SubmitTemplate {
                    job_id: v1::messages::JobId::from_str(payload.job_id()),
                    time: payload.time(),
                    version: payload.version(),
                },
            )
            .is_some()
        {
            error!("BUG: V2 id {} already exists...", v2_job.job_id);
            // TODO add graceful handling of this bug (shutdown?)
            panic!("V2 id already exists");
        }

        util::submit_message(&mut self.v2_tx, v2_job)?;

        if let Some(set_new_prev_hash) = maybe_set_new_prev_hash {
            util::submit_message(&mut self.v2_tx, set_new_prev_hash)?
        }
        Ok(())
    }

    /// The result visitor takes care of detecting a spurious response without matching request
    /// and passes processing further
    /// TODO write a solid unit test covering all 3 scenarios that can go wrong
    async fn visit_stratum_result_or_error<'a>(
        &mut self,
        id: &'a v1::MessageId,
        payload: V1ResultOrError<'a>,
    ) -> Result<()> {
        // Each response message should have an ID for pairing
        id.ok_or(Error::from(stratum::Error::from(v1::error::Error::Rpc(
            "Missing ID in ii_stratum result".to_string(),
        ))))
        // find the ID in the request map
        .and_then(|id| {
            self.v1_req_map
                .remove(&id)
                .ok_or(Error::from(stratum::Error::from(v1::error::Error::Rpc(
                    format!("Received invalid ID {}", id).into(),
                ))))
        })
        // run the result through the result handler
        .and_then(|handler| {
            let req_duration = handler.elapsed();
            match payload {
                V1ResultOrError::Result(r) => {
                    self.metrics.as_ref().map(|m| {
                        m.observe_v1_request_success(handler.request_method, req_duration)
                    });
                    (handler.v1_stratum_result_handler)(self, id, r)
                }
                V1ResultOrError::Error(e) => {
                    self.metrics
                        .as_ref()
                        .map(|m| m.observe_v1_request_error(handler.request_method, req_duration));
                    (handler.v1_stratum_error_handler)(self, id, e)
                }
            }
        })
    }

    /// Parse the stratum V1 reconnect message into new host/port pair, where host is
    /// converted into stratum v2 specific type. This method catches host name overflow attempts.
    fn parse_client_reconnect(msg: &v1::messages::ClientReconnect) -> Result<(Str0_255, u16)> {
        let new_host = match msg.host() {
            Some(host_val) => match host_val {
                Value::String(host_name) => Ok(host_name.clone()),
                _ => Err("host name not a string"),
            },
            None => Ok("".to_owned()),
        }
        .and_then(|host_name| {
            // TODO Str0_255 conversion returns () as an error, therefore we cannot mention the
            // error here. Once this changes, the error mapping can be redone
            Str0_255::try_from(host_name).map_err(|_e| "host name string too long")
        })
        .map_err(|e| {
            crate::error::Error::General(format!(
                "Cannot parse host ({}) in client.reconnect: {:?}",
                e, msg
            ))
        })?;

        let new_port = match msg.port() {
            Some(port_val) => match port_val {
                Value::Number(port) => match port.as_u64() {
                    Some(n) => u16::try_from(n).map_err(|_e| "invalid u16"),
                    None => Err("invalid u16"),
                },
                Value::String(port_str) => port_str
                    .parse::<u16>()
                    .map_err(|_e| "invalid number string"),
                _ => Err("port number neither string nor int"),
            },
            None => Ok(0),
        }
        .map_err(|e| {
            crate::error::Error::General(format!(
                "Cannot parse port ({}) client.reconnect: {:?}",
                e, msg
            ))
        })?;

        Ok((new_host, new_port))
    }

    fn log_session_details(&self, msg: &str) {
        let v2_channel_details = self
            .v2_channel_details
            .as_ref()
            .expect("BUG: V2 channel details missing");
        let v2_connection_details = self
            .v2_conn_details
            .as_ref()
            .expect("BUG: V2 channel details present but connection details missing?");
        debug!(
            "{} SESSION;{};{};{};{};{:x};{};{};{};{};{};{};",
            msg,
            v2_channel_details.user.to_string(),
            v2_connection_details.protocol,
            v2_connection_details.min_version,
            v2_connection_details.max_version,
            v2_connection_details.flags,
            v2_connection_details.endpoint_host.to_string(),
            v2_connection_details.endpoint_port,
            v2_connection_details.device.vendor.to_string(),
            v2_connection_details.device.hw_rev.to_string(),
            v2_connection_details.device.fw_ver.to_string(),
            v2_connection_details.device.dev_id.to_string(),
        );
    }
}

#[handler(async try v1::rpc::Rpc suffix _v1)]
impl V2ToV1Translation {
    async fn handle_stratum_result(
        &mut self,
        payload: (MessageId, v1::rpc::StratumResult),
    ) -> Result<()> {
        let (id, msg) = payload;
        trace!(
            "visit_stratum_result() id={:?} state={:?} payload:{:?}",
            id,
            self.state,
            &msg,
        );
        self.visit_stratum_result_or_error(&id, V1ResultOrError::Result(&msg))
            .await
    }

    async fn handle_stratum_error(
        &mut self,
        payload: (MessageId, v1::rpc::StratumError),
    ) -> Result<()> {
        let (id, msg) = payload;
        trace!(
            "visit_stratum_error() id={:?} state={:?} payload:{:?}",
            id,
            self.state,
            msg,
        );
        self.visit_stratum_result_or_error(&id, V1ResultOrError::Error(&msg))
            .await
    }

    async fn handle_set_difficulty(
        &mut self,
        payload: (MessageId, v1::messages::SetDifficulty),
    ) -> Result<()> {
        let (id, msg) = payload;
        trace!(
            "visit_set_difficulty() id={:?} state={:?} payload:{:?}",
            id,
            self.state,
            msg,
        );
        let diff = msg.value() as u32;
        self.v2_target = Some(Self::diff_to_target(diff));
        if self.v1_authorized && self.v1_extra_nonce1.is_some() {
            // Initial set difficulty finalizes open channel if all preconditions are met
            if self.state == V2ToV1TranslationState::OpenStandardMiningChannelPending {
                self.finalize_open_channel()
                    .map_err(|e| trace!("visit_set_difficulty: {}", e))
                    // Consume the error as there is no way to return anything from the visitor for now.
                    .ok();
            }
            // Anything after that is standard difficulty adjustment
            else {
                trace!("Sending current target: {:x?}", self.v2_target);
                if let Err(e) = self.send_set_target() {
                    info!("Cannot send SetTarget: {}", e);
                }
            }
        }
        Ok(())
    }

    async fn handle_set_extranonce(
        &mut self,
        payload: (MessageId, v1::messages::SetExtranonce),
    ) -> Result<()> {
        let (id, msg) = payload;
        trace!(
            "visit_set_extranonce() id={:?} state={:?} payload:{:?}",
            id,
            self.state,
            msg,
        );

        // Update extranonces.
        // Changes are reflected after new mining job as per:
        //   https://en.bitcoin.it/wiki/Stratum_mining_protocol#mining.set_extranonce
        self.v1_extra_nonce1 = Some(msg.extra_nonce1);
        self.v1_extra_nonce2_size = msg.extra_nonce2_size;
        Ok(())
    }

    /// Composes a new mining job and sends it downstream
    /// TODO: Only 1 channel is supported
    async fn handle_notify(&mut self, payload: (MessageId, v1::messages::Notify)) -> Result<()> {
        let (id, msg) = payload;
        trace!(
            "visit_notify() id={:?} state={:?} payload:{:?}",
            id,
            self.state,
            msg,
        );

        // We won't process the job as long as the channel is not operational
        if self.state != V2ToV1TranslationState::Operational {
            self.v1_deferred_notify = Some(msg.clone());
            debug!("Channel not yet operational, caching latest mining.notify from upstream");
            return Ok(());
        }
        self.perform_notify(&msg).map_err(|e| {
            Error::General(format!(
                "visit_notify: Sending new mining job failed error={:?} id={:?} state={:?} \
                 payload:{:?}",
                e, id, self.state, msg,
            ))
        })?;
        Ok(())
    }

    /// TODO currently unimplemented, the proxy should refuse changing the version mask from the server
    /// Since this is a notification only, the only action that the translation can do is log +
    /// report an error
    async fn handle_set_version_mask(
        &mut self,
        payload: (MessageId, v1::messages::SetVersionMask),
    ) -> Result<()> {
        let (id, msg) = payload;
        trace!(
            "visit_set_version_mask() id={:?} state={:?} payload:{:?}",
            id,
            self.state,
            msg,
        );
        Ok(())
    }

    async fn handle_client_reconnect(
        &mut self,
        payload: (MessageId, v1::messages::ClientReconnect),
    ) -> Result<()> {
        let (id, msg) = payload;
        trace!(
            "visit_client_reconnect() id={:?} state={:?} payload:{:?}",
            id,
            self.state,
            msg,
        );
        // Propagate the reconnect only if configured so
        if self.options.propagate_reconnect_downstream {
            let (new_host, new_port) = Self::parse_client_reconnect(&msg)
                .map_err(|e| Error::General(format!("visit_client_reconnect failed: {}", e)))?;

            let reconnect_msg = v2::messages::Reconnect { new_host, new_port };

            util::submit_message(&mut self.v2_tx, reconnect_msg)
                .map_err(|e| Error::General(format!("Cannot send 'Reconnect': {:?}", e)))?;
        }
        Ok(())
    }

    async fn handle_ping(&mut self, payload: (MessageId, v1::messages::Ping)) -> Result<()> {
        let msg = v1::messages::Pong("pong".into());
        debug!("Received {:?} message, sending {:?} response", payload, msg);
        let stratum_result = v1::rpc::ResponsePayload::try_from(msg)
            .expect("BUG: Pong response to ping couldn't be serialized")
            .ok();
        let rpc_pld = v1::rpc::Response {
            id: payload.0.unwrap_or_default(),
            stratum_result,
            stratum_error: None,
        };
        util::submit_message(&mut self.v1_tx, v1::rpc::Rpc::Response(rpc_pld)).map_err(|e| {
            Error::General(format!("Cannot send ping response to mining.ping: {:?}", e))
        })
    }

    #[handle(_)]
    async fn handle_unknown_v1(&mut self, parsed_frame: Result<v1::rpc::Rpc>) -> Result<()> {
        // Broken v1 frame is handled only with warning, since legacy Stratum protocol
        // has many dialects and failure to parse message should usually not result in closing
        // the connection
        match parsed_frame {
            Ok(rpc_msg) => {
                warn!("Unknown stratum v1 message received: {:?}", rpc_msg);
            }
            Err(e) => {
                warn!("Broken stratum v1 Rpc frame received: {:?}", e);
            }
        }
        Ok(())
    }
}

#[handler(async try v2::framing::Frame suffix _v2)]
impl V2ToV1Translation {
    async fn handle_setup_connection(&mut self, msg: v2::messages::SetupConnection) -> Result<()> {
        trace!("handle_setup_connection(): {:?}", msg);

        if self.state != V2ToV1TranslationState::Init {
            trace!("Cannot setup connection again, received: {:?}", msg);

            let err_msg = v2::messages::SetupConnectionError {
                code: "Connection can be setup only once"
                    .try_into()
                    .expect("BUG: incorrect error message"),
                flags: msg.flags, // TODO Flags indicating features causing an error
            };

            if let Err(submit_err) = util::submit_message(&mut self.v2_tx, err_msg)
                .map_err(V2ProtocolError::setup_connection)
            {
                info!("Cannot submit SetupConnectionError: {:?}", submit_err);
                Err(submit_err)?;
            }
        }

        self.v2_conn_details = Some(msg.clone());
        let mut configure = v1::messages::Configure::new();
        configure
            .add_feature(v1::messages::VersionRolling::new(
                ii_stratum::BIP320_N_VERSION_MASK,
                ii_stratum::BIP320_N_VERSION_MAX_BITS,
            ))
            .expect("BUG: addfeature failed"); // FIXME: how to handle errors from configure.add_feature() ?

        let v1_configure_message = self.v1_method_into_message(
            configure,
            Self::handle_configure_result,
            Self::handle_configure_error,
        );
        if let Err(submit_err) = util::submit_message(&mut self.v1_tx, v1_configure_message)
            .map_err(V2ProtocolError::setup_connection)
        {
            debug!("Cannot submit mining.configure: {:?}", submit_err);
            Err(submit_err)?;
        }
        self.state = V2ToV1TranslationState::V1Configure;
        Ok(())
    }

    /// Opening a channel is a 2 stage process when translating to  V1 ii_stratum, where
    /// both stages can be executed in arbitrary order:
    /// - perform subscribe (and start queuing incoming V1 jobs)
    /// - perform authorize
    ///
    /// Upon successful authorization:
    /// - communicate OpenStandardMiningChannelSuccess
    /// - start sending Jobs downstream to V2 client
    async fn handle_open_standard_mining_channel(
        &mut self,
        msg: v2::messages::OpenStandardMiningChannel,
    ) -> Result<()> {
        trace!(
            "handle_open_standard_mining_channel() state={:?} payload:{:?}",
            self.state,
            msg,
        );
        if self.state != V2ToV1TranslationState::ConnectionSetup
            && self.state != V2ToV1TranslationState::V1SubscribeOrAuthorizeFail
        {
            trace!(
                "Out of sequence OpenStandardMiningChannel message, received: {:?}",
                msg
            );
            let err_msg = v2::messages::OpenMiningChannelError {
                req_id: msg.req_id,
                code: "out-of-sequence-message"
                    .try_into()
                    .expect("BUG: incorrect error message"),
            };

            if let Err(submit_err) = util::submit_message(&mut self.v2_tx, err_msg)
                .map_err(V2ProtocolError::open_mining_channel)
            {
                info!(
                    "Cannot send OpenMiningChannelError message: {:?}",
                    submit_err
                );
                Err(submit_err)?;
            }
        }
        // Connection details are present by now
        if let Some(conn_details) = self.v2_conn_details.as_ref() {
            self.v2_channel_details = Some(msg.clone());
            self.state = V2ToV1TranslationState::OpenStandardMiningChannelPending;

            let hostname: String = conn_details
                .endpoint_host
                .clone()
                .try_into()
                .expect("BUG: Cannot convert to string from connection details");

            let hostname_port = format!("{}:{}", hostname, conn_details.endpoint_port);
            let subscribe = v1::messages::Subscribe {
                agent_signature: Some(conn_details.device.fw_ver.to_string()),
                extra_nonce1: None,
                url: Some(hostname_port),
                port: None,
            };

            let v1_subscribe_message = self.v1_method_into_message(
                subscribe,
                Self::handle_subscribe_result,
                Self::handle_authorize_or_subscribe_error,
            );

            if let Err(submit_err) = util::submit_message(&mut self.v1_tx, v1_subscribe_message)
                .map_err(V2ProtocolError::open_mining_channel)
            {
                info!("Cannot send V1 mining.subscribe: {:?}", submit_err);
                Err(submit_err)?;
            }

            if self.options.try_enable_xnsub {
                let extranonce_subscribe = v1::messages::ExtranonceSubscribe;
                let v1_extranonce_subscribe = self.v1_method_into_message(
                    extranonce_subscribe,
                    Self::handle_extranonce_subscribe_result,
                    Self::handle_extranonce_subscribe_error,
                );
                if let Err(submit_err) =
                    util::submit_message(&mut self.v1_tx, v1_extranonce_subscribe)
                        .map_err(V2ProtocolError::open_mining_channel)
                {
                    info!(
                        "Cannot send V1 mining.extranonce_subscribe: {:?}",
                        submit_err
                    );
                    Err(submit_err)?;
                }
            }

            let authorize = v1::messages::Authorize {
                name: msg.user.to_string(),
                password: self.v1_password.clone(),
            };
            let v1_authorize_message = self.v1_method_into_message(
                authorize,
                Self::handle_authorize_result,
                Self::handle_authorize_or_subscribe_error,
            );
            if let Err(submit_err) = util::submit_message(&mut self.v1_tx, v1_authorize_message)
                .map_err(V2ProtocolError::open_mining_channel)
            {
                info!("Cannot send V1 mining.authorized: {:?}", submit_err);
                Err(submit_err)?;
            }
        }
        Ok(())
    }

    /// The flow of share processing is as follows:
    ///
    /// - find corresponding job
    /// - verify the share that it meets the target (NOTE: currently not implemented)
    /// - emit V1 Submit message
    ///
    /// If any of the above points fail, reply with SubmitShareError + reasoning
    async fn handle_submit_shares_standard(
        &mut self,
        msg: v2::messages::SubmitSharesStandard,
    ) -> Result<()> {
        trace!(
            "handle_submit_shares_standard() state={:?} payload:{:?}",
            self.state,
            msg,
        );
        // Report invalid channel ID
        if msg.channel_id != Self::CHANNEL_ID {
            let _ = self.reject_shares(
                msg.channel_id,
                SeqNum::V2(msg.seq_num),
                format!("Unrecognized channel ID {}", msg.channel_id),
            );
            return Err(Error::Stratum(ii_stratum::error::Error::General(format!(
                "Unrecognized channel ID {}",
                msg.channel_id
            ))));
        }

        // Channel details must be filled by now, anything else is a bug, unfortunately, due to
        // the 'expect' we have to clone them. TODO review this code
        let v2_channel_details = &self
            .v2_channel_details
            .clone()
            .expect("BUG: Missing channel details");
        // TODO this is only here as we want to prevent locking up 'self' into multiple closures
        // and causing borrow checker complains
        let v1_extra_nonce2_size = self.v1_extra_nonce2_size;

        // Check job ID validity
        let v1_submit_template = self
            .v2_to_v1_job_map
            .get(&msg.job_id)
            // convert missing job ID (None) into an error
            .ok_or(crate::error::Error::General(format!(
                "V2 Job ID not present {} in registry",
                msg.job_id
            )))
            .map(|tmpl| tmpl.clone());
        // TODO validate the job (recalculate the hash and compare the target)
        // Submit upstream V1 job based on the found job ID in the map
        match v1_submit_template {
            Ok(v1_submit_template) => {
                let submit = v1::messages::Submit::new(
                    v2_channel_details.user.to_string(),
                    v1_submit_template.job_id.clone(),
                    Self::channel_to_extra_nonce2_bytes(Self::CHANNEL_ID, v1_extra_nonce2_size)
                        .as_ref(),
                    msg.ntime,
                    msg.nonce,
                    // ensure the version bits in the template follow BIP320
                    msg.version & ii_stratum::BIP320_N_VERSION_MASK,
                );
                // Convert the method into a message + provide handling methods
                let v1_submit_message = self.v1_method_into_message(
                    submit,
                    Self::handle_submit_result,
                    Self::handle_submit_error,
                );

                let v1_seq_num = if let v1::rpc::Rpc::Request(r) = &v1_submit_message {
                    r.id.expect("BUG: missing v1 request ID")
                } else {
                    panic!("BUG: expected v1 share request");
                };

                if let Err(submit_err) = util::submit_message(&mut self.v1_tx, v1_submit_message) {
                    info!(
                        "SubmitSharesStandard: cannot send translated V1 message: {:?}",
                        submit_err
                    );
                    let _ = self.reject_shares(
                        msg.channel_id,
                        SeqNum::V2(msg.seq_num),
                        format!("{}", submit_err),
                    );
                } else {
                    self.v2_submit_share_queue
                        .push_back(SubmitShare::V1ToV2Mapping(v1_seq_num, msg.seq_num));
                }
            }
            Err(e) => {
                let _ =
                    self.reject_shares(msg.channel_id, SeqNum::V2(msg.seq_num), format!("{}", e));
            }
        }
        Ok(())
    }

    #[handle(_)]
    async fn handle_unknown_v2(&mut self, parsed_frame: Result<v2::framing::Frame>) -> Result<()> {
        // Broken v2 frame should never occur, since stratum v2 is well defined
        // and processing broken frame should result in closing the connection
        match parsed_frame {
            Ok(v2_frame) => {
                warn!("Unknown stratum v2 message received: {:?}", v2_frame);
                Ok(())
            }
            Err(e) => Err(V2ProtocolError::Other(format!(
                "Broken stratum v2 frame received: {:?}",
                e
            )))?,
        }
    }
}
