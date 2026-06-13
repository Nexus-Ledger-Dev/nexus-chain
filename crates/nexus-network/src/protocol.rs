//! Network protocol definitions

use serde::{Deserialize, Serialize};
use nexus_primitives::{Hash, Transaction};
use nexus_dag::Vertex;
use nexus_consensus::{BftMessage, Proposal};

/// Protocol version
pub const PROTOCOL_VERSION: u32 = 1;

/// Network message types
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NetworkMessage {
    /// New transaction announcement
    NewTransaction(Transaction),
    /// New vertex announcement
    NewVertex(Vertex),
    /// BFT consensus message
    Consensus(BftMessage),
    /// Sync request
    SyncRequest(SyncRequest),
    /// Sync response
    SyncResponse(SyncResponse),
    /// Ping/heartbeat
    Ping(u64),
    /// Pong response
    Pong(u64),
}

/// Sync request
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SyncRequest {
    /// Request vertices by hash
    GetVertices(Vec<Hash>),
    /// Request vertices after height
    GetVerticesSince { height: u64, limit: u32 },
    /// Request transactions
    GetTransactions(Vec<Hash>),
    /// Request tips
    GetTips,
    /// Request finalized vertex
    GetFinalized,
}

/// Sync response
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SyncResponse {
    /// Vertices response
    Vertices(Vec<Vertex>),
    /// Transactions response
    Transactions(Vec<Transaction>),
    /// Tips response
    Tips(Vec<Hash>),
    /// Finalized vertex hash
    Finalized(Option<Hash>),
    /// Error response
    Error(String),
}

/// Topic identifiers for gossipsub
pub mod topics {
    pub const TRANSACTIONS: &str = "/nexus/tx/1.0.0";
    pub const VERTICES: &str = "/nexus/vertex/1.0.0";
    pub const CONSENSUS: &str = "/nexus/consensus/1.0.0";
}

/// Protocol identifiers
pub mod protocols {
    pub const SYNC: &str = "/nexus/sync/1.0.0";
    pub const IDENTIFY: &str = "/nexus/id/1.0.0";
}
