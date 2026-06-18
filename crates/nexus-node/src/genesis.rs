//! Genesis state import / export.
//!
//! A genesis file is a JSON document describing the initial chain state:
//! pre-funded accounts, pre-deployed contracts, and the initial validator set.
//! The format is intentionally simple and human-editable.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::info;

use nexus_primitives::{Address, Hash, U256};
use nexus_evm::{StateDb, Account};
use nexus_consensus::ProofOfStake;

// ─────────────────────────── data model ────────────────────────────

/// A single pre-funded / pre-deployed account entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenesisAccount {
    /// Hex-encoded address (with or without 0x prefix).
    pub address: String,
    /// Initial balance in the smallest token unit (decimal string).
    pub balance: String,
    /// Hex-encoded EVM bytecode, if this is a pre-deployed contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Pre-set storage slots `{ "0x<slot>": "0x<value>" }`.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub storage: HashMap<String, String>,
    /// Initial nonce (usually 0 for accounts, 1 for contracts).
    #[serde(default)]
    pub nonce: u64,
}

/// A genesis validator entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenesisValidator {
    /// Hex-encoded address.
    pub address: String,
    /// Hex-encoded compressed SEC1 public key (33 bytes).
    pub pubkey: String,
    /// Initial self-delegation stake (decimal string).
    pub stake: String,
    /// Commission rate in basis points (0–10000).
    pub commission: u16,
}

/// Complete genesis configuration document.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenesisConfig {
    /// Chain identifier.
    pub chain_id: u64,
    /// Unix timestamp of the genesis "block" (seconds).
    pub timestamp: u64,
    /// Human-readable network name.
    pub name: String,
    /// Pre-funded / pre-deployed accounts.
    #[serde(default)]
    pub accounts: Vec<GenesisAccount>,
    /// Initial validator set.
    #[serde(default)]
    pub validators: Vec<GenesisValidator>,
}

impl Default for GenesisConfig {
    fn default() -> Self {
        Self {
            chain_id: 1337,
            timestamp: 0,
            name: "NexusChain Devnet".to_string(),
            accounts: vec![],
            validators: vec![],
        }
    }
}

// ──────────────────────── parsing helpers ───────────────────────────

fn parse_addr(s: &str) -> Option<Address> {
    let s = s.trim_start_matches("0x");
    let bytes = hex::decode(s).ok()?;
    Address::from_slice(&bytes)
}

fn parse_u256_decimal(s: &str) -> U256 {
    // Parse a decimal string into U256 (no 0x prefix).
    let s = s.trim();
    let mut result = U256::ZERO;
    let ten = U256::from(10u64);
    for ch in s.chars() {
        if let Some(d) = ch.to_digit(10) {
            result = result * ten + U256::from(d as u64);
        }
    }
    result
}

fn parse_u256_hex(s: &str) -> U256 {
    let s = s.trim_start_matches("0x");
    // Parse up to 32 hex-encoded bytes big-endian into U256.
    let bytes = hex::decode(s).unwrap_or_default();
    let mut arr = [0u8; 32];
    let offset = arr.len().saturating_sub(bytes.len());
    arr[offset..].copy_from_slice(&bytes[bytes.len().saturating_sub(32)..]);
    // U256 stores little-endian limbs; bytes is big-endian.
    let mut limbs = [0u64; 4];
    for i in 0..4 {
        let start = (3 - i) * 8;
        limbs[i] = u64::from_be_bytes(arr[start..start + 8].try_into().unwrap());
    }
    U256(limbs)
}

// ────────────────────────── application ─────────────────────────────

/// Apply a genesis configuration to a freshly created `StateDb` and `ProofOfStake`.
///
/// Call this once before the node begins processing transactions.
pub fn apply_genesis(
    state: &Arc<StateDb>,
    pos: &Arc<ProofOfStake>,
    config: &GenesisConfig,
) -> Result<(), GenesisError> {
    info!("Applying genesis for chain {} ({})", config.chain_id, config.name);

    // ── accounts ──────────────────────────────────────────────────────
    for entry in &config.accounts {
        let addr = parse_addr(&entry.address)
            .ok_or_else(|| GenesisError::BadAddress(entry.address.clone()))?;

        let balance = parse_u256_decimal(&entry.balance);

        let account = Account {
            nonce: entry.nonce,
            balance,
            code_hash: Hash::ZERO,
            storage_root: Hash::ZERO,
        };

        state.set_account(addr.clone(), account);

        if let Some(code_hex) = &entry.code {
            let code = hex::decode(code_hex.trim_start_matches("0x"))
                .map_err(|_| GenesisError::BadCode(entry.address.clone()))?;
            state.set_code(&addr, code);
        }

        for (slot_str, val_str) in &entry.storage {
            let slot = parse_u256_hex(slot_str);
            let val = parse_u256_hex(val_str);
            state.set_storage(&addr, slot, val);
        }
    }

    // ── validators ────────────────────────────────────────────────────
    for v in &config.validators {
        let addr = parse_addr(&v.address)
            .ok_or_else(|| GenesisError::BadAddress(v.address.clone()))?;

        let pubkey = hex::decode(v.pubkey.trim_start_matches("0x"))
            .map_err(|_| GenesisError::BadPubkey(v.address.clone()))?;

        let stake_u256 = parse_u256_decimal(&v.stake);
        // We store stake as u64 in PoS; truncate (genesis stakes must fit in u64).
        let stake = stake_u256.as_u64();

        pos.register_validator(addr, pubkey, stake, v.commission)
            .map_err(|e| GenesisError::PoS(format!("{:?}", e)))?;
    }

    info!(
        "Genesis applied: {} accounts, {} validators",
        config.accounts.len(),
        config.validators.len()
    );
    Ok(())
}

// ─────────────────────── import / export ────────────────────────────

/// Load and apply a genesis file from disk.
pub fn load_genesis_file(
    path: &Path,
    state: &Arc<StateDb>,
    pos: &Arc<ProofOfStake>,
) -> Result<GenesisConfig, GenesisError> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| GenesisError::Io(e.to_string()))?;
    let config: GenesisConfig = serde_json::from_str(&raw)
        .map_err(|e| GenesisError::Parse(e.to_string()))?;
    apply_genesis(state, pos, &config)?;
    Ok(config)
}

/// Snapshot the current `StateDb` into a `GenesisConfig` (useful for devnets).
///
/// Only accounts that have non-zero balance or non-empty code are exported.
/// The validator set is reconstructed from `ProofOfStake`.
pub fn export_genesis(
    state: &Arc<StateDb>,
    pos: &Arc<ProofOfStake>,
    chain_id: u64,
    name: &str,
) -> GenesisConfig {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Collect validators.
    let validators: Vec<GenesisValidator> = pos
        .validator_set()
        .active_validators()
        .iter()
        .map(|v| GenesisValidator {
            address: format!("0x{}", hex::encode(v.address.as_bytes())),
            pubkey: format!("0x{}", hex::encode(&v.pubkey)),
            stake: v.stake.to_string(),
            commission: v.commission,
        })
        .collect();

    // Export all non-empty accounts from state.
    let accounts: Vec<GenesisAccount> = state.all_accounts()
        .into_iter()
        .filter(|(_, account)| !account.is_empty())
        .map(|(addr, account)| {
            let code = state.get_code(&addr);
            let code_hex = if code.is_empty() {
                None
            } else {
                Some(format!("0x{}", hex::encode(&code)))
            };

            GenesisAccount {
                address: format!("0x{}", hex::encode(addr.as_bytes())),
                balance: account.balance.to_string(),
                code: code_hex,
                storage: HashMap::new(),
                nonce: account.nonce,
            }
        })
        .collect();

    GenesisConfig {
        chain_id,
        timestamp,
        name: name.to_string(),
        accounts,
        validators,
    }
}

/// Write a `GenesisConfig` to a JSON file.
pub fn write_genesis_file(path: &Path, config: &GenesisConfig) -> Result<(), GenesisError> {
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| GenesisError::Parse(e.to_string()))?;
    std::fs::write(path, json)
        .map_err(|e| GenesisError::Io(e.to_string()))
}

// ───────────────────────────── errors ───────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum GenesisError {
    #[error("IO error: {0}")]
    Io(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Invalid address: {0}")]
    BadAddress(String),
    #[error("Invalid code for {0}")]
    BadCode(String),
    #[error("Invalid pubkey for {0}")]
    BadPubkey(String),
    #[error("PoS error: {0}")]
    PoS(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_u256_decimal() {
        let v = parse_u256_decimal("1000000000000000000");
        assert_eq!(v.as_u64(), 1_000_000_000_000_000_000);
    }

    #[test]
    fn test_apply_genesis_basic() {
        let state = Arc::new(StateDb::new());
        let pos = Arc::new(ProofOfStake::default());

        let config = GenesisConfig {
            chain_id: 1337,
            timestamp: 0,
            name: "test".to_string(),
            accounts: vec![GenesisAccount {
                address: "0x0000000000000000000000000000000000000001".to_string(),
                balance: "1000000000".to_string(),
                code: None,
                storage: HashMap::new(),
                nonce: 0,
            }],
            validators: vec![],
        };

        apply_genesis(&state, &pos, &config).unwrap();

        let addr = parse_addr("0x0000000000000000000000000000000000000001").unwrap();
        assert_eq!(state.get_balance(&addr), U256::from(1_000_000_000u64));
    }

    #[test]
    fn test_genesis_roundtrip() {
        let config = GenesisConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let back: GenesisConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.chain_id, 1337);
    }

    #[test]
    fn test_export_genesis_captures_accounts() {
        let state = Arc::new(StateDb::new());
        let pos = Arc::new(ProofOfStake::default());

        let addr = parse_addr("0x0000000000000000000000000000000000000002").unwrap();
        state.set_balance(&addr, U256::from(999_000_000u64));

        let exported = export_genesis(&state, &pos, 1337, "test");

        assert_eq!(exported.accounts.len(), 1);
        assert_eq!(exported.accounts[0].balance, "999000000");
    }
}
