use super::{
    booleans,
    node_info,
    Addr,
    NodeID,
    NodeInfo,
};
use crate::errors::{
    ErrorKind,
    Result,
};
use serde_bencode;
use serde_bytes::{
    self,
    ByteBuf,
};
use serde_derive::{
    Deserialize,
    Serialize,
};
use std::fmt;

// TODO: Rename to Envelope

/// Envelope holding information common to requests and responses
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Message {
    /// Public IP address of the requester. Only sent by peers supporting
    /// [BEP-0042].
    ///
    /// [BEP-0042]: http://www.bittorrent.org/beps/bep_0042.html
    pub ip: Option<Addr>,

    /// Transaction ID generated by the querying node and echoed in the
    /// response. Used to correlate requests and responses.
    #[serde(rename = "t", with = "serde_bytes")]
    pub transaction_id: Vec<u8>,

    /// Client version string
    #[serde(rename = "v")]
    pub version: Option<ByteBuf>,

    #[serde(flatten)]
    pub message_type: MessageType,

    /// Sent by read-only DHT nodes defined in [BEP-0043]
    ///
    /// [BEP-0043]: http://www.bittorrent.org/beps/bep_0043.html
    #[serde(
        rename = "ro",
        default,
        skip_serializing_if = "booleans::is_false",
        deserialize_with = "booleans::deserialize"
    )]
    pub read_only: bool,
}

impl Message {
    pub fn decode(bytes: &[u8]) -> Result<Message> {
        Ok(serde_bencode::de::from_bytes(bytes)
            .map_err(|cause| ErrorKind::DecodeError { cause })?)
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        Ok(serde_bencode::ser::to_bytes(self).map_err(|cause| ErrorKind::EncodeError { cause })?)
    }
}

// TODO: Rename to Message

/// Messages sent and received by nodes
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "y")]
pub enum MessageType {
    #[serde(rename = "q")]
    Query {
        #[serde(flatten)]
        query: Query,
    },

    #[serde(rename = "r")]
    Response {
        #[serde(rename = "r")]
        response: Response,
    },

    #[serde(rename = "e")]
    Error {
        #[serde(rename = "e")]
        error: KRPCError,
    },
}

/// Error sent when a query cannot be fulfilled
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct KRPCError(u8, String);

impl KRPCError {
    pub fn new(error_code: u8, message: &str) -> KRPCError {
        KRPCError(error_code, message.to_string())
    }
}

impl fmt::Display for KRPCError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Possible queries
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "q", content = "a")]
pub enum Query {
    /// Most basic query
    ///
    /// The appropriate response to a ping is [`Response::OnlyID`] with the node
    /// ID of the responding node.
    #[serde(rename = "ping")]
    Ping {
        /// Sender's node ID
        id: NodeID,
    },

    /// Used to find the contact information for a node given its ID.
    ///
    /// When a node receives this query, it should respond with a
    /// [`Response::NextHop`] with the target node or the K (8) closest good
    /// nodes in its own routing table.
    #[serde(rename = "find_node")]
    FindNode {
        /// Node ID of the querying node
        id: NodeID,

        /// ID of the node being searched for
        target: NodeID,
    },

    /// Get peers associated with a torrent infohash.
    ///
    /// If the queried node has no peers for the infohash, [`Response::NextHop`]
    /// will be returned containing the K nodes in the queried nodes routing
    /// table closest to the infohash supplied in the query. Otherwise,
    /// [`Response::GetPeers`] will be returned.
    ///
    /// In either case a `token` is included in the return value. The
    /// token value is a required argument for a future [Query::AnnouncePeer].
    /// The token value should be a short binary string.
    #[serde(rename = "get_peers")]
    GetPeers {
        /// Node ID of the querying node
        id: NodeID,

        /// Infohash of the torrent searching for peers of
        info_hash: NodeID,
    },

    /// Announce that the peer, controlling the querying node, is downloading a
    /// torrent on a port.
    ///
    /// The queried node must verify that the token was previously sent to the
    /// same IP address as the querying node. Then the queried node should
    /// store the IP address of the querying node and the supplied port
    /// number under the infohash in its store of peer contact information.
    #[serde(rename = "announce_peer")]
    AnnouncePeer {
        /// Node ID of the querying node
        id: NodeID,

        /// Whether or not the peer's port is implied by the source port of the
        /// UDP packet containing this query
        ///
        /// If `true`, the value of `port` should be ignored. This is useful for
        /// peers behind a NAT that may not know their external port, and
        /// supporting uTP, they accept incoming connections on the same port as
        /// the DHT port.
        #[serde(deserialize_with = "booleans::deserialize")]
        implied_port: bool,

        /// Peer's port
        port: Option<u16>,

        /// Infohash of the torrent being announced
        info_hash: NodeID,

        /// Token received in response to a previous [Query::GetPeers]
        #[serde(with = "serde_bytes")]
        token: Vec<u8>,
    },

    /// `sample_infohashes` query from [BEP-0051]
    ///
    /// [BEP-0051]: http://www.bittorrent.org/beps/bep_0051.html
    #[serde(rename = "sample_infohashes")]
    SampleInfoHashes {
        /// Node ID of the querying node
        id: NodeID,
        target: NodeID,
    },
}

/// Possible responses
///
/// See [`Query`] to understand when each variant is used.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum Response {
    NextHop {
        /// Identifier of queried node
        id: NodeID,

        /// Token used in [Query::AnnouncePeer]
        ///
        /// Empty when the responder decides we are unfit to send AnnouncePeer
        /// messages by [BEP-0042].
        ///
        /// [BEP-0042]: http://www.bittorrent.org/beps/bep_0042.html
        token: Option<Vec<u8>>,

        #[serde(with = "node_info")]
        nodes: Vec<NodeInfo>,
    },

    GetPeers {
        /// Identifier of queried node
        id: NodeID,

        /// Token used in [`Query::AnnouncePeer`]
        ///
        /// Empty when the responder decides we are unfit to send AnnouncePeer
        /// messages by [BEP-0042].
        ///
        /// [BEP-0042]: http://www.bittorrent.org/beps/bep_0042.html
        token: Option<Vec<u8>>,

        #[serde(rename = "values")]
        peers: Vec<Addr>,
    },

    /// Response to [`Query::Ping`] and [`Query::AnnouncePeer`]
    OnlyID {
        /// Identifier of queried node
        id: NodeID,
    },

    /// Response to [`Query::SampleInfoHashes`]
    Samples {
        /// Identifier of queried node
        id: NodeID,

        /// Number of seconds this node should not be queried again for
        interval: Option<u16>,

        /// Nodes close to target in request
        #[serde(with = "node_info")]
        nodes: Vec<NodeInfo>,

        /// Number of info hashes this peer has
        num: Option<u32>,

        /// Sample of info-hashes
        samples: Vec<NodeID>,
    },
}
