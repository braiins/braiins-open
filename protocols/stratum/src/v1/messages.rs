//! This module provides factory for producing stratum messages
use failure::ResultExt;
use hex;
use std::str::FromStr;

use super::error::ErrorKind;
use super::rpc;

/// Compounds all data required for mining subscription
pub struct Subscribe {
    pub id: u32,
    pub agent_signature: String,
    pub extra_nonce1: Option<Vec<u8>>,
}

impl Into<rpc::Request> for Subscribe {
    fn into(self) -> rpc::Request {
        rpc::Request {
            id: self.id,
            method: rpc::Method::Subscribe,
            // TODO add enonce1
            params: vec![self.agent_signature.into(), self.id.into()],
        }
    }
}

/// Subscription response
/// TODO: Do we need to track any subscription ID's or anyhow validate those fields?
#[derive(Debug)]
pub struct SubscribeResponse {
    pub id: u32,
    pub difficulty_subscription_id: Option<String>,
    pub notify_subscription_id: Option<String>,
    pub extra_nonce1: Vec<u8>,
    pub extra_nonce2_size: usize,
}

impl FromStr for SubscribeResponse {
    type Err = crate::error::Error;

    /// SubscribeResponse deserialization is very picky about field consistency
    ///
    /// Any error is being converted into Subscription
    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let response: rpc::Response = serde_json::from_str(s).context("Parsing JSON failed")?;
        // Check for any reported error from the server and convert it into V1 error
        if let Some(err) = response.error {
            Err(crate::error::ErrorKind::V1(ErrorKind::Subscribe(format!(
                "Server error: {:?}",
                err
            ))))?
        }

        // Missing result in response means that server has provided invalid subscription
        // response
        let result = response
            .result
            .ok_or(crate::error::ErrorKind::V1(ErrorKind::Subscribe(
                "Missing result".into(),
            )))?;

        // Extract response ID or generate an error
        let id = response
            .id
            .ok_or(crate::error::ErrorKind::V1(ErrorKind::Subscribe(
                "Missing ID".into(),
            )))?;
        // Parse extra nonce 1
        let extra_nonce1 =
            result[1]
                .as_str()
                .ok_or(crate::error::ErrorKind::V1(ErrorKind::Subscribe(
                    "Missing extranonce 1".into(),
                )))?;
        let extra_nonce1 = hex::decode(extra_nonce1).context("Parsing extranonce 1 failed")?;

        // Get extra nonce 2 size, note that json value is u64, we will cast it to usize as there
        // is no risk of losing information
        let extra_nonce2_size =
            result[2]
                .as_u64()
                .ok_or(crate::error::ErrorKind::V1(ErrorKind::Subscribe(
                    "Wrong extranonce 2 size".into(),
                )))? as usize;

        Ok(SubscribeResponse {
            id: id.into(),
            extra_nonce1,
            difficulty_subscription_id: None,
            notify_subscription_id: None,
            extra_nonce2_size,
        })
    }
}
