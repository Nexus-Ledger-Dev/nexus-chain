//! Gossip protocol implementation

use std::collections::HashSet;
use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use nexus_primitives::Hash;
use crate::{NetworkMessage, NetworkResult, NetworkError};

/// Message deduplication cache
pub struct MessageCache {
    /// Seen message hashes
    seen: DashMap<Hash, u64>, // hash -> timestamp
    /// Maximum cache size
    max_size: usize,
    /// TTL in seconds
    ttl: u64,
}

impl MessageCache {
    pub fn new(max_size: usize, ttl: u64) -> Self {
        Self {
            seen: DashMap::new(),
            max_size,
            ttl,
        }
    }
    
    /// Check if message was seen
    pub fn is_seen(&self, hash: &Hash) -> bool {
        self.seen.contains_key(hash)
    }
    
    /// Mark message as seen
    pub fn mark_seen(&self, hash: Hash) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        self.seen.insert(hash, now);
        
        // Prune if needed
        if self.seen.len() > self.max_size {
            self.prune(now);
        }
    }
    
    /// Prune old entries
    fn prune(&self, now: u64) {
        self.seen.retain(|_, &mut ts| now - ts < self.ttl);
    }
}

impl Default for MessageCache {
    fn default() -> Self {
        Self::new(100_000, 300) // 100k messages, 5 min TTL
    }
}

/// Gossip configuration
#[derive(Clone, Debug)]
pub struct GossipConfig {
    /// Maximum message size
    pub max_message_size: usize,
    /// Mesh degree (D)
    pub mesh_d: usize,
    /// Low watermark (D_lo)
    pub mesh_d_lo: usize,
    /// High watermark (D_hi)
    pub mesh_d_hi: usize,
    /// Fanout TTL
    pub fanout_ttl: u64,
    /// Heartbeat interval (seconds)
    pub heartbeat_interval: u64,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            max_message_size: 1024 * 1024, // 1 MB
            mesh_d: 6,
            mesh_d_lo: 4,
            mesh_d_hi: 12,
            fanout_ttl: 60,
            heartbeat_interval: 1,
        }
    }
}

/// Gossip handler
pub struct GossipHandler {
    config: GossipConfig,
    cache: MessageCache,
    message_tx: mpsc::Sender<NetworkMessage>,
}

impl GossipHandler {
    pub fn new(config: GossipConfig, message_tx: mpsc::Sender<NetworkMessage>) -> Self {
        Self {
            config,
            cache: MessageCache::default(),
            message_tx,
        }
    }
    
    /// Handle incoming gossip message
    pub async fn handle_message(&self, data: &[u8]) -> NetworkResult<()> {
        // Check size
        if data.len() > self.config.max_message_size {
            return Err(NetworkError::ProtocolError("Message too large".into()));
        }
        
        // Deserialize
        let message: NetworkMessage = bincode::deserialize(data)
            .map_err(|e| NetworkError::SerializationError(e.to_string()))?;
        
        // Get message hash for dedup
        let hash = self.message_hash(&message);
        
        // Check if seen
        if self.cache.is_seen(&hash) {
            debug!("Ignoring duplicate message");
            return Ok(());
        }
        
        // Mark as seen
        self.cache.mark_seen(hash);
        
        // Forward to handler
        self.message_tx.send(message).await
            .map_err(|_| NetworkError::ChannelClosed)?;
        
        Ok(())
    }
    
    /// Compute message hash for deduplication
    fn message_hash(&self, message: &NetworkMessage) -> Hash {
        let data = bincode::serialize(message).unwrap_or_default();
        Hash::from(sha3::Keccak256::digest(&data).as_slice())
    }
    
    /// Serialize message for sending
    pub fn serialize_message(&self, message: &NetworkMessage) -> NetworkResult<Vec<u8>> {
        bincode::serialize(message)
            .map_err(|e| NetworkError::SerializationError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_message_cache() {
        let cache = MessageCache::new(100, 60);
        let hash = Hash::from([1u8; 32]);
        
        assert!(!cache.is_seen(&hash));
        cache.mark_seen(hash.clone());
        assert!(cache.is_seen(&hash));
    }
}
