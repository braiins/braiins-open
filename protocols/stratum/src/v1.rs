pub mod error;
pub mod framing;
pub mod messages;

use crate::error::Result;
use crate::v1::error::ErrorKind;

use crate::v1::framing::Frame;
use crate::v1::framing::Method;

use failure::ResultExt;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::str::FromStr;
use wire::{Message, Payload, ProtocolBase};

pub struct V1Protocol;
impl ProtocolBase for V1Protocol {
    type Handler = V1Handler;
}

/// Specifies all messages to be visited
pub trait V1Handler {
    /// Handles the result part of the response
    fn visit_stratum_result(&self, _msg: &Message<V1Protocol>, _payload: &framing::StratumResult) {}
    /// Handles the response part of the response
    fn visit_stratum_error(&self, _msg: &Message<V1Protocol>, _payload: &framing::StratumError) {}

    fn visit_subscribe(&self, _msg: &Message<V1Protocol>, _payload: &messages::Subscribe) {}
}

pub fn deserialize_message(src: &str) -> Result<Message<V1Protocol>> {
    let deserialized = framing::Frame::from_str(src)?;

    let (id, payload) = match deserialized {
        Frame::RpcRequest(request) => match request.payload.method {
            Method::Subscribe => (
                request.id,
                Ok(Box::new(messages::Subscribe::try_from(request)?)
                    as Box<dyn Payload<V1Protocol>>),
            ),
            _ => (
                None,
                Err(ErrorKind::Rpc(format!("Unsupported request {:?}", request))),
            ),
        },
        // This is not ideal implementation as we clone() the result or error parts of the response.
        // Note, however, the unwrap() is safe as the error/result are 'Some'
        Frame::RpcResponse(response) => {
            let msg = if response.payload.error.is_some() {
                Ok(Box::new(response.payload.error.unwrap().clone())
                    as Box<dyn Payload<V1Protocol>>)
            } else if response.payload.result.is_some() {
                Ok(Box::new(response.payload.result.unwrap().clone())
                    as Box<dyn Payload<V1Protocol>>)
            } else {
                Err(ErrorKind::Rpc(format!(
                    "Malformed response no error, no result specified {:?}",
                    response
                )))
            };
            (Some(response.id), msg)
        }
    };
    // convert the payload into message
    Ok(payload.map(|p| Message::new(id, p))?)
}

/// Extranonce 1 introduced as new type to provide shared conversions to/from string
/// TODO: find out correct byte order for extra nonce 1
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(into = "String", from = "String")]
pub struct ExtraNonce1(Vec<u8>);

impl TryFrom<&str> for ExtraNonce1 {
    type Error = crate::error::Error;

    fn try_from(value: &str) -> Result<Self> {
        Ok(ExtraNonce1(
            hex::decode(value).context("Parsing extranonce 1 failed")?,
        ))
    }
}

/// TODO: this is not the cleanest way as any deserialization error is essentially consumed and
/// manifested as empty vector. However, it is very comfortable to use this trait implementation
/// in Extranonce1 serde support
impl From<String> for ExtraNonce1 {
    fn from(value: String) -> Self {
        ExtraNonce1::try_from(value.as_str()).unwrap_or(ExtraNonce1(vec![]))
    }
}

/// Helper Serializer
impl Into<String> for ExtraNonce1 {
    fn into(self) -> String {
        hex::encode(self.0)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::v1::*;

    #[test]
    fn test_extra_nonce1() {
        let expected_enonce1 = ExtraNonce1(vec![0xde, 0xad, 0xbe, 0xef, 0x11, 0x22, 0x33]);
        let expected_enonce1_str = "332211efbeadde";

        let checked_enonce1_str: String = expected_enonce1.clone().into();
        assert_eq!(
            expected_enonce1_str, checked_enonce1_str,
            "Mismatched extranonce 1 strings",
        );

        let checked_enonce1 = ExtraNonce1::try_from(expected_enonce1_str);

        assert!(checked_enonce1.is_ok());
        assert_eq!(
            expected_enonce1,
            checked_enonce1.unwrap(),
            "Mismatched extranonce 1 \
             values",
        )
    }

    /// This test demonstrates an actual implementation of protocol handler (aka visitor to a set of
    /// desired messsages
    #[test]
    fn test_deserialize_request_message() {
        let msg = deserialize_message(MINING_SUBSCRIBE_REQ_JSON).expect("Deserialization failed");
        msg.accept(&TestIdentityHandler);
        // TODO also perform serialization and check the output matches (missing port...)
    }

    #[test]
    fn test_deserialize_response_message() {
        let msg =
            deserialize_message(MINING_SUBSCRIBE_OK_RESULT_JSON).expect("Deserialization failed");
        msg.accept(&TestIdentityHandler);
        // TODO also perform serialization and check the output matches
    }

    // add also a separate stratum error test as per above response
}
