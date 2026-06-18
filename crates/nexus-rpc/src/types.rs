//! RPC type definitions

use serde::{Deserialize, Serialize};
use nexus_primitives::{Address, Hash, U256};

/// JSON-RPC request
#[derive(Clone, Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// JSON-RPC response
#[derive(Clone, Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }
    
    pub fn error(id: serde_json::Value, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError { code, message, data: None }),
        }
    }
}

/// JSON-RPC error
#[derive(Clone, Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Block identifier
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum BlockId {
    /// Block number
    Number(u64),
    /// Block hash
    Hash(Hash),
    /// Named block
    Tag(BlockTag),
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockTag {
    Latest,
    Earliest,
    Pending,
    Safe,
    Finalized,
}

/// Transaction object (RPC format)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransaction {
    pub hash: String,
    pub nonce: String,
    pub block_hash: Option<String>,
    pub block_number: Option<String>,
    pub transaction_index: Option<String>,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub gas: String,
    pub gas_price: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_fee_per_gas: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_priority_fee_per_gas: Option<String>,
    pub input: String,
    pub v: String,
    pub r: String,
    pub s: String,
    #[serde(rename = "type")]
    pub tx_type: String,
    pub chain_id: String,
}

/// Block object (RPC format)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcBlock {
    pub number: String,
    pub hash: String,
    pub parent_hash: String,
    pub nonce: String,
    pub sha3_uncles: String,
    pub logs_bloom: String,
    pub transactions_root: String,
    pub state_root: String,
    pub receipts_root: String,
    pub miner: String,
    pub difficulty: String,
    pub total_difficulty: String,
    pub extra_data: String,
    pub size: String,
    pub gas_limit: String,
    pub gas_used: String,
    pub timestamp: String,
    pub transactions: Vec<serde_json::Value>,
    pub uncles: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_fee_per_gas: Option<String>,
}

/// Receipt object
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcReceipt {
    pub transaction_hash: String,
    pub transaction_index: String,
    pub block_hash: String,
    pub block_number: String,
    pub from: String,
    pub to: Option<String>,
    pub cumulative_gas_used: String,
    pub gas_used: String,
    pub contract_address: Option<String>,
    pub logs: Vec<RpcLog>,
    pub logs_bloom: String,
    pub status: String,
    pub effective_gas_price: String,
    #[serde(rename = "type")]
    pub tx_type: String,
}

/// Log object
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcLog {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub block_number: String,
    pub transaction_hash: String,
    pub transaction_index: String,
    pub block_hash: String,
    pub log_index: String,
    pub removed: bool,
}

/// Call request
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallRequest {
    pub from: Option<String>,
    pub to: String,
    #[serde(default)]
    pub gas: Option<String>,
    #[serde(default)]
    pub gas_price: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub data: Option<String>,
}

/// Filter for logs
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFilter {
    #[serde(default)]
    pub from_block: Option<BlockId>,
    #[serde(default)]
    pub to_block: Option<BlockId>,
    #[serde(default)]
    pub address: Option<Vec<String>>,
    #[serde(default)]
    pub topics: Option<Vec<Option<Vec<String>>>>,
}

/// Sync status
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub starting_block: String,
    pub current_block: String,
    pub highest_block: String,
}

/// Format utilities
pub fn format_u256(value: &U256) -> String {
    format!("0x{:x}", value.as_u64())
}

pub fn format_u64(value: u64) -> String {
    format!("0x{:x}", value)
}

pub fn format_hash(hash: &Hash) -> String {
    format!("0x{}", hex::encode(hash.as_bytes()))
}

pub fn format_address(addr: &Address) -> String {
    format!("0x{}", hex::encode(addr.as_bytes()))
}

pub fn format_bytes(data: &[u8]) -> String {
    format!("0x{}", hex::encode(data))
}

pub fn parse_u64(s: &str) -> Option<u64> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    u64::from_str_radix(s, 16).ok()
}

pub fn parse_address(s: &str) -> Option<Address> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).ok()?;
    if bytes.len() != 20 {
        return None;
    }
    Address::from_slice(&bytes)
}

pub fn parse_hash(s: &str) -> Option<Hash> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).ok()?;
    if bytes.len() != 32 {
        return None;
    }
    Hash::from_slice(&bytes)
}

pub fn parse_bytes(s: &str) -> Option<Vec<u8>> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    hex::decode(s).ok()
}
