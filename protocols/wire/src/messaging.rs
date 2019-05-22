//!
// TODO: rename to 'Protocol'
pub trait ProtocolBase {
    type Handler: ?Sized;
}

/// Generic payload for a message.
pub trait Payload<P: ProtocolBase>: Sync + Send {
    fn accept(&self, msg: &Message<P>, handler: &P::Handler);
}

/// Generic message as e.g. created by reading from a stream.
/// The P type parameter allows passing specific protocol implementation
pub struct Message<P: ProtocolBase> {
    /// Optional message identifier (to be discussed)
    pub id: Option<u32>,
    payload: Box<dyn Payload<P>>,
}

impl<P: ProtocolBase> Message<P> {
    pub fn new(id: Option<u32>, payload: Box<dyn Payload<P>>) -> Self {
        Self { id, payload }
    }

    /// Adaptor method to allow visiting messages directly even if the actual
    /// visitor pattern is implemented over payloads.
    pub fn accept(&self, handler: &P::Handler) {
        self.payload.accept(self, handler);
    }
}

/// TODO show an example of implementing a custom protocol
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_accept() {}
}
