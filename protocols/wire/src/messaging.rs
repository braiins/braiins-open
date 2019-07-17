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
