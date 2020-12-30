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

//! This module provides functions for a negotiation step before the noise handshake phase.
//! Currently used to negotiate the encryption algorithm that will be used during the snow
//! communication.

use crate::v2::types::*;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use snow::{params::NoiseParams, Builder};

/// Builds noise params given a certain EncryptionAlgorithm
pub struct NoiseParamsBuilder {
    params: NoiseParams,
}

impl NoiseParamsBuilder {
    pub fn new(chosen_algorithm: EncryptionAlgorithm) -> Self {
        Self {
            params: format!("Noise_NX_25519_{:?}_BLAKE2s", chosen_algorithm)
                .parse()
                .expect("BUG: cannot parse noise parameters"),
        }
    }

    pub fn get_builder<'a>(self) -> Builder<'a> {
        // Initialize our initiator using a builder.
        Builder::new(self.params)
    }
}

const MAGIC: u32 = u32::from_le_bytes(*b"STR2");

/// Negotiation prologue; if initiator and responder prologue don't match the entire negotiation
/// fails.
/// Made of the initiator message (the list of algorithms) and the responder message (the
/// algorithm chosen). If both of them are None, no negotiation happened.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Prologue {
    pub initiator_msg: Option<NegotiationMessage>,
    pub responder_msg: Option<NegotiationMessage>,
}

#[allow(clippy::new_without_default)]
impl Prologue {
    pub fn new() -> Self {
        Self {
            initiator_msg: None,
            responder_msg: None,
        }
    }
}

#[derive(Serialize_repr, Deserialize_repr, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum EncryptionAlgorithm {
    AESGCM = u32::from_le_bytes(*b"AESG"),
    ChaChaPoly = u32::from_le_bytes(*b"CHCH"),
}

/// Message used for negotiation of the encryption algorithm
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NegotiationMessage {
    magic: u32,
    pub encryption_algos: Seq0_255<EncryptionAlgorithm>,
}

impl NegotiationMessage {
    pub fn new(encryption_algos: Vec<EncryptionAlgorithm>) -> Self {
        Self {
            magic: MAGIC,
            encryption_algos: Seq0_255::try_from(encryption_algos)
                .expect("BUG: cannot convert EncryptionAlgorithm vector"),
        }
    }
}

/// Holds encryption negotiation params, such as the Prologue and the algorithm chosen.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EncryptionNegotiation {
    pub chosen_algorithm: EncryptionAlgorithm,
    pub prologue: Prologue,
}

impl EncryptionNegotiation {
    pub fn new(prologue: Prologue, chosen_algorithm: EncryptionAlgorithm) -> Self {
        EncryptionNegotiation {
            chosen_algorithm,
            prologue,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::v2;

    #[test]
    fn test_negotiation_message() {
        let negotiation_message = NegotiationMessage::new(vec![
            EncryptionAlgorithm::AESGCM,
            EncryptionAlgorithm::ChaChaPoly,
        ]);
        let mut serialized_negotiation_message = Vec::new();
        serialized_negotiation_message.extend_from_slice(b"STR2"); // magic bytes
        serialized_negotiation_message.push(2); // number of algorithms provided
        serialized_negotiation_message.extend_from_slice(b"AESG"); // AESGCM
        serialized_negotiation_message.extend_from_slice(b"CHCH"); // ChaChaPoly

        let v2_serialized_negotiation_message = v2::serialization::to_vec(&negotiation_message)
            .expect("BUG: can't serialize negotiation_message");
        assert_eq!(
            serialized_negotiation_message,
            v2_serialized_negotiation_message
        );

        let v2_deserialized_negotiation_message =
            v2::serialization::from_slice(&v2_serialized_negotiation_message)
                .expect("BUG: can't deserialize negotiation_message");
        assert_eq!(negotiation_message, v2_deserialized_negotiation_message);
    }
}
