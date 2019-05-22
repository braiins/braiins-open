use futures::compat::Future01CompatExt;
use futures::lock::Mutex;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::io;
use std::marker::PhantomData;
use std::net::{Shutdown, SocketAddr};
use std::pin::Pin;
use std::sync::atomic;
use std::sync::Arc;
use tokio::codec::{FramedRead, FramedWrite};
use tokio::net::{tcp, TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use crate::framing::Framing;
use crate::utils::{tcp_split, TcpStreamRecv, TcpStreamSend};

const BUFFER_SIZE: usize = 1024 * 1024;

/// A wrapper over atomic u32 that outputs IDs in a thread-safe way
#[derive(Default, Debug)]
struct MessageId(atomic::AtomicU32);

impl MessageId {
    fn new() -> MessageId {
        Self::default()
    }

    /// Get a new ID, increment internal state
    fn next(&self) -> u32 {
        self.0.fetch_add(1, atomic::Ordering::SeqCst)
        // FIXME: The atomic addition wraps around
    }
}

/// Provides mapping of request ID to a response channel that will be used for
/// communicating the protocol response to the original sender.
/// TODO: Replace Mutex+HashMap with a concurrent map? Eg. crossbeam-skiplist or the like...
type ReqMap<RESP> = Arc<Mutex<HashMap<u32, oneshot::Sender<RESP>>>>;

type DispatcherMsg<F: Framing> = Option<(F::Send, oneshot::Sender<F::Receive>)>;

#[derive(Debug)]
pub struct Dispatcher<F: Framing> {
    /// Request sender submits the request to this queue
    queue: mpsc::Sender<DispatcherMsg<F>>,
}
/// Client takes care of receiving requests via the dispatcher and sends them to the server
/// that accepts this protocol
pub struct Client<F: Framing> {
    /// Incoming requests to be processed and sent out
    queue: mpsc::Receiver<DispatcherMsg<F>>,

    req_map: ReqMap<F::Receive>,
    /// Unique request identifier
    req_id: MessageId,
    // /// Notifications handler
    // ntf_handler: Option<Box<dyn NotificationHandler<P::Message>>>,
}

impl<F: Framing> Client<F> {
    // fn handle(msg: P::Message, req_map: ReqMap<P::Message>) {}

    async fn run_rx<S>(
        req_map: ReqMap<F::Receive>,
        // ntf_handler: Option<Box<dyn NotificationHandler<P::Message>>>,
        mut stream_rx: S,
    ) where
        S: Stream<Item = F::Receive> + Unpin + Send + 'static,
    {
        while let Some(Ok(msg)) = await!(stream_rx.next()) {
            unimplemented!()

            // if let Some(id) = msg.id() {
            //     let mut req_map_lock = await!(req_map.lock());
            //     let channel = req_map_lock
            //         .remove(&id)
            //         .expect("Could not find a response channel");
            //     channel
            //         .send(msg)
            //         .map_err(|_| ())
            //         .expect("Could not deliver response");
            // } else {
            //     if let Some(ntf_handler) = ntf_handler.as_ref() {
            //         ntf_handler.notification(msg);
            //     }
            // }
        }

        // Note: Any pending channels will be closed when Client and Dispatcher are dropped
    }

    /// Main transfer loop that fetches requests, assigns them unique ID and sends them after
    /// serialization down the line
    /// TODO: this won't work as we cannot easily share 'self' instance. Consuming self is an
    /// option. However, we have to take care of receiving/pairing of responses
    async fn run_tx(mut self, mut lines_tx: FramedWrite<TcpStreamSend, F::Codec>) {
        while let Some(Ok(dispatch_msg)) = await!(self.queue.next()) {
            let (mut req, resp_tx) = match dispatch_msg {
                Some((req, resp_tx)) => (req, resp_tx),
                None => {
                    // Client is to be disconnected.
                    // This shutdown() call forwards to TcpStream::shutdown(),
                    // see comments in utils::tcp_split() as to how this is done.
                    let _ = await!(lines_tx.into_inner().shutdown(Shutdown::Both));
                    return;
                }
            };

            // TODO: decorate the request with a new unique ID -> this is the request ID
            // serialization point.
            let id = self.req_id.next();
            // FIXME: id
            // req.set_id(id);

            {
                let mut req_map_lock = await!(self.req_map.lock());
                if req_map_lock.insert(id, resp_tx).is_some() {
                    // There was already an entry for this ID, this is bad...
                    panic!("Client: Invalid state");
                }
            }

            // Can't be done in async, because codec_tx can't be shared:
            await!(lines_tx.send_async(req))
                .map_err(|_| ())
                .expect("Could not send request");
        }
    }

    pub async fn connect(mut self, addr: &str) {
        let addr: SocketAddr = addr.parse().expect("Failed to parse server address");
        let stream = await!(TcpStream::connect(&addr).compat()).expect("Connection Failed");

        // We will be handling request and response asynchronously
        //
        // We're splitting the stream first using custom function
        // and aplying framing to each half separately rather than
        // the other way around.
        // This is done to make connection shutdown easier and avoid
        // locking between writing and reading half.
        // See comments in utils::tcp_split()
        let (stream_tx, stream_rx) = tcp_split(stream);
        let codec_tx = FramedWrite::new(stream_tx, F::Codec::default());
        let codec_rx = FramedRead::new(stream_rx, F::Codec::default());

        tokio::spawn_async(Self::run_rx(
            self.req_map.clone(),
            // self.ntf_handler.take(),
            codec_rx,
        ));
        await!(self.run_tx(codec_tx));
    }

    // pub fn set_notification_handler<H: NotificationHandler<P::Message>>(&mut self, handler: H) {
    //     self.ntf_handler = Some(Box::new(handler));
    // }
}

impl<F: Framing> Dispatcher<F> {
    pub fn new_client() -> (Dispatcher<F>, Client<F>) {
        let (queue_tx, queue_rx) = mpsc::channel(BUFFER_SIZE);
        let req_map = ReqMap::default();

        let dispatcher = Dispatcher { queue: queue_tx };
        let client = Client {
            queue: queue_rx,
            req_map,
            req_id: MessageId::new(),
            // ntf_handler: None,
        };

        (dispatcher, client)
    }

    /// TODO add reasonable error type + how to deal with timeouts, the response should be
    /// optional
    /// Add explicit timeout parameter or should the timeout be part of the dispatcher instance
    /// (common for all messages)?
    pub async fn send<REQ, RESP>(&mut self, request: REQ) -> Result<RESP, RESP::Error>
    where
        REQ: Into<F::Send>,
        RESP: TryFrom<F::Receive>,
    {
        // Construct the channel for the response
        let request: F::Send = request.into();
        let (resp_tx, resp_rx) = oneshot::channel();

        // Enqueue the request
        await!(self.queue.send_async(Some((request, resp_tx)))).expect("Cannot send request");

        // Wait for the response
        let response = await!(resp_rx.compat()).expect("Broken response channel");
        response.try_into()
    }

    fn do_close(&mut self) {
        self.queue
            .try_send(None)
            .map_err(|_| ())
            .expect("Cannot send close notification");
    }

    pub fn close(mut self) {
        self.do_close();
    }
}

impl<F: Framing> Drop for Dispatcher<F> {
    fn drop(&mut self) {
        self.do_close();
    }
}

#[derive(Debug)]
struct ConnectionTx<F: Framing> {
    queue: mpsc::Receiver<Option<F::Send>>,
    sink: FramedWrite<TcpStreamSend, F::Codec>,
}

impl<F: Framing> ConnectionTx<F> {
    async fn run(mut self) {
        while let Some(Ok(dispatch_msg)) = await!(self.queue.next()) {
            let msg = match dispatch_msg {
                Some(msg) => msg,
                None => {
                    // Client is to be disconnected.
                    // This shutdown() call forwards to TcpStream::shutdown(),
                    // see comments in utils::tcp_split() as to how this is done.
                    await!(self.sink.into_inner().shutdown(Shutdown::Both));
                    return;
                }
            };

            // Can't be done in async, because codec_tx can't be shared:
            await!(self.sink.send_async(msg))
                .map_err(|_| ())
                .expect("Could not send message");
        }
    }
}

#[derive(Debug)]
pub struct Connection<F: Framing> {
    queue: mpsc::Sender<Option<F::Send>>,
    /// TODO rename to rx_stream
    stream: FramedRead<TcpStreamRecv, F::Codec>,
}

impl<F: Framing> Connection<F> {
    /// Establishes builds all parts of the connection for a specified TCP stream
    /// It can be used by the server after successful bind or by client
    fn new(stream: TcpStream) -> Self {
        let (queue_tx, queue_rx) = mpsc::channel(BUFFER_SIZE);

        // Regarding stream splitting, see comment in Client::connect()
        // and utils::tcp_split()

        let (stream_tx, stream_rx) = tcp_split(stream);
        let codec_tx = FramedWrite::new(stream_tx, F::Codec::default());
        let codec_rx = FramedRead::new(stream_rx, F::Codec::default());

        let tx = ConnectionTx::<F> {
            queue: queue_rx,
            sink: codec_tx,
        };

        tokio::spawn_async(tx.run());

        Connection {
            queue: queue_tx,
            stream: codec_rx,
        }
    }

    pub async fn connect(addr: &SocketAddr) -> Result<Self, F::Error> {
        let conn = await!(TcpStream::connect(addr).compat())?;

        Ok(Self::new(conn))
    }

    pub async fn send<M>(&mut self, message: M)
    where
        M: Into<F::Send>,
    {
        let message = message.into();

        // Enqueue the message
        await!(self.queue.send_async(Some(message))).expect("Cannot send request");
    }

    pub fn local_addr(&self) -> Result<SocketAddr, io::Error> {
        self.stream.get_ref().local_addr()
    }

    pub fn peer_addr(&self) -> Result<SocketAddr, io::Error> {
        self.stream.get_ref().peer_addr()
    }

    fn do_close(&mut self) {
        self.queue
            .try_send(None)
            .map_err(|_| ())
            .expect("Cannot send close notification");
    }

    pub fn close(mut self) {
        self.do_close();
    }
}

impl<F: Framing> Drop for Connection<F> {
    fn drop(&mut self) {
        self.do_close();
    }
}

impl<F: Framing> Stream for Connection<F> {
    type Item = F::Receive;
    type Error = F::Error;

    fn poll(&mut self) -> Result<Async<Option<F::Receive>>, F::Error> {
        self.stream.poll()
    }
}

#[derive(Debug)]
pub struct Server<F: Framing> {
    tcp: tcp::Incoming,
    _marker: PhantomData<&'static F>,
}

impl<F: Framing> Server<F> {
    pub fn bind(addr: &SocketAddr) -> Result<Server<F>, F::Error> {
        let tcp = TcpListener::bind(addr)?;
        Ok(Server {
            tcp: tcp.incoming(),
            _marker: PhantomData,
        })
    }
}

impl<F: Framing> Stream for Server<F> {
    type Item = Connection<F>;
    type Error = F::Error;

    /// An incoming TCP connection is converted into a new stratum connection with associated receiving codec
    fn poll(&mut self) -> Result<Async<Option<Connection<F>>>, F::Error> {
        self.tcp
            .poll()
            .map(|async_res| async_res.map(|stream_opt| stream_opt.map(Connection::new)))
            .map_err(F::Error::from)
    }
}
