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

use std::convert::TryInto;

use super::*;
use crate::test_utils::v2::*;
use crate::v2::framing;

#[test]
fn test_deserialize_setup_connection() {
    let deserialized =
        SetupConnection::try_from(SETUP_CONNECTION_SERIALIZED).expect("Deserialization failed");

    assert_eq!(
        deserialized,
        build_setup_connection(),
        "Deserialization is not correct"
    );
}

#[test]
fn test_serialize_setup_connection() {
    let frame: ii_wire::TxFrame = build_setup_connection()
        .try_into()
        .expect("Could not serialize message");

    // The message has ben serialized completely, let's skip the header for now
    assert_eq!(SETUP_CONNECTION_SERIALIZED, &frame[framing::Header::SIZE..],);
}
