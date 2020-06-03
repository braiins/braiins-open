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

//! Noise protocol codec implementation that takes care of framing

use bytes::BytesMut;
use tokio_util::codec::length_delimited::{self, LengthDelimitedCodec};
use tokio_util::codec::{Decoder, Encoder};

use crate::error::Error;

use super::TransportMode;

/// State of the noise codec
#[derive(Debug)]
enum State {
    /// Handshake mode where codec is negotiating keys
    HandShake,
    /// Transport mode where AEAD is fully operational. The `TransportMode` object in this variant
    /// as able to perform encryption and decryption resp.
    Transport(TransportMode),
}

/// Noise codec compound object that stacks length delimited codec and stratum codec
#[derive(Debug)]
pub struct Codec {
    /// Codec noise messages have simple length delimited framing
    codec: LengthDelimitedCodec,
    /// Describes mode of operation of the codec
    state: State,
}

impl Codec {
    const LENGTH_FIELD_OFFSET: usize = 0;
    const LENGTH_FIELD_LENGTH: usize = 2;

    /// Consume the `transport_mode` and set it as the current mode of the codec
    pub fn set_transport_mode(&mut self, transport_mode: TransportMode) {
        if let State::Transport(_) = &self.state {
            panic!("BUG: codec is already in transport mode!");
        }
        self.state = State::Transport(transport_mode);
    }

    pub fn is_in_transport_mode(&self) -> bool {
        match self.state {
            State::Transport(_) => true,
            _ => false,
        }
    }
}

impl Decoder for Codec {
    type Item = BytesMut;
    type Error = Error;

    fn decode(
        &mut self,
        src: &mut BytesMut,
    ) -> std::result::Result<Option<Self::Item>, Self::Error> {
        let noise_msg: Option<BytesMut> = self.codec.decode(src)?;
        let payload = match &mut self.state {
            State::HandShake => noise_msg,
            State::Transport(transport_mode) => match noise_msg {
                Some(msg) => {
                    let mut decrypted_msg = BytesMut::new();
                    transport_mode.read(msg, &mut decrypted_msg)?;
                    Some(decrypted_msg)
                }
                None => None,
            },
        };
        Ok(payload)
    }
}

impl Encoder<BytesMut> for Codec {
    type Error = Error;

    fn encode(
        &mut self,
        item: BytesMut,
        dst: &mut BytesMut,
    ) -> std::result::Result<(), Self::Error> {
        let payload = match &mut self.state {
            State::HandShake => item,
            State::Transport(transport_mode) => {
                assert!(
                    item.len() <= super::MAX_PAYLOAD_SIZE,
                    "TODO: noise transport doesn't currently support messages bigger than {} \
                     (requested: {})",
                    super::MAX_PAYLOAD_SIZE,
                    item.len()
                );
                let mut encrypted_payload = BytesMut::new();
                transport_mode.write(item, &mut encrypted_payload)?;
                encrypted_payload
            }
        };
        self.codec.encode(payload.freeze(), dst).map_err(Into::into)
    }
}

impl Default for Codec {
    fn default() -> Self {
        // TODO: limit frame size with max_frame_length() ?
        Codec {
            codec: length_delimited::Builder::new()
                .little_endian()
                .length_field_offset(Self::LENGTH_FIELD_OFFSET)
                .length_field_length(Self::LENGTH_FIELD_LENGTH)
                .new_codec(),
            state: State::HandShake,
        }
    }
}

/// This codec allows stacking of:
/// - noise codec
/// - user specified codec
/// on top of each other and pumps data between both layers.
///
/// Note, that the `noise_codec` is an optional field. If omitted, `CompoundCodec` transparently
/// forward date to `l2_codec`
///
/// TODO: this functionality will not be needed once we are able to convert a `Framed`
/// Stream/Sink instance into an AsyncRead/AsyncWrite instance and feed it to another Framed
/// along with a new codec
#[derive(Debug)]
pub struct CompoundCodec<U> {
    /// Optional noise codec that handles encryption/decryption of messages
    noise_codec: Option<Codec>,
    /// User specified codec that is provided with decrypted data
    l2_codec: U,
}

impl<U> CompoundCodec<U>
where
    U: Default,
{
    /// TODO add l2_codec as parameter
    pub fn new(noise_codec: Option<Codec>) -> Self {
        if let Some(codec) = noise_codec.as_ref() {
            assert!(
                codec.is_in_transport_mode(),
                "BUG: noise codec is not in tranport mode, cannot build CompoundCodec!"
            )
        }
        Self {
            noise_codec,
            l2_codec: U::default(),
        }
    }
}

/// Default is required e.g. by ii_wire::Framing, TODO: consider refactoring ii-wire to drop this
/// constraint
impl<U> Default for CompoundCodec<U>
where
    U: Default,
{
    fn default() -> Self {
        Self::new(None)
    }
}

impl<E, F, U> Decoder for CompoundCodec<U>
where
    E: Into<Error> + From<std::io::Error>,
    U: Decoder<Item = F, Error = E>,
{
    type Item = F;
    type Error = Error;

    fn decode(
        &mut self,
        src: &mut BytesMut,
    ) -> std::result::Result<Option<Self::Item>, Self::Error> {
        let frame_result = match self.noise_codec {
            Some(ref mut noise_codec) => noise_codec
                .decode(src)?
                .and_then(|mut noise_bytes| self.l2_codec.decode(&mut noise_bytes).transpose())
                // If codec has performed decoding we have received Option<Result<>> as
                // an output of the previous transpose (so that it's compatible with and_then).
                // Now perform yet another transpose to be compatible with return type
                .transpose(),
            None => self.l2_codec.decode(src),
        };
        // TODO not sure, why the compiler cannot see that there actually is From<E> for Error
        // that the Into has been tailored from.
        frame_result.map_err(Into::into)
    }
}

/// Encoder that delegates serialization of frame `F` to level 2 codec and then optionally runs
/// the resulting bytes through the noise codec
impl<E, F, U> Encoder<F> for CompoundCodec<U>
where
    E: Into<Error> + From<std::io::Error>,
    U: Encoder<F, Error = E>,
{
    type Error = Error;

    fn encode(&mut self, item: F, dst: &mut BytesMut) -> std::result::Result<(), Self::Error> {
        let mut l2_encoded_frame = BytesMut::new();
        self.l2_codec
            .encode(item, &mut l2_encoded_frame)
            .map_err(Into::into)?;

        match self.noise_codec {
            Some(ref mut noise_codec) => noise_codec.encode(l2_encoded_frame, dst)?,
            None => dst.unsplit(l2_encoded_frame),
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::v2;

    /// Verify that we can encode/decode noise frames. Use dummy payload (non-noise)
    #[test]
    fn test_noise_codec_in_handshake_state() {
        let mut codec = Codec::default();
        let mut expected_frame = BytesMut::new();
        expected_frame.extend_from_slice(&[1, 2, 3, 4]);

        let mut buffer = BytesMut::new();
        codec
            .encode(expected_frame.clone(), &mut buffer)
            .expect("Codec failed to encode message");

        let decoded_frame = codec
            .decode(&mut buffer)
            .expect("Codec failed to decode message")
            .expect("Incomplete message");

        assert_eq!(
            expected_frame, decoded_frame,
            "Expected ({:x?}) and decoded ({:x?}) frames don't match",
            expected_frame, decoded_frame
        );
    }

    #[test]
    fn test_noise_codec_in_transport_state() {
        let mut initiator_codec = Codec::default();
        let mut responder_codec = Codec::default();

        let (initiator_transport_mode, responder_transport_mode) =
            super::super::test::perform_handshake();

        initiator_codec.set_transport_mode(initiator_transport_mode);
        responder_codec.set_transport_mode(responder_transport_mode);

        let mut expected_frame = BytesMut::new();
        expected_frame.extend_from_slice(&[1, 2, 3, 4]);

        let mut buffer = BytesMut::new();
        initiator_codec
            .encode(expected_frame.clone(), &mut buffer)
            .expect("BUG: Initiator codec failed to encode message");

        let decoded_frame = responder_codec
            .decode(&mut buffer)
            .expect("BUG: Responder codec failed to decode message")
            .expect("BUG: Responder coded provided incomplete message");

        assert_eq!(
            expected_frame, decoded_frame,
            "Expected ({:x?}) and decoded ({:x?}) frames don't match",
            expected_frame, decoded_frame
        );
    }

    /// Attempt to build a V2 CompoundCodec that is still in handshake mode (=contains
    /// no noise transport) must result in panic
    #[test]
    #[should_panic]
    fn bug_on_noise_codec_in_handshake_mode() {
        CompoundCodec::<v2::framing::codec::Codec>::new(Some(Codec::default()));
    }

    /// Helper that runs 2 compound codec instances against each other with noise enabled.
    fn run_compound_codec_with_noise(payload: BytesMut) {
        let mut initiator_noise_codec = Codec::default();
        let mut responder_noise_codec = Codec::default();

        let (initiator_transport_mode, responder_transport_mode) =
            super::super::test::perform_handshake();

        initiator_noise_codec.set_transport_mode(initiator_transport_mode);
        responder_noise_codec.set_transport_mode(responder_transport_mode);

        let mut initiator_codec =
            CompoundCodec::<v2::framing::codec::Codec>::new(Some(initiator_noise_codec));
        let mut responder_codec =
            CompoundCodec::<v2::framing::codec::Codec>::new(Some(responder_noise_codec));

        let expected_frame =
            v2::framing::Frame::from_serialized_payload(true, 0, 0x16, payload.clone());
        // This is currently due to the fact that Frame doesn't support cloning and it will be
        // consumed by the initiator codec
        let expected_frame_copy =
            v2::framing::Frame::from_serialized_payload(true, 0, 0x16, payload);
        let mut buffer = BytesMut::new();
        initiator_codec
            .encode(expected_frame, &mut buffer)
            .expect("BUG: Initiator codec failed to encode message");

        let decoded_frame = responder_codec
            .decode(&mut buffer)
            .expect("BUG: Responder codec failed to decode message")
            .expect("BUG: Responder coded provided incomplete message");

        assert_eq!(
            expected_frame_copy, decoded_frame,
            "BUG: Expected ({:x?}) and decoded ({:x?}) frames don't match",
            expected_frame_copy, decoded_frame
        );
    }

    /// Verify `CompoundCodec` operation for a payload that fits into a standard noise frame.
    #[test]
    fn compound_codec_payload_under_noise_max() {
        let mut payload = BytesMut::new();
        payload.extend_from_slice(&[1, 2, 3, 4]);

        assert!(
            payload.len() <= super::super::MAX_PAYLOAD_SIZE,
            "BUG: cannot test, provided frame must be <= noise::MAX_PAYLOAD"
        );
        run_compound_codec_with_noise(payload);
    }

    /// Verify `CompoundCodec` operation for a payload that exceeds the size of a noise
    /// frame and thus would require some fragmentation.
    /// TODO: remove the expected panic once we have support for bigger payload
    #[test]
    #[should_panic]
    fn compound_codec_payload_over_noise_max() {
        let payload = v2::framing::test::build_large_payload(v2::framing::Header::MAX_LEN as usize);
        assert!(
            payload.len() > super::super::MAX_PAYLOAD_SIZE,
            "BUG: cannot test, provided frame must exceed noise::MAX_PAYLOAD"
        );
        run_compound_codec_with_noise(payload);
    }
}
