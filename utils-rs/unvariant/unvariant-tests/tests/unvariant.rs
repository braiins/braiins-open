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

use ii_unvariant::{unvariant, Id};

mod common;
use common::*;

#[test]
fn unvariant_macro() {
    let get_value = |frame: Frame| -> Result<u32, u32> {
        unvariant!(frame {
            foo: Foo => Ok(foo.into()),
            bar: Bar => Ok(bar.into()),
            id: _ => Err(id),
        })
    };

    let res = get_value(Frame::new_foo(9001));
    assert_eq!(res, Ok(9001 + Foo::ID));

    let res = get_value(Frame::new_bar(false));
    assert_eq!(res, Ok(1 + Bar::ID));

    let res = get_value(Frame::new_unknown());
    assert_eq!(res, Err(0xff));
}

#[test]
fn unvariant_macro_try() {
    let get_value = |frame: Frame| -> Result<u32, u32> {
        unvariant!(try frame {
            res: TryFoo => res.map(Into::into),
            res: TryBar => res.map(Into::into),
            id: _ => Err(id),
        })
    };

    let res = get_value(Frame::new_foo(9001));
    assert_eq!(res, Ok(9001 + TryFoo::ID));

    let res = get_value(Frame::new_bar(true));
    assert_eq!(res, Ok(2 + TryBar::ID));

    let res = get_value(Frame::new_foo_bad());
    assert_eq!(res, Err(TryFoo::ID));

    let res = get_value(Frame::new_bar_bad());
    assert_eq!(res, Err(TryBar::ID));

    let res = get_value(Frame::new_unknown());
    assert_eq!(res, Err(0xff));
}
