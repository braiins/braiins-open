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

use std::sync::atomic;

pub trait Protocol {
    type Handler: ?Sized;
}

/// Generic payload for a message.
pub trait Payload<P: Protocol>: Sync + Send {
    fn accept(&self, msg: &Message<P>, handler: &mut P::Handler);
}

/// Generic message as e.g. created by reading from a stream.
/// The P type parameter allows passing specific protocol implementation
pub struct Message<P: Protocol> {
    /// Optional message identifier (to be discussed)
    pub id: Option<u32>,
    payload: Box<dyn Payload<P>>,
}

impl<P: Protocol> Message<P> {
    pub fn new(id: Option<u32>, payload: Box<dyn Payload<P>>) -> Self {
        Self { id, payload }
    }

    /// Adaptor method to allow visiting messages directly even if the actual
    /// visitor pattern is implemented over payloads.
    pub fn accept(&self, handler: &mut P::Handler) {
        self.payload.accept(self, handler);
    }
}

/// Sequeantial ID to pair up messages.
///
/// A wrapper over atomic u32 that outputs IDs in a thread-safe way.
#[derive(Default, Debug)]
pub struct MessageId(atomic::AtomicU32);

impl MessageId {
    pub fn new() -> MessageId {
        Self::default()
    }

    /// Get a new ID, increment internal state
    pub fn next(&self) -> u32 {
        self.0.fetch_add(1, atomic::Ordering::SeqCst)
        // FIXME: The atomic addition wraps around
    }

    pub fn get(&self) -> u32 {
        self.0.load(atomic::Ordering::SeqCst)
    }
}

/// TODO show an example of implementing a custom protocol
#[cfg(test)]
mod test {}
