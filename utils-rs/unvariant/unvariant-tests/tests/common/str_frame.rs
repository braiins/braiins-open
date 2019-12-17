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

use std::convert::TryFrom;

use ii_unvariant::{id, GetId, /* HandleFuture, */ Id};

/// Similar to `Frame` but contains a string reference
/// and has string reference ID type.
/// This type is for testing of both
/// a) passing a variant by reference, and
/// b) the variant having a lifetime generic argument.
pub struct StrFrame<'a>(pub &'a str, pub bool);

/// When passing variants by reference, we need to implement
/// `GetId` on the reference type instead:
impl<'a> GetId for StrFrame<'a> {
    type Id = &'a str;

    fn get_id(&self) -> &'a str {
        self.0.as_ref()
    }
}

#[id("foo")]
pub struct StrFoo;

impl<'a, 'b> From<&'a StrFrame<'b>> for StrFoo {
    fn from(frame: &'a StrFrame<'b>) -> Self {
        assert_eq!(frame.get_id(), Self::ID);
        StrFoo
    }
}
impl<'a> From<&'a str> for StrFoo {
    fn from(frame: &'a str) -> Self {
        assert_eq!(frame.get_id(), Self::ID);
        StrFoo
    }
}

#[id("bar")]
pub struct StrBar;

impl<'a, 'b> From<&'a StrFrame<'b>> for StrBar {
    fn from(frame: &'a StrFrame<'b>) -> Self {
        assert_eq!(frame.get_id(), Self::ID);
        StrBar
    }
}
impl<'a> From<&'a str> for StrBar {
    fn from(frame: &'a str) -> Self {
        assert_eq!(frame.get_id(), Self::ID);
        StrBar
    }
}

#[derive(PartialEq, Debug)]
pub struct TryStrFrameError;

impl From<TryStrFrameError> for bool {
    fn from(_: TryStrFrameError) -> Self {
        false
    }
}

#[id("foo")]
pub struct TryStrFoo;

impl<'a, 'b> TryFrom<&'a StrFrame<'b>> for TryStrFoo {
    type Error = TryStrFrameError;

    fn try_from(frame: &'a StrFrame<'b>) -> Result<Self, TryStrFrameError> {
        assert_eq!(frame.get_id(), Self::ID);
        if frame.1 {
            Ok(TryStrFoo)
        } else {
            Err(TryStrFrameError)
        }
    }
}

#[id("bar")]
pub struct TryStrBar;

impl<'a, 'b> TryFrom<&'a StrFrame<'b>> for TryStrBar {
    type Error = TryStrFrameError;

    fn try_from(frame: &'a StrFrame<'b>) -> Result<Self, TryStrFrameError> {
        assert_eq!(frame.get_id(), Self::ID);
        if frame.1 {
            Ok(TryStrBar)
        } else {
            Err(TryStrFrameError)
        }
    }
}

pub trait TryStrFrameHandler {
    fn handle<'a, 'b>(&mut self, variant: &'a StrFrame<'b>) -> Result<(), bool>;
}
