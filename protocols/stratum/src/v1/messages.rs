//! Definition of all Stratum V1 messages

use failure::ResultExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::convert::TryFrom;

use super::error::ErrorKind;
use super::framing;
use super::{ExtraNonce1, V1Handler, V1Protocol};
use crate::error::Result;
use crate::v1::framing::Method;

#[cfg(test)]
pub mod test;

/// Compounds all data required for mining subscription
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Subscribe {
    pub agent_signature: Option<String>,
    pub extra_nonce1: Option<ExtraNonce1>,
    pub url: Option<String>,
    pub port: Option<String>,
}

impl From<Subscribe> for framing::RequestPayload {
    fn from(msg: Subscribe) -> Self {
        framing::RequestPayload {
            method: framing::Method::Subscribe,
            // TODO add enonce1
            params: vec![
                msg.agent_signature.map_or(Value::Null, |s| s.into()),
                Value::Null,
            ],
        }
    }
}

// TODO remove probably
//impl From<Subscribe> for rpc::Rpc {
//    fn from(msg: Subscribe) -> Self {
//        rpc::Rpc::RpcRequest(rpc::Request {
//            id: Some(msg.id),
//            method: rpc::Method::Subscribe,
//            // TODO add enonce1
//            params: vec![msg.agent_signature.into(), msg.id.into()],
//        })
//    }
//}

/// Attempts building Subscribe from an RPC request
impl TryFrom<framing::Request> for Subscribe {
    type Error = crate::error::Error;

    fn try_from(req: framing::Request) -> std::result::Result<Self, Self::Error> {
        // Invariant: it's caller's responsibility to ensure not to pass wrong request
        // for conversion
        assert_eq!(req.payload.method, Method::Subscribe);

        // Extract optional agent signature
        let agent_signature =
            req.param_to_string(0, ErrorKind::Subscribe("Invalid signature".into()))?;

        // Extract optional extra nonce 1 as miner may wish to continue mining on the same extra nonce 1
        let extra_nonce1 = req.param_to_value(
            1,
            |v| {
                serde_json::Value::as_str(&v)
                    .map(ExtraNonce1::try_from)
                    .transpose()
            },
            ErrorKind::Subscribe("Invalid extranonce 1".into()),
        )?;

        let url = req.param_to_string(2, ErrorKind::Subscribe("Invalid pool URL".into()))?;
        let port = req.param_to_string(3, ErrorKind::Subscribe("Invalid TCP port".into()))?;

        Ok(Subscribe {
            agent_signature,
            extra_nonce1,
            url,
            port,
        })
    }
}

/// Specific protocol implementation for subscribe
impl wire::Payload<V1Protocol> for Subscribe {
    fn accept(&self, msg: &wire::Message<V1Protocol>, handler: &mut V1Handler) {
        handler.visit_subscribe(msg, self);
    }
}

/// Custom subscriptions
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Subscription(pub String, pub String);

// TODO to be removed
impl Subscription {
    pub fn new_json_value_from_str(name: &str, id: &str) -> Result<serde_json::Value> {
        serde_json::to_value(Self(name.to_string(), id.to_string()))
            .context("Failed to create")
            .map_err(Into::into)
    }
}

/// Subscription response
/// TODO: Do we need to track any subscription ID's or anyhow validate those fields?
/// see StratumError for reasons why this structure doesn't have named fields
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct SubscribeResult(pub Vec<Subscription>, pub ExtraNonce1, pub usize);

impl SubscribeResult {
    fn subscriptions(&self) -> &Vec<Subscription> {
        &self.0
    }

    fn extra_nonce_1(&self) -> &ExtraNonce1 {
        &self.1
    }

    fn extra_nonce_2_size(&self) -> usize {
        self.2
    }
}

impl TryFrom<SubscribeResult> for framing::ResponsePayload {
    type Error = crate::error::Error;

    fn try_from(
        result: SubscribeResult,
    ) -> std::result::Result<framing::ResponsePayload, Self::Error> {
        let result_value = framing::StratumResult(
            serde_json::to_value(result).context("Cannot parse Subscribe response")?,
        );

        Ok(framing::ResponsePayload {
            result: Some(result_value),
            error: None,
        })
    }
}

/// Attempts building Subscribe result from an RPC
/// TODO write a test case for parsing incorrect response
impl TryFrom<framing::Response> for SubscribeResult {
    type Error = crate::error::Error;

    fn try_from(resp: framing::Response) -> std::result::Result<Self, Self::Error> {
        let result = resp
            .payload
            .result
            .ok_or(ErrorKind::Subscribe("No result".into()))?;
        serde_json::from_value(result.0)
            .context("Failed to parse Subscribe response")
            .map_err(Into::into)
        //        let result = resp
        //            .payload
        //            .result
        //            .ok_or(ErrorKind::Subscribe("Missing result".into()))?;
        //
        //        // Parse extra nonce 1
        //        let extra_nonce1 = ExtraNonce1::try_from(
        //            result[1]
        //                .as_str()
        //                .ok_or(ErrorKind::Subscribe("Missing extranonce 1".into()))?,
        //        )?;
        //
        //        // Get extra nonce 2 size, note that json value is u64, we will cast it to usize as there
        //        // is no risk of losing information
        //        let extra_nonce2_size = result[2]
        //            .as_u64()
        //            .ok_or(ErrorKind::Subscribe("Wrong extranonce 2 size".into()))?
        //            as usize;
        //
        //        Ok(SubscribeResponse {
        //            extra_nonce1,
        //            // TODO: parse subscriptions
        //            subscriptions: vec![],
        //            extra_nonce2_size,
        //        })
    }
}

// TODO: To be removed
//impl FromStr for SubscribeResponse {
//    type Err = crate::error::Error;
//
//    /// SubscribeResponse deserialization is very picky about field consistency
//    ///
//    /// Any error is being converted into Subscription Error
//    #[inline]
//    fn from_str(s: &str) -> Result<Self, Self::Err> {
//        let response: rpc::Response = serde_json::from_str(s).context("Parsing JSON failed")?;
//        // Check for any reported error from the server and convert it into V1 error
//        if let Some(err) = response.error {
//            Err(crate::error::ErrorKind::V1(ErrorKind::Subscribe(format!(
//                "Server error: {:?}",
//                err
//            ))))?
//        }
//
//        // Missing result in response means that server has provided invalid subscription
//        // response
//        let result = response
//            .result
//            .ok_or(crate::error::ErrorKind::V1(ErrorKind::Subscribe(
//                "Missing result".into(),
//            )))?;
//
//        // Extract response ID or generate an error
//        let id = response.id;
//
//        // Parse extra nonce 1
//        let extra_nonce1 =
//            result[1]
//                .as_str()
//                .ok_or(crate::error::ErrorKind::V1(ErrorKind::Subscribe(
//                    "Missing extranonce 1".into(),
//                )))?;
//        let extra_nonce1 = hex::decode(extra_nonce1).context("Parsing extranonce 1 failed")?;
//
//        // Get extra nonce 2 size, note that json value is u64, we will cast it to usize as there
//        // is no risk of losing information
//        let extra_nonce2_size =
//            result[2]
//                .as_u64()
//                .ok_or(crate::error::ErrorKind::V1(ErrorKind::Subscribe(
//                    "Wrong extranonce 2 size".into(),
//                )))? as usize;
//
//        Ok(SubscribeResponse {
//            id: id.into(),
//            extra_nonce1,
//            difficulty_subscription_id: None,
//            notify_subscription_id: None,
//            extra_nonce2_size,
//        })
//    }
//}
