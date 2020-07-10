// Copyright (C) 2020  Braiins Systems s.r.o.
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

//! This module defines Stratum V1 JSON-RPC.
//! Eventhough, the protocol is pure JSON it distinguishes 2 types of frames with different
//! JSON scheme:
//! - stratum request (with optional ID), request with NULL ID is considered a notification.
//! - stratum response (associated with a previously issued request by the ID)

use serde;
use serde::de::{Deserializer, Error as _};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use serde_json;
use serde_json::Value;

use std::convert::{TryFrom, TryInto};
use std::result::Result as StdResult;
use std::str::FromStr;

use super::error::Error as V1Error;
use super::{framing, MessageId, Protocol};
use crate::error::{Error, Result};
use crate::AnyPayload;
use ii_unvariant::{id, GetId, Id};

#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum Method {
    #[serde(rename = "mining.subscribe")]
    Subscribe,
    #[serde(rename = "mining.extranonce.subscribe")]
    ExtranonceSubscribe,
    #[serde(rename = "mining.authorize")]
    Authorize,
    #[serde(rename = "mining.set_difficulty")]
    SetDifficulty,
    #[serde(rename = "mining.set_extranonce")]
    SetExtranonce,
    #[serde(rename = "mining.configure")]
    Configure,
    #[serde(rename = "mining.submit")]
    Submit,
    #[serde(rename = "mining.notify")]
    Notify,
    #[serde(rename = "mining.set_version_mask")]
    SetVersionMask,
    #[serde(rename = "client.reconnect")]
    ClientReconnect,
    // Extensions so that Method can be used as an Id by Rpc's GetId
    #[serde(skip)]
    Result,
    #[serde(skip)]
    Error,
}

/// The motivation is to provide only the payload part of the message to ID
/// handling
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct RequestPayload {
    /// Protocol method to be 'called'.
    /// If not recognized, the method string is stored in `Err`.
    pub method: Method,
    /// Vector of method parameters
    pub params: Value,
}

/// Generic stratum request
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Request {
    /// Optional identifier for pairing with the response. Empty ID is a notification
    pub id: MessageId,

    /// Request payload doesn't have special tag, we need to separate it to simplify
    /// serialization/deserialization
    #[serde(flatten)]
    pub payload: RequestPayload,
}

/// New type that represents stratum result that can be further parsed based on the actual
/// response type
#[id(Method::Result type Method)]
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct StratumResult(pub serde_json::Value);

impl StratumResult {
    pub fn new<T: Serialize>(value: T) -> Result<Self> {
        let value = serde_json::to_value(value)?;
        Ok(Self(value))
    }
}

impl Id<Method> for (MessageId, StratumResult) {
    const ID: Method = StratumResult::ID;
}

impl TryFrom<Rpc> for StratumResult {
    type Error = Error;

    fn try_from(value: Rpc) -> Result<Self> {
        let (_, res) = value.try_into()?;
        Ok(res)
    }
}

impl TryFrom<Rpc> for (MessageId, StratumResult) {
    type Error = Error;

    fn try_from(value: Rpc) -> Result<Self> {
        if let Rpc::Response(r) = value {
            if let Ok(res) = r.result {
                return Ok((Some(r.id), res));
            }
        }

        Err(V1Error::Rpc("This Rpc is not a valid StratumResult".into()).into())
    }
}

impl AnyPayload<Protocol> for StratumResult {
    fn serialize_to_writer(&self, _writer: &mut dyn std::io::Write) -> Result<()> {
        panic!(
            "BUG: serialization of partial message without Rpc not supported {:?}",
            self
        );
    }
}

#[id(Method::Error type Method)]
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct StratumError(pub i32, pub String, pub Option<String>);
// TODO this currently doesn't compile. Investigate serde_tuple issue.
//#[derive(Serialize_tuple, Deserialize_tuple, Debug)]
//pub struct StratumError {
//    pub code: i32,
//    pub msg: String,
//    pub trace_back: Option<String>,
//}

impl Id<Method> for (MessageId, StratumError) {
    const ID: Method = StratumError::ID;
}

impl AnyPayload<Protocol> for StratumError {
    fn serialize_to_writer(&self, _writer: &mut dyn std::io::Write) -> Result<()> {
        panic!(
            "BUG: serialization of partial message without Rpc not supported {:?}",
            self
        );
    }
}

impl TryFrom<Rpc> for StratumError {
    type Error = Error;

    fn try_from(value: Rpc) -> Result<Self> {
        let (_, err) = value.try_into()?;
        Ok(err)
    }
}

impl TryFrom<Rpc> for (MessageId, StratumError) {
    type Error = Error;

    fn try_from(value: Rpc) -> Result<Self> {
        if let Rpc::Response(r) = value {
            if let Err(err) = r.result {
                return Ok((Some(r.id), err));
            }
        }

        Err(V1Error::Rpc("This Rpc is not a valid StratumError".into()).into())
    }
}

pub type ResponsePayload = StdResult<StratumResult, StratumError>;

/// Generic stratum response
///
/// The response maybe optionally paired via 'id' with original request. Empty ID
/// represents a notification.
#[derive(PartialEq, Debug)]
pub struct Response {
    /// Response pairing identifier
    pub id: u32,
    /// A Result type is mapped to either `"result": value` or `"error": value`
    pub result: ResponsePayload,
}

#[derive(Serialize, Deserialize)]
struct ResponseSeriaize {
    id: u32,
    result: Option<StratumResult>,
    error: Option<StratumError>,
}

impl Serialize for Response {
    fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let id = self.id;
        let (result, error) = match self.result.clone() {
            Ok(ok) => (Some(ok), None),
            Err(err) => (None, Some(err)),
        };

        ResponseSeriaize { id, result, error }.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Response {
    fn deserialize<D>(deserializer: D) -> StdResult<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let deserialize = ResponseSeriaize::deserialize(deserializer)?;
        let result = match (deserialize.result, deserialize.error) {
            (Some(ok), None) => Ok(ok),
            (None, Some(err)) => Err(err),
            _ => {
                return Err(D::Error::custom(
                    "V1 Response: Either `result` or `error` field must be set",
                ))
            }
        };

        Ok(Self {
            id: deserialize.id,
            result,
        })
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
#[serde(untagged)]
pub enum Rpc {
    Request(Request),
    Response(Response),
}

impl GetId for Rpc {
    type Id = Method;

    fn get_id(&self) -> Self::Id {
        match self {
            Rpc::Request(r) => r.payload.method,
            Rpc::Response(r) => {
                if r.result.is_ok() {
                    Method::Result
                } else {
                    Method::Error
                }
            }
        }
    }
}

impl From<Request> for Rpc {
    fn from(req: Request) -> Self {
        Rpc::Request(req)
    }
}

impl From<Response> for Rpc {
    fn from(resp: Response) -> Self {
        Rpc::Response(resp)
    }
}

impl TryFrom<&[u8]> for Rpc {
    type Error = Error;

    fn try_from(frame: &[u8]) -> Result<Self> {
        serde_json::from_slice(&frame).map_err(|e| {
            let (frame, suffix) = if frame.len() > 256 {
                (&frame[..256], "[snip]")
            } else {
                (&frame[..], "")
            };
            let frame = String::from_utf8_lossy(frame);

            V1Error::Json(format!("Invalid V1 message: {}\n{}{}", e, frame, suffix)).into()
        })
    }
}

impl TryFrom<Rpc> for framing::Frame {
    type Error = Error;
    /// Prepares a frame for serializing the specified message just in time (the message is
    /// treated as a `SerializablePayload`)
    fn try_from(m: Rpc) -> Result<Self> {
        Ok(framing::Frame::from_serializable_payload(m))
    }
}

impl TryFrom<framing::Frame> for Rpc {
    type Error = Error;

    fn try_from(frame: framing::Frame) -> Result<Self> {
        let payload = frame.into_inner();
        let payload = payload.into_bytes_mut()?;
        Self::try_from(&payload[..])
    }
}

impl FromStr for Rpc {
    type Err = Error;

    /// Any error is being converted into JSON parsing error
    #[inline]
    fn from_str(s: &str) -> Result<Self> {
        let x = serde_json::from_str(s)?;
        Ok(x)
    }
}

impl AnyPayload<Protocol> for Rpc {
    /// This will never get used as we don't do any handling on Rpc level. Since the RPC is also a
    /// SerializablePayload we have to provide a default implementation.
    ///
    fn serialize_to_writer(&self, writer: &mut dyn std::io::Write) -> Result<()> {
        serde_json::to_writer(writer, self).map_err(Into::into)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::v1::*;
    use bytes::BytesMut;
    use std::convert::TryInto;

    #[test]
    fn test_deserialize_serialize_request() {
        for &req in V1_TEST_REQUESTS {
            let deserialized_request =
                Rpc::try_from(req.as_bytes()).expect("BUG: Failed to deseriliaze request");

            let request_frame: framing::Frame = deserialized_request
                .try_into()
                .expect("BUG: Failed to serialize");
            let mut serialized_request = BytesMut::new();

            request_frame
                .serialize(&mut serialized_request)
                .expect("BUG: Cannot serialize frame");

            assert_eq!(
                req,
                std::str::from_utf8(&serialized_request[..])
                    .expect("BUG: UTF-8 from serialized frame"),
                "Rpc requests don't match"
            );
        }
    }

    #[test]
    fn test_deserialize_broken_request() {
        Rpc::try_from(MINING_BROKEN_REQ_JSON.as_bytes())
            .expect_err("BUG: Deserializing a broken request should've failed");
    }

    fn test_deserialize_response(serialized_response: &str, expected_rpc: Rpc) {
        let deserialized_response = Rpc::try_from(serialized_response.as_bytes())
            .expect("BUG: Cannot deserialize JSON request");

        assert_eq!(
            expected_rpc, deserialized_response,
            "Stratum responses don't match!"
        );
    }
    #[test]
    fn test_deserialize_ok_response() {
        test_deserialize_response(
            MINING_SUBSCRIBE_OK_RESULT_JSON,
            build_subscribe_ok_response_message(),
        );
    }

    #[test]
    fn test_deserialize_err_response() {
        test_deserialize_response(STRATUM_ERROR_JSON, build_stratum_err_response());
    }

    /// Helper function that runs the serialization test on arbitrary response
    fn test_serialize_response(response: Rpc, expected_serialized_response: &str) {
        let response_frame: framing::Frame = response.try_into().expect("BUG: Failed to serialize");
        let mut serialized_response = BytesMut::new();

        response_frame
            .serialize(&mut serialized_response)
            .expect("BUG: Cannot serialize frame");

        assert_eq!(
            expected_serialized_response,
            std::str::from_utf8(&serialized_response[..])
                .expect("BUG: UTF-8 from serialized frame"),
            "Serializing test request yields different results!"
        );
    }

    #[test]
    fn test_serialize_ok_response() {
        test_serialize_response(
            build_subscribe_ok_response_message(),
            MINING_SUBSCRIBE_OK_RESULT_JSON,
        );
    }

    /// Verifies correct implementation of `SerializablePayload` trait for `Rpc`
    #[test]
    fn test_serialize_err_response() {
        test_serialize_response(build_stratum_err_response(), STRATUM_ERROR_JSON);
    }
}
