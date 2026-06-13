//! Node implementation

use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn, error};

use nexus_primitives::Hash;
use nexus_dag::Dag;
use nexus_evm::StateDb;
use nexus_consensus::{
    ProofOfStake, BftConsensus, ValidatorKeys,
    Proposer, ProposerConfig, TransactionPool,
};
use nexus_rpc::{RpcServer, RpcConfig as RpcServerConfig, MethodDispatcher};
use nexus_network::{PeerManager, SyncManager, SyncConfig};

use crate::{NodeConfig, ConsensusConfig};

/// NexusChain node
pub struct Node {
    config: NodeConfig,
    dag: Arc<Dag>,
    state: Arc<StateDb>,
    pos: Arc<ProofOfStake>,
    tx_pool: Arc<TransactionPool>,
    peer_manager: Arc<PeerManager>,
    validator_keys: Option<ValidatorKeys>,
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl Node {
    /// Create new node
    pub fn new(config: NodeConfig) -> Result<Self, NodeError> {
        info!("Initializing NexusChain node: {}", config.name);
        
        // Create core components
        let dag = Arc::new(Dag::new());
        let state = Arc::new(StateDb::new());
        let pos = Arc::new(ProofOfStake::default());
        let tx_pool = Arc::new(TransactionPool::default());
        let peer_manager = Arc::new(PeerManager::new(config.network.max_peers));
        
        // Load validator keys if in validator mode
        let validator_keys = if config.consensus.validator {
            if let Some(key_path) = &config.consensus.validator_key {
                let key_bytes = std::fs::read(key_path)
                    .map_err(|e| NodeError::Config(format!("Failed to read validator key: {}", e)))?;
                Some(ValidatorKeys::from_bytes(&key_bytes)
                    .map_err(|e| NodeError::Config(format!("Invalid validator key: {:?}", e)))?)
            } else {
                // Generate new keys for testing
                warn!("No validator key specified, generating new keys");
                Some(ValidatorKeys::generate())
            }
        } else {
            None
        };
        
        Ok(Self {
            config,
            dag,
            state,
            pos,
            tx_pool,
            peer_manager,
            validator_keys,
            shutdown_tx: None,
        })
    }
    
    /// Start the node
    pub async fn start(&mut self) -> Result<(), NodeError> {
        info!("Starting NexusChain node");
        
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);
        
        // Create data directory
        std::fs::create_dir_all(&self.config.data_dir)
            .map_err(|e| NodeError::Io(e.to_string()))?;
        
        // Start RPC server
        if self.config.rpc.http_enabled {
            self.start_rpc().await?;
        }
        
        // Start validator if enabled
        if self.config.consensus.validator {
            self.start_validator(shutdown_rx).await?;
        }
        
        info!("NexusChain node started successfully");
        Ok(())
    }
    
    /// Start RPC server
    async fn start_rpc(&self) -> Result<(), NodeError> {
        let dispatcher = MethodDispatcher::new(
            self.dag.clone(),
            self.state.clone(),
            self.pos.clone(),
            Arc::new(Box::new(nexus_zkp::DefaultVerifier::default())),
            self.config.chain_id,
        );
        
        let rpc_config = RpcServerConfig {
            listen_addr: self.config.rpc.http_addr,
            cors: self.config.rpc.cors,
            max_request_size: 1024 * 1024,
        };
        
        let server = RpcServer::new(rpc_config, dispatcher);
        
        tokio::spawn(async move {
            if let Err(e) = server.run().await {
                error!("RPC server error: {:?}", e);
            }
        });
        
        info!("RPC server listening on {}", self.config.rpc.http_addr);
        Ok(())
    }
    
    /// Start validator
    async fn start_validator(&self, mut shutdown_rx: mpsc::Receiver<()>) -> Result<(), NodeError> {
        let keys = self.validator_keys.as_ref()
            .ok_or_else(|| NodeError::Config("Validator keys not loaded".into()))?;
        
        info!("Starting validator: {:?}", keys.address());
        
        // Register validator (in production, this would be done via staking contract)
        self.pos.register_validator(
            keys.address().clone(),
            keys.public_key(),
            100_000_000_000, // Initial stake
            500, // 5% commission
        ).map_err(|e| NodeError::Consensus(format!("{:?}", e)))?;
        
        // Create BFT consensus
        let validator_set = Arc::new(parking_lot::RwLock::new(self.pos.validator_set()));
        let bft = Arc::new(BftConsensus::new(keys.clone(), validator_set));
        
        // Create proposer
        let proposer_config = ProposerConfig {
            max_txs_per_vertex: self.config.consensus.max_txs_per_vertex,
            target_interval: std::time::Duration::from_millis(
                self.config.consensus.vertex_interval_ms
            ),
            ..Default::default()
        };
        
        let proposer = Proposer::new(
            proposer_config,
            keys.address().clone(),
            self.tx_pool.clone(),
            self.dag.clone(),
            bft.clone(),
            self.pos.clone(),
        );
        
        // Start proposer loop
        let (prop_shutdown_tx, prop_shutdown_rx) = mpsc::channel(1);
        
        tokio::spawn(async move {
            proposer.run(prop_shutdown_rx).await;
        });
        
        info!("Validator started");
        Ok(())
    }
    
    /// Stop the node
    pub async fn stop(&mut self) {
        info!("Stopping NexusChain node");
        
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
        
        info!("NexusChain node stopped");
    }
    
    /// Get node status
    pub fn status(&self) -> NodeStatus {
        let dag_metrics = self.dag.metrics();
        
        NodeStatus {
            name: self.config.name.clone(),
            chain_id: self.config.chain_id,
            is_validator: self.config.consensus.validator,
            validator_address: self.validator_keys.as_ref()
                .map(|k| format!("{:?}", k.address())),
            dag_height: self.dag.max_height(),
            total_vertices: dag_metrics.total_vertices as u64,
            total_transactions: dag_metrics.total_transactions as u64,
            tips_count: dag_metrics.tips_count as u64,
            peers_count: self.peer_manager.connected_count() as u64,
            epoch: self.pos.epoch(),
            synced: true, // TODO: Check sync status
        }
    }
    
    /// Get DAG reference
    pub fn dag(&self) -> &Arc<Dag> {
        &self.dag
    }
    
    /// Get state reference
    pub fn state(&self) -> &Arc<StateDb> {
        &self.state
    }
    
    /// Get transaction pool
    pub fn tx_pool(&self) -> &Arc<TransactionPool> {
        &self.tx_pool
    }
}

/// Node status
#[derive(Clone, Debug)]
pub struct NodeStatus {
    pub name: String,
    pub chain_id: u64,
    pub is_validator: bool,
    pub validator_address: Option<String>,
    pub dag_height: u64,
    pub total_vertices: u64,
    pub total_transactions: u64,
    pub tips_count: u64,
    pub peers_count: u64,
    pub epoch: u64,
    pub synced: bool,
}

/// Node error
#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("IO error: {0}")]
    Io(String),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Consensus error: {0}")]
    Consensus(String),
    
    #[error("Storage error: {0}")]
    Storage(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_node_creation() {
        let config = NodeConfig::default();
        let node = Node::new(config).unwrap();
        
        let status = node.status();
        assert_eq!(status.chain_id, 1337);
        assert!(!status.is_validator);
    }
}
