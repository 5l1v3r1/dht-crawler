use errors::{Error, ErrorKind, Result};
use failure::ResultExt;

use proto::{NodeID, Query};

use rand;

use std;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use byteorder::{NetworkEndian, WriteBytesExt};

use tokio::prelude::*;

use transport::messages::{
    FindNodeResponse, GetPeersResponse, NodeIDResponse, PortType, Request, Response, TransactionId,
};
use transport::response::{ResponseFuture, TransactionMap};

pub struct SendTransport {
    socket: std::net::UdpSocket,

    /// Collection of in-flight transactions awaiting a response
    transactions: Arc<Mutex<TransactionMap>>,
}

impl SendTransport {
    pub fn new(
        socket: std::net::UdpSocket,
        transactions: Arc<Mutex<TransactionMap>>,
    ) -> SendTransport {
        SendTransport {
            socket,
            transactions,
        }
    }

    pub fn request(
        &self,
        address: SocketAddr,
        transaction_id: TransactionId,
        request: Request,
    ) -> impl Future<Item = Response, Error = Error> {
        let transaction_future_result =
            ResponseFuture::wait_for_tx(transaction_id, self.transactions.clone());

        self.send_request(address, transaction_id, request)
            .into_future()
            .and_then(move |_| transaction_future_result)
            .and_then(|fut| fut)
            .and_then(|envelope| Response::from(envelope))
    }

    /// Synchronously sends a request to `address`.
    ///
    /// The sending is done synchronously because doing it asynchronously was cumbersome and didn't
    /// make anything faster. UDP sending rarely blocks.
    fn send_request(
        &self,
        address: SocketAddr,
        transaction_id: TransactionId,
        mut request: Request,
    ) -> Result<()> {
        request
            .transaction_id
            .write_u32::<NetworkEndian>(transaction_id)
            .with_context(|_| ErrorKind::SendError { to: address })?;

        let encoded = request.encode()?;

        self.socket
            .send_to(&encoded, &address)
            .with_context(|_| ErrorKind::SendError { to: address })?;

        Ok(())
    }

    fn get_transaction_id() -> TransactionId {
        rand::random::<TransactionId>()
    }

    fn build_request(query: Query) -> Request {
        Request {
            transaction_id: Vec::new(),
            version: None,
            query,
        }
    }

    pub fn ping(
        &self,
        id: NodeID,
        address: SocketAddr,
    ) -> impl Future<Item = NodeID, Error = Error> {
        self.request(
            address,
            Self::get_transaction_id(),
            Self::build_request(Query::Ping { id }),
        ).and_then(NodeIDResponse::from_response)
    }

    pub fn find_node(
        &self,
        id: NodeID,
        address: SocketAddr,
        target: NodeID,
    ) -> impl Future<Item = FindNodeResponse, Error = Error> {
        self.request(
            address,
            Self::get_transaction_id(),
            Self::build_request(Query::FindNode { id, target }),
        ).and_then(FindNodeResponse::from_response)
    }

    pub fn get_peers(
        &self,
        id: NodeID,
        address: SocketAddr,
        info_hash: NodeID,
    ) -> impl Future<Item = GetPeersResponse, Error = Error> {
        self.request(
            address,
            Self::get_transaction_id(),
            Self::build_request(Query::GetPeers { id, info_hash }),
        ).and_then(GetPeersResponse::from_response)
    }

    pub fn announce_peer(
        &self,
        id: NodeID,
        token: Vec<u8>,
        address: SocketAddr,
        info_hash: NodeID,
        port_type: PortType,
    ) -> impl Future<Item = NodeID, Error = Error> {
        let (port, implied_port) = match port_type {
            PortType::Implied => (None, 1),
            PortType::Port(port) => (Some(port), 0),
        };

        self.request(
            address,
            Self::get_transaction_id(),
            Self::build_request(Query::AnnouncePeer {
                id,
                token,
                info_hash,
                port,
                implied_port,
            }),
        ).and_then(NodeIDResponse::from_response)
    }
}