//! Node implementation

use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn, error};

use nexus_dag::{Dag, DagConfig};
use nexus_evm::{StateDb, Executor, EvmEnv};
use nexus_consensus::{
    ProofOfStake, BftConsensus, ValidatorKeys,
    Proposer, ProposerConfig, TransactionPool,
};
use nexus_rpc::{RpcServer, RpcConfig as RpcServerConfig, MethodDispatcher, mempool::Mempool};
use nexus_network::PeerManager;

use crate::NodeConfig;
use crate::genesis::{load_genesis_file, write_genesis_file, export_genesis};

/// NexusChain node
pub struct Node {
    config: NodeConfig,
    dag: Arc<Dag>,
    state: Arc<StateDb>,
    pos: Arc<ProofOfStake>,
    tx_pool: Arc<TransactionPool>,
    /// Shared mempool — transactions submitted via RPC land here, then are
    /// bridged into tx_pool so the proposer can include them in vertices.
    mempool: Arc<Mempool>,
    peer_manager: Arc<PeerManager>,
    validator_keys: Option<ValidatorKeys>,
    shutdown_tx: Option<mpsc::Sender<()>>,
    #[cfg(feature = "p2p-network")]
    network_handle: Option<nexus_network::service::NetworkHandle>,
}

impl Node {
    /// Create new node
    pub fn new(config: NodeConfig) -> Result<Self, NodeError> {
        info!("Initializing NexusChain node: {}", config.name);
        
        // Create core components
        let dag = Arc::new(Dag::new(DagConfig::default()));
        let state = Arc::new(StateDb::new());
        let pos = Arc::new(ProofOfStake::default());
        let tx_pool = Arc::new(TransactionPool::default());
        let mempool = Arc::new(Mempool::new(10_000));
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
            mempool,
            peer_manager,
            validator_keys,
            shutdown_tx: None,
            #[cfg(feature = "p2p-network")]
            network_handle: None,
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

        // Apply genesis state if a genesis file exists (first-run).
        let genesis_path = self.config.data_dir.join("genesis.json");
        if genesis_path.exists() {
            info!("Loading genesis from {}", genesis_path.display());
            load_genesis_file(&genesis_path, &self.state, &self.pos)
                .map_err(|e| NodeError::Config(format!("Genesis error: {}", e)))?;
        }

        // Start RPC server
        if self.config.rpc.http_enabled {
            self.start_rpc().await?;
        }
        
        // Start validator if enabled
        if self.config.consensus.validator {
            self.start_validator(shutdown_rx).await?;
        }

        // Start P2P network service (requires the `p2p-network` feature)
        #[cfg(feature = "p2p-network")]
        {
            use nexus_network::{NetworkMessage, service::{NetworkConfig, start_network}};

            let (msg_tx, mut msg_rx) = mpsc::channel::<NetworkMessage>(1024);

            // Forward incoming transactions to the local mempool.
            let net_mempool = self.mempool.clone();
            tokio::spawn(async move {
                while let Some(msg) = msg_rx.recv().await {
                    if let NetworkMessage::NewTransaction(tx) = msg {
                        net_mempool.add(tx);
                    }
                }
            });

            let listen_addr = self.config.network.listen_addr
                .parse()
                .unwrap_or_else(|_| "/ip4/0.0.0.0/tcp/30333".parse().unwrap());

            let bootstrap_nodes = self.config.network.bootstrap_nodes.clone();
            if !bootstrap_nodes.is_empty() {
                info!("Using {} bootstrap peer(s)", bootstrap_nodes.len());
            }

            let net_cfg = NetworkConfig {
                listen_addr,
                bootstrap_nodes,
                max_message_size: 1024 * 1024,
            };

            match start_network(net_cfg, self.peer_manager.clone(), msg_tx).await {
                Ok(handle) => {
                    info!("P2P network service started on {}", self.config.network.listen_addr);
                    self.network_handle = Some(handle);
                }
                Err(e) => {
                    warn!("Failed to start P2P network service: {:?}", e);
                }
            }
        }

        info!("NexusChain node started successfully");
        Ok(())
    }
    
    /// Start RPC server
    async fn start_rpc(&self) -> Result<(), NodeError> {
        let mempool = self.mempool.clone();
        // Peer count is read from the shared atomic that the network layer increments.
        let peer_count = Arc::new(std::sync::atomic::AtomicUsize::new(
            self.peer_manager.connected_count(),
        ));
        let dispatcher = MethodDispatcher::new(
            self.dag.clone(),
            self.state.clone(),
            self.pos.clone(),
            Arc::new(Box::new(nexus_zkp::DefaultVerifier::default())),
            self.config.chain_id,
            mempool,
            peer_count,
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
    async fn start_validator(&self, _shutdown_rx: mpsc::Receiver<()>) -> Result<(), NodeError> {
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

        let mut proposer = Proposer::new(
            proposer_config,
            keys.address().clone(),
            self.tx_pool.clone(),
            self.dag.clone(),
            bft.clone(),
            self.pos.clone(),
        );

        // Channel: proposer → EVM execution task.
        // The proposer sends each finalized vertex here; the execution task runs
        // its transactions through the EVM in order.
        let (vertex_tx, mut vertex_rx) = mpsc::channel(64);
        proposer.set_vertex_channel(vertex_tx);

        // Execution task: apply transactions from finalized vertices to the EVM.
        let exec_state = self.state.clone();
        let exec_chain_id = self.config.chain_id;
        let exec_coinbase = keys.address().clone();
        tokio::spawn(async move {
            while let Some(proposed) = vertex_rx.recv().await {
                let vertex = &proposed.vertex;
                let env = EvmEnv {
                    chain_id: exec_chain_id,
                    number: vertex.height,
                    timestamp: vertex.timestamp,
                    coinbase: exec_coinbase.clone(),
                    ..Default::default()
                };
                let mut executor = Executor::new(exec_state.clone(), env);
                for tx in &proposed.transactions {
                    match executor.execute_tx(tx) {
                        Ok(receipt) => {
                            info!(
                                "Executed tx {:?}: status={:?} gas_used={}",
                                tx.hash(), receipt.status, receipt.gas_used
                            );
                        }
                        Err(e) => {
                            warn!("Failed to execute tx {:?}: {:?}", tx.hash(), e);
                        }
                    }
                }
            }
        });

        // Bridge task: drain RPC mempool → proposer tx_pool every 100 ms.
        let bridge_mempool = self.mempool.clone();
        let bridge_tx_pool = self.tx_pool.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
            loop {
                interval.tick().await;
                let txs = bridge_mempool.drain_batch(256);
                for tx in txs {
                    bridge_tx_pool.add(tx);
                }
            }
        });

        // Start proposer loop
        let (_prop_shutdown_tx, prop_shutdown_rx) = mpsc::channel(1);

        tokio::spawn(async move {
            proposer.run(prop_shutdown_rx).await;
        });

        info!("Validator started");
        Ok(())
    }
    
    /// Stop the node
    pub async fn stop(&mut self) {
        info!("Stopping NexusChain node");

        #[cfg(feature = "p2p-network")]
        if let Some(handle) = self.network_handle.take() {
            handle.shutdown().await;
        }

        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }

        info!("NexusChain node stopped");
    }
    
    /// Export current state as a genesis file (useful for devnet snapshots).
    pub fn export_genesis_to_file(&self, path: &std::path::Path) -> Result<(), NodeError> {
        let config = export_genesis(
            &self.state,
            &self.pos,
            self.config.chain_id,
            &self.config.name,
        );
        write_genesis_file(path, &config)
            .map_err(|e| NodeError::Io(e.to_string()))
    }

    /// Get node status
    pub fn status(&self) -> NodeStatus {
        let dag_stats = self.dag.stats();

        NodeStatus {
            name: self.config.name.clone(),
            chain_id: self.config.chain_id,
            is_validator: self.config.consensus.validator,
            validator_address: self.validator_keys.as_ref()
                .map(|k| format!("{:?}", k.address())),
            dag_height: self.dag.latest_height(),
            total_vertices: dag_stats.total_vertices as u64,
            total_transactions: 0,
            tips_count: dag_stats.tips_count as u64,
            peers_count: self.peer_manager.connected_count() as u64,
            epoch: self.pos.epoch(),
            // Considered synced when we either have no peers (solo/devnet) or the
            // DAG has at least one vertex beyond genesis.
            synced: self.peer_manager.connected_count() == 0 || dag_stats.total_vertices > 0,
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
