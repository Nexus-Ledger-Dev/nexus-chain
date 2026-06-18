//! Node configuration

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::net::SocketAddr;

/// Node configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Node name
    pub name: String,
    /// Data directory
    pub data_dir: PathBuf,
    /// Chain ID
    pub chain_id: u64,
    /// Network configuration
    pub network: NetworkConfig,
    /// RPC configuration
    pub rpc: RpcConfig,
    /// Consensus configuration
    pub consensus: ConsensusConfig,
    /// Storage configuration
    pub storage: StorageConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            name: "nexus-node".to_string(),
            data_dir: directories::ProjectDirs::from("io", "nexuschain", "nexus")
                .map(|d| d.data_dir().to_path_buf())
                .unwrap_or_else(|| PathBuf::from(".nexus")),
            chain_id: 1337,
            network: NetworkConfig::default(),
            rpc: RpcConfig::default(),
            consensus: ConsensusConfig::default(),
            storage: StorageConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

/// Network configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Listen address
    pub listen_addr: String,
    /// Bootstrap nodes
    pub bootstrap_nodes: Vec<String>,
    /// Maximum peers
    pub max_peers: usize,
    /// Enable discovery
    pub discovery: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addr: "/ip4/0.0.0.0/tcp/9000".to_string(),
            bootstrap_nodes: vec![],
            max_peers: 50,
            discovery: true,
        }
    }
}

/// RPC configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RpcConfig {
    /// Enable HTTP RPC
    pub http_enabled: bool,
    /// HTTP listen address
    pub http_addr: SocketAddr,
    /// Enable WebSocket RPC
    pub ws_enabled: bool,
    /// WebSocket listen address
    pub ws_addr: SocketAddr,
    /// Enable CORS
    pub cors: bool,
    /// Max connections
    pub max_connections: usize,
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            http_enabled: true,
            http_addr: "127.0.0.1:8545".parse().unwrap(),
            ws_enabled: true,
            ws_addr: "127.0.0.1:8546".parse().unwrap(),
            cors: true,
            max_connections: 100,
        }
    }
}

/// Consensus configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConsensusConfig {
    /// Enable validator mode
    pub validator: bool,
    /// Validator key file
    pub validator_key: Option<PathBuf>,
    /// Target vertex interval (ms)
    pub vertex_interval_ms: u64,
    /// Maximum transactions per vertex
    pub max_txs_per_vertex: usize,
    /// Epoch duration (seconds)
    pub epoch_duration: u64,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            validator: false,
            validator_key: None,
            vertex_interval_ms: 500,
            max_txs_per_vertex: 1000,
            epoch_duration: 86400,
        }
    }
}

/// Storage configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Database type
    pub db_type: DatabaseType,
    /// Cache size (MB)
    pub cache_size_mb: usize,
    /// Enable pruning
    pub pruning: bool,
    /// Pruning depth (epochs)
    pub pruning_depth: u64,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            db_type: DatabaseType::Memory,
            cache_size_mb: 256,
            pruning: false,
            pruning_depth: 30,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DatabaseType {
    Memory,
    RocksDb,
}

/// Logging configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    pub level: String,
    /// Log format (text or json)
    pub format: String,
    /// Log file
    pub file: Option<PathBuf>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "text".to_string(),
            file: None,
        }
    }
}

impl NodeConfig {
    /// Load configuration from file
    pub fn load(path: &PathBuf) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::IoError(e.to_string()))?;
        
        toml::from_str(&content)
            .map_err(|e| ConfigError::ParseError(e.to_string()))
    }
    
    /// Save configuration to file
    pub fn save(&self, path: &PathBuf) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::ParseError(e.to_string()))?;
        
        std::fs::write(path, content)
            .map_err(|e| ConfigError::IoError(e.to_string()))
    }
    
    /// Validate configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        // validator_key = None is fine — Node::new() auto-generates an ephemeral keypair
        Ok(())
    }
}

/// Configuration error
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    IoError(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Invalid configuration: {0}")]
    Invalid(String),
}
