use std::net::Ipv4Addr;

use futures::SinkExt;
use ii_stratum::v2::noise::{
    auth::{SignedPart, SignedPartHeader},
    negotiation::EncryptionAlgorithm,
    CompoundCodec, Initiator, StaticKeypair,
};
use tokio::net::TcpStream;

// TODO consolidate code between initiator.rs and responder.rs

const TEST_MESSAGE: &str = "Some short test message";
const TEST_SERVER_SOCKET: (Ipv4Addr, u16) = (Ipv4Addr::new(127, 0, 0, 1), 13225);

fn build_deterministic_signed_part_and_auth() -> (
    SignedPart,
    ed25519_dalek::Keypair,
    StaticKeypair,
    ed25519_dalek::Signature,
) {
    let ca_keypair_bytes = [
        228, 230, 186, 46, 141, 75, 176, 50, 58, 88, 5, 122, 144, 27, 124, 162, 103, 98, 75, 204,
        205, 238, 48, 242, 170, 21, 38, 183, 32, 199, 88, 251, 48, 45, 168, 81, 159, 57, 81, 233,
        0, 127, 137, 160, 19, 132, 253, 60, 188, 136, 48, 64, 180, 215, 118, 149, 61, 223, 246,
        125, 215, 76, 73, 28,
    ];
    let server_static_pub = [
        21, 50, 22, 157, 231, 160, 237, 11, 91, 131, 166, 162, 185, 55, 24, 125, 138, 176, 99, 166,
        20, 161, 157, 57, 177, 241, 215, 0, 51, 13, 150, 31,
    ];
    let server_static_priv = [
        83, 75, 77, 152, 164, 249, 65, 65, 239, 36, 159, 145, 250, 29, 58, 215, 250, 9, 55, 243,
        134, 157, 198, 189, 182, 21, 182, 36, 34, 4, 125, 122,
    ];

    let static_server_keypair = snow::Keypair {
        public: server_static_pub.to_vec(),
        private: server_static_priv.to_vec(),
    };
    let ca_keypair = ed25519_dalek::Keypair::from_bytes(&ca_keypair_bytes)
        .expect("BUG: Failed to construct key_pair");
    let signed_part = SignedPart::new(
        SignedPartHeader::new(0, u32::MAX),
        static_server_keypair.public.clone(),
        ca_keypair.public,
    );
    let signature = signed_part
        .sign_with(&ca_keypair)
        .expect("BUG: Failed to sign certificate");
    (signed_part, ca_keypair, static_server_keypair, signature)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut g_cfg = ii_logging::LoggingConfig::for_app(10);
    g_cfg.level = ii_logging::Level::Trace;
    let _g = ii_logging::setup(g_cfg);

    // Prepare test certificate and a serialized noise message that contains the signature
    let (_, authority_keys, _, _) = build_deterministic_signed_part_and_auth();
    let stream = TcpStream::connect(TEST_SERVER_SOCKET).await?;
    let initiator = Initiator::new(
        authority_keys.public,
        vec![EncryptionAlgorithm::ChaChaPoly, EncryptionAlgorithm::AESGCM],
    );
    let mut framed = initiator
        .connect_with_codec::<String, _, _>(stream, |noise| {
            CompoundCodec::<tokio_util::codec::LinesCodec>::new(Some(noise))
        })
        .await?;
    framed.send(TEST_MESSAGE).await?;
    Ok(())
}
