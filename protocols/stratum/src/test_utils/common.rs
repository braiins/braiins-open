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
