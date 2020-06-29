use super::*;
use bytes::BytesMut;

use ii_unvariant::unvariant;

use crate::test_utils::v1::*;
use crate::v1::rpc::Rpc;

/// Test traits that will be used by serded for HexBytes when converting from/to string
#[test]
fn hex_bytes() {
    let hex_bytes = HexBytes(vec![0xde, 0xad, 0xbe, 0xef, 0x11, 0x22, 0x33]);
    let hex_bytes_str = "deadbeef112233";

    let checked_hex_bytes_str: String = hex_bytes.clone().into();
    assert_eq!(
        hex_bytes_str, checked_hex_bytes_str,
        "Mismatched hex bytes strings",
    );

    let checked_hex_bytes =
        HexBytes::try_from(hex_bytes_str).expect("BUG: Failed to decode hex bytes");

    assert_eq!(hex_bytes, checked_hex_bytes, "Mismatched hex bytes values",)
}

#[test]
fn hex_u32_from_string() {
    let hex_bytes_str = "deadbeef";
    let hex_bytes_str_0x = "0xdeadbeef";

    let le_from_plain = HexU32Le::try_from(hex_bytes_str).expect("BUG: parsing hex string failed");
    let le_from_prefixed =
        HexU32Le::try_from(hex_bytes_str_0x).expect("BUG: parsing hex string failed");
    let ref_le = HexU32Le(0xefbeadde);
    let be_from_plain = HexU32Be::try_from(hex_bytes_str).expect("BUG: parsing hex string failed");
    let be_from_prefixed =
        HexU32Be::try_from(hex_bytes_str_0x).expect("BUG: parsing hex string failed");
    let ref_be = HexU32Be(0xdeadbeef);
    assert_eq!(le_from_plain, ref_le);
    assert_eq!(le_from_prefixed, ref_le);
    assert_eq!(be_from_plain, ref_be);
    assert_eq!(be_from_prefixed, ref_be);
}

#[test]
fn extra_nonce1() {
    let expected_enonce1 = ExtraNonce1(HexBytes(vec![0xde, 0xad, 0xbe, 0xef, 0x11, 0x22, 0x33]));
    let expected_enonce1_str = r#""deadbeef112233""#;

    let checked_enonce1_str: String =
        serde_json::to_string(&expected_enonce1).expect("BUG: Serialization failed");
    assert_eq!(
        expected_enonce1_str, checked_enonce1_str,
        "Mismatched extranonce 1 strings",
    );

    let checked_enonce1 =
        serde_json::from_str(expected_enonce1_str).expect("BUG: Deserialization failed");

    assert_eq!(
        expected_enonce1, checked_enonce1,
        "Mismatched extranonce 1 values",
    )
}

/// This test demonstrates an actual implementation of protocol handler for a set of
/// messsages
#[tokio::test]
async fn message_from_frame() {
    for &req in V1_TEST_REQUESTS {
        let msg_rpc = Rpc::try_from(Frame::from_serialized_payload(BytesMut::from(req)))
            .expect("BUG: Deserialization failed");
        let mut handler = TestIdentityHandler;
        handler.handle_v1(msg_rpc).await;
    }
}

#[tokio::test]
async fn deserialize_response_message() {
    let fr = Frame::from_serialized_payload(BytesMut::from(MINING_SUBSCRIBE_OK_RESULT_JSON));
    let deserialized = Rpc::try_from(fr).expect("BUG: Deserialization failed");

    unvariant!(try deserialized {
        x: rpc::StratumResult => {
            x.expect("BUG: Deserialization failed");
        },
        _x: _ => panic!("BUG: Incorrect message unvariated"),
    });
}

// add also a separate stratum error test as per above response
