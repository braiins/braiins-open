// Copyright (C) 2021  Braiins Systems s.r.o.
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

//! Definition of all Stratum V1 messages

use std::convert::{TryFrom, TryInto};
use std::result::Result as StdResult;

use bitcoin_hashes::{sha256d, Hash, HashEngine};
use serde::de::Deserializer;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

use ii_unvariant::Id;

use super::error::Error;
use super::rpc::{self, Method, Rpc};
use super::{ExtraNonce1, HexBytes, HexU32Be, MessageId, PrevHash};
use crate::error::Result;
use crate::v2;

#[cfg(test)]
pub mod test;

/// Implement Id and several to/from rpc types conversion traits
macro_rules! impl_request {
    ($request:tt, $method:path) => {
        // Unvariant IDs
        impl Id<Method> for $request {
            const ID: Method = $method;
        }
        impl Id<Method> for (MessageId, $request) {
            const ID: Method = $method;
        }

        impl TryFrom<rpc::Rpc> for $request {
            type Error = crate::error::Error;

            fn try_from(value: Rpc) -> Result<Self> {
                let (_, this) = value.try_into()?;
                Ok(this)
            }
        }

        impl TryFrom<rpc::Rpc> for (MessageId, $request) {
            type Error = crate::error::Error;

            fn try_from(value: Rpc) -> Result<Self> {
                if let Rpc::Request(request) = value {
                    Ok((request.id, $request::try_from(request)?))
                } else {
                    Err(Error::Rpc(format!("BUG: response handled as request")).into())
                }
            }
        }

        impl TryFrom<$request> for rpc::RequestPayload {
            type Error = crate::error::Error;

            fn try_from(msg: $request) -> Result<Self> {
                let params = serde_json::to_value(msg)?;

                Ok(Self {
                    method: $method,
                    params,
                })
            }
        }

        impl TryFrom<rpc::Request> for $request {
            type Error = crate::error::Error;

            fn try_from(req: rpc::Request) -> Result<Self> {
                // Invariant: it's caller's responsibility to ensure not to pass wrong request
                // for conversion
                assert_eq!(req.payload.method, $method);

                serde_json::from_value(req.payload.params).map_err(Into::into)
            }
        }
    };
}

/// Generates a declaration of a request message and De/Serialize impls
/// which convert a struct with fields to/from a tuple de/serialization.
/// This is done because in v1 the arguments are simply stored in an array,
/// but in code regular structs with named fields are a lot more readable.
/// impl_request!() is also called as part of this macro
/// to generate the IDs and conversions.
macro_rules! declare_request {
    ($doc:expr, $method:path, struct $request:tt { $($field:ident : $ty:ty,)* }) => {
        #[doc=$doc]
        #[derive(PartialEq, Clone, Debug)]
        pub struct $request {
            $(pub $field : $ty),*
        }

        // Serialize and Deserialize are implemented through
        // conversion to tuple that omits the `id` field.

        impl Serialize for $request {
            fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
            where
                S: Serializer,
            {
                // Convert to a tuple and serialize:
                let tuple = ($(&self.$field,)*);
                tuple.serialize(serializer)
            }
        }

        impl<'de> Deserialize<'de> for $request {
            fn deserialize<D>(deserializer: D) -> StdResult<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                // Deserialize into a tuple:
                type Tuple = ($($ty,)*);
                let tuple = Tuple::deserialize(deserializer)?;

                // Unpack the tuple into fields & construct Self out of them:
                let ($($field,)*) = tuple;
                Ok(Self {
                    $($field),*
                })
            }
        }

        impl_request!($request, $method);
    };
}

macro_rules! impl_response {
    ($response:ty) => {
        impl TryFrom<$response> for rpc::ResponsePayload {
            type Error = crate::error::Error;

            fn try_from(resp: $response) -> Result<rpc::ResponsePayload> {
                let result = rpc::StratumResult::new(resp)?;
                Ok(Ok(result))
            }
        }

        impl TryFrom<rpc::Response> for $response {
            type Error = crate::error::Error;

            fn try_from(resp: rpc::Response) -> Result<Self> {
                let result = resp
                    .stratum_result
                    .ok_or_else(|| Error::Json("No result".into()))?;
                <$response>::try_from(&result)
            }
        }

        impl TryFrom<&rpc::StratumResult> for $response {
            type Error = crate::error::Error;

            fn try_from(result: &rpc::StratumResult) -> Result<Self> {
                // TODO this is needs to be fixed within the deserialization stack with regards
                // to the visitor pattern. We shouldn't clone any part of the incoming message
                // However, since the result is being passed by reference
                serde_json::from_value(result.0.clone()).map_err(Into::into)
            }
        }
    };
}

/// Version rolling mask has a new type to provide one consistent place
/// that determines the exact serialization format of it
/// Mask bits are allocated as per BIP320
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct VersionMask(pub HexU32Be);

/// Version rolling configuration extension that follows the model in BIP310
/// Miner requests a certain mask and minimum amount of bits
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct VersionRolling {
    /// Mask bits are allocated as per BIP320
    #[serde(rename = "version-rolling.mask")]
    pub mask: VersionMask,
    /// Minimum required number of bits for rolling
    #[serde(rename = "version-rolling.min-bit-count")]
    pub min_bit_count: usize,
}

impl VersionRolling {
    pub fn new(mask: u32, min_bit_count: usize) -> Self {
        Self {
            mask: VersionMask(HexU32Be(mask)),
            min_bit_count,
        }
    }
}

impl TryInto<(String, serde_json::Value)> for VersionRolling {
    type Error = crate::error::Error;

    fn try_into(self) -> Result<(String, serde_json::Value)> {
        Ok(("version-rolling".to_string(), serde_json::to_value(self)?))
    }
}

declare_request!(
    "Mining configure",
    Method::Configure,
    struct Configure {
        features: Vec<String>,
        configure_map: serde_json::Value,
    }
);

#[allow(clippy::new_without_default)]
impl Configure {
    /// Constructs an empty configuration
    pub fn new() -> Self {
        Self {
            features: vec![],
            configure_map: serde_json::Map::new().into(),
        }
    }

    /// Simplifies adding new feature to the current map
    pub fn add_feature<T>(&mut self, feature: T) -> Result<()>
    where
        T: TryInto<(String, serde_json::Value), Error = crate::error::Error>,
    {
        let (feature_name, feature_map) = feature.try_into()?;
        self.features.push(feature_name);

        // Merge the feature into the current configuration map
        if let Some(configure_map) = self.configure_map.as_object_mut() {
            if let Some(feature_map) = feature_map.as_object() {
                for (k, v) in feature_map {
                    configure_map.insert(k.clone(), v.clone());
                }
            }
        }

        Ok(())
    }
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct ConfigureResult(pub serde_json::Value);

impl_response!(ConfigureResult);

/// Extranonce subscriptionMessage
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct ExtranonceSubscribe;

impl_request!(ExtranonceSubscribe, Method::ExtranonceSubscribe);

declare_request!(
    "SetExtranonce message (sent if we subscribed with `ExtranonceSubscribe`)",
    Method::SetExtranonce,
    struct SetExtranonce {
        extra_nonce1: ExtraNonce1,
        extra_nonce2_size: usize,
    }
);

declare_request!(
    "Compounds all data required for mining subscription",
    Method::Subscribe,
    struct Subscribe {
        agent_signature: Option<String>,
        extra_nonce1: Option<ExtraNonce1>,
        url: Option<String>,
        port: Option<String>,
    }
);

impl Subscribe {
    pub fn agent_signature(&self) -> Option<&String> {
        self.agent_signature.as_ref()
    }

    pub fn extra_nonce1(&self) -> Option<&ExtraNonce1> {
        self.extra_nonce1.as_ref()
    }

    pub fn url(&self) -> Option<&String> {
        self.url.as_ref()
    }

    pub fn port(&self) -> Option<&String> {
        self.port.as_ref()
    }
}

/// Subscription response
/// TODO: Do we need to track any subscription ID's or anyhow validate those fields?
/// see StratumError for reasons why this structure doesn't have named fields
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct SubscribeResult(pub Vec<serde_json::Value>, pub ExtraNonce1, pub usize);

impl SubscribeResult {
    pub fn subscriptions(&self) -> &Vec<serde_json::Value> {
        &self.0
    }

    pub fn extra_nonce_1(&self) -> &ExtraNonce1 {
        &self.1
    }

    pub fn extra_nonce_2_size(&self) -> usize {
        self.2
    }
}

// TODO write a test case for parsing incorrect response
impl_response!(SubscribeResult);

/// A boolean result
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct BooleanResult(pub bool);

impl_response!(BooleanResult);

declare_request!(
    "Subscription response
    TODO: Do we need to track any subscription ID's or anyhow validate those fields?
    see StratumError for reasons why this structure doesn't have named fields",
    Method::Authorize,
    struct Authorize {
        name: String,
        password: String,
    }
);

/// Difficulty value set by the upstream stratum server
/// Note, that we explicitly enforce 1 one element array so that serde doesn't flatten the
/// 'params' JSON array to a single value, eliminating the array completely.
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct SetDifficulty(pub [f32; 1]);

impl_request!(SetDifficulty, Method::SetDifficulty);

impl SetDifficulty {
    pub fn value(&self) -> f32 {
        self.0[0]
    }
}

impl From<f32> for SetDifficulty {
    fn from(f: f32) -> Self {
        Self([f])
    }
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct JobId(String);

impl std::str::FromStr for JobId {
    type Err = crate::error::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(String::from(s)))
    }
}
/// Leading part of the coinbase transaction
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct CoinBase1(HexBytes);

/// Trailing part of the coinbase transaction
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct CoinBase2(HexBytes);

/// Merkle branch of transaction hashes leading to coinbase
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct MerkleBranch(Vec<HexBytes>);

/// Version field of Bitcoin block header
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct Version(HexU32Be);

/// Network difficulty target
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct Bits(HexU32Be);

/// Network time
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct Time(HexU32Be);

declare_request!(
    "New mining job notification",
    Method::Notify,
    struct Notify {
        job_id: JobId,
        prev_hash: PrevHash,
        coin_base_1: CoinBase1,
        coin_base_2: CoinBase2,
        merkle_branch: MerkleBranch,
        version: Version,
        bits: Bits,
        time: Time,
        clean_jobs: bool,
    }
);

impl MerkleBranch {
    pub fn fold_branch_into_merkle_root(&self, cb_tx_hash: sha256d::Hash) -> sha256d::Hash {
        self.0.iter().fold(cb_tx_hash, |curr_merkle_root, tx_hash| {
            let mut engine = sha256d::Hash::engine();
            engine.input(&curr_merkle_root.into_inner());
            engine.input(tx_hash.as_ref().as_slice());
            sha256d::Hash::from_engine(engine)
        })
    }

    pub fn v2_encode(&self) -> Result<v2::types::Seq0_255<v2::types::Uint256Bytes>> {
        use v2::types::{Seq0_255, Uint256Bytes};
        let mut branch = Vec::with_capacity(self.0.len());
        for leaf in self.0.iter() {
            branch.push(Uint256Bytes::try_from(leaf.clone())?);
        }
        Ok(Seq0_255::from_vec(branch))
    }
}

// TODO consider making the attributes return new type references, it would be less prone to typos
impl Notify {
    pub fn job_id(&self) -> &str {
        &(self.job_id).0
    }

    pub fn prev_hash(&self) -> &[u8] {
        self.prev_hash.as_ref()
    }

    pub fn coin_base_1(&self) -> &[u8] {
        &((self.coin_base_1).0).0
    }

    pub fn coin_base_2(&self) -> &[u8] {
        &((self.coin_base_2).0).0
    }

    pub fn merkle_branch(&self) -> &MerkleBranch {
        &(self.merkle_branch)
    }

    pub fn merkle_root(&self, extranonce1: &[u8], extranonce2: &[u8]) -> sha256d::Hash {
        let mut coin_base = Vec::with_capacity(
            self.coin_base_1().len()
                + extranonce1.len()
                + extranonce2.len()
                + self.coin_base_2().len(),
        );
        coin_base.extend_from_slice(self.coin_base_1());
        coin_base.extend_from_slice(extranonce1);
        coin_base.extend_from_slice(extranonce2);
        coin_base.extend_from_slice(self.coin_base_2());

        let cb_tx_hash = sha256d::Hash::hash(&coin_base);
        self.merkle_branch()
            .fold_branch_into_merkle_root(cb_tx_hash)
    }

    pub fn version(&self) -> u32 {
        ((self.version).0).0
    }

    pub fn bits(&self) -> u32 {
        ((self.bits).0).0
    }

    pub fn time(&self) -> u32 {
        ((self.time).0).0
    }

    pub fn clean_jobs(&self) -> bool {
        self.clean_jobs
    }
}

declare_request!(
    "Server may arbitrarily adjust version mask",
    Method::SetVersionMask,
    struct SetVersionMask {
        mask: VersionMask,
    }
);

impl SetVersionMask {
    pub fn value(&self) -> u32 {
        ((self.mask).0).0
    }
}

/// Combined username and worker
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct UserName(String);

/// Extra nonce 2, note the underlying serialization type
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct ExtraNonce2(HexBytes);

/// Nonce for the block
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct Nonce(HexU32Be);

// TODO (jca) generate the field accessors
declare_request!(
    "New mining job notification",
    Method::Submit,
    struct Submit {
        user_name: UserName,
        job_id: JobId,
        extra_nonce_2: ExtraNonce2,
        time: Time,
        nonce: Nonce,
        version: Version,
    }
);

impl Submit {
    pub fn new(
        user_name: String,
        job_id: JobId,
        extra_nonce2: &[u8],
        time: u32,
        nonce: u32,
        version: u32,
    ) -> Self {
        Self {
            user_name: UserName(user_name),
            job_id,
            extra_nonce_2: ExtraNonce2(HexBytes(extra_nonce2.into())),
            time: Time(HexU32Be(time)),
            nonce: Nonce(HexU32Be(nonce)),
            version: Version(HexU32Be(version)),
        }
    }

    pub fn user_name(&self) -> &String {
        &(self.user_name).0
    }

    pub fn job_id(&self) -> &String {
        &(self.job_id).0
    }

    pub fn extra_nonce_2(&self) -> &[u8] {
        &((self.extra_nonce_2).0).0
    }

    pub fn time(&self) -> u32 {
        ((self.time).0).0
    }

    pub fn nonce(&self) -> u32 {
        ((self.nonce).0).0
    }

    pub fn version(&self) -> u32 {
        ((self.version).0).0
    }
}

/// Server initiated message requiring client to perform a reconnect, all fields are optional and
/// we don't know which of them the server sends
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct ClientReconnect(pub Vec<serde_json::Value>);

impl_request!(ClientReconnect, Method::ClientReconnect);

impl ClientReconnect {
    pub fn host(&self) -> Option<&serde_json::Value> {
        self.0.get(0)
    }

    pub fn port(&self) -> Option<&serde_json::Value> {
        self.0.get(1)
    }

    pub fn wait_time(&self) -> Option<&serde_json::Value> {
        self.0.get(2)
    }
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct Ping(pub Vec<serde_json::Value>);
impl_request!(Ping, Method::Ping);

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct Pong(pub String);
impl_response!(Pong);
