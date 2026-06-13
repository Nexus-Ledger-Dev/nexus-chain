//! Sync protocol implementation

use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use nexus_primitives::Hash;
use nexus_dag::{Dag, Vertex};
use crate::{NetworkResult, NetworkError, SyncRequest, SyncResponse};

/// Sync state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncState {
    /// Not syncing
    Idle,
    /// Syncing headers/vertices
    Syncing,
    /// Synced
    Synced,
}

/// Sync configuration
#[derive(Clone, Debug)]
pub struct SyncConfig {
    /// Batch size for sync requests
    pub batch_size: u32,
    /// Maximum concurrent requests
    pub max_concurrent: usize,
    /// Request timeout (seconds)
    pub timeout: u64,
    /// Minimum peers to sync from
    pub min_peers: usize,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            max_concurrent: 4,
            timeout: 30,
            min_peers: 1,
        }
    }
}

/// Sync manager
pub struct SyncManager {
    config: SyncConfig,
    dag: Arc<Dag>,
    state: SyncState,
    pending_requests: VecDeque<Hash>,
    in_flight: usize,
}

impl SyncManager {
    pub fn new(config: SyncConfig, dag: Arc<Dag>) -> Self {
        Self {
            config,
            dag,
            state: SyncState::Idle,
            pending_requests: VecDeque::new(),
            in_flight: 0,
        }
    }
    
    /// Handle sync request
    pub fn handle_request(&self, request: SyncRequest) -> SyncResponse {
        match request {
            SyncRequest::GetVertices(hashes) => {
                let vertices: Vec<Vertex> = hashes.iter()
                    .filter_map(|h| self.dag.get_vertex(h))
                    .collect();
                SyncResponse::Vertices(vertices)
            }
            
            SyncRequest::GetVerticesSince { height, limit } => {
                // Get vertices above height
                let vertices: Vec<Vertex> = self.dag.vertices_above_height(height)
                    .into_iter()
                    .take(limit as usize)
                    .collect();
                SyncResponse::Vertices(vertices)
            }
            
            SyncRequest::GetTransactions(_hashes) => {
                // Would need tx pool integration
                SyncResponse::Transactions(vec![])
            }
            
            SyncRequest::GetTips => {
                let tips = self.dag.tips();
                SyncResponse::Tips(tips)
            }
            
            SyncRequest::GetFinalized => {
                // Would need consensus integration
                SyncResponse::Finalized(None)
            }
        }
    }
    
    /// Handle sync response
    pub fn handle_response(&mut self, response: SyncResponse) -> NetworkResult<()> {
        match response {
            SyncResponse::Vertices(vertices) => {
                for vertex in vertices {
                    if let Err(e) = self.dag.add_vertex(vertex) {
                        debug!("Failed to add synced vertex: {:?}", e);
                    }
                }
                self.in_flight = self.in_flight.saturating_sub(1);
            }
            SyncResponse::Tips(tips) => {
                // Queue missing vertices for sync
                for tip in tips {
                    if self.dag.get_vertex(&tip).is_none() {
                        self.pending_requests.push_back(tip);
                    }
                }
            }
            SyncResponse::Error(msg) => {
                warn!("Sync error: {}", msg);
                self.in_flight = self.in_flight.saturating_sub(1);
            }
            _ => {}
        }
        
        Ok(())
    }
    
    /// Get next sync request
    pub fn next_request(&mut self) -> Option<SyncRequest> {
        if self.in_flight >= self.config.max_concurrent {
            return None;
        }
        
        // Batch pending requests
        let mut batch = Vec::new();
        while batch.len() < self.config.batch_size as usize {
            if let Some(hash) = self.pending_requests.pop_front() {
                batch.push(hash);
            } else {
                break;
            }
        }
        
        if batch.is_empty() {
            None
        } else {
            self.in_flight += 1;
            Some(SyncRequest::GetVertices(batch))
        }
    }
    
    /// Get sync state
    pub fn state(&self) -> SyncState {
        self.state
    }
    
    /// Check if synced
    pub fn is_synced(&self) -> bool {
        self.state == SyncState::Synced
    }
    
    /// Start sync process
    pub fn start_sync(&mut self) {
        self.state = SyncState::Syncing;
        info!("Starting sync");
    }
    
    /// Mark as synced
    pub fn mark_synced(&mut self) {
        self.state = SyncState::Synced;
        info!("Sync complete");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_sync_state() {
        let dag = Arc::new(Dag::new());
        let mut sync = SyncManager::new(SyncConfig::default(), dag);
        
        assert_eq!(sync.state(), SyncState::Idle);
        
        sync.start_sync();
        assert_eq!(sync.state(), SyncState::Syncing);
        
        sync.mark_synced();
        assert!(sync.is_synced());
    }
}
