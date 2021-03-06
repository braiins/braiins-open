// Copyright (C) 2021  Braiins Systems s.r.o.
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

use futures::channel::mpsc;
use std::{convert::TryInto, fmt};

/// Converts the response message into a `Frame` and submits it into the
/// specified queue
pub fn submit_message<F, T>(tx: &mut mpsc::Sender<F>, msg: T) -> Result<(), mpsc::TrySendError<F>>
where
    F: Send + Sync + 'static,
    T: TryInto<F>,
    <T as TryInto<F>>::Error: fmt::Debug,
{
    let frame = msg
        .try_into()
        .expect("BUG: Could convert the message to frame");
    tx.try_send(frame)
}
