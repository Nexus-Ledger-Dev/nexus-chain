//! Command-line interface

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// NexusChain - A DAG-based Layer 1 blockchain with EVM compatibility
#[derive(Parser, Debug)]
#[command(name = "nexus")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Configuration file
    #[arg(short, long, value_name = "FILE")]
    pub config: Option<PathBuf>,
    
    /// Data directory
    #[arg(short, long, value_name = "DIR")]
    pub data_dir: Option<PathBuf>,
    
    /// Log level
    #[arg(long, default_value = "info")]
    pub log_level: String,
    
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run the node
    Run {
        /// Enable validator mode
        #[arg(long)]
        validator: bool,
        
        /// Validator key file
        #[arg(long, value_name = "FILE")]
        validator_key: Option<PathBuf>,
        
        /// RPC listen address
        #[arg(long, default_value = "127.0.0.1:8545")]
        rpc_addr: String,
        
        /// P2P listen address
        #[arg(long, default_value = "/ip4/0.0.0.0/tcp/9000")]
        p2p_addr: String,
        
        /// Bootstrap nodes
        #[arg(long)]
        bootstrap: Vec<String>,
        
        /// Chain ID
        #[arg(long, default_value = "1337")]
        chain_id: u64,
    },
    
    /// Generate a new validator key
    GenerateKey {
        /// Output file
        #[arg(short, long, value_name = "FILE")]
        output: PathBuf,
    },
    
    /// Initialize a new data directory
    Init {
        /// Data directory
        #[arg(short, long, value_name = "DIR")]
        data_dir: PathBuf,
        
        /// Chain ID
        #[arg(long, default_value = "1337")]
        chain_id: u64,
    },
    
    /// Show node status
    Status {
        /// RPC endpoint
        #[arg(long, default_value = "http://127.0.0.1:8545")]
        rpc: String,
    },
    
    /// Import genesis state
    ImportGenesis {
        /// Genesis file
        #[arg(short, long, value_name = "FILE")]
        genesis: PathBuf,
    },
    
    /// Export node data
    Export {
        /// Output file
        #[arg(short, long, value_name = "FILE")]
        output: PathBuf,
        
        /// Export format (json, binary)
        #[arg(long, default_value = "json")]
        format: String,
    },
    
    /// Run benchmarks
    Benchmark {
        /// Benchmark type
        #[arg(short, long, default_value = "all")]
        bench_type: String,
        
        /// Number of iterations
        #[arg(short, long, default_value = "1000")]
        iterations: u64,
    },
}

impl Cli {
    /// Parse CLI arguments
    pub fn parse_args() -> Self {
        Self::parse()
    }
}
