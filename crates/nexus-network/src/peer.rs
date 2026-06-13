//! Peer management

use std::collections::HashMap;
use std::time::{Duration, Instant};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use nexus_primitives::Address;

/// Peer identifier (libp2p PeerId serialized)
pub type PeerId = String;

/// Peer information
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Peer ID
    pub id: PeerId,
    /// Multiaddresses
    pub addresses: Vec<String>,
    /// Protocol version
    pub protocol_version: u32,
    /// Agent string
    pub agent: String,
    /// Is validator
    pub is_validator: bool,
    /// Validator address (if validator)
    pub validator_address: Option<Address>,
    /// Last seen timestamp
    pub last_seen: u64,
    /// Connection state
    pub state: PeerState,
    /// Reputation score
    pub reputation: i32,
}

/// Peer connection state
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PeerState {
    /// Not connected
    Disconnected,
    /// Connecting
    Connecting,
    /// Connected
    Connected,
    /// Banned
    Banned,
}

/// Peer statistics
#[derive(Clone, Debug, Default)]
pub struct PeerStats {
    /// Messages received
    pub messages_received: u64,
    /// Messages sent
    pub messages_sent: u64,
    /// Bytes received
    pub bytes_received: u64,
    /// Bytes sent
    pub bytes_sent: u64,
    /// Latency (ms)
    pub latency_ms: u32,
    /// Request failures
    pub failures: u32,
}

/// Peer manager
pub struct PeerManager {
    /// Known peers
    peers: DashMap<PeerId, PeerInfo>,
    /// Peer statistics
    stats: DashMap<PeerId, PeerStats>,
    /// Maximum peers
    max_peers: usize,
    /// Ban duration
    ban_duration: Duration,
}

impl PeerManager {
    pub fn new(max_peers: usize) -> Self {
        Self {
            peers: DashMap::new(),
            stats: DashMap::new(),
            max_peers,
            ban_duration: Duration::from_secs(3600), // 1 hour
        }
    }
    
    /// Add or update peer
    pub fn add_peer(&self, peer: PeerInfo) {
        if self.peers.len() >= self.max_peers {
            // Remove disconnected peers first
            self.peers.retain(|_, p| p.state != PeerState::Disconnected);
        }
        
        self.peers.insert(peer.id.clone(), peer);
    }
    
    /// Get peer info
    pub fn get_peer(&self, id: &PeerId) -> Option<PeerInfo> {
        self.peers.get(id).map(|p| p.clone())
    }
    
    /// Update peer state
    pub fn set_state(&self, id: &PeerId, state: PeerState) {
        if let Some(mut peer) = self.peers.get_mut(id) {
            peer.state = state;
        }
    }
    
    /// Ban peer
    pub fn ban_peer(&self, id: &PeerId, reason: &str) {
        if let Some(mut peer) = self.peers.get_mut(id) {
            peer.state = PeerState::Banned;
            peer.reputation = -1000;
        }
        tracing::warn!("Banned peer {}: {}", id, reason);
    }
    
    /// Update reputation
    pub fn update_reputation(&self, id: &PeerId, delta: i32) {
        if let Some(mut peer) = self.peers.get_mut(id) {
            peer.reputation = (peer.reputation + delta).max(-1000).min(1000);
            
            // Auto-ban if reputation too low
            if peer.reputation <= -100 {
                peer.state = PeerState::Banned;
            }
        }
    }
    
    /// Get connected peers
    pub fn connected_peers(&self) -> Vec<PeerInfo> {
        self.peers.iter()
            .filter(|e| e.state == PeerState::Connected)
            .map(|e| e.clone())
            .collect()
    }
    
    /// Get validator peers
    pub fn validator_peers(&self) -> Vec<PeerInfo> {
        self.peers.iter()
            .filter(|e| e.state == PeerState::Connected && e.is_validator)
            .map(|e| e.clone())
            .collect()
    }
    
    /// Get peers sorted by reputation
    pub fn best_peers(&self, count: usize) -> Vec<PeerInfo> {
        let mut peers: Vec<_> = self.connected_peers();
        peers.sort_by(|a, b| b.reputation.cmp(&a.reputation));
        peers.into_iter().take(count).collect()
    }
    
    /// Record message sent
    pub fn record_sent(&self, id: &PeerId, bytes: u64) {
        self.stats.entry(id.clone())
            .or_insert_with(PeerStats::default)
            .messages_sent += 1;
        
        if let Some(mut stats) = self.stats.get_mut(id) {
            stats.bytes_sent += bytes;
        }
    }
    
    /// Record message received
    pub fn record_received(&self, id: &PeerId, bytes: u64) {
        self.stats.entry(id.clone())
            .or_insert_with(PeerStats::default)
            .messages_received += 1;
        
        if let Some(mut stats) = self.stats.get_mut(id) {
            stats.bytes_received += bytes;
        }
    }
    
    /// Get peer statistics
    pub fn get_stats(&self, id: &PeerId) -> Option<PeerStats> {
        self.stats.get(id).map(|s| s.clone())
    }
    
    /// Number of connected peers
    pub fn connected_count(&self) -> usize {
        self.peers.iter()
            .filter(|e| e.state == PeerState::Connected)
            .count()
    }
    
    /// Total peer count
    pub fn total_count(&self) -> usize {
        self.peers.len()
    }
    
    /// Cleanup stale peers
    pub fn cleanup(&self, max_age: Duration) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        self.peers.retain(|_, p| {
            // Keep connected peers
            if p.state == PeerState::Connected {
                return true;
            }
            // Keep recently seen
            now - p.last_seen < max_age.as_secs()
        });
    }
}

impl Default for PeerManager {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_peer_manager() {
        let manager = PeerManager::new(10);
        
        let peer = PeerInfo {
            id: "peer1".to_string(),
            addresses: vec!["/ip4/127.0.0.1/tcp/9000".to_string()],
            protocol_version: 1,
            agent: "nexus/1.0".to_string(),
            is_validator: false,
            validator_address: None,
            last_seen: 0,
            state: PeerState::Connected,
            reputation: 0,
        };
        
        manager.add_peer(peer.clone());
        
        assert_eq!(manager.connected_count(), 1);
        assert_eq!(manager.total_count(), 1);
        
        manager.update_reputation(&peer.id, 10);
        let info = manager.get_peer(&peer.id).unwrap();
        assert_eq!(info.reputation, 10);
    }
}
