// TODO: is it best practice to reexport this?
pub use packed_struct;
pub mod error;
pub mod v1;
pub mod v2;

/// Mask for allowed version bits that can be rolled based on BIP320
pub const BIP320_N_VERSION_MASK: u32 = 0x1fffe000;

/// Maximum number of bits allowed by BIP320_N_VERSION_MASK
pub const BIP320_N_VERSION_MAX_BITS: usize = 16;

// This is here because some test utilities need to be shared between
// both unit and integration tests.
#[doc(hidden)]
pub mod test_utils;
