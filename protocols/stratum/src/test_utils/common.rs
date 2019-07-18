//! Miscellaneous constants shared among test utils to generate consistent messages

pub const BRAIINS_OS_RELEASE: &str = "2019-06-05";
pub const MINER_SW_SIGNATURE: &str = "Braiins OS 2019-06-05";
pub const POOL_URL: &str = "stratum.slushpool.com";
pub const POOL_PORT: usize = 3333;
pub const USER_CREDENTIALS: &str = "braiins.worker0";

/// Nonce for the block header of the sample mining job
pub const MINING_WORK_NONCE: u32 = 0x0443c37b;
/// Version for the block header
pub const MINING_WORK_VERSION: u32 = 0x20000000;
/// Ntime for the block header
pub const MINING_WORK_NTIME: u32 = 0x5d10bc0a;
