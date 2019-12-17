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

use ii_unvariant::{id, GetId, Id};

/// A simple "network frame" example.
/// It contains a one-byte header followed by arbitrary data.
/// The header is an ID of the contained type.
/// The type of data is either `Foo` or `Bar`,
/// `Foo` is basically a `u32`, `Bar` is a one-byte `bool` value.
pub struct Frame(Box<[u8]>);

/// Implementing `GetId` for the `Frame` lets the unvariant macros know
/// how to get specific type ID out of the variant type `Frame`.
impl GetId for Frame {
    type Id = u32;

    fn get_id(&self) -> u32 {
        self.0[0] as _
    }
}

// Utilities: various ctors
impl Frame {
    pub fn new_foo(value: u32) -> Self {
        let mut bytes = Vec::with_capacity(5);
        bytes.push(Foo::ID as u8);
        bytes.extend(&value.to_le_bytes());
        Self(bytes.into())
    }

    pub fn new_foo_bad() -> Self {
        Self(vec![1].into())
    }

    pub fn new_bar(value: bool) -> Self {
        let bytes = vec![Bar::ID as u8, value as u8];
        Self(bytes.into())
    }

    pub fn new_bar_bad() -> Self {
        Self(vec![2].into())
    }

    pub fn new_unknown() -> Self {
        Self(vec![0xff].into())
    }
}

/// The `Foo` "message" type, annotated with an ID.
/// The annotation generates an Id trait implementation.
#[id(1)]
pub struct Foo(u32);

impl From<Frame> for Foo {
    fn from(frame: Frame) -> Self {
        From::from(&frame)
    }
}

impl<'a> From<&'a Frame> for Foo {
    fn from(frame: &'a Frame) -> Self {
        assert_eq!(frame.get_id(), Self::ID);
        let mut num = [0; 4];
        num.copy_from_slice(&frame.0[1..5]);
        Self(u32::from_le_bytes(num))
    }
}

/// A `From` implementation is required so that the macro code
/// knows how to create this type from the generic `Frame` type.
/// This is the non-failing use-case, for fallible example see `TryFoo` below.
impl From<Foo> for u32 {
    fn from(foo: Foo) -> u32 {
        foo.0 + Foo::ID
    }
}

/// The `Bar` "message" type, annotated with an ID.
#[id(2)]
pub struct Bar(bool);

impl From<Frame> for Bar {
    fn from(frame: Frame) -> Self {
        From::from(&frame)
    }
}

impl<'a> From<&'a Frame> for Bar {
    fn from(frame: &'a Frame) -> Self {
        assert_eq!(frame.get_id(), Self::ID);
        Self(frame.0[1] == 1)
    }
}

impl From<Bar> for u32 {
    fn from(bar: Bar) -> u32 {
        bar.0 as u32 + Bar::ID + 1
    }
}

/// The `TryFoo` type is the same as `Foo`, except that it implements
/// `TryFrom<Frame>` instead of simple `From<Frame>` to demonstrate
/// usage of the macro's `try` use-case.
#[id(1)]
pub struct TryFoo(u32);

impl TryFrom<Frame> for TryFoo {
    type Error = u32;

    fn try_from(frame: Frame) -> Result<Self, u32> {
        assert_eq!(frame.get_id(), Self::ID);
        let mut num = [0; 4];
        num.copy_from_slice(frame.0.get(1..5).ok_or(Self::ID)?);
        Ok(Self(u32::from_le_bytes(num)))
    }
}

impl From<TryFoo> for u32 {
    fn from(foo: TryFoo) -> u32 {
        foo.0 + TryFoo::ID
    }
}

// Ditto, cf. `TryFoo`
#[id(2)]
pub struct TryBar(bool);

impl TryFrom<Frame> for TryBar {
    type Error = u32;

    fn try_from(frame: Frame) -> Result<Self, u32> {
        assert_eq!(frame.get_id(), Self::ID);
        let byte = *frame.0.get(1).ok_or(Self::ID)?;
        Ok(Self(byte == 1))
    }
}

impl From<TryBar> for u32 {
    fn from(bar: TryBar) -> u32 {
        bar.0 as u32 + TryBar::ID + 1
    }
}
