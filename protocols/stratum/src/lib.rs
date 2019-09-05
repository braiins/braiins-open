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
