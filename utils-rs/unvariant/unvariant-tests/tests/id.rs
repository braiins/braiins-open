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

use ii_unvariant::{id, id_for, Id};

#[test]
fn id_macro() {
    #[id(0x11)]
    struct IdU32;
    let id: u32 = IdU32::ID;
    assert_eq!(id, 0x11);

    #[id(0x11u8)]
    struct IdU8;
    let id: u8 = IdU8::ID;
    assert_eq!(id, 0x11);

    #[id(0x11u8 type u8)]
    struct IdU8Explicit;
    let id: u8 = IdU8Explicit::ID;
    assert_eq!(id, 0x11);

    #[id(0x11u64)]
    struct IdU64;
    let id: u64 = IdU64::ID;
    assert_eq!(id, 0x11);

    #[id(0x11i64)]
    struct IdI64;
    let id: i64 = IdI64::ID;
    assert_eq!(id, 0x11);

    #[id("ii")]
    struct IdStr;
    let id: &str = IdStr::ID;
    assert_eq!(id, "ii");

    #[id((3, "ii") type (u8, &'static str))]
    struct IdComplex;
    let id: (u8, &str) = IdComplex::ID;
    assert_eq!(id, (3, "ii"));

    struct IdFor;
    id_for!(u8, IdFor => 0x11);
    let id: u8 = IdFor::ID;
    assert_eq!(id, 0x11);

    struct IdForMulti1;
    struct IdForMulti2;
    struct IdForMulti3;
    id_for!(u8,
        IdForMulti1 => 0x11,
        IdForMulti2 => 0x12,
        IdForMulti3 => 0x13,
    );
    let id: u8 = IdForMulti1::ID;
    assert_eq!(id, 0x11);
    let id: u8 = IdForMulti2::ID;
    assert_eq!(id, 0x12);
    let id: u8 = IdForMulti3::ID;
    assert_eq!(id, 0x13);
}
