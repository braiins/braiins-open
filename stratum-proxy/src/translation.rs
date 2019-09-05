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

use bitcoin_hashes::{sha256d, Hash, HashEngine};
use bytes::BytesMut;
use failure::ResultExt;
use futures::channel::mpsc;
use serde_json;
use std::collections::HashMap;
use std::convert::From;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::fmt;
use std::mem::size_of;

use ii_stratum::v1;
use ii_stratum::v2;
use ii_stratum::v2::types::Uint256Bytes;

use ii_logging::macros::*;
use ii_wire::{Message, MessageId, TxFrame};

#[cfg(test)]
mod test;

/// TODO consider whether the v1/v2 TX channels should use a 'Message'. Currently the reason
/// for not doing that is that we want to prevent dynamic dispatch when serializing a particular
/// message
pub struct V2ToV1Translation {
    /// Statemachine tracking the translation setup
    state: V2ToV1TranslationState,

    /// Channel for sending out V1 responses
    v1_tx: mpsc::Sender<TxFrame>,
    /// Unique request ID generator
    v1_req_id: MessageId,
    /// Mapping for pairing of incoming V1 message with original requests
    v1_req_map: V1ReqMap,

    v1_extra_nonce1: Option<v1::ExtraNonce1>,
    v1_extra_nonce2_size: usize,
    v1_authorized: bool,

    /// Latest mining.notify payload that arrived before V1 authorize has completed.
    /// This allows immediate completion of channel open on V2.
    v1_deferred_notify: Option<v1::messages::Notify>,

    /// Channel for sending out V2 responses
    v2_tx: mpsc::Sender<TxFrame>,
    v2_req_id: MessageId,
    /// All connection details
    v2_conn_details: Option<v2::messages::SetupMiningConnection>,
    /// Additional information about the pending channel being open
    v2_channel_details: Option<v2::messages::OpenChannel>,
    /// Target difficulty derived from mining.set_difficulty message
    /// The channel opening is not complete until the target is determined
    v2_target: Option<uint::U256>,
    /// Unique job ID generator
    v2_job_id: MessageId,
    /// Translates V2 job ID to V1 job ID
    v2_to_v1_job_map: JobMap,

    /// TODO: Temporary local blockheight. We will extract the value from coinbase part 1.
    block_height: MessageId,
}

/// States of the Translation setup
#[derive(PartialEq, Debug)]
enum V2ToV1TranslationState {
    /// No message received yet
    Init,
    /// Stratum V1 mining.configure is in progress
    V1Configure,
    /// Connection successfully setup, waiting for OpenChannel message
    ConnectionSetup,
    /// Channel now needs finalization of subscribe+authorize+set difficulty target with the
    /// upstream V1 server
    OpenChannelPending,
    /// Upstream subscribe/authorize failed state ensures sending OpenChannelError only once
    V1SubscribeOrAuthorizeFail,
    /// Channel is operational
    Operational,
}

/// Represents a handler method that can process a particular ii_stratum result.
type V1StratumResultHandler = fn(
    &mut V2ToV1Translation,
    &ii_wire::Message<v1::Protocol>,
    &v1::framing::StratumResult,
) -> ii_stratum::error::Result<()>;

/// Represents a handler method that can process a particular ii_stratum error.
type V1StratumErrorHandler = fn(
    &mut V2ToV1Translation,
    &ii_wire::Message<v1::Protocol>,
    &v1::framing::StratumError,
) -> ii_stratum::error::Result<()>;

/// Custom mapping of V1 request id onto result/error handlers
type V1ReqMap = HashMap<u32, (V1StratumResultHandler, V1StratumErrorHandler)>;

/// Helper template stored in V2->V1 job map
#[derive(Clone, PartialEq, Debug)]
struct V1SubmitTemplate {
    job_id: v1::messages::JobId,
    time: u32,
    version: u32,
}

/// Maps V2 job ID to V1 job ID so that we can submit mining results upstream to V1 server
type JobMap = HashMap<u32, V1SubmitTemplate>;

//type V2ReqMap = HashMap<u32, FnMut(&mut V2ToV1Translation, &ii_wire::Message<Protocol>, &v1::framing::StratumResult)>;

impl V2ToV1Translation {
    const PROTOCOL_VERSION: usize = 0;
    /// No support for the extended protocol yet, therefore, no extranonce advertised
    const MAX_EXTRANONCE_SIZE: usize = 0;
    /// Currently, no support for multiple channels in the proxy
    const CHANNEL_ID: u32 = 0;

    /// U256 in little endian
    /// TODO: consolidate into common part/generalize
    /// TODO: DIFF1 const target is broken, the last U64 word gets actually initialized to 0xffffffff, not sure why
    const DIFF1_TARGET: uint::U256 = uint::U256([0, 0, 0, 0xffff0000u64]);

    pub fn new(v1_tx: mpsc::Sender<TxFrame>, v2_tx: mpsc::Sender<TxFrame>) -> Self {
        // TODO: unused?
        let diff_1_target = uint::U256::from_big_endian(&[
            0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ]);

        Self {
            v2_conn_details: None,
            v2_channel_details: None,
            v2_target: None,
            state: V2ToV1TranslationState::Init,
            v1_tx,
            v1_req_id: MessageId::new(),
            v1_req_map: V1ReqMap::default(),
            v1_extra_nonce1: None,
            v1_extra_nonce2_size: 0,
            v1_authorized: false,
            v1_deferred_notify: None,
            v2_tx,
            v2_req_id: MessageId::new(),
            v2_job_id: MessageId::new(),
            v2_to_v1_job_map: JobMap::default(),
            block_height: MessageId::new(),
        }
    }

    /// Converts the response message into a TxFrame and submits it into the specified queue
    /// TODO: handle serialization errors (logger + terminate?)
    fn submit_message<T, E>(tx: &mut mpsc::Sender<TxFrame>, msg: T)
    where
        E: fmt::Debug,
        T: TryInto<TxFrame, Error = E>,
    {
        let msg = msg.try_into().expect("Could not serialize message");
        tx.try_send(msg).expect("Cannot send message")
    }

    /// Builds a V1 request from V1 method and assigns a unique identifier to it
    fn v1_method_into_message<M, E>(
        &mut self,
        method: M,
        result_handler: V1StratumResultHandler,
        error_handler: V1StratumErrorHandler,
    ) -> v1::framing::Frame
    where
        E: fmt::Debug,
        M: TryInto<v1::framing::RequestPayload, Error = E>,
    {
        let payload = method
            .try_into()
            .expect("Cannot convert V1 method into a message");

        // TODO: decorate the request with a new unique ID -> this is the request ID
        let id = self.v1_req_id.next();
        trace!("Registering v1, request ID: {} method: {:?}", id, payload);
        if self
            .v1_req_map
            .insert(id, (result_handler, error_handler))
            .is_some()
        {
            error!("BUG: V1 id {} already exists...", id);
            // TODO add graceful handling of this bug (shutdown?)
            panic!("V1 id already exists");
        }

        v1::framing::Request {
            id: Some(id),
            payload,
        }
        .into()
    }

    /// Sets the current pending channel to operational state and submits success message
    fn finalize_open_channel(&mut self) -> ii_stratum::error::Result<()> {
        trace!("finalize_open_channel()");
        let mut init_target: Uint256Bytes = Uint256Bytes([0; 32]);
        self.v2_target
            .expect("Bug: initial target still not defined when attempting to finalize OpenChannel")
            .to_little_endian(init_target.as_mut());

        // when V1 authorization has already taken place, report channel opening success
        // TODO: this is a workaround, eliminate the clone()
        self.v2_channel_details
            .clone()
            .and_then(|v2_channel_details| {
                self.state = V2ToV1TranslationState::Operational;
                let msg = v2::messages::OpenChannelSuccess {
                    req_id: v2_channel_details.req_id,
                    channel_id: Self::CHANNEL_ID,
                    dev_id: Default::default(),
                    init_target: init_target.clone(),
                    group_channel_id: 0,
                };
                Self::submit_message(&mut self.v2_tx, msg);
                if let Some(notify_payload) = self.v1_deferred_notify.take() {
                    self.perform_notify(&notify_payload);
                }
                Some(())
            })
            .ok_or(
                v2::error::ErrorKind::ChannelNotOperational("Channel details missing".to_string())
                    .into(),
            )
    }

    /// Send new target
    /// TODO extend the translation unit test accordingly
    fn send_set_target(&mut self) {
        trace!("send_set_target()");
        let max_target = Uint256Bytes::from(self.v2_target.expect(
            "Bug: initial target still not defined when attempting to finalize \
             OpenChannel",
        ));

        let msg = v2::messages::SetTarget {
            channel_id: Self::CHANNEL_ID,
            max_target,
        };

        Self::submit_message(&mut self.v2_tx, msg);
    }

    /// Reports failure to open the channel and changes the translation state
    /// From this point on a new OpenChannel message is expected as an attempt to reopen the channel
    fn abort_open_channel(&mut self, err_msg: &str) {
        trace!(
            "abort_open_channel() - channel details: {:?}, msg: {}",
            self.v2_channel_details,
            err_msg
        );
        self.state = V2ToV1TranslationState::V1SubscribeOrAuthorizeFail;
        // TODO eliminate the unnecessary clone
        self.v2_channel_details
            .clone()
            .and_then(|v2_channel_details| {
                let msg = v2::messages::OpenChannelError {
                    req_id: v2_channel_details.req_id,
                    code: err_msg.try_into().unwrap(), // FIXME: error handling
                };
                Self::submit_message(&mut self.v2_tx, msg);
                Some(())
            });
        // Cleanup all parts associated with opening the channel
        self.v1_authorized = false;
        self.v1_extra_nonce1 = None;
        self.v1_extra_nonce2_size = 0;
        self.v2_channel_details = None;
    }

    /// Finalizes a pending SetupMiningConnection upon successful negotiation of
    /// mining configuration of version rolling bits
    fn handle_configure_result(
        &mut self,
        msg: &ii_wire::Message<v1::Protocol>,
        payload: &v1::framing::StratumResult,
    ) -> ii_stratum::error::Result<()> {
        trace!(
            "handle_configure_result() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );

        // TODO review the use of serde_json here, it may be possible to eliminate this dependency
        // Extract version mask and verify it matches the maximum possible value
        let proposed_version_mask: ii_stratum::error::Result<v1::messages::VersionMask> =
            serde_json::from_value(payload.0["version-rolling.mask"].clone())
                .context("Failed to parse version-rolling mask")
                .map_err(Into::into);
        proposed_version_mask.map(|proposed_version_mask| {
            trace!(
                "Evaluating: version-rolling state == {:?} && mask=={:x?}",
                payload.0["version-rolling"].as_bool(),
                proposed_version_mask
            );
            if payload.0["version-rolling"].as_bool() == Some(true)
                && (proposed_version_mask.0).0 == ii_stratum::BIP320_N_VERSION_MASK
            {
                self.state = V2ToV1TranslationState::ConnectionSetup;

                let success = v2::messages::SetupMiningConnectionSuccess {
                    used_protocol_version: Self::PROTOCOL_VERSION as u16,
                    max_extranonce_size: Self::MAX_EXTRANONCE_SIZE as u16,
                    // TODO provide public key for TOFU
                    pub_key: vec![0xde, 0xad, 0xbe, 0xef].try_into().unwrap(),
                };
                Self::submit_message(&mut self.v2_tx, success);
            } else {
                // TODO consolidate into abort_connection() + communicate shutdown of this
                // connection similarly everywhere in the code
                let response = v2::messages::SetupMiningConnectionError {
                    code: "Cannot negotiate upstream V1 version mask"
                        .try_into()
                        .unwrap(),
                };
                Self::submit_message(&mut self.v2_tx, response);
            }
            ()
        })
    }

    fn handle_configure_error(
        &mut self,
        msg: &ii_wire::Message<v1::Protocol>,
        payload: &v1::framing::StratumError,
    ) -> ii_stratum::error::Result<()> {
        trace!(
            "handle_configure_error() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        // TODO consolidate into abort_connection() + communicate shutdown of this
        // connection similarly everywhere in the code
        let response = v2::messages::SetupMiningConnectionError {
            code: "Cannot negotiate upstream V1 version mask"
                .try_into()
                .unwrap(),
        };
        Self::submit_message(&mut self.v2_tx, response);
        Ok(())
    }

    fn handle_subscribe_result(
        &mut self,
        msg: &ii_wire::Message<v1::Protocol>,
        payload: &v1::framing::StratumResult,
    ) -> ii_stratum::error::Result<()> {
        trace!(
            "handle_subscribe_result() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        v1::messages::SubscribeResult::try_from(payload)
            .and_then(|subscribe_result| {
                self.v1_extra_nonce1 = Some(subscribe_result.extra_nonce_1().clone());
                self.v1_extra_nonce2_size = subscribe_result.extra_nonce_2_size().clone();

                // In order to finalize the opening procedure we need 3 items: authorization,
                // subscription and difficulty
                if self.v1_authorized && self.v2_target.is_some() {
                    self.finalize_open_channel()
                }
                // Channel opening will be finalized by authorize success or failure
                else {
                    Ok(())
                }
            })
            .map_err(|e| {
                self.abort_open_channel("Upstream subscribe failed");
                e
            })
    }

    /// An authorize result should be true, any other problem results in aborting the channel
    fn handle_authorize_result(
        &mut self,
        msg: &ii_wire::Message<v1::Protocol>,
        payload: &v1::framing::StratumResult,
    ) -> ii_stratum::error::Result<()> {
        trace!(
            "handle_authorize_result() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        // Authorize is expected as a plain boolean answer
        v1::messages::BooleanResult::try_from(payload)
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
                    Err(
                        v1::error::ErrorKind::Subscribe("Authorize result is false".to_string())
                            .into(),
                    )
                }
            })
            // any problem in parsing the response results in authorization failure
            .map_err(|e| {
                self.abort_open_channel("Not authorized");
                e
            })
    }

    // TODO: unused?
    fn handle_ok_result(
        &mut self,
        _msg: &ii_wire::Message<v1::Protocol>,
        payload: &v1::framing::StratumResult,
    ) -> ii_stratum::error::Result<()> {
        let bool_result = v1::messages::BooleanResult::try_from(payload)?;
        trace!("Received: {:?}", bool_result);

        Ok(())
    }

    fn handle_authorize_or_subscribe_error(
        &mut self,
        msg: &ii_wire::Message<v1::Protocol>,
        payload: &v1::framing::StratumError,
    ) -> ii_stratum::error::Result<()> {
        trace!(
            "handle_authorize_or_subscribe_error() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        // Only the first of authorize or subscribe error issues the OpenChannelError message
        if self.state != V2ToV1TranslationState::V1SubscribeOrAuthorizeFail {
            trace!(
                "Upstream connection init failed, dropping channel: {:?}",
                payload
            );
            self.abort_open_channel("Service not ready");
            Err(v1::error::ErrorKind::Subscribe(format!("{:?}", payload)).into())
        } else {
            trace!("Ok, received the second of subscribe/authorize failures, channel is already closed: {:?}", payload);
            Ok(())
        }
    }

    fn handle_submit_result(
        &mut self,
        msg: &ii_wire::Message<v1::Protocol>,
        payload: &v1::framing::StratumResult,
    ) -> ii_stratum::error::Result<()> {
        trace!(
            "handle_submit_result() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        // Authorize is expected as a plain boolean answer
        v1::messages::BooleanResult::try_from(payload)
            .and_then(|bool_result| {
                if bool_result.0 {
                    // TODO this is currently incomplete, we have to track all pending mining
                    // results so that we can correlate the success message and ack
                    let success_msg = v2::messages::SubmitSharesSuccess {
                        channel_id: Self::CHANNEL_ID,
                        last_seq_num: 0,
                        new_submits_accepted_count: 1,
                        new_shares_count: 1,
                    };
                    Self::submit_message(&mut self.v2_tx, success_msg);
                } else {
                    // TODO use reject_shares() method once we can track the original payload message
                    let err_msg = v2::messages::SubmitSharesError {
                        channel_id: Self::CHANNEL_ID,
                        // TODO the sequence number needs to be determined from the failed submit, currently,
                        // there is no infrastructure to get this
                        seq_num: 0,
                        code: format!("ShareRjct:{:?}", payload)[..32].try_into().unwrap(), // FIXME: error code
                    };
                    Self::submit_message(&mut self.v2_tx, err_msg);
                }
                trace!("Submit result: {:?}", bool_result);

                Ok(())
            })
            // TODO what should be the behavior when the result is incorrectly passed, shall we
            // report it as a SubmitSharesError?
            .map_err(|e| e)
    }

    fn handle_submit_error(
        &mut self,
        msg: &ii_wire::Message<v1::Protocol>,
        payload: &v1::framing::StratumError,
    ) -> ii_stratum::error::Result<()> {
        trace!(
            "handle_submit_error() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        // TODO use reject_shares() method once we can track the original payload message
        let err_msg = v2::messages::SubmitSharesError {
            channel_id: Self::CHANNEL_ID,
            // TODO the sequence number needs to be determined from the failed submit, currently,
            // there is no infrastructure to get this
            seq_num: 0,
            code: format!("ShareRjct:{:?}", payload)[..32].try_into().unwrap(), // FIXME: error code
        };

        Self::submit_message(&mut self.v2_tx, err_msg);
        Ok(())
    }

    // TODO: unused?
    fn handle_any_stratum_error(
        &mut self,
        msg: &ii_wire::Message<v1::Protocol>,
        payload: &v1::framing::StratumError,
    ) -> ii_stratum::error::Result<()> {
        trace!(
            "handle_any_stratum_error() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        unimplemented!();
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
            Err(super::error::ErrorKind::General(
                "Extra nonce 1 missing, cannot calculate merkle root".into(),
            )
            .into())
        }
    }

    /// TODO temporary workaround that provides locally tracked block height (from start of the
    /// mining session. This is yet to be implemented
    fn extract_block_height_from_notify(&self, _payload: &v1::messages::Notify) -> u32 {
        self.block_height.get()
    }

    /// Builds SetNewPrevHash for the specified v1 Notify `payload`
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
        payload: &v1::messages::Notify,
    ) -> crate::error::Result<v2::messages::SetNewPrevHash> {
        let max_ntime_offset = (7200 - 0/*min(0, sys_time - payload.time())*/) / 4;
        // TODO review how this can be prevented from failing. If this fails, it should result in
        // panic as it marks a software bug
        let prev_hash =
            sha256d::Hash::from_slice(payload.prev_hash()).context("Build SetNewPrevHash")?;
        let prev_hash = Uint256Bytes(prev_hash.into_inner());

        Ok(v2::messages::SetNewPrevHash {
            block_height: self.extract_block_height_from_notify(payload),
            prev_hash,
            min_ntime: payload.time(),
            max_ntime_offset,
            nbits: payload.bits(),
        })
    }

    /// Converts specified `channel_id` into extra nonce 2 with a specified
    /// `v1_extra_nonce2_size`
    /// TODO review the implementation 'how to efficiently render a u32 into a byte array'
    #[inline]
    fn channel_to_extra_nonce2_bytes(channel_id: u32, v1_extra_nonce2_size: usize) -> BytesMut {
        let mut extra_nonce2: BytesMut = BytesMut::with_capacity(v1_extra_nonce2_size);

        extra_nonce2.extend_from_slice(&u32::to_le_bytes(channel_id));

        if v1_extra_nonce2_size > size_of::<u32>() {
            let padding = v1_extra_nonce2_size - size_of::<u32>();
            extra_nonce2.extend_from_slice(&vec![0; padding]);
        }
        extra_nonce2
    }

    /// Generates log trace entry and reject shares error reply to the client
    fn reject_shares(&mut self, payload: &v2::messages::SubmitShares, err_msg: String) {
        trace!("Unrecognized channel ID: {}", payload.channel_id);
        Self::submit_message(
            &mut self.v2_tx,
            v2::messages::SubmitSharesError {
                channel_id: payload.channel_id,
                seq_num: payload.seq_num,
                code: err_msg[..32].try_into().unwrap(), // FIXME: error code,
            },
        );
    }

    fn perform_notify(&mut self, payload: &v1::messages::Notify) {
        self.calculate_merkle_root(payload)
            .and_then(|merkle_root| {
                let v2_job = v2::messages::NewMiningJob {
                    channel_id: Self::CHANNEL_ID,
                    job_id: self.v2_job_id.next(),
                    block_height: self.extract_block_height_from_notify(payload),
                    merkle_root: Uint256Bytes(merkle_root.into_inner()),
                    version: payload.version(),
                };

                // Make sure we generate new prev hash. Empty JobMap means this is the first
                // mining.notify message and we also
                // have to issue NewPrevHash. In addition to that, we also check the clean
                // jobs flag that indicates a must for new prev hash, too.
                let maybe_set_new_prev_hash =
                    if self.v2_to_v1_job_map.is_empty() || payload.clean_jobs() {
                        self.v2_to_v1_job_map.clear();
                        // Any error means immediate termination
                        // TODO write a unit test for such scenario, too
                        Some(self.build_set_new_prev_hash(payload)?)
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
                            job_id: v1::messages::JobId::from_slice(payload.job_id()),
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

                Self::submit_message(&mut self.v2_tx, v2_job);

                maybe_set_new_prev_hash.and_then(|set_new_prev_hash| {
                    Self::submit_message(&mut self.v2_tx, set_new_prev_hash);
                    Some(())
                });
                Ok(())
            })
            .map_err(|e| trace!("visit_notify: {}", e))
            // Consume the result as we cannot perform any action
            .ok();
    }
}

impl v1::Handler for V2ToV1Translation {
    /// The result visitor takes care of detecting a spurious response without matching request
    /// and passes processing further
    /// TODO write a solid unit test covering all 3 scenarios that can go wrong
    fn visit_stratum_result(
        &mut self,
        msg: &ii_wire::Message<v1::Protocol>,
        payload: &v1::framing::StratumResult,
    ) {
        trace!(
            "visit_stratum_result() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        // Each response message should have an ID for pairing
        msg.id
            .ok_or(ii_stratum::error::Error::from(v1::error::ErrorKind::Rpc(
                "Missing ID in ii_stratum result".to_string(),
            )))
            // find the ID in the request map
            .and_then(|id| {
                self.v1_req_map
                    .remove(&id)
                    .ok_or(ii_stratum::error::Error::from(v1::error::ErrorKind::Rpc(
                        format!("Received invalid ID {}", id).into(),
                    )))
            })
            // run the result through the result handler
            .and_then(|handler| handler.0(self, msg, payload))
            .map_err(|e| trace!("visit_stratum_result: {}", e))
            // Consume the error as there is no way to return anything from the visitor for now.
            .ok();
    }

    fn visit_set_difficulty(
        &mut self,
        msg: &Message<v1::Protocol>,
        payload: &v1::messages::SetDifficulty,
    ) {
        trace!(
            "visit_set_difficulty() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        let diff = payload.value() as u32;
        self.v2_target = Some(Self::DIFF1_TARGET / diff);
        if self.v1_authorized && self.v1_extra_nonce1.is_some() {
            // Initial set difficulty finalizes open channel if all preconditions are met
            if self.state == V2ToV1TranslationState::OpenChannelPending {
                self.finalize_open_channel()
                    .map_err(|e| trace!("visit_set_difficulty: {}", e))
                    // Consume the error as there is no way to return anything from the visitor for now.
                    .ok();
            }
            // Anything after that is standard difficulty adjustment
            else {
                trace!("Sending current target: {:x?}", self.v2_target);
                self.send_set_target();
            }
        }
    }

    /// Composes a new mining job and sends it downstream
    /// TODO: Only 1 channel is supported
    fn visit_notify(&mut self, msg: &Message<v1::Protocol>, payload: &v1::messages::Notify) {
        trace!(
            "visit_notify() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );

        // We won't process the job as long as the channel is not operational
        if self.state != V2ToV1TranslationState::Operational {
            self.v1_deferred_notify = Some(payload.clone());
            info!("Channel not yet operational, caching latest mining.notify from upstream");
            return;
        }
        self.perform_notify(payload);
    }
    /// TODO currently unimplemented, the proxy should refuse changing the version mask from the server
    /// Since this is a notification only, the only action that the translation can do is log +
    /// report an error
    fn visit_set_version_mask(
        &mut self,
        msg: &Message<v1::Protocol>,
        payload: &v1::messages::SetVersionMask,
    ) {
        trace!(
            "visit_set_version_mask() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
    }
}

/// TODO: implement an internal state where in each state only a subset of visit methods is valid,
/// the rest of the methods have default implementation that only reports error in the log and to the client, dropping a connection?
/// Connection dropping is to be clarified
impl v2::Handler for V2ToV1Translation {
    fn visit_setup_mining_connection(
        &mut self,
        msg: &Message<v2::Protocol>,
        payload: &v2::messages::SetupMiningConnection,
    ) {
        trace!(
            "visit_setup_mining_connection() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        if self.state != V2ToV1TranslationState::Init {
            trace!("Cannot setup connection again, received: {:?}", payload);
            Self::submit_message(
                &mut self.v2_tx,
                v2::messages::SetupMiningConnectionError {
                    code: "Connection can be setup only once".try_into().unwrap(),
                },
            );
            return;
        }

        self.v2_conn_details = Some(payload.clone());
        let mut configure = v1::messages::Configure::new();
        configure.add_feature(v1::messages::VersionRolling::new(
            ii_stratum::BIP320_N_VERSION_MASK,
            ii_stratum::BIP320_N_VERSION_MAX_BITS,
        )); // FIXME: how to handle errors from configure.add_feature() ?

        let v1_configure_message = self.v1_method_into_message(
            configure,
            Self::handle_configure_result,
            Self::handle_configure_error,
        );
        Self::submit_message(&mut self.v1_tx, v1_configure_message);
        self.state = V2ToV1TranslationState::V1Configure;
    }

    /// Opening a channel is a 2 stage process when translating to  V1 ii_stratum, where
    /// both stages can be executed in arbitrary order:
    /// - perform subscribe (and start queuing incoming V1 jobs)
    /// - perform authorize
    ///
    /// Upon successful authorization:
    /// - communicate OpenChannelSuccess
    /// - start sending Jobs downstream to V2 client
    fn visit_open_channel(
        &mut self,
        msg: &Message<v2::Protocol>,
        payload: &v2::messages::OpenChannel,
    ) {
        trace!(
            "visit_open_channel() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        if self.state != V2ToV1TranslationState::ConnectionSetup
            && self.state != V2ToV1TranslationState::V1SubscribeOrAuthorizeFail
        {
            trace!(
                "Out of sequence OpenChannel message, received: {:?}",
                payload
            );
            Self::submit_message(
                &mut self.v2_tx,
                v2::messages::OpenChannelError {
                    req_id: payload.req_id,
                    code: "Out of sequence OpenChannel msg".try_into().unwrap(),
                },
            );
            return;
        }
        // Connection details are present by now
        // TODO eliminate the connection details clone()
        self.v2_conn_details.clone().and_then(|conn_details| {
            self.v2_channel_details = Some(payload.clone());
            self.state = V2ToV1TranslationState::OpenChannelPending;

            let subscribe = v1::messages::Subscribe(
                Some(payload.device.fw_ver.to_string()),
                None,
                Some(conn_details.connection_url.try_into().unwrap()), // FIXME: error handling
                None,
            );

            let v1_subscribe_message = self.v1_method_into_message(
                subscribe,
                Self::handle_subscribe_result,
                Self::handle_authorize_or_subscribe_error,
            );

            Self::submit_message(&mut self.v1_tx, v1_subscribe_message);

            let authorize = v1::messages::Authorize(payload.user.to_string(), "".to_string());
            let v1_authorize_message = self.v1_method_into_message(
                authorize,
                Self::handle_authorize_result,
                Self::handle_authorize_or_subscribe_error,
            );
            Self::submit_message(&mut self.v1_tx, v1_authorize_message);
            // TODO cleanup
            Some(())
        });
    }

    /// The flow of share processing is as follows:
    ///
    /// - find corresponding job
    /// - verify the share that it meets the target
    /// - emit V1 Submit message
    ///
    /// If any of the above points fail, reply with SubmitShareError + reasoning
    fn visit_submit_shares(
        &mut self,
        msg: &Message<v2::Protocol>,
        payload: &v2::messages::SubmitShares,
    ) {
        trace!(
            "visit_submit_shares() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        // Report invalid channel ID
        if payload.channel_id != Self::CHANNEL_ID {
            self.reject_shares(
                payload,
                format!("Unrecognized channel ID {}", payload.channel_id),
            );
            return;
        }

        // Channel details must be filled by now, anything else is a bug, unfortunately, due to
        // the 'expect' we have to clone them. TODO review this code
        let v2_channel_details = &self
            .v2_channel_details
            .clone()
            .expect("Missing channel details");
        // TODO this is only here as we want to prevent locking up 'self' into multiple closures
        // and causing borrow checker complains
        let v1_extra_nonce2_size = self.v1_extra_nonce2_size;

        // Check job ID validity
        let v1_submit_template = self
            .v2_to_v1_job_map
            .get(&payload.job_id)
            // convert missing job ID (None) into an error
            .ok_or(crate::error::ErrorKind::General(format!(
                "V2 Job ID not present {} in registry",
                payload.job_id
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
                    payload.ntime,
                    payload.nonce,
                    // ensure the version bits in the template follow BIP320
                    payload.version & ii_stratum::BIP320_N_VERSION_MASK,
                );
                // Convert the method into a message + provide handling methods
                let v1_submit_message = self.v1_method_into_message(
                    submit,
                    Self::handle_submit_result,
                    Self::handle_submit_error,
                );
                Self::submit_message(&mut self.v1_tx, v1_submit_message);
            }
            Err(e) => self.reject_shares(payload, format!("{}", e)),
        }
    }
}
