//! This module defines framing of Stratum V1 messages.
//! Eventhough, the protocol is pure json JSON it distinguishes 2 types of frames with different
//! JSON scheme:
//! - stratum request (with optional ID), request with NULL ID is considered a notification.
//! - stratum response (associated with a previously issued request by the ID)

pub mod codec;

use failure::ResultExt;
use serde;
use serde::{Deserialize, Serialize};
use serde_json;
use serde_json::Value;
use std::convert::{TryFrom, TryInto};
use std::str;
use std::str::FromStr;
//use serde_tuple::*;
//use serde_tuple::{Deserialize_tuple, Serialize_tuple};

use super::{V1Handler, V1Protocol};
use crate::error::Result;
use crate::test_utils::v1::*;
use crate::v1::error::ErrorKind;
use wire;

pub const MAX_MESSAGE_LENGTH: usize = 16384;

/// All recognized methods of the V1 protocol have the 'mining.' prefix in json.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum Method {
    #[serde(rename = "mining.subscribe")]
    Subscribe,
    #[serde(rename = "mining.authorize")]
    Authorize,
    #[serde(rename = "mining.set_difficulty")]
    SetDifficulty,
    #[serde(rename = "mining.configure")]
    Configure,
    #[serde(rename = "mining.submit")]
    Submit,
    #[serde(rename = "mining.notify")]
    Notify,
    /// Catch all variant
    #[serde(other)]
    Unknown,
}

/// The motivation is to provide only the payload part of the message to ID
/// handling
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct RequestPayload {
    /// Protocol method to be 'called'
    pub method: Method,
    /// Vector of method parameters
    pub params: Value,
}

/// Generic stratum request
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Request {
    /// Optional identifier for pairing with the response. Empty ID is a notification
    pub id: Option<u32>,

    /// Request payload doesn't have special tag, we need to separate it to simplify
    /// serialization/deserialization
    #[serde(flatten)]
    pub payload: RequestPayload,
}

/// New type that represents stratum result that can be further parsed based on the actual
/// response type
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct StratumResult(pub serde_json::Value);

impl StratumResult {
    pub fn new_from<T: Serialize>(value: T) -> Result<Self> {
        let value = serde_json::to_value(value).context("Failed to convert to result")?;
        Ok(Self(value))
    }
}

// Note: this doesn't work due to conflicting implementation from core (
// ```impl<T, U> std::convert::TryFrom<U> for T
//            where U: std::convert::Into<T>; ```
//
//impl<T: Serialize> TryFrom<T> for StratumResult {
//    type Error = crate::error::Error;
//
//    fn try_from(value: T) -> std::result::Result<Self, Self::Error> {
//        let value = serde_json::to_value(value).context("Failed to convert to result")?;
//        StratumResult(value)
//    }
//}

/// Specific protocol implementation for any stratum result
impl wire::Payload<V1Protocol> for StratumResult {
    fn accept(&self, msg: &wire::Message<V1Protocol>, handler: &mut V1Handler) {
        handler.visit_stratum_result(msg, self);
    }
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct StratumError(pub i32, pub String, pub Option<String>);
// TODO this currently doesn't compile. Investigate serde_tuple issue.
//#[derive(Serialize_tuple, Deserialize_tuple, Debug)]
//pub struct StratumError {
//    pub code: i32,
//    pub msg: String,
//    pub trace_back: Option<String>,
//}

/// Specific protocol implementation for any stratum result
impl wire::Payload<V1Protocol> for StratumError {
    fn accept(&self, msg: &wire::Message<V1Protocol>, handler: &mut V1Handler) {
        handler.visit_stratum_error(msg, self);
    }
}

/// The motivation is to provide only the payload part of the message to ID
/// handling
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct ResponsePayload {
    /// Successful responses/notification have a result
    pub result: Option<StratumResult>,
    /// Error responses provide details via this field
    pub error: Option<StratumError>,
}

/// Generic stratum response
///
/// The response maybe optionally paired via 'id' with original request. Empty ID
/// represents a notification.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Response {
    /// Response pairing identifier
    pub id: u32,
    /// Response payload doesn't have special tag, we need to separate it to simplify
    /// serialization/deserialization
    #[serde(flatten)]
    pub payload: ResponsePayload,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
#[serde(untagged)]
pub enum Frame {
    RpcRequest(Request),
    RpcResponse(Response),
}

impl Frame {
    pub fn to_json_string(&self) -> serde_json::error::Result<String> {
        serde_json::to_string(self)
    }
}

impl From<Request> for Frame {
    fn from(req: Request) -> Self {
        Frame::RpcRequest(req)
    }
}

impl From<Response> for Frame {
    fn from(resp: Response) -> Self {
        Frame::RpcResponse(resp)
    }
}

impl TryInto<wire::TxFrame> for Frame {
    type Error = crate::error::Error;

    fn try_into(self: Frame) -> std::result::Result<wire::TxFrame, Self::Error> {
        let serialized = serde_json::to_vec(&self).context("Serializing RPC to JSON failed")?;
        Ok(wire::Frame::new(serialized.into_boxed_slice()))
    }
}

impl TryFrom<&[u8]> for Frame {
    type Error = crate::error::Error;

    fn try_from(frame: &[u8]) -> std::result::Result<Self, Self::Error> {
        let x = serde_json::from_slice(&frame)?;
        Ok(x)
    }
}

impl FromStr for Frame {
    type Err = crate::error::Error;

    /// Any error is being converted into JSON parsing error
    #[inline]
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let x = serde_json::from_str(s).context("Parsing JSON failed")?;
        Ok(x)
    }
}

/// ?? To be removed
//pub trait ReceivedRpc {
//    type Item;
//    type Error;
//
//    fn handle_request(&mut self) -> Result<Self::Item, Self::Error>;
//    fn handle_response(&mut self) -> Result<Self::Item, Self::Error>;
//}

//impl Rpc {
//    pub fn handle_received(self, &mut handler: impl ReceivedRpc) {
//        match self {
//            Rpc::RpcRequest(req) => handler.handle_request(self),
//            Rpc::RpcResponse(resp) => handler.handle,
//        }
//        // Check for any reported error from the server and convert it into V1 error
//        if let Some(err) = rpc.error {
//            Err(crate::error::ErrorKind::V1(super::error::ErrorKind::Rpc(
//                format!("{:?}", err),
//            )))?
//        }
//
//        // Missing result in response means an invalid response as there was no error detected
//        // either.
//        let result =
//            response
//                .result
//                .ok_or(crate::error::ErrorKind::V1(super::error::ErrorKind::Rpc(
//                    "Missing result".into(),
//                )))?;
//
//        // Extract response ID or generate an error
//        let id = response
//            .id
//            .ok_or(crate::error::ErrorKind::V1(ErrorKind::Subscribe(
//                "Missing ID".into(),
//            )))?;
//    }
//}

#[cfg(test)]
mod test {
    use super::*;
    use wire::TxFrame;

    #[test]
    fn test_deserialize_request() {
        let _deserialized = Frame::try_from(MINING_SUBSCRIBE_REQ_JSON.as_bytes()).unwrap();
    }

    #[test]
    fn test_deserialize_broken_request() {
        let deserialized = Frame::try_from(MINING_BROKEN_REQ_JSON.as_bytes()).unwrap();

        match deserialized {
            Frame::RpcRequest(request) => assert_eq!(
                Method::Unknown,
                request.payload.method,
                "Unknown method not detected!"
            ),
            Frame::RpcResponse(resp) => assert!(false, "Unexpected response: {:x?}", resp),
        }
    }

    #[test]
    fn test_serialize_request() {
        let request = build_subscribe_request_frame();
        let serialized_frame: TxFrame = request
            .try_into()
            .expect("Failed to serialize JSON request");
        let serialized_frame =
            std::str::from_utf8(&serialized_frame).expect("Failed to convert to UTF-8");
        assert_eq!(
            MINING_SUBSCRIBE_REQ_JSON.to_string(),
            serialized_frame,
            "Serializing test yields different results!",
        );
    }

    #[test]
    fn test_deserialize_ok_response() {
        let expected_response = build_subscribe_ok_response_frame();
        let deserialized_response = Frame::try_from(MINING_SUBSCRIBE_OK_RESULT_JSON.as_bytes())
            .expect("Cannot deserialize JSON request");

        assert_eq!(
            expected_response, deserialized_response,
            "Stratum OK responses don't match!"
        );
    }

    #[test]
    fn test_serialize_ok_response() {
        let response = build_subscribe_ok_response_frame();
        let serialized_frame: TxFrame = response.try_into().expect("Failed to serialize");
        let serialized_frame =
            std::str::from_utf8(&serialized_frame).expect("Failed to convert to UTF-8");

        assert_eq!(
            MINING_SUBSCRIBE_OK_RESULT_JSON, serialized_frame,
            "Serializing test request yields different results!"
        );
    }

    #[test]
    fn test_deserialize_err_response() {
        let expected_response = Frame::RpcResponse(build_stratum_err_response_frame());
        let deserialized_response = Frame::try_from(STRATUM_ERROR_JSON.as_bytes())
            .expect("Cannot deserialize JSON Response");

        assert_eq!(
            expected_response, deserialized_response,
            "Stratum error responses don't match!"
        );
    }

    #[test]
    fn test_serialize_err_response() {
        let response = Frame::RpcResponse(build_stratum_err_response_frame());
        let serialized_frame: TxFrame = response.try_into().expect("Failed to serialize");
        let serialized_frame =
            std::str::from_utf8(&serialized_frame).expect("Failed to convert to UTF-8");

        assert_eq!(
            STRATUM_ERROR_JSON, serialized_frame,
            "Serializing test request yields different results!"
        );
    }
}
