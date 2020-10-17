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

use bytes::Bytes;
use std::convert::TryFrom;
use std::path::Path;

use ii_stratum::v2::{
    self,
    noise::{
        auth::{Certificate, StaticSecretKeyFormat},
        CompoundCodec, Responder,
    },
};
use tokio::{fs::File, io::AsyncReadExt, net::TcpStream};
use tokio_util::codec::{Decoder, Encoder, Framed};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("De-/Serialization of certificate or key failed: {0}")]
    KeySerializationError(String),

    #[error("IoError: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Error during noise initialization: {0}")]
    NoiseInitError(String),
}
pub type Result<T> = std::result::Result<T, Error>;

/// Security context is held by the server and provided to each (noise secured) connection so
/// that it can successfully perform the noise handshake and authenticate itself to the client
/// NOTE: this struct doesn't intentionally derive Debug to prevent leakage of the secure key
/// into log messages
pub struct SecurityContext {
    /// Serialized Signature noise message that contains the necessary part of the certificate for
    /// succesfully authenticating with the Initiator. We store it as Bytes as it will be shared
    /// to among all incoming connections
    signature_noise_message: Bytes,
    /// Static key pair that the server will use within the noise handshake
    static_key_pair: v2::noise::StaticKeypair,
}

impl SecurityContext {
    fn from_certificate_and_secret_key(
        certificate: v2::noise::auth::Certificate,
        secret_key: v2::noise::auth::StaticSecretKeyFormat,
    ) -> Result<Self> {
        let signature_noise_message = certificate
            .build_noise_message()
            .serialize_to_bytes_mut()
            .map_err(|e| Error::KeySerializationError(e.to_string()))?
            .freeze();
        // TODO secret key validation is currently not possible
        //let public_key = certificate.validate_secret_key(&secret_key)?;
        let static_key_pair = v2::noise::StaticKeypair {
            private: secret_key.into_inner(),
            public: certificate.public_key.into_inner(),
        };

        Ok(Self {
            signature_noise_message,
            static_key_pair,
        })
    }

    pub async fn read_from_file(certificate_file: &Path, secret_key_file: &Path) -> Result<Self> {
        let mut cert_file = File::open(certificate_file).await?;
        let mut key_file = File::open(secret_key_file).await?;

        let mut cert_string = String::new();
        cert_file.read_to_string(&mut cert_string).await?;
        let mut key_string = String::new();
        key_file.read_to_string(&mut key_string).await?;

        let cert = Certificate::try_from(cert_string)
            .map_err(|e| Error::KeySerializationError(e.to_string()))?;

        let key = StaticSecretKeyFormat::try_from(key_string)
            .map_err(|e| Error::KeySerializationError(e.to_string()))?;

        SecurityContext::from_certificate_and_secret_key(cert, key)
    }

    fn build_responder(&self) -> Responder {
        use v2::noise::negotiation::EncryptionAlgorithm::{ChaChaPoly, AESGCM};
        Responder::new(
            &self.static_key_pair,
            self.signature_noise_message.clone(),
            vec![AESGCM, ChaChaPoly],
        )
    }

    pub async fn build_framed_tcp<C, F>(
        &self,
        tcp_stream: TcpStream,
    ) -> Result<Framed<TcpStream, CompoundCodec<C>>>
    where
        C: Default + Decoder + Encoder<F>,
        <C as tokio_util::codec::Encoder<F>>::Error: Into<ii_stratum::error::Error>,
    {
        let responder = self.build_responder();
        responder
            .accept_with_codec(tcp_stream, |noise_codec| {
                CompoundCodec::<C>::new(Some(noise_codec))
            })
            .await
            .map_err(|e| Error::NoiseInitError(e.to_string()))
    }
}
