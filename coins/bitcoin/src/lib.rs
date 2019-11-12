// Copyright (C) 2019  Braiins Systems s.r.o.
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

pub mod test_blocks;

// reexport Bitcoin test structures
pub use test_blocks::{TestBlock, TEST_BLOCKS};

use packed_struct::prelude::*;
use packed_struct_codegen::PackedStruct;

use bitcoin_hashes::{sha256, HashEngine};
// reexport Bitcoin hash to remove dependency on bitcoin_hashes in other modules
pub use bitcoin_hashes::{hex::FromHex, sha256d::Hash as DHash, Hash as HashTrait};

use std::convert::TryInto;
use std::fmt;
use std::mem::size_of;
use std::slice::Chunks;
use std::time;

/// SHA256 digest size used in Bitcoin protocol
pub const SHA256_DIGEST_SIZE: usize = 32;

/// Binary representation of target for difficulty 1 coded in big endian
const DIFFICULTY_1_TARGET_BYTES: [u8; SHA256_DIGEST_SIZE] = [
    0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// https://github.com/bitcoin/bips/blob/master/bip-0320.mediawiki
/// Sixteen bits from the block header nVersion field, starting from 13 and ending at 28 inclusive,
/// are reserved for general use.
/// This specification does not reserve specific bits for specific purposes.
pub const BIP320_VERSION_MASK: u32 = 0x1fffe000;
pub const BIP320_VERSION_SHIFT: u32 = 13;
pub const BIP320_VERSION_MAX: u32 = std::u16::MAX as u32;

/// A Bitcoin block header is 80 bytes long
pub const BLOCK_HEADER_SIZE: usize = 80;

/// First chunk of Bitcoin block header used for midstate computation
pub const BLOCK_HEADER_CHUNK1_SIZE: usize = 64;

/// Bitcoin block header structure which can be packed to binary representation
/// which is 80 bytes long
#[derive(PackedStruct, Debug, Clone, Copy, Default)]
#[packed_struct(endian = "lsb")]
pub struct BlockHeader {
    /// Version field that reflects the current network consensus and rolled bits
    pub version: u32,
    /// Double SHA256 hash of the previous block header
    pub previous_hash: [u8; 32],
    /// Double SHA256 hash based on all of the transactions in the block
    pub merkle_root: [u8; 32],
    /// Current block timestamp as seconds since 1970-01-01T00:00 UTC
    pub time: u32,
    /// Current target in compact format (network difficulty)
    pub bits: u32,
    /// The nonce used to generate this block witch is below pool/network target
    pub nonce: u32,
}

impl BlockHeader {
    /// Get binary representation of Bitcoin block header
    #[inline]
    pub fn into_bytes(self) -> [u8; BLOCK_HEADER_SIZE] {
        self.pack()
    }

    /// Compute SHA256 double hash
    pub fn hash(&self) -> DHash {
        let block_bytes = self.into_bytes();
        DHash::hash(&block_bytes)
    }

    /// Compute SHA256 midstate from first chunk of block header
    pub fn midstate(&self) -> Midstate {
        let mut engine = sha256::Hash::engine();
        engine.input(&self.into_bytes()[..BLOCK_HEADER_CHUNK1_SIZE]);
        engine.midstate().into()
    }
}

/// Array containing SHA256 digest
type Sha256Array = [u8; SHA256_DIGEST_SIZE];

/// Type representing SHA256 midstate used for conversion simplification and printing
#[derive(Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord)]
pub struct Midstate(Sha256Array);

impl Midstate {
    pub fn from_hex(s: &str) -> Result<Self, bitcoin_hashes::Error> {
        // bitcoin crate implements `FromHex` trait for byte arrays with macro `impl_fromhex_array!`
        // this conversion is compatible with `Sha256Array` which is alias to array
        Ok(Self(FromHex::from_hex(s)?))
    }

    /// Get iterator for midstate words of specified type treated as a little endian
    pub fn words<T: FromMidstateWord<T>>(&self) -> MidstateWords<T> {
        MidstateWords::new(self.as_ref())
    }
}

impl From<Sha256Array> for Midstate {
    /// Get midstate from binary representation of SHA256
    fn from(bytes: Sha256Array) -> Self {
        Self(bytes)
    }
}

impl From<Midstate> for Sha256Array {
    /// Get binary representation of SHA256 from midstate
    fn from(midstate: Midstate) -> Self {
        midstate.0
    }
}

impl AsRef<Sha256Array> for Midstate {
    fn as_ref(&self) -> &Sha256Array {
        &self.0
    }
}

macro_rules! midstate_hex_fmt_impl(
    ($imp:ident) => (
        impl ::std::fmt::$imp for Midstate {
            fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                ::bitcoin_hashes::hex::format_hex(self.as_ref(), fmt)
            }
        }
    )
);

midstate_hex_fmt_impl!(Debug);
midstate_hex_fmt_impl!(Display);
midstate_hex_fmt_impl!(LowerHex);

/// Helper trait used by `MidstateWords` for reading little endian midstate word from slice created
/// from original midstate bytes
pub trait FromMidstateWord<T> {
    fn from_le_bytes(bytes: &[u8]) -> T;
}

/// Macro for implementation of `FromMidstateWord` for standard integer types
macro_rules! from_midstate_word_impl (
    ($imp:ident) => (
        impl FromMidstateWord<$imp> for $imp {
            fn from_le_bytes(bytes: &[u8]) -> $imp {
                $imp::from_le_bytes(bytes.try_into().expect("slice with incorrect length"))
            }
        }
    )
);

// add more integer types when needed
from_midstate_word_impl!(u32);
from_midstate_word_impl!(u64);

/// Iterator type for midstate words of specified type treated as a little endian
/// The iterator is returned by `Midstate::words`.
pub struct MidstateWords<'a, T: FromMidstateWord<T>> {
    chunks: Chunks<'a, u8>,
    /// Marker to silient the compiler because `T` is not used in this structure
    /// but it is required in constructor for creating chunks of size specified by this type
    _marker: std::marker::PhantomData<T>,
}

impl<'a, T: FromMidstateWord<T>> MidstateWords<'a, T> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            chunks: bytes.chunks(size_of::<T>()),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'a, T: FromMidstateWord<T>> Iterator for MidstateWords<'a, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<T> {
        self.chunks
            .next()
            .map(|midstate_word| T::from_le_bytes(midstate_word))
    }
}

impl<'a, T: FromMidstateWord<T>> DoubleEndedIterator for MidstateWords<'a, T> {
    #[inline]
    fn next_back(&mut self) -> Option<T> {
        self.chunks
            .next_back()
            .map(|midstate_word| T::from_le_bytes(midstate_word))
    }
}

/// Bitcoin target represents the network/pool difficulty as a 256bit number
/// The structure provides various conversion functions and formatters for uniform display of the
/// target as a hexadecimal string similar to Bitcoin double hash which is SHA256 double hash
/// printed in reverse order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Target(uint::U256);

impl Target {
    fn difficulty_1_target() -> uint::U256 {
        uint::U256::from_big_endian(&DIFFICULTY_1_TARGET_BYTES)
    }

    /// Create target from hexadecimal string which has the same representation as Bitcoin SHA256
    /// double hash (the hash is written in reverse order because the hash is treated as 256bit
    /// little endian number)
    pub fn from_hex(s: &str) -> Result<Self, bitcoin_hashes::Error> {
        // the target is treated the same as Bitcoin's double hash
        // the hexadecimal string is already reversed so load it as a big endian
        let target_dhash = DHash::from_hex(s)?;
        Ok(target_dhash.into_inner().into())
    }

    /// Create target from difficulty used by pools
    /// This implementation can produce different results than targets for network difficulty.
    pub fn from_pool_difficulty(difficulty: usize) -> Self {
        // TODO: use floating point division to get the same result expected by pool
        Self(Self::difficulty_1_target() / difficulty)
    }

    /// Create target from its compact representation used by Bitcoin protocol
    pub fn from_compact(bits: u32) -> Result<Self, &'static str> {
        // this code is inspired by `rust-bitcoin` crate implementation
        // original comment:
        //
        // This is a floating-point "compact" encoding originally used by
        // OpenSSL, which satoshi put into consensus code, so we're stuck
        // with it. The exponent needs to have 3 subtracted from it, hence
        // this goofy decoding code:
        // TODO: use packed structure
        let mantissa = bits & 0xffffff;
        let exponent = bits >> 24;

        // the mantissa is signed but may not be negative
        if mantissa > 0x7fffff {
            return Err("largest legal value for mantissa has been exceeded");
        }

        Ok(if exponent <= 3 {
            Into::<uint::U256>::into(mantissa >> (8 * (3 - exponent)))
        } else {
            Into::<uint::U256>::into(mantissa) << (8 * (exponent - 3))
        }
        .into())
    }

    /// Convert target to pool difficulty
    pub fn get_difficulty(&self) -> usize {
        (Self::difficulty_1_target() / self.0).low_u64() as usize
    }

    /// Convert target to its compact representation used by Bitcoin protocol
    pub fn into_compact(self) -> u32 {
        // this code is inspired by `rust-bitcoin` crate implementation
        let mut exponent = (self.0.bits() + 7) / 8;
        let mut mantissa = if exponent <= 3 {
            (self.0.low_u64() << (8 * (3 - exponent))) as u32
        } else {
            (self.0 >> (8 * (exponent - 3))).low_u32()
        };

        if (mantissa & 0x00800000) != 0 {
            mantissa >>= 8;
            exponent += 1;
        }

        // TODO: use packed structure
        mantissa | (exponent << 24) as u32
    }

    /// Yields the U256 number that represents the target
    pub fn into_inner(self) -> uint::U256 {
        self.0
    }

    /// Auxiliary function to check if the target is greater or equal to some 256bit number
    #[inline]
    fn is_greater_or_equal(&self, other: &Target) -> bool {
        self.0 >= other.0
    }
}

impl Default for Target {
    /// The default target represents value with difficulty 1
    fn default() -> Self {
        Self(Self::difficulty_1_target())
    }
}

impl From<uint::U256> for Target {
    /// Get target from 256bit integer
    fn from(value: uint::U256) -> Self {
        Self(value)
    }
}

impl From<Target> for uint::U256 {
    /// Get 256bit integer from target
    fn from(target: Target) -> Self {
        target.0
    }
}

impl From<Target> for Sha256Array {
    /// Get binary representation of SHA256 from target
    /// The target has the same binary representation as Bitcoin SHA256 double hash.
    fn from(target: Target) -> Self {
        let mut bytes = [0u8; SHA256_DIGEST_SIZE];
        target.0.to_little_endian(&mut bytes);
        bytes
    }
}

impl From<Sha256Array> for Target {
    /// Get target from binary representation of SHA256
    /// The target has the same binary representation as Bitcoin SHA256 double hash.
    fn from(bytes: Sha256Array) -> Self {
        Self(uint::U256::from_little_endian(&bytes))
    }
}

impl From<DHash> for Target {
    /// Convenience conversion directly from Sha256d into Target
    fn from(dhash: DHash) -> Self {
        dhash.into_inner().into()
    }
}

impl AsRef<uint::U256> for Target {
    fn as_ref(&self) -> &uint::U256 {
        &self.0
    }
}

macro_rules! target_hex_fmt_impl(
    ($imp:ident) => (
        impl ::std::fmt::$imp for Target {
            fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                let bytes: Sha256Array = (*self).into();
                ::bitcoin_hashes::hex::format_hex_reverse(&bytes, fmt)
            }
        }
    )
);

target_hex_fmt_impl!(Debug);
target_hex_fmt_impl!(Display);
target_hex_fmt_impl!(LowerHex);

/// Auxiliary trait for adding target comparison with various types compatible with the `Target`
pub trait MeetsTarget {
    /// Check if the type is less or equal to the target
    /// In other words, the type (usually hash computed from some block) would be accepted as
    /// a valid by the remote server/pool.
    fn meets(&self, target: &Target) -> bool;
}

/// Extend SHA256 double hash with ability to validate that it is below target
impl MeetsTarget for DHash {
    fn meets(&self, target: &Target) -> bool {
        // convert it to number suitable for target comparison
        let double_hash_u256 = Target::from(self.into_inner());
        // and check it with current target (pool difficulty)
        target.is_greater_or_equal(&double_hash_u256)
    }
}

/// Structure used for storing all shares determined from solution target difficulty
/// Share=1 represents a space of 2^32 calculated hashes for Bitcoin mainnet; exactly
/// 2^256 / (0xffff << 208), where 0xffff << 208 is defined as target difficulty 1 for Bitcoin
/// mainnet. Each solution that meets a target at difficulty D is accounted as D shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord)]
pub struct Shares(u64);

impl Shares {
    const DIFFICULTY_1_SHIFT: usize = 32;

    /// Create shares object with some initial value based on target
    pub fn new(target: &Target) -> Self {
        Self(target.get_difficulty() as u64)
    }

    /// Account solution specified by its target difficulty to the shares
    pub fn account_solution(&mut self, target: &Target) {
        self.0 += target.get_difficulty() as u64
    }

    #[inline]
    pub fn into_hashes(self) -> HashesUnit {
        HashesUnit::Hashes((self.0 as u128) << Self::DIFFICULTY_1_SHIFT)
    }

    #[inline]
    pub fn into_kilo_hashes(self) -> HashesUnit {
        self.into_hashes().into_kilo_hashes()
    }

    #[inline]
    pub fn into_mega_hashes(self) -> HashesUnit {
        self.into_hashes().into_mega_hashes()
    }

    #[inline]
    pub fn into_giga_hashes(self) -> HashesUnit {
        self.into_hashes().into_giga_hashes()
    }

    #[inline]
    pub fn into_tera_hashes(self) -> HashesUnit {
        self.into_hashes().into_tera_hashes()
    }

    #[inline]
    pub fn into_pretty_hashes(self) -> HashesUnit {
        self.into_hashes().into_pretty_hashes()
    }

    /// Compute number of shares per second
    pub fn to_sharerate(&self, interval: time::Duration) -> f64 {
        let secs = interval.as_secs_f64();
        if secs == 0.0 {
            self.0 as f64
        } else {
            self.0 as f64 / secs
        }
    }
}

/// Helper conversion from a u64 share counter
impl From<u64> for Shares {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl std::ops::Add for Shares {
    type Output = Self;

    fn add(self, shares: Self) -> Self {
        Self(self.0 + shares.0)
    }
}

/// Provides consistent interface for working with hash units and convert between them including
/// pretty printing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HashesUnit {
    Hashes(u128),
    KiloHashes(f64),
    MegaHashes(f64),
    GigaHashes(f64),
    TeraHashes(f64),
}

impl HashesUnit {
    pub fn into_u128(self) -> u128 {
        match self {
            Self::Hashes(value) => value,
            Self::KiloHashes(value)
            | Self::MegaHashes(value)
            | Self::GigaHashes(value)
            | Self::TeraHashes(value) => value as u128,
        }
    }

    pub fn into_f64(self) -> f64 {
        match self {
            Self::Hashes(value) => value as f64,
            Self::KiloHashes(value)
            | Self::MegaHashes(value)
            | Self::GigaHashes(value)
            | Self::TeraHashes(value) => value,
        }
    }

    pub fn into_hashes(self) -> HashesUnit {
        match self {
            Self::Hashes(value) => Self::Hashes(value),
            Self::KiloHashes(value) => Self::Hashes((value * 1e+3) as u128),
            Self::MegaHashes(value) => Self::Hashes((value * 1e+6) as u128),
            Self::GigaHashes(value) => Self::Hashes((value * 1e+9) as u128),
            Self::TeraHashes(value) => Self::Hashes((value * 1e+12) as u128),
        }
    }

    pub fn into_kilo_hashes(self) -> HashesUnit {
        match self {
            Self::Hashes(value) => Self::KiloHashes(value as f64 * 1e-3),
            Self::KiloHashes(value) => Self::KiloHashes(value),
            Self::MegaHashes(value) => Self::KiloHashes(value * 1e+3),
            Self::GigaHashes(value) => Self::KiloHashes(value * 1e+6),
            Self::TeraHashes(value) => Self::KiloHashes(value * 1e+9),
        }
    }

    pub fn into_mega_hashes(self) -> HashesUnit {
        match self {
            Self::Hashes(value) => Self::MegaHashes(value as f64 * 1e-6),
            Self::KiloHashes(value) => Self::MegaHashes(value * 1e-3),
            Self::MegaHashes(value) => Self::MegaHashes(value),
            Self::GigaHashes(value) => Self::MegaHashes(value * 1e+3),
            Self::TeraHashes(value) => Self::MegaHashes(value * 1e+6),
        }
    }

    pub fn into_giga_hashes(self) -> HashesUnit {
        match self {
            Self::Hashes(value) => Self::GigaHashes(value as f64 * 1e-9),
            Self::KiloHashes(value) => Self::GigaHashes(value * 1e-6),
            Self::MegaHashes(value) => Self::GigaHashes(value * 1e-3),
            Self::GigaHashes(value) => Self::GigaHashes(value),
            Self::TeraHashes(value) => Self::GigaHashes(value * 1e+3),
        }
    }

    pub fn into_tera_hashes(self) -> HashesUnit {
        match self {
            Self::Hashes(value) => Self::TeraHashes(value as f64 * 1e-12),
            Self::KiloHashes(value) => Self::TeraHashes(value * 1e-9),
            Self::MegaHashes(value) => Self::TeraHashes(value * 1e-6),
            Self::GigaHashes(value) => Self::TeraHashes(value * 1e-3),
            Self::TeraHashes(value) => Self::TeraHashes(value),
        }
    }

    pub fn into_pretty_hashes(self) -> HashesUnit {
        let units: [Box<dyn Fn() -> HashesUnit>; 4] = [
            Box::new(|| self.into_tera_hashes()),
            Box::new(|| self.into_giga_hashes()),
            Box::new(|| self.into_mega_hashes()),
            Box::new(|| self.into_kilo_hashes()),
        ];

        for unit in units.iter() {
            let pretty_hashes = (unit)();
            // check if current unit is not truncated too much
            if pretty_hashes.into_u128() > 0 {
                return pretty_hashes;
            }
        }
        self.into_hashes()
    }
}

impl fmt::Display for HashesUnit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Hashes(value) => write!(f, "{} H", value),
            Self::KiloHashes(value) => write!(f, "{:.2} kH", value),
            Self::MegaHashes(value) => write!(f, "{:.2} MH", value),
            Self::GigaHashes(value) => write!(f, "{:.2} GH", value),
            Self::TeraHashes(value) => write!(f, "{:.2} TH", value),
        }
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use test_blocks::TEST_BLOCKS;

    use bitcoin_hashes::hex::ToHex;

    #[test]
    fn test_block_header() {
        for block in TEST_BLOCKS.iter() {
            let block_header = BlockHeader {
                version: block.version,
                previous_hash: block.previous_hash.into_inner(),
                merkle_root: block.merkle_root.into_inner(),
                time: block.time,
                bits: block.bits,
                nonce: block.nonce,
            };

            // test computation of SHA256 double hash of Bitcoin block header
            let block_hash = block_header.hash();
            assert_eq!(block.hash, block_hash);

            // check expected format of hash in hex string with multiple formaters
            assert_eq!(block.hash_str, block_hash.to_hex());
            assert_eq!(block.hash_str, format!("{}", block_hash));
            assert_eq!(block.hash_str, format!("{:?}", block_hash));
            assert_eq!(block.hash_str, format!("{:x}", block_hash));

            // check binary representation of Bitcoin block header
            assert_eq!(block.header_bytes[..], block_header.into_bytes()[..]);
        }
    }

    #[test]
    fn test_block_header_midstate() {
        for block in TEST_BLOCKS.iter() {
            let block_header = BlockHeader {
                version: block.version,
                previous_hash: block.previous_hash.into_inner(),
                merkle_root: block.merkle_root.into_inner(),
                ..Default::default()
            };

            // test computation of SHA256 midstate of Bitcoin block header
            let block_midstate = block_header.midstate();
            assert_eq!(block.midstate, block_midstate);

            // check expected format of midstate in hex string with multiple formatters
            assert_eq!(block.midstate_str, block_midstate.to_hex());
            assert_eq!(block.midstate_str, format!("{}", block_midstate));
            assert_eq!(block.midstate_str, format!("{:?}", block_midstate));
            assert_eq!(block.midstate_str, format!("{:x}", block_midstate));
        }
    }

    #[test]
    fn test_midstate_words() {
        use bytes::{BufMut, BytesMut};

        for block in TEST_BLOCKS.iter() {
            // test midstate conversion to words iterator and back to bytes representation
            // * for u32 words
            let mut midstate = BytesMut::with_capacity(32);

            for midstate_word in block.midstate.words() {
                midstate.put_u32_le(midstate_word);
            }
            assert_eq!(block.midstate.as_ref()[..], midstate);
            // * for u64 words
            midstate.clear();
            for midstate_word in block.midstate.words() {
                midstate.put_u64_le(midstate_word);
            }
            assert_eq!(block.midstate.as_ref()[..], midstate);

            // revert midstate as a reference result
            let mut midstate_rev: Sha256Array = block.midstate.into();
            midstate_rev.reverse();

            // test midstate reversion with words iterator
            // * for u32 words
            midstate.clear();
            for midstate_word in block.midstate.words().rev() {
                midstate.put_u32_be(midstate_word);
            }
            assert_eq!(midstate_rev[..], midstate);
            // * for u64 words
            midstate.clear();
            for midstate_word in block.midstate.words().rev() {
                midstate.put_u64_be(midstate_word);
            }
            assert_eq!(midstate_rev[..], midstate);
        }
    }

    #[test]
    fn test_target_difficulty_1() {
        const TARGET_1_STR: &str =
            "00000000ffff0000000000000000000000000000000000000000000000000000";
        const TARGET_1_BITS: u32 = 0x1d00ffff;

        // the default target is equal to target with difficulty 1
        assert_eq!(
            Into::<uint::U256>::into(Target::default()),
            uint::U256::from_big_endian(&DIFFICULTY_1_TARGET_BYTES)
        );

        // the target is stored into byte array as a 256bit little endian integer
        // the `DIFFICULTY_1_TARGET_BYTES` is stored as a big endian array to represent the value
        // in the same order as a hexadecimal string
        assert_eq!(
            Into::<Sha256Array>::into(Target::default())[..],
            DIFFICULTY_1_TARGET_BYTES
                .iter()
                .cloned()
                .rev()
                .collect::<Vec<_>>()[..]
        );

        // reference value of target with difficulty 1
        let difficulty_1_target: Target =
            uint::U256::from_big_endian(&DIFFICULTY_1_TARGET_BYTES).into();

        // check conversion from difficulty
        assert_eq!(difficulty_1_target, Target::from_pool_difficulty(1));

        // check conversion from hexadecimal string
        assert_eq!(difficulty_1_target, Target::from_hex(TARGET_1_STR).unwrap());

        // check conversion from compact representation of target with difficulty 1
        assert_eq!(
            difficulty_1_target,
            Target::from_compact(TARGET_1_BITS).unwrap()
        );
        // and conversion back to compact representation
        assert_eq!(difficulty_1_target.into_compact(), TARGET_1_BITS);

        // check expected format of midstate in hex string with multiple formatters
        assert_eq!(TARGET_1_STR, difficulty_1_target.to_hex());
        assert_eq!(TARGET_1_STR, format!("{}", difficulty_1_target));
        assert_eq!(TARGET_1_STR, format!("{:?}", difficulty_1_target));
        assert_eq!(TARGET_1_STR, format!("{:x}", difficulty_1_target));
    }

    #[test]
    fn test_target_compact() {
        for block in TEST_BLOCKS.iter() {
            let bits = block.bits;

            // check conversion from compact representation of target to full one and back
            assert_eq!(bits, Target::from_compact(bits).unwrap().into_compact());
        }
    }

    /// Check detection of invalid representation of target in compact format
    #[test]
    fn test_corrupted_compact() {
        assert!(Target::from_compact(0xfffffff).is_err())
    }

    #[test]
    fn test_meets_target() {
        for block in TEST_BLOCKS.iter() {
            // convert network difficulty to target
            let target = Target::from_compact(block.bits).unwrap();

            // check if test block meets the target
            assert!(block.hash.meets(&target));
        }
    }

    #[test]
    fn test_target_bytes() {
        for block in TEST_BLOCKS.iter() {
            // convert hash of block to target
            let target: Target = block.hash.into();

            // check if conversion to bytes returns the same result as a inner representation of
            // the block hash
            let target_bytes: Sha256Array = target.into();
            let hash_bytes = block.hash.into_inner();

            assert_eq!(target_bytes, hash_bytes);
        }
    }

    #[test]
    fn test_shares() {
        let target_difficulty_1: Target = Default::default();

        // test default value
        let shares = Shares::default();
        assert_eq!(shares.into_hashes(), HashesUnit::Hashes(0));

        // test shares based on target difficulty 1
        let mut shares = Shares::new(&target_difficulty_1);
        assert_eq!(shares.into_hashes(), HashesUnit::Hashes(0x100000000));

        // test number transformation
        assert_eq!(
            shares.into_kilo_hashes(),
            HashesUnit::KiloHashes(4294967.296)
        );
        assert_eq!(
            shares.into_mega_hashes(),
            HashesUnit::MegaHashes(4294.967296)
        );
        assert_eq!(
            shares.into_giga_hashes(),
            HashesUnit::GigaHashes(4.294967296)
        );

        // test share accounting for another target with difficulty 1
        shares.account_solution(&target_difficulty_1);
        assert_eq!(shares.into_hashes(), HashesUnit::Hashes(0x200000000));

        // test add operator (2 * shares) == (shares + shares)
        let shares = shares + shares;
        assert_eq!(shares.into_hashes(), HashesUnit::Hashes(0x400000000));

        // test comparison operators
        assert_eq!(Shares::default(), Shares::default());
        assert!(Shares::default() < shares);
        assert!(shares > Shares::default());
    }
}
