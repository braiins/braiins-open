//! Definition of all Stratum V1 messages

use failure::ResultExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::convert::TryFrom;

use super::error::ErrorKind;
use super::framing;
use super::{ExtraNonce1, V1Handler, V1Protocol};
use crate::error::{Error, Result};
use crate::v1::framing::Method;

#[cfg(test)]
pub mod test;

/// Compounds all data required for mining subscription
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Subscribe(
    pub Option<String>,
    pub Option<ExtraNonce1>,
    pub Option<String>,
    pub Option<String>,
);

impl Subscribe {
    pub fn agent_signature(&self) -> Option<&String> {
        self.0.as_ref()
    }
    pub fn extra_nonce1(&self) -> Option<&ExtraNonce1> {
        self.1.as_ref()
    }
    pub fn url(&self) -> Option<&String> {
        self.2.as_ref()
    }
    pub fn port(&self) -> Option<&String> {
        self.3.as_ref()
    }
}

macro_rules! impl_conversion_request {
    ($request:ty, $method:path, $handler_fn:ident) => {
        impl TryFrom<$request> for framing::RequestPayload {
            type Error = crate::error::Error;

            fn try_from(msg: $request) -> Result<Self> {
                let params = serde_json::to_value(msg).context("Cannot parse request")?;

                Ok(Self {
                    method: $method,
                    params,
                })
            }
        }

        impl TryFrom<framing::Request> for $request {
            type Error = crate::error::Error;

            fn try_from(req: framing::Request) -> std::result::Result<Self, Self::Error> {
                // Invariant: it's caller's responsibility to ensure not to pass wrong request
                // for conversion
                assert_eq!(req.payload.method, $method);

                serde_json::from_value(req.payload.params).map_err(Into::into)
            }
        }

        impl wire::Payload<V1Protocol> for $request {
            fn accept(&self, msg: &wire::Message<V1Protocol>, handler: &mut V1Handler) {
                handler.$handler_fn(msg, self);
            }
        }
    };
}

macro_rules! impl_conversion_response {
    ($response:ty, $handler_fn:ident) => {
        impl TryFrom<$response> for framing::ResponsePayload {
            type Error = crate::error::Error;

            fn try_from(resp: $response) -> Result<framing::ResponsePayload> {
                let result = framing::StratumResult(
                    serde_json::to_value(resp).context("Cannot parse response")?,
                );

                Ok(framing::ResponsePayload {
                    result: Some(result),
                    error: None,
                })
            }
        }

        impl TryFrom<framing::Response> for $response {
            type Error = crate::error::Error;

            fn try_from(resp: framing::Response) -> Result<Self> {
                let result = resp
                    .payload
                    .result
                    .ok_or(ErrorKind::Subscribe("No result".into()))?;

                serde_json::from_value(result.0)
                    .context("Failed to parse response")
                    .map_err(Into::into)
            }
        }

        impl wire::Payload<V1Protocol> for $response {
            fn accept(&self, msg: &wire::Message<V1Protocol>, handler: &mut V1Handler) {
                handler.$handler_fn(msg, self);
            }
        }
    };
}

// Subscribe::try_from()
//  FIXME: verify signature, url, and port?
//  let agent_signature =
//      req.param_to_string(0, ErrorKind::Subscribe("Invalid signature".into()))?;
//  let url = req.param_to_string(2, ErrorKind::Subscribe("Invalid pool URL".into()))?;
//  let port = req.param_to_string(3, ErrorKind::Subscribe("Invalid TCP port".into()))?;
impl_conversion_request!(Subscribe, Method::Subscribe, visit_subscribe);

/// Custom subscriptions
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Subscription(pub String, pub String);

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

// TODO write a test case for parsing incorrect response
impl_conversion_response!(SubscribeResult, visit_subscribe_result);
