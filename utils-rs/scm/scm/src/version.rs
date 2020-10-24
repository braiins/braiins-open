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

#[macro_export]
macro_rules! version_full {
    ($($tt:tt)*) => {
        format!(
            "{} {}-{}",
            env!("CARGO_PKG_NAME"),
            $crate::version_semantic!(),
            $crate::version_git!($($tt)*)
        )
    }
}

#[macro_export]
macro_rules! version_semantic {
    () => {
        env!("CARGO_PKG_VERSION")
    };
}

#[macro_export]
macro_rules! version_git {
    ($($tt:tt)*) => {
        $crate::git_hash!(length = 8, $($tt)*)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn full() {
        let hash = ii_scm_git::git_hash!(object = "HEAD", length = 8);
        assert_eq!(hash.len(), 8);
        assert_eq!(version_full!(), format!("ii-scm 0.1.0-{}", hash));
    }

    #[test]
    fn full_args() {
        let hash = ii_scm_git::git_hash!(object = "HEAD", length = 5);
        assert_eq!(hash.len(), 5);
        assert_eq!(
            version_full!(object = "HEAD", length = 5),
            format!("ii-scm 0.1.0-{}", hash)
        );
    }

    #[test]
    fn semantic() {
        assert_eq!(version_semantic!(), "0.1.0");
    }

    #[test]
    fn git() {
        let hash = ii_scm_git::git_hash!(object = "HEAD", length = 8);
        assert_eq!(hash.len(), 8);
        assert_eq!(version_git!(), hash);
    }

    #[test]
    fn git_args() {
        let hash = ii_scm_git::git_hash!(object = "HEAD", length = 3);
        assert_eq!(hash.len(), 3);
        assert_eq!(version_git!(object = "HEAD", length = 3), hash);
    }
}
