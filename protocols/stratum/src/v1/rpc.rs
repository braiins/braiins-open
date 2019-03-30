//! This module defines all Stratum V1 messages
//!
//! TODO: implement
use serde;
use serde::{Deserialize, Serialize};
use serde_json;
use serde_json::Value;
//use serde_tuple::*;
//use serde_tuple::{Deserialize_tuple, Serialize_tuple};
use crate::error;
use failure::ResultExt;
use std::str::FromStr;

pub const MAX_MESSAGE_LENGTH: usize = 16384;

/// All recognized methods of the V1 protocol have the 'mining.' prefix in json.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum Method {
    #[serde(rename = "mining.subscribe")]
    Subscribe,
    #[serde(rename = "mining.authorize")]
    Authorize,
    #[serde(rename = "mining.configure")]
    Configure,
    #[serde(rename = "mining.submit")]
    Submit,
    /// Catch all variant
    #[serde(other)]
    Unknown,
}

/// Generic stratum request
#[derive(Serialize, Deserialize, Debug)]
pub struct Request {
    /// Mandatory identifier for pairing with the response
    pub id: u32,
    /// Protocol method to be 'called'
    pub method: Method,
    /// Vector of method parameters
    pub params: Vec<Value>,
}

impl Request {
    pub fn to_json_string(&self) -> serde_json::error::Result<String> {
        serde_json::to_string(self)
    }
}

/// Generic stratum response
///
/// The response maybe optionally paired via 'id' with original request. Empty ID
/// represents a notification.
#[derive(Serialize, Deserialize, Debug)]
pub struct Response {
    /// Original request pairing identifier
    pub id: Option<u32>,
    /// Successful responses/notification have a result
    pub result: Option<Value>,
    /// Error responses provide details via this field
    pub error: Option<StratumError>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum Rpc {
    RpcRequest(Request),
    RpcResponse(Response),
}

///
pub trait ReceivedRpc {
    type Item;
    type Error;

    fn handle_request(&mut self) -> Result<Self::Item, Self::Error>;
    fn handle_response(&mut self) -> Result<Self::Item, Self::Error>;
}

impl FromStr for Rpc {
    type Err = crate::error::Error;

    /// Any error is being converted into Subscription
    #[inline]
    fn from_str(s: &str) -> Result<Self, crate::error::Error> {
        let x = serde_json::from_str(s).context("Parsing JSON failed")?;
        Ok(x)
    }
}

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

#[derive(Serialize, Deserialize, Debug)]
pub struct StratumError(i32, String, Option<String>);
// TODO this currently doesn't compile. Investigate serde_tuple issue.
//#[derive(Serialize_tuple, Deserialize_tuple, Debug)]
//pub struct StratumError {
//    pub code: i32,
//    pub msg: String,
//    pub trace_back: Option<String>,
//}

#[cfg(test)]
mod test {
    use super::*;
    // Testing request in a dense form without any spaces
    const STRATUM_JSON_REQ: &str = concat!(
        r#"{"id":1,"method":"mining.subscribe","#,
        r#""params":["Braiins OS",null,"stratum.slushpool.com","3333"]}"#
    );
    // Testing a random broken request
    const STRATUM_JSON_BROKEN_REQ: &str = concat!(
        r#"{"id":1,"method":"mining.none_existing","#,
        r#""params":["10","12"]}"#
    );

    // Testing success response in a dense form without any spaces
    const STRATUM_JSON_OK_RESP: &str = concat!(
        r#"{"id":1,"result":[[["mining.set_difficulty","1"],"#,
        r#"["mining.notify","1"]],"01650f001f25ea",4],"error":null}"#
    );

    // Testing error response in a dense form without any spaces
    const STRATUM_JSON_ERR_RESP: &str =
        r#"{"id":1,"result":null,"error":[21,"Job not found",null]}"#;

    #[test]
    fn test_deserialize_request() {
        let deserialized: Request = serde_json::from_str(&STRATUM_JSON_REQ).unwrap();
    }

    #[test]
    fn test_deserialize_broken_request() {
        let deserialized: Request = serde_json::from_str(&STRATUM_JSON_BROKEN_REQ).unwrap();
        assert_eq!(
            Method::Unknown,
            deserialized.method,
            "Unknown method not detected!"
        );
    }

    #[test]
    fn test_serialize_request() {
        let req = Request {
            id: 1,
            method: Method::Subscribe,
            params: vec![
                "Braiins OS".into(),
                Value::Null,
                "stratum.slushpool.com".into(),
                "3333".into(),
            ],
        };

        let rpc = Rpc::RpcRequest(req);
        let serialized_rpc = serde_json::to_string(&rpc).expect("Failed to serialize RPC");

        assert_eq!(
            serialized_rpc, STRATUM_JSON_REQ,
            "Serializing test yields different results!",
        );
    }

    #[test]
    fn test_deserialize_ok_response() {
        let deserialized: Rpc = serde_json::from_str(&STRATUM_JSON_OK_RESP).unwrap();

        match deserialized {
            Rpc::RpcResponse(response) => assert!(
                response.error.is_none(),
                "Error should be empty: {:?}",
                response
            ),
            other => assert!(false, "Expected Response, got: {:?}", other),
        }
    }

    #[test]
    fn test_serialize_ok_response() {
        let p1: Vec<Value> = vec!["mining.set_difficulty".into(), "1".into()];
        let p2: Vec<Value> = vec!["mining.notify".into(), "1".into()];
        let all_subscriptions: Vec<Value> = vec![p1.into(), p2.into()];

        let resp = Response {
            id: Some(1),
            result: Some(Value::Array(vec![
                all_subscriptions.into(),
                "01650f001f25ea".into(),
                4.into(),
            ])),
            error: None,
        };

        let rpc = Rpc::RpcResponse(resp);
        let serialized = serde_json::to_string(&rpc).expect("Failed to serialize");

        assert_eq!(
            STRATUM_JSON_OK_RESP, serialized,
            "Serializing test request yields different results!"
        );
    }

    #[test]
    fn test_deserialize_err_response() {
        let deserialized: Rpc = serde_json::from_str(&STRATUM_JSON_ERR_RESP).unwrap();
        match deserialized {
            Rpc::RpcResponse(response) => assert!(
                response.error.is_some(),
                "Error should be defined: {:?}",
                response
            ),
            other => assert!(false, "Expected Response, got: {:?}", other),
        }
    }

    #[test]
    fn test_serialize_err_response() {
        let resp = Response {
            id: Some(1),
            result: None,
            error: Some(StratumError(21, "Job not found".into(), None)),
        };
        let rpc = Rpc::RpcResponse(resp);
        let serialized = serde_json::to_string(&rpc).expect("Failed to serialize");

        assert_eq!(
            STRATUM_JSON_ERR_RESP, serialized,
            "Serializing test request yields different results!"
        );
    }
}
