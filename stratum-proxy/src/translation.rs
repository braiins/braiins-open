use bitcoin_hashes::{sha256, sha256d, Hash, HashEngine};
use bytes::BytesMut;
use failure::ResultExt;
use futures::channel::mpsc;
use slog::{error, info, trace, warn};
use std::collections::HashMap;
use std::convert::From;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::fmt;
use std::mem::size_of;

use stratum;
use stratum::v1;
use stratum::v2;
use stratum::v2::types::Uint256Bytes;
use stratum::LOGGER;

use wire::{Message, MessageId, TxFrame};

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

/// Represents a handler method that can process a particular stratum result.
type V1StratumResultHandler = fn(
    &mut V2ToV1Translation,
    &wire::Message<v1::V1Protocol>,
    &v1::framing::StratumResult,
) -> stratum::error::Result<()>;

/// Represents a handler method that can process a particular stratum error.
type V1StratumErrorHandler = fn(
    &mut V2ToV1Translation,
    &wire::Message<v1::V1Protocol>,
    &v1::framing::StratumError,
) -> stratum::error::Result<()>;

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

//type V2ReqMap = HashMap<u32, FnMut(&mut V2ToV1Translation, &wire::Message<V2Protocol>, &v1::framing::StratumResult)>;

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
        trace!(
            LOGGER,
            "Registering v1, request ID: {} method: {:?}",
            id,
            payload
        );
        if self
            .v1_req_map
            .insert(id, (result_handler, error_handler))
            .is_some()
        {
            error!(LOGGER, "BUG: V1 id {} already exists...", id);
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
    fn finalize_open_channel(&mut self) -> stratum::error::Result<()> {
        trace!(LOGGER, "finalize_open_channel()");
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
                    channel_id: 0,
                    dev_id: None,
                    init_target: init_target.clone(),
                    group_channel_id: 0,
                };
                Self::submit_message(&mut self.v2_tx, msg);
                Some(())
            })
            .ok_or(
                v2::error::ErrorKind::ChannelNotOperational("Channel details missing".to_string())
                    .into(),
            )
    }

    /// Reports failure to open the channel and changes the translation state
    /// From this point on a new OpenChannel message is expected as an attempt to reopen the channel
    fn abort_open_channel(&mut self, err_msg: &str) {
        trace!(
            LOGGER,
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
                    code: err_msg.to_string(),
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

    fn handle_subscribe_result(
        &mut self,
        msg: &wire::Message<v1::V1Protocol>,
        payload: &v1::framing::StratumResult,
    ) -> stratum::error::Result<()> {
        trace!(
            LOGGER,
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
        msg: &wire::Message<v1::V1Protocol>,
        payload: &v1::framing::StratumResult,
    ) -> stratum::error::Result<()> {
        trace!(
            LOGGER,
            "handle_authorize_result() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        // Authorize is expected as a plain boolean answer
        v1::messages::BooleanResult::try_from(payload)
            .and_then(|bool_result| {
                trace!(LOGGER, "Authorize result: {:?}", bool_result);
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

    fn handle_ok_result(
        &mut self,
        msg: &wire::Message<v1::V1Protocol>,
        payload: &v1::framing::StratumResult,
    ) -> stratum::error::Result<()> {
        let bool_result = v1::messages::BooleanResult::try_from(payload)?;
        trace!(LOGGER, "Received: {:?}", bool_result);

        Ok(())
    }

    fn handle_authorize_or_subscribe_error(
        &mut self,
        msg: &wire::Message<v1::V1Protocol>,
        payload: &v1::framing::StratumError,
    ) -> stratum::error::Result<()> {
        trace!(
            LOGGER,
            "handle_authorize_or_subscribe_error() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        // Only the first of authorize or subscribe error issues the OpenChannelError message
        if self.state != V2ToV1TranslationState::V1SubscribeOrAuthorizeFail {
            trace!(
                LOGGER,
                "Upstream connection init failed, dropping channel: {:?}",
                payload
            );
            self.abort_open_channel("Service not ready");
            Err(v1::error::ErrorKind::Subscribe(format!("{:?}", payload)).into())
        } else {
            trace!(LOGGER, "Ok, received the second of subscribe/authorize failures, channel is already closed: {:?}", payload);
            Ok(())
        }
    }

    fn handle_submit_result(
        &mut self,
        msg: &wire::Message<v1::V1Protocol>,
        payload: &v1::framing::StratumResult,
    ) -> stratum::error::Result<()> {
        trace!(
            LOGGER,
            "handle_submit_result() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        unimplemented!();
    }

    fn handle_submit_error(
        &mut self,
        msg: &wire::Message<v1::V1Protocol>,
        payload: &v1::framing::StratumError,
    ) -> stratum::error::Result<()> {
        trace!(
            LOGGER,
            "handle_submit_result() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        unimplemented!();
    }

    fn handle_any_stratum_error(
        &mut self,
        msg: &wire::Message<v1::V1Protocol>,
        payload: &v1::framing::StratumError,
    ) -> stratum::error::Result<()> {
        trace!(
            LOGGER,
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
            trace!(
                LOGGER,
                "Coinbase TX hash: {:x?} {:x?}",
                cb_tx_hash,
                coin_base
            );

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
            trace!(LOGGER, "Merkle root calculated: {:x?}", merkle_root);
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
    fn extract_block_height_from_notify(&self, payload: &v1::messages::Notify) -> u32 {
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
        trace!(LOGGER, "Unrecognized channel ID: {}", payload.channel_id);
        Self::submit_message(
            &mut self.v2_tx,
            v2::messages::SubmitSharesError {
                channel_id: payload.channel_id,
                seq_num: payload.seq_num,
                code: err_msg,
            },
        );
    }
}

impl v1::V1Handler for V2ToV1Translation {
    /// The result visitor takes care of detecting a spurious response without matching request
    /// and passes processing further
    /// TODO write a solid unit test covering all 3 scenarios that can go wrong
    fn visit_stratum_result(
        &mut self,
        msg: &wire::Message<v1::V1Protocol>,
        payload: &v1::framing::StratumResult,
    ) {
        trace!(
            LOGGER,
            "visit_stratum_result() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        // Each response message should have an ID for pairing
        msg.id
            .ok_or(stratum::error::Error::from(v1::error::ErrorKind::Rpc(
                "Missing ID in stratum result".to_string(),
            )))
            // find the ID in the request map
            .and_then(|id| {
                self.v1_req_map
                    .remove(&id)
                    .ok_or(stratum::error::Error::from(v1::error::ErrorKind::Rpc(
                        format!("Received invalid ID {}", id).into(),
                    )))
            })
            // run the result through the result handler
            .and_then(|handler| handler.0(self, msg, payload))
            .map_err(|e| trace!(LOGGER, "visit_stratum_result: {}", e))
            // Consume the error as there is no way to return anything from the visitor for now.
            .ok();
    }

    fn visit_set_difficulty(
        &mut self,
        msg: &Message<v1::V1Protocol>,
        payload: &v1::messages::SetDifficulty,
    ) {
        trace!(
            LOGGER,
            "visit_set_difficulty() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        let diff = payload.value() as u32;
        self.v2_target = Some(Self::DIFF1_TARGET / diff);
        if self.v1_authorized && self.v1_extra_nonce1.is_some() {
            self.finalize_open_channel()
                .map_err(|e| trace!(LOGGER, "visit_set_difficulty: {}", e))
                // Consume the error as there is no way to return anything from the visitor for now.
                .ok();
        }
    }

    /// Composes a new mining job and sends it downstream
    /// TODO: Only 1 channel is supported
    fn visit_notify(&mut self, msg: &Message<v1::V1Protocol>, payload: &v1::messages::Notify) {
        trace!(
            LOGGER,
            "visit_notify() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );

        // We won't process the job as long as the channel is not operational
        if self.state != V2ToV1TranslationState::Operational {
            info!(
                LOGGER,
                "Channel not yet operational, ignoring mining.notify from upstream"
            );
            return;
        }

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
                    LOGGER,
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
                    error!(LOGGER, "BUG: V2 id {} already exists...", v2_job.job_id);
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
            .map_err(|e| trace!(LOGGER, "visit_notify: {}", e))
            // Consume the result as we cannot perform any action
            .ok();
    }
}

/// TODO: implement an internal state where in each state only a subset of visit methods is valid,
/// the rest of the methods have default implementation that only reports error in the log and to the client, dropping a connection?
/// Connection dropping is to be clarified
impl v2::V2Handler for V2ToV1Translation {
    fn visit_setup_mining_connection(
        &mut self,
        msg: &Message<v2::V2Protocol>,
        payload: &v2::messages::SetupMiningConnection,
    ) {
        trace!(
            LOGGER,
            "visit_setup_mining_connection() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        if self.state != V2ToV1TranslationState::Init {
            trace!(
                LOGGER,
                "Cannot setup connection again, received: {:?}",
                payload
            );
            Self::submit_message(
                &mut self.v2_tx,
                v2::messages::SetupMiningConnectionError {
                    code: "Connection can be setup only once".to_string(),
                },
            );
            return;
        }

        self.v2_conn_details = Some(payload.clone());
        self.state = V2ToV1TranslationState::ConnectionSetup;

        let response = v2::messages::SetupMiningConnectionSuccess {
            used_protocol_version: Self::PROTOCOL_VERSION as u16,
            max_extranonce_size: Self::MAX_EXTRANONCE_SIZE as u16,
            // TODO provide public key for TOFU
            pub_key: vec![0xde, 0xad, 0xbe, 0xef],
        };
        Self::submit_message(&mut self.v2_tx, response);
    }

    /// Opening a channel is a 2 stage process when translating to  V1 stratum, where
    /// both stages can be executed in arbitrary order:
    /// - perform subscribe (and start queuing incoming V1 jobs)
    /// - perform authorize
    ///
    /// Upon successful authorization:
    /// - communicate OpenChannelSuccess
    /// - start sending Jobs downstream to V2 client
    fn visit_open_channel(
        &mut self,
        msg: &Message<v2::V2Protocol>,
        payload: &v2::messages::OpenChannel,
    ) {
        trace!(
            LOGGER,
            "visit_open_channel() msg.id={:?} state={:?} payload:{:?}",
            msg.id,
            self.state,
            payload,
        );
        if self.state != V2ToV1TranslationState::ConnectionSetup
            && self.state != V2ToV1TranslationState::V1SubscribeOrAuthorizeFail
        {
            trace!(
                LOGGER,
                "Out of sequence OpenChannel message, received: {:?}",
                payload
            );
            Self::submit_message(
                &mut self.v2_tx,
                v2::messages::OpenChannelError {
                    req_id: payload.req_id,
                    code: "Out of sequence OpenChannel message".to_string(),
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
                Some(payload.device.fw_ver.clone()),
                None,
                Some(conn_details.connection_url.clone()),
                None,
            );

            let v1_subscribe_message = self.v1_method_into_message(
                subscribe,
                Self::handle_subscribe_result,
                Self::handle_authorize_or_subscribe_error,
            );

            Self::submit_message(&mut self.v1_tx, v1_subscribe_message);

            let authorize = v1::messages::Authorize(payload.user.clone(), "".to_string());
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
        msg: &Message<v2::V2Protocol>,
        payload: &v2::messages::SubmitShares,
    ) {
        trace!(
            LOGGER,
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
                    v2_channel_details.user.clone(),
                    v1_submit_template.job_id.clone(),
                    Self::channel_to_extra_nonce2_bytes(Self::CHANNEL_ID, v1_extra_nonce2_size)
                        .as_ref(),
                    v1_submit_template.time + payload.ntime_offset as u32,
                    payload.nonce,
                    // ensure the version bits in the template follow BIP320
                    (v1_submit_template.version & !stratum::BIP320_N_VERSION_MASK)
                        | (payload.version & stratum::BIP320_N_VERSION_MASK),
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
