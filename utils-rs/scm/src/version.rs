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

use git_version::git_version;

use once_cell::sync::Lazy;

/// Provides package version combined with git version as a single string.
/// TODO: create special proc-macro which returns constant string. The problem here is that the
/// CARGO_PKG_VERSION is really taken from this crate which is not correct.
pub static STRING: Lazy<String> =
    Lazy::new(|| format!(concat!(env!("CARGO_PKG_VERSION"), "-{}"), git_version!()));
