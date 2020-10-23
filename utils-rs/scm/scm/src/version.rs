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
macro_rules! full {
    ($($tt:tt)+) => {
        format!(
            "{} {}-{}",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
            git_version::git_version!($($tt)+)
        )
    };
    () => {
        format!(
            "{} {}-{}",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
            git_version::git_version!(fallback = "unknown")
        )
    }
}

#[macro_export]
macro_rules! semantic {
    () => {
        env!("CARGO_PKG_VERSION")
    };
}

#[macro_export]
macro_rules! git {
    ($($tt:tt)+) => {
        git_version::git_version!($($tt)+)
    };
    () => {
        git_version::git_version!(fallback = "unknown")
    }
}

#[cfg(test)]
mod tests {
    use git_version::git_version;

    #[test]
    fn test_full() {
        let git_version = git_version!();
        assert!(
            git_version
                .split('-')
                .next()
                .expect("BUG: cannot split")
                .len()
                == 10
        );
        assert_eq!(full!(), format!("ii-scm 0.1.0-{}", git_version));
    }

    #[test]
    fn test_full_args() {
        assert_eq!(
            full!(args = ["--abbrev=40", "--always"]),
            format!(
                "ii-scm 0.1.0-{}",
                git_version!(args = ["--abbrev=40", "--always"])
            )
        );
        assert_eq!(
            full!(prefix = "git:", cargo_prefix = "cargo:"),
            format!(
                "ii-scm 0.1.0-{}",
                git_version!(prefix = "git:", cargo_prefix = "cargo:")
            )
        );
    }

    #[test]
    fn test_semantic() {
        assert_eq!(semantic!(), "0.1.0");
    }

    #[test]
    fn test_git() {
        assert_eq!(git!(), git_version!(fallback = "unknown"));
    }

    #[test]
    fn test_git_args() {
        assert_eq!(
            git!(args = ["--abbrev=40", "--always"]),
            git_version!(args = ["--abbrev=40", "--always"])
        );
        assert_eq!(
            git!(prefix = "git:", cargo_prefix = "cargo:"),
            git_version!(prefix = "git:", cargo_prefix = "cargo:")
        );
    }
}
