use bitcoin_hashes::hex::{FromHex, ToHex};
use bitcoin_hashes::{sha256, sha256d, Hash, HashEngine};

#[test]
fn test_midstate_computation() {
    // Block 171874 binary representation
    // https://blockchain.info/rawblock/00000000000004b64108a8e4168cfaa890d62b8c061c6b74305b7f6cb2cf9fda

    let mut engine = sha256::Hash::engine();
    engine.input(&[
        // ver = 1
        0x01, 0x00, 0x00, 0x00,
        // prev_block = 0000000000000488d0b6c4c05f24afe4817a122a1e1a5f009dd391fb0cc1aeb3
        0xb3, 0xae, 0xc1, 0x0c, 0xfb, 0x91, 0xd3, 0x9d, 0x00, 0x5f, 0x1a, 0x1e, 0x2a, 0x12, 0x7a,
        0x81, 0xe4, 0xaf, 0x24, 0x5f, 0xc0, 0xc4, 0xb6, 0xd0, 0x88, 0x04, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
        // mrkl_root = ce22a72fa0e9f309830fdb3f75d6c95f051f23ef288a137693ab5c03f2bb6e7e
        0x7e, 0x6e, 0xbb, 0xf2, 0x03, 0x5c, 0xab, 0x93, 0x76, 0x13, 0x8a, 0x28, 0xef, 0x23, 0x1f,
        0x05, 0x5f, 0xc9, 0xd6, 0x75, 0x3f, 0xdb, 0x0f, 0x83, 0x09, 0xf3, 0xe9,
        0xa0,
        // mrkl_root in chunk2
        // 0x2f, 0xa7, 0x22, 0xce
    ]);

    let midstate = engine.midstate();
    assert_eq!(
        midstate,
        // expected midstate result
        [
            0xe4, 0x8f, 0x54, 0x4a, 0x9a, 0x3a, 0xfa, 0x71, 0x45, 0x14, 0x71, 0x13, 0x4d, 0xf6,
            0xc3, 0x56, 0x82, 0xb4, 0x00, 0x25, 0x4b, 0xfe, 0x08, 0x60, 0xc9, 0x98, 0x76, 0xbf,
            0x46, 0x79, 0xba, 0x4e,
        ]
    );
}

#[test]
fn test_hash_to_uint256() {
    // the hex string represents the target coded as 256bit integer
    let target_str = "000000000007fff8000000000000000000000000000000000000000000000000";
    // the hex string represents the double SHA256 digest which is written in reverse order
    let hash_str = "00000000b86d6d17e45da42d09ddaf6a76041b703d09c566422ec53110874afb";

    // convert double SHA256 target to `Hash` structure. sha256d unlike sha256 module converts the
    // string as actual hexadecimal number
    let target = sha256d::Hash::from_hex(target_str).expect("parse hex");
    let hash = sha256d::Hash::from_hex(hash_str).expect("parse hex");

    // the inner representation of bytes is reverted back to its original binary digest
    let target_bytes = target.into_inner();
    let hash_bytes = hash.into_inner();

    // internal representation is always little endian
    let target_uint256 = uint::U256::from_little_endian(&target_bytes);
    let hash_uint256 = uint::U256::from_little_endian(&hash_bytes);

    assert_eq!(&target.to_hex(), &target_str);
    assert_eq!(&hash.to_hex(), &hash_str);
    assert_eq!(
        &target_uint256.to_hex(),
        &target_str.trim_start_matches('0')
    );
    assert_eq!(&hash_uint256.to_hex(), &hash_str.trim_start_matches('0'));
    assert!(target_uint256 < hash_uint256);
}

#[test]
fn test_target_difficulty() {
    struct Test {
        difficulty: u32,
        output: [u8; 32],
    }

    let difficulty_1_target_str =
        "00000000ffff0000000000000000000000000000000000000000000000000000";
    let difficulty_1_target_bytes: [u8; 32] = [
        0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ];
    let difficulty_1_target_uint256 = uint::U256::from_big_endian(&difficulty_1_target_bytes);

    assert_eq!(
        &difficulty_1_target_uint256.to_hex(),
        &difficulty_1_target_str.trim_start_matches('0')
    );

    let tests = vec![
        Test {
            difficulty: 512,
            output: [
                // 00000000007fff80000000000000000000000000000000000000000000000000
                0x00, 0x00, 0x00, 0x00, 0x00, 0x7f, 0xff, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00,
            ],
        },
        Test {
            difficulty: 1638,
            output: [
                // 0000000000280258258258258258258258258258258258258258258258258258
                0x00, 0x00, 0x00, 0x00, 0x00, 0x28, 0x02, 0x58, 0x25, 0x82, 0x58, 0x25, 0x82, 0x58,
                0x25, 0x82, 0x58, 0x25, 0x82, 0x58, 0x25, 0x82, 0x58, 0x25, 0x82, 0x58, 0x25, 0x82,
                0x58, 0x25, 0x82, 0x58,
            ],
        },
        Test {
            difficulty: 8192,
            output: [
                // 000000000007fff8000000000000000000000000000000000000000000000000
                0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0xff, 0xf8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00,
            ],
        },
    ];

    for test in tests {
        let difficulty_target_uint256 = uint::U256::from_big_endian(&test.output);
        assert_eq!(
            difficulty_1_target_uint256 / test.difficulty,
            difficulty_target_uint256
        );
    }
}
