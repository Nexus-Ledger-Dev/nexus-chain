//! Sync protocol implementation

use std::collections::VecDeque;
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::{debug, info, warn};

use nexus_primitives::{Hash, Transaction};
use nexus_dag::{Dag, Vertex};
use crate::{NetworkResult, SyncRequest, SyncResponse};

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
    /// Callback to look up pending transactions by hash (injected from the mempool).
    tx_provider: Option<Arc<dyn Fn(&[Hash]) -> Vec<Transaction> + Send + Sync>>,
    /// Last BFT-finalized vertex hash, updated by the consensus layer.
    finalized_tip: Option<Arc<RwLock<Option<Hash>>>>,
}

impl SyncManager {
    pub fn new(config: SyncConfig, dag: Arc<Dag>) -> Self {
        Self {
            config,
            dag,
            state: SyncState::Idle,
            pending_requests: VecDeque::new(),
            in_flight: 0,
            tx_provider: None,
            finalized_tip: None,
        }
    }

    /// Provide a function that resolves transaction hashes against the local mempool.
    pub fn set_tx_provider<F>(&mut self, f: F)
    where
        F: Fn(&[Hash]) -> Vec<Transaction> + Send + Sync + 'static,
    {
        self.tx_provider = Some(Arc::new(f));
    }

    /// Share the finalized-tip pointer written by the BFT consensus engine.
    pub fn set_finalized_tip(&mut self, tip: Arc<RwLock<Option<Hash>>>) {
        self.finalized_tip = Some(tip);
    }
    
    /// Handle sync request
    pub fn handle_request(&self, request: SyncRequest) -> SyncResponse {
        match request {
            SyncRequest::GetVertices(hashes) => {
                let vertices: Vec<Vertex> = hashes.iter()
                    .filter_map(|h| self.dag.get_vertex(h).map(|v| (*v).clone()))
                    .collect();
                SyncResponse::Vertices(vertices)
            }

            SyncRequest::GetVerticesSince { height, limit } => {
                let mut vertices: Vec<Vertex> = Vec::new();
                let current_height = self.dag.latest_height();
                let mut h = height + 1;
                while h <= current_height && vertices.len() < limit as usize {
                    for hash in self.dag.get_vertices_at_height(h) {
                        if let Some(v) = self.dag.get_vertex(&hash) {
                            vertices.push((*v).clone());
                        }
                        if vertices.len() >= limit as usize {
                            break;
                        }
                    }
                    h += 1;
                }
                SyncResponse::Vertices(vertices)
            }
            
            SyncRequest::GetTransactions(hashes) => {
                let txs = match &self.tx_provider {
                    Some(provider) => provider(&hashes),
                    None => vec![],
                };
                SyncResponse::Transactions(txs)
            }

            SyncRequest::GetTips => {
                let tips = self.dag.tips();
                SyncResponse::Tips(tips)
            }

            SyncRequest::GetFinalized => {
                let hash = self.finalized_tip
                    .as_ref()
                    .and_then(|tip: &Arc<RwLock<Option<Hash>>>| *tip.read());
                SyncResponse::Finalized(hash)
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
        let dag = Arc::new(Dag::new(DagConfig::default()));
        let mut sync = SyncManager::new(SyncConfig::default(), dag);
        
        assert_eq!(sync.state(), SyncState::Idle);
        
        sync.start_sync();
        assert_eq!(sync.state(), SyncState::Syncing);
        
        sync.mark_synced();
        assert!(sync.is_synced());
    }
}
