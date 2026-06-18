//! Vertex proposer for DAG

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use nexus_primitives::{Address, Hash, Transaction, TxType};
use nexus_dag::{Dag, Vertex};
use crate::{ConsensusResult, ConsensusError, BftConsensus, ProofOfStake};

/// Configuration for vertex proposer
#[derive(Clone, Debug)]
pub struct ProposerConfig {
    /// Maximum transactions per vertex
    pub max_txs_per_vertex: usize,
    /// Maximum vertex size in bytes
    pub max_vertex_size: usize,
    /// Target vertex interval
    pub target_interval: Duration,
    /// Maximum parents per vertex
    pub max_parents: usize,
    /// Minimum parents required
    pub min_parents: usize,
}

impl Default for ProposerConfig {
    fn default() -> Self {
        Self {
            max_txs_per_vertex: 1000,
            max_vertex_size: 1024 * 1024, // 1 MB
            target_interval: Duration::from_millis(500),
            max_parents: 8,
            min_parents: 1,
        }
    }
}

/// Transaction pool
pub struct TransactionPool {
    /// Pending transactions
    pending: RwLock<VecDeque<Transaction>>,
    /// Maximum pool size
    max_size: usize,
}

impl TransactionPool {
    pub fn new(max_size: usize) -> Self {
        Self {
            pending: RwLock::new(VecDeque::new()),
            max_size,
        }
    }
    
    /// Add transaction to pool
    pub fn add(&self, tx: Transaction) -> bool {
        let mut pending = self.pending.write();
        
        if pending.len() >= self.max_size {
            return false;
        }
        
        // Check for duplicates
        let hash = tx.hash();
        if pending.iter().any(|t| t.hash() == hash) {
            return false;
        }
        
        pending.push_back(tx);
        true
    }
    
    /// Get transactions for new vertex
    pub fn get_pending(&self, max: usize) -> Vec<Transaction> {
        let pending = self.pending.read();
        pending.iter().take(max).cloned().collect()
    }
    
    /// Remove transactions that were included
    pub fn remove(&self, hashes: &[Hash]) {
        let mut pending = self.pending.write();
        pending.retain(|tx| !hashes.contains(&tx.hash()));
    }
    
    /// Pool size
    pub fn len(&self) -> usize {
        self.pending.read().len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.pending.read().is_empty()
    }
}

impl Default for TransactionPool {
    fn default() -> Self {
        Self::new(100_000)
    }
}

/// Proposed vertex
#[derive(Clone, Debug)]
pub struct ProposedVertex {
    pub vertex: Vertex,
    pub transactions: Vec<Transaction>,
}

/// Vertex proposer
pub struct Proposer {
    /// Configuration
    config: ProposerConfig,
    /// Our address
    address: Address,
    /// Transaction pool
    tx_pool: Arc<TransactionPool>,
    /// DAG reference
    dag: Arc<Dag>,
    /// BFT consensus
    bft: Arc<BftConsensus>,
    /// Proof of stake
    pos: Arc<ProofOfStake>,
    /// Last proposal time
    last_proposal: RwLock<Instant>,
    /// Proposed vertices channel
    vertex_tx: Option<mpsc::Sender<ProposedVertex>>,
}

impl Proposer {
    /// Create new proposer
    pub fn new(
        config: ProposerConfig,
        address: Address,
        tx_pool: Arc<TransactionPool>,
        dag: Arc<Dag>,
        bft: Arc<BftConsensus>,
        pos: Arc<ProofOfStake>,
    ) -> Self {
        Self {
            config,
            address,
            tx_pool,
            dag,
            bft,
            pos,
            last_proposal: RwLock::new(Instant::now()),
            vertex_tx: None,
        }
    }
    
    /// Set vertex channel
    pub fn set_vertex_channel(&mut self, tx: mpsc::Sender<ProposedVertex>) {
        self.vertex_tx = Some(tx);
    }
    
    /// Check if we should propose
    pub fn should_propose(&self) -> bool {
        // Check if we're the weighted leader
        if !self.bft.is_weighted_leader(&self.address) {
            return false;
        }
        
        // Check time since last proposal
        let elapsed = self.last_proposal.read().elapsed();
        if elapsed < self.config.target_interval {
            return false;
        }
        
        // Check if we have transactions
        if self.tx_pool.is_empty() {
            return false;
        }
        
        true
    }
    
    /// Create new vertex proposal
    pub fn create_proposal(&self) -> ConsensusResult<ProposedVertex> {
        debug!("Creating new vertex proposal");
        
        // Select parent tips
        let tips = self.dag.get_tips();
        let parents: Vec<Hash> = tips.into_iter()
            .take(self.config.max_parents)
            .collect();
        
        if parents.len() < self.config.min_parents {
            return Err(ConsensusError::InvalidProposal(
                "Not enough parent tips available".into()
            ));
        }
        
        // Get transactions from pool
        let transactions = self.tx_pool.get_pending(self.config.max_txs_per_vertex);
        // ISO‑8583 / ISO‑20022 validation – any IsoPayment must carry iso_data
        for tx in &transactions {
            if tx.tx_type == TxType::IsoPayment && tx.iso_data.is_none() {
                return Err(ConsensusError::InvalidProposal(
                    "ISO payment transaction missing iso_data".into(),
                ));
            }
        }
        let tx_hashes: Vec<Hash> = transactions.iter().map(|tx| tx.hash()).collect();
        
        // Calculate vertex height (max parent height + 1)
        let height = parents.iter()
            .filter_map(|h| self.dag.get_vertex(h))
            .map(|v| v.height)
            .max()
            .unwrap_or(0) + 1;
        
        // Create vertex
        let vertex = Vertex::new(
            self.address.clone(),
            parents.clone(),
            tx_hashes,
            self.pos.epoch(),
            height,
        );
        
        *self.last_proposal.write() = Instant::now();
        
        info!(
            "Created vertex proposal: hash={:?}, height={}, txs={}",
            vertex.hash, height, transactions.len()
        );
        
        Ok(ProposedVertex {
            vertex,
            transactions,
        })
    }
    
    /// Submit vertex for consensus
    pub async fn submit_proposal(&self, proposed: ProposedVertex) -> ConsensusResult<Hash> {
        let hash = proposed.vertex.hash.clone();
        
        // Add to DAG
        self.dag.add_vertex(proposed.vertex.clone())
            .map_err(|e| ConsensusError::InvalidProposal(format!("{:?}", e)))?;
        
        // Start BFT consensus
        let parents = proposed.vertex.parents.clone();
        self.bft.propose(hash.clone(), parents)?;
        
        // Remove included transactions from pool
        let tx_hashes: Vec<Hash> = proposed.transactions.iter()
            .map(|tx| tx.hash())
            .collect();
        self.tx_pool.remove(&tx_hashes);
        
        // Notify via channel
        if let Some(tx) = &self.vertex_tx {
            let _ = tx.send(proposed).await;
        }
        
        Ok(hash)
    }
    
    /// Main proposer loop
    pub async fn run(&self, mut shutdown: mpsc::Receiver<()>) {
        info!("Starting vertex proposer for {:?}", self.address);
        
        let mut interval = tokio::time::interval(self.config.target_interval / 2);
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if self.should_propose() {
                        match self.create_proposal() {
                            Ok(proposed) => {
                                if let Err(e) = self.submit_proposal(proposed).await {
                                    warn!("Failed to submit proposal: {:?}", e);
                                }
                            }
                            Err(e) => {
                                debug!("Cannot create proposal: {:?}", e);
                            }
                        }
                    }
                }
                _ = shutdown.recv() => {
                    info!("Proposer shutting down");
                    break;
                }
            }
        }
    }
}

/// Transaction prioritization
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxPriority {
    /// High priority (high gas price, compliance transactions)
    High,
    /// Normal priority
    Normal,
    /// Low priority
    Low,
}

impl TxPriority {
    /// Calculate priority from transaction
    pub fn from_tx(tx: &Transaction) -> Self {
        // Use gas price as primary factor
        let gas_price_gwei = tx.gas_price.as_u64() / 1_000_000_000;
        
        if gas_price_gwei > 100 {
            TxPriority::High
        } else if gas_price_gwei > 10 {
            TxPriority::Normal
        } else {
            TxPriority::Low
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_primitives::{Nonce, U256};
    
    #[test]
    fn test_transaction_pool() {
        let pool = TransactionPool::new(100);
        
        let tx = Transaction::new_legacy(
            1, // chain_id
            Nonce::default(),
            None, // to
            U256::ZERO, // value
            Vec::new(), // data
            21000, // gas_limit
            U256::from(1_000_000_000u64), // gas_price 1 gwei
        );
        assert!(pool.add(tx.clone()));
        assert_eq!(pool.len(), 1);
        
        // Duplicate should fail
        assert!(!pool.add(tx.clone()));
        assert_eq!(pool.len(), 1);
        
        // Get pending
        let pending = pool.get_pending(10);
        assert_eq!(pending.len(), 1);
        
        // Remove
        pool.remove(&[tx.hash()]);
        assert!(pool.is_empty());
    }
    
    #[test]
    fn test_tx_priority() {
        let mut tx = Transaction::new_legacy(0, 0, None, U256::ZERO, vec![], 21_000, U256::ZERO);
        
        tx.gas_price = nexus_primitives::U256::from(5_000_000_000u64); // 5 gwei
        assert_eq!(TxPriority::from_tx(&tx), TxPriority::Low);
        
        tx.gas_price = nexus_primitives::U256::from(50_000_000_000u64); // 50 gwei
        assert_eq!(TxPriority::from_tx(&tx), TxPriority::Normal);
        
        tx.gas_price = nexus_primitives::U256::from(200_000_000_000u64); // 200 gwei
        assert_eq!(TxPriority::from_tx(&tx), TxPriority::High);
    }
}
