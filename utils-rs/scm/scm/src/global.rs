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

use once_cell::sync::OnceCell;

static VERSION: OnceCell<Version> = OnceCell::new();

#[derive(Clone, Debug)]
pub struct Version {
    signature: String,
    full: String,
}

impl Version {
    /// Example values: `Version::set("StratumProxy", ii_scm::version_full!().as_str())`
    pub fn set(signature: &str, full: &str) {
        VERSION
            .set(Self {
                signature: signature.to_string(),
                full: full.to_string(),
            })
            .expect("BUG: version is already set");
    }

    /// Try to set version, return resul of operation
    pub fn try_set(signature: &str, full: &str) -> Result<(), Self> {
        VERSION.set(Self {
            signature: signature.to_string(),
            full: full.to_string(),
        })
    }

    #[inline]
    pub fn get() -> &'static Self {
        VERSION.get().expect("BUG: version is not set")
    }

    #[inline]
    pub fn signature() -> &'static String {
        &Self::get().signature
    }

    #[inline]
    pub fn full() -> &'static String {
        &Self::get().full
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_version() {
        Version::set("scm", "1");

        assert_eq!(Version::signature(), "scm");
        assert_eq!(Version::full(), "1");
    }
}
