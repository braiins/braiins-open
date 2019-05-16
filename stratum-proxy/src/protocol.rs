use std::sync::atomic;
use std::collections::HashMap;
use std::future::Future as StdFuture;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::net::SocketAddr;
use futures::compat::Future01CompatExt;
use futures::future::FutureExt;
use futures::lock::Mutex;
use tokio::prelude::*;
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::sync::oneshot;
use tokio::codec::{Decoder, LinesCodec, Framed};
use tokio::net::{TcpListener, TcpStream};


//trait Message: Serialize + DeserializeOwned {
trait Message {
    // TODO
}

/// A wrapper over atomic u32 that outputs IDs in a thread-safe way
#[derive(Default, Debug)]
struct ReqId(atomic::AtomicU32);

impl ReqId {
    fn new() -> ReqId {
        Self::default()
    }

    /// Get a new ID, increment internal state
    fn next(&self) -> u32 {
        self.0.fetch_add(1, atomic::Ordering::SeqCst)
        // Note: The atomic addition wraps around
    }
}

/// Provides mapping of request ID to a response channel that will be used for
/// communicating the protocol response to the original sender.
/// TODO: Replace Mutex+HashMap with a concurrent map? Eg. crossbeam-skiplist or the like...
type ReqMap<RESP> = Arc<Mutex<HashMap<u32, oneshot::Sender<RESP>>>>;

#[derive(Debug)]
pub struct Dispatcher<REQ, RESP> {
    /// Request sender submits the request to this queue
    queue: mpsc::Sender<(REQ, oneshot::Sender<RESP>)>,
}

//#[derive(Debug)]
//pub struct Server<REQ, RESP> {
//    queue: Receiver<REQ>,
//    req_map: ReqMap<RESP>,
//}

/// Client takes care of receiving requests via the dispatcher and sends them to the server
/// that accepts this protocol
#[derive(Debug)]
pub struct Client<REQ, RESP> {
    /// Incoming requests to be processed and sent out
    queue: mpsc::Receiver<(REQ, oneshot::Sender<RESP>)>,

    req_map: ReqMap<RESP>,
    /// Unique request identifier
    req_id: ReqId,
}

impl<REQ, RESP> Client<REQ, RESP> where
    RESP: Send + From<stratum::v1::rpc::Response> + 'static,     // TODO: these bounds probably don't make sense, deserialize RESP directly
    REQ: Send + Into<stratum::v1::rpc::Request> + 'static {

    async fn run_rx<S>(req_map: ReqMap<RESP>, mut lines_rx: S) where
        S: Stream<Item=String> + Unpin + Send + 'static {

        while let Some(Ok(msg)) = await!(lines_rx.next()) {
            use stratum::v1::rpc::Rpc;  // XXX: ?

            let resp = match Rpc::from_str(&msg) {
                Ok(Rpc::RpcResponse(resp)) => resp,
                _ => panic!("Error parsing response"),
            };

            println!("resp: {:?}", resp);

            if let Some(id) = resp.id {
                let mut req_map_lock = await!(req_map.lock());
                let channel = req_map_lock.remove(&id).expect("Could not find a response channel");
                channel.send(resp.into());
            } else {
                // TODO
            }
        }
    }

    /// Main transfer loop that fetches requests, assigns them unique ID and sends them after
    /// serialization down the line
    /// TODO: this won't work as we cannot easily share 'self' instance. Consuming self is an
    /// option. However, we have to take care of receiving/pairing of responses
    async fn run_tx<S>(mut self, mut lines_tx: S) where
        S: Sink<SinkItem=String> + Unpin + Send + 'static {

        while let Some(Ok((req, resp_tx))) = await!(self.queue.next()) {
            // TODO: decorate the request with a new unique ID -> this is the request ID
            // serialization point. For now, we know it's a V1 RPC request so we access the ID
            // directly (this is to be removed)
            let mut req = req.into();
            req.id = self.req_id.next();
            // TODO temporary workaround with hardcoded json serializer
            let req_json = req.to_json_string().expect("Failed to serialize");

            {
                let mut req_map_lock = await!(self.req_map.lock());
                if req_map_lock.insert(req.id, resp_tx).is_some() {
                    // There was already an entry for this ID, this is bad...
                    panic!("Client: Invalid state");
                }
            }

            // Can't be done in async, because lines_tx can't be shared:
            // tokio::spawn_async(lines_tx.send_async(req_json).map(|_| ()));
            await!(lines_tx.send_async(req_json));
        }
    }

    pub async fn connect(self, addr: &str) {
        let addr: SocketAddr = addr.parse().expect("Failed to parse server address");
        let mut stream = await!(TcpStream::connect(&addr).compat()).expect("Connection Failed");

        // TODO generalize this and pass in codec object externally
        let mut lines =
            LinesCodec::new_with_max_length(stratum::v1::rpc::MAX_MESSAGE_LENGTH).framed(stream);

        // We will be handling request and response asynchronously
        let (mut lines_tx, mut lines_rx) = lines.split();

        tokio::spawn_async(Self::run_rx(self.req_map.clone(), lines_rx));
        await!(self.run_tx(lines_tx));
    }
}

impl<REQ, RESP> Dispatcher<REQ, RESP> {
    const BUFFER_SIZE: usize = 1024 * 1024;

//    pub fn new_server() -> (Dispatcher<REQ, RESP>, Server<REQ, RESP>) {
//        let (queue_tx, queue_rx) = mpsc::channel(Self::BUFFER_SIZE);
//        let req_map = ReqMap::default();
//
//        let dispatcher = Dispatcher {
//            queue: queue_tx,
//            req_map: req_map.clone(),
//        };
//        let server = Server {
//            queue: queue_rx,
//            req_map
//        };
//
//        (dispatcher, server)
//    }

    pub fn new_client() -> (Dispatcher<REQ, RESP>, Client<REQ, RESP>) {
        let (queue_tx, queue_rx) = mpsc::channel(Self::BUFFER_SIZE);
        let req_map = ReqMap::default();

        let dispatcher = Dispatcher {
            queue: queue_tx,
        };
        let client = Client {
            queue: queue_rx,
            req_map,
            req_id: ReqId::new(),
        };

        (dispatcher, client)
    }

    /// TODO add reasonable error type + how to deal with timeouts, the response should be
    /// optional
    /// Add explicit timeout parameter or should the timeout be part of the dispatcher instance
    /// (common for all messages)?
    pub async fn send(&mut self, request: REQ) -> Result<RESP, ()> {
        let mut response: RESP;
        // Construct the channel for the response
        let (resp_tx, mut resp_rx) = oneshot::channel::<RESP>();

        await!(self.queue.send_async((request, resp_tx))).expect("Cannot send request");
        let response = await!(resp_rx.compat()).expect("Broken response channel");
        Ok(response)
    }
}

// impl<REQ, RESP> Stream for Client<REQ, RESP> {
//     type Item = REQ;
//     type Error = ();

//     fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
//        self.queue.poll().map_err(|_| ())   // XXX: error?
//     }
// }
