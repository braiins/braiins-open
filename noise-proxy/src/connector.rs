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

//! Implementation of noise initiator

use ii_stratum::v2;
use ii_stratum::v2::noise::{auth, negotiation, CompoundCodec};
use tokio::net::TcpStream;
use tokio_util::codec::{Decoder, Encoder, Framed};

#[derive(thiserror::Error, Debug)]
#[error("Failed to construct connector: {0}")]
pub struct Error(#[from] ii_stratum::error::Error);

/// Struct that is able to initiate noise encrypted connection to upstream
/// using some provided L2 codec (stratum v1 or stratum v2)
pub struct Connector {
    /// Upstream authority public key that will be used to authenticate the endpoint
    upstream_authority_public_key: v2::noise::AuthorityPublicKey,
}

impl Connector {
    pub fn with_key(key: auth::EncodedEd25519PublicKey) -> Self {
        Self {
            upstream_authority_public_key: key.into_inner(),
        }
    }

    /// Build framed tcp stream using l2-codec `C` producing frames `F`
    pub async fn connect<C, F>(
        self,
        connection: TcpStream,
    ) -> Result<Framed<TcpStream, CompoundCodec<C>>, Error>
    where
        C: Default + Decoder + Encoder<F>,
        <C as tokio_util::codec::Encoder<F>>::Error: Into<ii_stratum::error::Error>,
    {
        let noise_initiator = ii_stratum::v2::noise::Initiator::new(
            self.upstream_authority_public_key,
            vec![negotiation::EncryptionAlgorithm::AESGCM],
        );
        trace!(
            "Stratum V2 noise connector: {:?}, {:?}",
            connection,
            noise_initiator
        );
        noise_initiator
            .connect_with_codec(connection, |noise_codec| {
                CompoundCodec::<C>::new(Some(noise_codec))
            })
            .await
            .map_err(Into::into)
    }
}
