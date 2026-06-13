//! NexusChain Node Entry Point

use std::path::PathBuf;
use tracing::{info, error, Level};
use tracing_subscriber::FmtSubscriber;

use nexus_node::{Cli, Commands, Node, NodeConfig};
use nexus_consensus::ValidatorKeys;

#[tokio::main]
async fn main() {
    let cli = Cli::parse_args();
    
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(match cli.log_level.as_str() {
            "trace" => Level::TRACE,
            "debug" => Level::DEBUG,
            "info" => Level::INFO,
            "warn" => Level::WARN,
            "error" => Level::ERROR,
            _ => Level::INFO,
        })
        .with_target(true)
        .init();
    
    info!("NexusChain v{}", env!("CARGO_PKG_VERSION"));
    
    let result = match cli.command {
        Some(Commands::Run { 
            validator, 
            validator_key, 
            rpc_addr, 
            p2p_addr, 
            bootstrap, 
            chain_id 
        }) => {
            run_node(
                cli.config,
                cli.data_dir,
                validator,
                validator_key,
                rpc_addr,
                p2p_addr,
                bootstrap,
                chain_id,
            ).await
        }
        
        Some(Commands::GenerateKey { output }) => {
            generate_key(output)
        }
        
        Some(Commands::Init { data_dir, chain_id }) => {
            init_data_dir(data_dir, chain_id)
        }
        
        Some(Commands::Status { rpc }) => {
            show_status(rpc).await
        }
        
        Some(Commands::ImportGenesis { genesis }) => {
            import_genesis(genesis)
        }
        
        Some(Commands::Export { output, format }) => {
            export_data(output, format)
        }
        
        Some(Commands::Benchmark { bench_type, iterations }) => {
            run_benchmarks(bench_type, iterations).await
        }
        
        None => {
            // Default: run node
            run_node(
                cli.config,
                cli.data_dir,
                false,
                None,
                "127.0.0.1:8545".to_string(),
                "/ip4/0.0.0.0/tcp/9000".to_string(),
                vec![],
                1337,
            ).await
        }
    };
    
    if let Err(e) = result {
        error!("Error: {}", e);
        std::process::exit(1);
    }
}

async fn run_node(
    config_path: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    validator: bool,
    validator_key: Option<PathBuf>,
    rpc_addr: String,
    p2p_addr: String,
    bootstrap: Vec<String>,
    chain_id: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Load or create configuration
    let mut config = if let Some(path) = config_path {
        NodeConfig::load(&path)?
    } else {
        NodeConfig::default()
    };
    
    // Apply CLI overrides
    if let Some(dir) = data_dir {
        config.data_dir = dir;
    }
    config.chain_id = chain_id;
    config.consensus.validator = validator;
    config.consensus.validator_key = validator_key;
    config.rpc.http_addr = rpc_addr.parse()?;
    config.network.listen_addr = p2p_addr;
    config.network.bootstrap_nodes = bootstrap;
    
    // Validate configuration
    config.validate()?;
    
    // Create and start node
    let mut node = Node::new(config)?;
    node.start().await?;
    
    // Wait for shutdown signal
    info!("Node running. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;
    
    info!("Shutdown signal received");
    node.stop().await;
    
    Ok(())
}

fn generate_key(output: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    info!("Generating new validator key...");
    
    let keys = ValidatorKeys::generate();
    let address = keys.address();
    
    // Export private key bytes
    // In production, this should be encrypted
    let pubkey = keys.public_key();
    
    // For demo, save public key (in production, save encrypted private key)
    std::fs::write(&output, &pubkey)?;
    
    info!("Validator key generated:");
    info!("  Address: {:?}", address);
    info!("  Saved to: {:?}", output);
    
    Ok(())
}

fn init_data_dir(data_dir: PathBuf, chain_id: u64) -> Result<(), Box<dyn std::error::Error>> {
    info!("Initializing data directory: {:?}", data_dir);
    
    // Create directories
    std::fs::create_dir_all(&data_dir)?;
    std::fs::create_dir_all(data_dir.join("db"))?;
    std::fs::create_dir_all(data_dir.join("keys"))?;
    
    // Create default config
    let mut config = NodeConfig::default();
    config.data_dir = data_dir.clone();
    config.chain_id = chain_id;
    
    let config_path = data_dir.join("config.toml");
    config.save(&config_path)?;
    
    info!("Data directory initialized");
    info!("  Config: {:?}", config_path);
    
    Ok(())
}

async fn show_status(rpc: String) -> Result<(), Box<dyn std::error::Error>> {
    info!("Querying node status from {}", rpc);
    
    // Make RPC call
    let client = reqwest::Client::new();
    let response = client.post(&rpc)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "nexus_dagInfo",
            "params": []
        }))
        .send()
        .await?;
    
    let result: serde_json::Value = response.json().await?;
    
    println!("Node Status:");
    println!("{}", serde_json::to_string_pretty(&result)?);
    
    Ok(())
}

fn import_genesis(genesis: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    info!("Importing genesis from: {:?}", genesis);
    
    // TODO: Implement genesis import
    info!("Genesis import not yet implemented");
    
    Ok(())
}

fn export_data(output: PathBuf, format: String) -> Result<(), Box<dyn std::error::Error>> {
    info!("Exporting data to: {:?} (format: {})", output, format);
    
    // TODO: Implement data export
    info!("Data export not yet implemented");
    
    Ok(())
}

async fn run_benchmarks(bench_type: String, iterations: u64) -> Result<(), Box<dyn std::error::Error>> {
    info!("Running benchmarks: {} ({} iterations)", bench_type, iterations);
    
    use std::time::Instant;
    use nexus_dag::{Dag, Vertex};
    use nexus_primitives::{Address, Hash};
    
    match bench_type.as_str() {
        "dag" | "all" => {
            info!("Benchmarking DAG operations...");
            
            let dag = Dag::new();
            let proposer = Address::from([1u8; 20]);
            
            let start = Instant::now();
            for i in 0..iterations {
                let parents = dag.tips().into_iter().take(4).collect();
                let vertex = Vertex::new(
                    proposer.clone(),
                    parents,
                    vec![Hash::from([i as u8; 32])],
                    0,
                    i,
                );
                dag.add_vertex(vertex).ok();
            }
            let elapsed = start.elapsed();
            
            info!("DAG benchmark results:");
            info!("  Total vertices: {}", iterations);
            info!("  Time: {:?}", elapsed);
            info!("  Throughput: {:.2} vertices/sec", iterations as f64 / elapsed.as_secs_f64());
        }
        
        "zkp" => {
            info!("Benchmarking ZKP operations...");
            // TODO: Add ZKP benchmarks
        }
        
        "evm" => {
            info!("Benchmarking EVM operations...");
            // TODO: Add EVM benchmarks
        }
        
        _ => {
            info!("Unknown benchmark type: {}", bench_type);
        }
    }
    
    Ok(())
}
