//! Ethereum-compatible RPC methods

use std::sync::Arc;
use serde_json::{json, Value};
use tracing::debug;

use std::sync::atomic::{AtomicUsize, Ordering};

use sha3::Digest;
use nexus_primitives::{Address, Transaction, U256};
use nexus_dag::Dag;
use nexus_evm::{Executor, EvmEnv, StateDb};
use nexus_consensus::ProofOfStake;

use crate::{RpcResult, RpcError, types::*, mempool::Mempool};

/// Ethereum namespace handler
pub struct EthHandler {
    dag: Arc<Dag>,
    state: Arc<StateDb>,
    pos: Arc<ProofOfStake>,
    chain_id: u64,
    mempool: Arc<Mempool>,
    /// Live peer count — updated by the node layer, read by net_peerCount / eth_syncing.
    peer_count: Arc<AtomicUsize>,
}

impl EthHandler {
    pub fn new(
        dag: Arc<Dag>,
        state: Arc<StateDb>,
        pos: Arc<ProofOfStake>,
        chain_id: u64,
        mempool: Arc<Mempool>,
        peer_count: Arc<AtomicUsize>,
    ) -> Self {
        Self { dag, state, pos, chain_id, mempool, peer_count }
    }
    
    /// eth_chainId
    pub fn chain_id(&self) -> RpcResult<Value> {
        Ok(json!(format_u64(self.chain_id)))
    }
    
    /// eth_blockNumber
    pub fn block_number(&self) -> RpcResult<Value> {
        let height = self.dag.latest_height();
        Ok(json!(format_u64(height)))
    }
    
    /// eth_gasPrice
    pub fn gas_price(&self) -> RpcResult<Value> {
        // Return base fee + tip estimate
        let price = 1_000_000_000u64; // 1 gwei
        Ok(json!(format_u64(price)))
    }
    
    /// eth_getBalance
    pub fn get_balance(&self, params: &Value) -> RpcResult<Value> {
        let address = params.get(0)
            .and_then(|v| v.as_str())
            .and_then(parse_address)
            .ok_or_else(|| RpcError::InvalidParams("Invalid address".into()))?;
        
        let balance = self.state.get_balance(&address);
        Ok(json!(format_u256(&balance)))
    }
    
    /// eth_getTransactionCount
    pub fn get_transaction_count(&self, params: &Value) -> RpcResult<Value> {
        let address = params.get(0)
            .and_then(|v| v.as_str())
            .and_then(parse_address)
            .ok_or_else(|| RpcError::InvalidParams("Invalid address".into()))?;
        
        let nonce = self.state.get_nonce(&address);
        Ok(json!(format_u64(nonce)))
    }
    
    /// eth_getCode
    pub fn get_code(&self, params: &Value) -> RpcResult<Value> {
        let address = params.get(0)
            .and_then(|v| v.as_str())
            .and_then(parse_address)
            .ok_or_else(|| RpcError::InvalidParams("Invalid address".into()))?;
        
        let code = self.state.get_code(&address);
        Ok(json!(format_bytes(&code)))
    }
    
    /// eth_getStorageAt
    pub fn get_storage_at(&self, params: &Value) -> RpcResult<Value> {
        let address = params.get(0)
            .and_then(|v| v.as_str())
            .and_then(parse_address)
            .ok_or_else(|| RpcError::InvalidParams("Invalid address".into()))?;
        
        let slot = params.get(1)
            .and_then(|v| v.as_str())
            .and_then(parse_u64)
            .ok_or_else(|| RpcError::InvalidParams("Invalid slot".into()))?;
        
        let value = self.state.get_storage(&address, &U256::from(slot));
        Ok(json!(format_u256(&value)))
    }
    
    /// eth_sendRawTransaction
    pub fn send_raw_transaction(&self, params: &Value) -> RpcResult<Value> {
        let raw = params.get(0)
            .and_then(|v| v.as_str())
            .and_then(parse_bytes)
            .ok_or_else(|| RpcError::InvalidParams("Invalid transaction data".into()))?;
        
        // Compute the tx hash first (used as the return value regardless of decode result).
        let digest = sha3::Keccak256::digest(&raw);
        let tx_hash = nexus_primitives::Hash::from(digest.as_ref());

        // Try EIP-1559/EIP-2930/legacy RLP decode first (MetaMask, ethers.js, cast).
        // Fall back to JSON for NexusChain-native submissions.
        let tx_opt = Transaction::from_rlp_bytes(&raw)
            .or_else(|| serde_json::from_slice::<Transaction>(&raw).ok());

        if let Some(tx) = tx_opt {
            if !self.mempool.add(tx) {
                return Err(RpcError::TransactionRejected("Mempool full or duplicate".into()));
            }
            debug!("Transaction {:?} added to mempool", tx_hash);
        } else {
            debug!("Could not decode transaction body; hash {:?} recorded", tx_hash);
        }

        Ok(json!(format_hash(&tx_hash)))
    }
    
    /// eth_call
    pub fn call(&self, params: &Value) -> RpcResult<Value> {
        let call_req: CallRequest = serde_json::from_value(params.get(0).cloned().unwrap_or_default())
            .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
        
        // Simulate call
        let to = parse_address(&call_req.to)
            .ok_or_else(|| RpcError::InvalidParams("Invalid to address".into()))?;
        
        let caller = call_req.from
            .as_deref()
            .and_then(parse_address)
            .unwrap_or(Address::ZERO);

        let value = call_req.value
            .as_deref()
            .and_then(|v| parse_u64(v).map(U256::from_u64))
            .unwrap_or(U256::ZERO);

        let data = call_req.data
            .as_deref()
            .and_then(parse_bytes)
            .unwrap_or_default();

        let gas = call_req.gas
            .as_deref()
            .and_then(parse_u64)
            .unwrap_or(30_000_000);

        let env = EvmEnv {
            chain_id: self.chain_id,
            number: self.dag.latest_height(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            ..EvmEnv::default()
        };

        let mut executor = Executor::new(self.state.clone(), env);
        let (output, _gas_used) = executor
            .call_dry_run(caller, to, value, data, gas)
            .map_err(|e| RpcError::InternalError(e.to_string()))?;

        Ok(json!(format_bytes(&output)))
    }
    
    /// eth_estimateGas
    pub fn estimate_gas(&self, params: &Value) -> RpcResult<Value> {
        let call_req: CallRequest = serde_json::from_value(params.get(0).cloned().unwrap_or_default())
            .map_err(|e| RpcError::InvalidParams(e.to_string()))?;

        let to = parse_address(&call_req.to)
            .ok_or_else(|| RpcError::InvalidParams("Invalid to address".into()))?;

        let caller = call_req.from
            .as_deref()
            .and_then(parse_address)
            .unwrap_or(Address::ZERO);

        let value = call_req.value
            .as_deref()
            .and_then(|v| parse_u64(v).map(U256::from_u64))
            .unwrap_or(U256::ZERO);

        let data = call_req.data
            .as_deref()
            .and_then(parse_bytes)
            .unwrap_or_default();

        let env = EvmEnv {
            chain_id: self.chain_id,
            number: self.dag.latest_height(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            ..EvmEnv::default()
        };

        let mut executor = Executor::new(self.state.clone(), env);
        // Offer a high gas ceiling for estimation; intrinsic cost is 21,000 minimum.
        let (_output, gas_used) = executor
            .call_dry_run(caller, to, value, data, 30_000_000)
            .map_err(|e| RpcError::InternalError(e.to_string()))?;

        let estimate = gas_used.max(21_000);
        Ok(json!(format_u64(estimate)))
    }
    
    /// eth_getBlockByNumber
    pub fn get_block_by_number(&self, params: &Value) -> RpcResult<Value> {
        let block_id = params.get(0)
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::InvalidParams("Missing block number".into()))?;
        
        let _full_txs = params.get(1)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        
        let height = if block_id == "latest" {
            self.dag.latest_height()
        } else {
            parse_u64(block_id).unwrap_or(0)
        };
        
        // Find vertex at height
        let hashes = self.dag.get_vertices_at_height(height);
        if hashes.is_empty() {
            return Ok(Value::Null);
        }

        // Return first vertex as "block"
        let vertex = self.dag.get_vertex(&hashes[0])
            .ok_or_else(|| RpcError::NotFound("Block not found".into()))?;
        let block = RpcBlock {
            number: format_u64(height),
            hash: format_hash(&vertex.hash),
            parent_hash: vertex.parents.first()
                .map(format_hash)
                .unwrap_or_else(|| "0x".repeat(32)),
            nonce: "0x0".to_string(),
            sha3_uncles: "0x".to_string() + &"0".repeat(64),
            logs_bloom: "0x".to_string() + &"0".repeat(512),
            transactions_root: "0x".to_string() + &"0".repeat(64),
            state_root: format_hash(&self.state.state_root()),
            receipts_root: "0x".to_string() + &"0".repeat(64),
            miner: format!("0x{}", hex::encode(&vertex.validator.0[12..])),
            difficulty: "0x0".to_string(),
            total_difficulty: "0x0".to_string(),
            extra_data: "0x".to_string(),
            size: format_u64(1000),
            gas_limit: format_u64(30_000_000),
            gas_used: format_u64(0),
            timestamp: format_u64(vertex.timestamp),
            transactions: vertex.transactions.iter()
                .map(|tx| json!(format_hash(&tx.hash())))
                .collect(),
            uncles: vec![],
            base_fee_per_gas: Some(format_u64(1_000_000_000)),
        };
        
        Ok(serde_json::to_value(block).unwrap())
    }
    
    /// eth_getBlockByHash
    pub fn get_block_by_hash(&self, params: &Value) -> RpcResult<Value> {
        let hash = params.get(0)
            .and_then(|v| v.as_str())
            .and_then(parse_hash)
            .ok_or_else(|| RpcError::InvalidParams("Invalid hash".into()))?;
        
        let vertex = self.dag.get_vertex(&hash)
            .ok_or_else(|| RpcError::NotFound("Block not found".into()))?;
        
        let block = RpcBlock {
            number: format_u64(vertex.height),
            hash: format_hash(&vertex.hash),
            parent_hash: vertex.parents.first()
                .map(format_hash)
                .unwrap_or_else(|| "0x".repeat(32)),
            nonce: "0x0".to_string(),
            sha3_uncles: "0x".to_string() + &"0".repeat(64),
            logs_bloom: "0x".to_string() + &"0".repeat(512),
            transactions_root: "0x".to_string() + &"0".repeat(64),
            state_root: format_hash(&self.state.state_root()),
            receipts_root: "0x".to_string() + &"0".repeat(64),
            miner: format!("0x{}", hex::encode(&vertex.validator.0[12..])),
            difficulty: "0x0".to_string(),
            total_difficulty: "0x0".to_string(),
            extra_data: "0x".to_string(),
            size: format_u64(1000),
            gas_limit: format_u64(30_000_000),
            gas_used: format_u64(0),
            timestamp: format_u64(vertex.timestamp),
            transactions: vertex.transactions.iter()
                .map(|tx| json!(format_hash(&tx.hash())))
                .collect(),
            uncles: vec![],
            base_fee_per_gas: Some(format_u64(1_000_000_000)),
        };
        
        Ok(serde_json::to_value(block).unwrap())
    }
    
    /// eth_syncing
    pub fn syncing(&self) -> RpcResult<Value> {
        let peers = self.peer_count.load(Ordering::Relaxed);
        let current = self.dag.latest_height();
        // Without an authoritative "highest block" from peers we can only report
        // that we are syncing when we have peers but our DAG is still at genesis.
        if peers > 0 && current == 0 {
            Ok(json!({
                "startingBlock": "0x0",
                "currentBlock": format_u64(current),
                "highestBlock": format_u64(current), // best effort; unknown until header exchange
            }))
        } else {
            Ok(json!(false))
        }
    }
    
    /// eth_coinbase
    pub fn coinbase(&self) -> RpcResult<Value> {
        // Return first validator
        let validators = self.pos.validator_set();
        if let Some(v) = validators.iter().next() {
            return Ok(json!(format_address(&v.address)));
        }
        Ok(json!("0x".to_string() + &"0".repeat(40)))
    }
    
    /// eth_mining
    pub fn mining(&self) -> RpcResult<Value> {
        Ok(json!(false))
    }
    
    /// eth_hashrate
    pub fn hashrate(&self) -> RpcResult<Value> {
        Ok(json!("0x0"))
    }
    
    /// eth_accounts
    pub fn accounts(&self) -> RpcResult<Value> {
        Ok(json!([]))
    }
    
    /// net_version
    pub fn net_version(&self) -> RpcResult<Value> {
        Ok(json!(self.chain_id.to_string()))
    }
    
    /// net_listening
    pub fn net_listening(&self) -> RpcResult<Value> {
        Ok(json!(true))
    }
    
    /// net_peerCount
    pub fn net_peer_count(&self) -> RpcResult<Value> {
        let n = self.peer_count.load(Ordering::Relaxed);
        Ok(json!(format_u64(n as u64)))
    }
    
    /// web3_clientVersion
    pub fn client_version(&self) -> RpcResult<Value> {
        Ok(json!("NexusChain/v0.1.0"))
    }
    
    /// web3_sha3
    pub fn sha3(&self, params: &Value) -> RpcResult<Value> {
        let data = params.get(0)
            .and_then(|v| v.as_str())
            .and_then(parse_bytes)
            .ok_or_else(|| RpcError::InvalidParams("Invalid data".into()))?;
        
        let hash = sha3::Keccak256::digest(&data);
        Ok(json!(format!("0x{}", hex::encode(hash))))
    }
}
