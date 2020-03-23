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

//! All formats that need to be persisted as physical files, too

use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::fmt;
use std::time::SystemTime;

use super::{SignatureNoiseMessage, SignedPart, SignedPartHeader};
use crate::error::{Error, ErrorKind, Result};

/// Generates implementation of conversions from/to Base58 encoding that we use for representing
/// keys, signatures etc.
macro_rules! impl_encoding_conversion {
    ($full_name:tt, $ed25519_struct:tt) => {
        // NOTE: $request and $handler_fn need to be tt because of https://github.com/dtolnay/async-trait/issues/46

        impl $full_name {
            pub fn new(inner: ed25519_dalek::$ed25519_struct) -> Self {
                Self { inner }
            }

            pub fn into_inner(self) -> ed25519_dalek::$ed25519_struct {
                self.inner
            }
        }

        impl TryFrom<String> for $full_name {
            type Error = Error;

            fn try_from(value: String) -> Result<Self> {
                let bytes = bs58::decode(value).into_vec()?;
                Ok(Self::new(ed25519_dalek::$ed25519_struct::from_bytes(
                    &bytes,
                )?))
            }
        }

        impl From<$full_name> for String {
            fn from(value: $full_name) -> Self {
                bs58::encode(&value.inner.to_bytes()[..]).into_string()
            }
        }

        impl fmt::Display for $full_name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", String::from(self.clone()))
            }
        }
    };
}

/// Helper that ensures serialization of the `ed25519::PublicKey` into a prefered encoding
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
#[serde(into = "String", try_from = "String")]
struct EncodedPublicKey {
    inner: ed25519_dalek::PublicKey,
}

impl_encoding_conversion!(EncodedPublicKey, PublicKey);

/// Publickey that can represents itself in base64 encoding
#[derive(Serialize, Deserialize, Debug)]
#[serde(into = "String", try_from = "String")]
pub struct EncodedSecretKey {
    inner: ed25519_dalek::SecretKey,
}

impl_encoding_conversion!(EncodedSecretKey, SecretKey);

/// Required by serde's Serialize trait, `ed25519_dalek::SecretKey` doesn't support
/// clone
impl Clone for EncodedSecretKey {
    fn clone(&self) -> Self {
        // Cloning the secret key should never fail and is considered bug as the original private
        // key is correct
        Self::new(
            ed25519_dalek::SecretKey::from_bytes(self.inner.as_bytes())
                .expect("BUG: cannot clone secret key"),
        )
    }
}

/// Signature that can be represented in encoded form
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
#[serde(into = "String", try_from = "String")]
pub struct EncodedSignature {
    inner: ed25519_dalek::Signature,
}

impl_encoding_conversion!(EncodedSignature, Signature);

/// Public key intended e.g. for json serialization where the 'inner' field has an explicit
/// name denoting the keytype
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct Ed25519PublicKeyFormat {
    #[serde(rename = "ed25519_public_key")]
    inner: EncodedPublicKey,
}

impl Ed25519PublicKeyFormat {
    pub fn new(inner: ed25519_dalek::PublicKey) -> Self {
        Self {
            inner: EncodedPublicKey::new(inner),
        }
    }
    pub fn into_inner(self) -> ed25519_dalek::PublicKey {
        self.inner.into_inner()
    }
}

impl TryFrom<String> for Ed25519PublicKeyFormat {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        serde_json::from_str(value.as_str()).map_err(Into::into)
    }
}

/// Helper serializer into string
impl TryFrom<Ed25519PublicKeyFormat> for String {
    type Error = Error;
    fn try_from(value: Ed25519PublicKeyFormat) -> Result<String> {
        serde_json::to_string_pretty(&value).map_err(Into::into)
    }
}

/// Secret key intended e.g. for json serialization where the 'inner' field has an explicit
/// name denoting the keytype
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Ed25519SecretKeyFormat {
    #[serde(rename = "ed25519_secret_key")]
    inner: EncodedSecretKey,
}

impl Ed25519SecretKeyFormat {
    pub fn new(inner: ed25519_dalek::SecretKey) -> Self {
        Self {
            inner: EncodedSecretKey::new(inner),
        }
    }

    pub fn into_inner(self) -> ed25519_dalek::SecretKey {
        self.inner.into_inner()
    }
}

impl TryFrom<String> for Ed25519SecretKeyFormat {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        serde_json::from_str(value.as_str()).map_err(Into::into)
    }
}

/// Helper serializer into string
impl TryFrom<Ed25519SecretKeyFormat> for String {
    type Error = Error;
    fn try_from(value: Ed25519SecretKeyFormat) -> Result<String> {
        serde_json::to_string_pretty(&value).map_err(Into::into)
    }
}

/// Certificate is intended to be serialized and deserialized from/into a file and loaded on the
/// stratum server.
/// Second use of the certificate is to build it from `SignatureNoiseMessage` and check its
/// validity
#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct Certificate {
    signed_part_header: SignedPartHeader,
    pubkey: Ed25519PublicKeyFormat,
    signature: EncodedSignature,
}

impl Certificate {
    pub fn new(signed_part: SignedPart, signature: ed25519_dalek::Signature) -> Self {
        Self {
            signed_part_header: signed_part.header,
            pubkey: Ed25519PublicKeyFormat::new(signed_part.pubkey),
            signature: EncodedSignature::new(signature),
        }
    }

    /// TODO implement unit test
    pub fn validate_secret_key(
        &self,
        secret_key: &ed25519_dalek::SecretKey,
    ) -> Result<ed25519_dalek::PublicKey> {
        let public_key = Ed25519PublicKeyFormat::new(ed25519_dalek::PublicKey::from(secret_key));

        match public_key == self.pubkey {
            true => Ok(public_key.into_inner()),
            false => Err(ErrorKind::Noise(format!(
                "Invalid certificate: public key({}) doesn't match public key({}) generated from \
                 secret key",
                public_key.inner, self.pubkey.inner,
            ))
            .into()),
        }
    }

    /// See  https://docs.rs/ed25519-dalek/1.0.0-pre.3/ed25519_dalek/struct.PublicKey.html on
    /// details for the strict verification
    pub fn validate(&self, authority_pubkey: &ed25519_dalek::PublicKey) -> Result<()> {
        let signed_part = SignedPart::new(
            self.signed_part_header.clone(),
            self.pubkey.clone().into_inner(),
        );
        signed_part.verify_with(authority_pubkey, &self.signature.inner)?;
        signed_part.verify_expiration(SystemTime::now())
    }

    pub fn from_noise_message(
        signature_noise_message: SignatureNoiseMessage,
        pubkey: ed25519_dalek::PublicKey,
    ) -> Self {
        Self::new(
            SignedPart::new(signature_noise_message.header, pubkey),
            signature_noise_message.signature,
        )
    }

    pub fn build_noise_message(&self) -> SignatureNoiseMessage {
        SignatureNoiseMessage {
            header: self.signed_part_header.clone(),
            signature: self.signature.inner.clone(),
        }
    }
}

impl TryFrom<String> for Certificate {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        serde_json::from_str(value.as_str()).map_err(Into::into)
    }
}

impl TryFrom<Certificate> for String {
    type Error = Error;
    fn try_from(value: Certificate) -> Result<String> {
        serde_json::to_string_pretty(&value).map_err(Into::into)
    }
}

#[cfg(test)]
pub mod test {
    use super::super::test::build_test_signed_part_and_auth;
    use super::*;

    #[test]
    fn certificate_validate() {
        let (signed_part, authority_keypair, signature) = build_test_signed_part_and_auth();
        let certificate = Certificate::new(signed_part, signature);

        certificate
            .validate(&authority_keypair.public)
            .expect("BUG: Certificate not valid!");
    }

    #[test]
    fn certificate_serialization() {
        let (signed_part, _authority_keypair, signature) = build_test_signed_part_and_auth();
        let certificate = Certificate::new(signed_part, signature);

        let serialized_cert =
            serde_json::to_string(&certificate).expect("BUG: cannot serialize certificate");
        let deserialized_cert = serde_json::from_str(serialized_cert.as_str())
            .expect("BUG: cannot deserialized certificate");

        assert_eq!(certificate, deserialized_cert, "Certificates don't match!");
    }
}
