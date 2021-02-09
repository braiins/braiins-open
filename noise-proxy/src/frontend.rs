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

use std::fmt;

use std::convert::TryFrom;
use std::path::Path;
use std::time::{Duration, SystemTime};

use ii_stratum::v2::{
    self,
    noise::{
        auth::{Certificate, EncodedEd25519PublicKey, StaticSecretKeyFormat},
        negotiation::EncryptionAlgorithm::{ChaChaPoly, AESGCM},
        CompoundCodec, Responder, StaticKeypair,
    },
};
use tokio::{fs::File, io::AsyncReadExt, net::TcpStream};
use tokio_util::codec::{Decoder, Encoder, Framed, FramedParts};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("De-/Serialization of certificate or key failed: {0}")]
    KeySerializationError(String),

    #[error("IoError: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Error during noise initialization: {0}")]
    NoiseInitError(String),

    #[error("Noise certificate has expired, contact Braiins support")]
    TimeValidationError,
}
pub type Result<T> = std::result::Result<T, Error>;

/// Security context is held by the server and provided to each (noise secured) connection so
/// that it can successfully perform the noise handshake and authenticate itself to the client
/// NOTE: this struct intentionally implements Debug manually to prevent leakage of the secure key
/// into log messages
pub struct SecurityContext {
    /// Serialized Signature noise message that contains the necessary part of the certificate for
    /// succesfully authenticating with the Initiator. We store it as Bytes as it will be shared
    /// to among all incoming connections
    certificate: v2::noise::auth::Certificate,
    secret_key: v2::noise::auth::StaticSecretKeyFormat,
}

// Show certificate authority public key and expiry timestamp
impl fmt::Debug for SecurityContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let certificate_authority = self.authority_pubkey();
        let expiry_timestamp = self.certificate.validate(SystemTime::now).map_or_else(
            |_| "certificate is invalid".to_owned(),
            |t| format!("{:?}", t),
        );
        f.debug_struct("SecurityContext")
            .field("certificate_authority", &certificate_authority.to_string())
            .field("certificate_expiry", &expiry_timestamp)
            .finish()
    }
}

impl SecurityContext {
    fn from_certificate_and_secret_key(
        certificate: v2::noise::auth::Certificate,
        secret_key: v2::noise::auth::StaticSecretKeyFormat,
    ) -> Result<Self> {
        certificate
            .validate(SystemTime::now)
            .map_err(|e| Error::NoiseInitError(e.to_string()))?;
        // TODO secret key validation is currently not possible
        // let public_key = certificate.validate_secret_key(&secret_key)?;
        Ok(Self {
            certificate,
            secret_key,
        })
    }

    fn authority_pubkey(&self) -> EncodedEd25519PublicKey {
        EncodedEd25519PublicKey::new(self.certificate.authority_public_key.clone().into_inner())
    }

    /// Returns remaining time of certificate validity or error if the certificate has expired
    pub fn validate_by_time<FN>(&self, get_current_time: FN) -> Result<Duration>
    where
        FN: FnOnce() -> SystemTime,
    {
        self.certificate
            .validate(get_current_time)
            .map_err(|_| Error::TimeValidationError)
            .and_then(|expiry_time| {
                expiry_time
                    .duration_since(SystemTime::now())
                    .map_err(|_| Error::TimeValidationError)
            })
    }

    pub fn read_from_strings(certificate: String, secret_key: String) -> Result<Self> {
        let cert = Certificate::try_from(certificate)
            .map_err(|e| Error::KeySerializationError(e.to_string()))?;

        let key = StaticSecretKeyFormat::try_from(secret_key)
            .map_err(|e| Error::KeySerializationError(e.to_string()))?;

        SecurityContext::from_certificate_and_secret_key(cert, key)
    }

    pub async fn read_from_file(certificate_file: &Path, secret_key_file: &Path) -> Result<Self> {
        let mut cert_file = File::open(certificate_file).await?;
        let mut key_file = File::open(secret_key_file).await?;

        let mut cert_string = String::new();
        cert_file.read_to_string(&mut cert_string).await?;
        let mut key_string = String::new();
        key_file.read_to_string(&mut key_string).await?;

        Self::read_from_strings(cert_string, key_string)
    }

    pub async fn build_framed_tcp<C, F>(
        &self,
        tcp_stream: TcpStream,
    ) -> Result<Framed<TcpStream, CompoundCodec<C>>>
    where
        C: Default + Decoder + Encoder<F>,
        <C as tokio_util::codec::Encoder<F>>::Error: Into<ii_stratum::error::Error>,
    {
        // TODO: consolidate the two functions build_framed_tcp and build_framed_tcp_from_parts
        // Note that Responder construction cannot be moved to a separate function because
        // it contains reference to a static_key_pair
        let signature_noise_message = self
            .certificate
            .build_noise_message()
            .serialize_to_bytes_mut()
            .map_err(|e| Error::KeySerializationError(e.to_string()))?
            .freeze();
        let static_key_pair = StaticKeypair {
            private: self.secret_key.clone().into_inner(),
            public: self.certificate.public_key.clone().into_inner(),
        };
        let responder = Responder::new(
            &static_key_pair,
            signature_noise_message,
            vec![AESGCM, ChaChaPoly],
        );
        responder
            .accept_with_codec(tcp_stream, |noise_codec| {
                CompoundCodec::<C>::new(Some(noise_codec))
            })
            .await
            .map_err(|e| Error::NoiseInitError(e.to_string()))
    }

    pub async fn build_framed_tcp_from_parts<C, F, P>(
        &self,
        parts: P,
    ) -> Result<Framed<TcpStream, CompoundCodec<C>>>
    where
        C: Default + Decoder + Encoder<F>,
        <C as tokio_util::codec::Encoder<F>>::Error: Into<ii_stratum::error::Error>,
        P: Into<FramedParts<TcpStream, v2::noise::Codec>>,
    {
        let signature_noise_message = self
            .certificate
            .build_noise_message()
            .serialize_to_bytes_mut()
            .map_err(|e| Error::KeySerializationError(e.to_string()))?
            .freeze();
        let static_key_pair = StaticKeypair {
            private: self.secret_key.clone().into_inner(),
            public: self.certificate.public_key.clone().into_inner(),
        };
        let responder = Responder::new(
            &static_key_pair,
            signature_noise_message,
            vec![AESGCM, ChaChaPoly],
        );
        responder
            // TODO this needs refactoring there is no point of passing the codec
            // type, we should be able to run noise just with anything that
            // implements AsyncRead/AsyncWrite
            .accept_parts_with_codec(parts, |noise_codec| {
                CompoundCodec::<C>::new(Some(noise_codec))
            })
            .await
            .map_err(|e| Error::NoiseInitError(e.to_string()))
    }
}
