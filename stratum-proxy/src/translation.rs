use futures::channel::mpsc;

use stratum;
use stratum::v1;
use stratum::v1::framing::codec::V1Framing;
use stratum::v1::{V1Handler, V1Protocol};
use stratum::v2;
use stratum::v2::framing::codec::V2Framing;
use stratum::v2::framing::MessageType;
use stratum::v2::{V2Handler, V2Protocol};
use stratum::LOGGER;

use stratum::v2::messages::SetupMiningConnection;
use wire::{Message, TxFrame};

#[cfg(test)]
mod test;

/// TODO consider whether the v1/v2 TX channels should use a 'Message'. Currently the reason
/// for not doing that is that we want to prevent dynamic dispatch when serializing a particular
/// message
#[derive(Debug)]
pub struct V2ToV1Translation {
    /// Connection URL that the client reported when opening connection
    conn_details: Option<SetupMiningConnection>,

    /// Channel for sending out V1 responses
    v1_tx: mpsc::Sender<TxFrame>,
    /// Channel for sending out V2 responses
    v2_tx: mpsc::Sender<TxFrame>,
}

impl V2ToV1Translation {
    const PROTOCOL_VERSION: usize = 0;
    const MAX_EXTRANONCE_SIZE: usize = 0;

    pub fn new(v1_tx: mpsc::Sender<TxFrame>, v2_tx: mpsc::Sender<TxFrame>) -> Self {
        Self {
            conn_details: None,
            v1_tx,
            v2_tx,
        }
    }

    fn v2_send<T>(&mut self, msg: T)
    where
        T: Into<TxFrame>,
    {
        self.v2_tx
            .try_send(msg.into())
            .expect("Cannot send message")
    }
}

impl V1Handler for V2ToV1Translation {}

impl V2Handler for V2ToV1Translation {
    fn visit_setup_mining_connection(
        &mut self,
        msg: &Message<V2Protocol>,
        payload: &v2::messages::SetupMiningConnection,
    ) {
        self.conn_details = Some(payload.clone());

        let response = v2::messages::SetupMiningConnectionSuccess {
            used_protocol_version: Self::PROTOCOL_VERSION as u16,
            max_extranonce_size: Self::MAX_EXTRANONCE_SIZE as u16,
            // TODO provide public key for TOFU
            pub_key: vec![1, 2, 3, 4, 5],
        };
        self.v2_send(response);
    }

    fn visit_setup_mining_connection_success(
        &mut self,
        _msg: &Message<V2Protocol>,
        _payload: &v2::messages::SetupMiningConnectionSuccess,
    ) {
    }
}
