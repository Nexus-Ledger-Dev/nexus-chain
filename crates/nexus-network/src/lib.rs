//! P2P Network Module for NexusChain
//!
//! Implements libp2p-based networking with:
//! - Gossipsub for message propagation
//! - Kademlia DHT for peer discovery
//! - Request-response for sync

pub mod protocol;
pub mod gossip;
pub mod sync;
pub mod peer;
#[cfg(feature = "p2p-network")]
pub mod service;

pub use protocol::*;
pub use gossip::*;
pub use sync::*;
pub use peer::*;

use thiserror::Error;

/// Result type for network operations
pub type NetworkResult<T> = Result<T, NetworkError>;

/// Network errors
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    
    #[error("Peer not found: {0}")]
    PeerNotFound(String),
    
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Timeout")]
    Timeout,
    
    #[error("Channel closed")]
    ChannelClosed,
}
