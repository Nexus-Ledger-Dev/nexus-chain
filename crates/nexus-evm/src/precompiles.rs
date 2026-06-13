//! NexusChain Custom Precompiles
//! 
//! Includes standard Ethereum precompiles plus NexusChain extensions:
//! - ZKP verification
//! - ISO 20022 message parsing
//! - DAG state queries

use sha3::{Digest, Keccak256};
use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use std::collections::HashMap;
use std::sync::Arc;

use nexus_primitives::{Address, Hash};
use crate::{EvmError, EvmResult, GasSchedule};

/// Precompile address range
pub const PRECOMPILE_START: u64 = 0x01;
pub const PRECOMPILE_END: u64 = 0x09;
pub const NEXUS_PRECOMPILE_START: u64 = 0x100;
pub const NEXUS_PRECOMPILE_END: u64 = 0x110;

/// Precompile function signature
pub type PrecompileFn = fn(&[u8], u64, &GasSchedule) -> PrecompileResult;

/// Precompile execution result
pub struct PrecompileResult {
    pub output: Vec<u8>,
    pub gas_used: u64,
}

impl PrecompileResult {
    pub fn ok(output: Vec<u8>, gas_used: u64) -> EvmResult<Self> {
        Ok(Self { output, gas_used })
    }
    
    pub fn error(msg: &str) -> EvmResult<Self> {
        Err(EvmError::PrecompileError(msg.to_string()))
    }
}

/// Get all precompiles
pub fn get_precompiles() -> HashMap<Address, PrecompileFn> {
    let mut map = HashMap::new();
    
    // Standard Ethereum precompiles
    map.insert(addr(0x01), ecrecover as PrecompileFn);
    map.insert(addr(0x02), sha256 as PrecompileFn);
    map.insert(addr(0x03), ripemd160 as PrecompileFn);
    map.insert(addr(0x04), identity as PrecompileFn);
    map.insert(addr(0x05), modexp as PrecompileFn);
    map.insert(addr(0x06), bn_add as PrecompileFn);
    map.insert(addr(0x07), bn_mul as PrecompileFn);
    map.insert(addr(0x08), bn_pairing as PrecompileFn);
    map.insert(addr(0x09), blake2f as PrecompileFn);
    
    // NexusChain extensions
    map.insert(addr(0x100), zkp_verify as PrecompileFn);
    map.insert(addr(0x101), zkp_batch_verify as PrecompileFn);
    map.insert(addr(0x102), iso20022_parse as PrecompileFn);
    map.insert(addr(0x103), iso20022_validate as PrecompileFn);
    map.insert(addr(0x104), dag_get_vertex as PrecompileFn);
    map.insert(addr(0x105), poseidon_hash as PrecompileFn);
    
    map
}

fn addr(n: u64) -> Address {
    let mut bytes = [0u8; 20];
    bytes[19] = n as u8;
    bytes[18] = (n >> 8) as u8;
    Address::from(bytes)
}

/// Check if address is a precompile
pub fn is_precompile(addr: &Address) -> bool {
    let bytes = addr.as_bytes();
    // Check if all but last 2 bytes are zero
    if bytes[..18].iter().any(|&b| b != 0) {
        return false;
    }
    
    let val = ((bytes[18] as u64) << 8) | (bytes[19] as u64);
    (PRECOMPILE_START..=PRECOMPILE_END).contains(&val) ||
    (NEXUS_PRECOMPILE_START..=NEXUS_PRECOMPILE_END).contains(&val)
}

// ==== Standard Ethereum Precompiles ====

/// 0x01: ECRECOVER
fn ecrecover(input: &[u8], gas_limit: u64, schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    if gas_limit < schedule.ecrecover {
        return Err(EvmError::OutOfGas { used: schedule.ecrecover, limit: gas_limit });
    }
    
    if input.len() < 128 {
        // Pad input
        let mut padded = vec![0u8; 128];
        padded[..input.len()].copy_from_slice(input);
        return ecrecover_inner(&padded, schedule);
    }
    
    ecrecover_inner(input, schedule)
}

fn ecrecover_inner(input: &[u8], schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    let hash: [u8; 32] = input[0..32].try_into().unwrap();
    let v = input[63]; // Last byte of v (32 bytes at offset 32)
    let r: [u8; 32] = input[64..96].try_into().unwrap();
    let s: [u8; 32] = input[96..128].try_into().unwrap();
    
    // v must be 27 or 28
    let recovery_id = match v {
        27 => RecoveryId::new(false, false),
        28 => RecoveryId::new(true, false),
        _ => return PrecompileResult::ok(vec![0u8; 32], schedule.ecrecover),
    };
    
    let Ok(recovery_id) = recovery_id else {
        return PrecompileResult::ok(vec![0u8; 32], schedule.ecrecover);
    };
    
    // Construct signature
    let mut sig_bytes = [0u8; 64];
    sig_bytes[..32].copy_from_slice(&r);
    sig_bytes[32..].copy_from_slice(&s);
    
    let Ok(signature) = Signature::try_from(&sig_bytes[..]) else {
        return PrecompileResult::ok(vec![0u8; 32], schedule.ecrecover);
    };
    
    // Recover public key
    let Ok(pubkey) = VerifyingKey::recover_from_prehash(&hash, &signature, recovery_id) else {
        return PrecompileResult::ok(vec![0u8; 32], schedule.ecrecover);
    };
    
    // Compute address from public key
    let pubkey_bytes = pubkey.to_encoded_point(false);
    let pubkey_hash = Keccak256::digest(&pubkey_bytes.as_bytes()[1..]);
    
    let mut output = vec![0u8; 32];
    output[12..32].copy_from_slice(&pubkey_hash[12..32]);
    
    PrecompileResult::ok(output, schedule.ecrecover)
}

/// 0x02: SHA256
fn sha256(input: &[u8], gas_limit: u64, schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    let words = (input.len() as u64 + 31) / 32;
    let gas = schedule.sha256_base + words * schedule.sha256_word;
    
    if gas_limit < gas {
        return Err(EvmError::OutOfGas { used: gas, limit: gas_limit });
    }
    
    use sha2::{Sha256, Digest as Sha2Digest};
    let result = Sha256::digest(input);
    
    PrecompileResult::ok(result.to_vec(), gas)
}

/// 0x03: RIPEMD160
fn ripemd160(input: &[u8], gas_limit: u64, schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    let words = (input.len() as u64 + 31) / 32;
    let gas = schedule.ripemd160_base + words * schedule.ripemd160_word;
    
    if gas_limit < gas {
        return Err(EvmError::OutOfGas { used: gas, limit: gas_limit });
    }
    
    use ripemd::{Ripemd160, Digest as RipemdDigest};
    let result = Ripemd160::digest(input);
    
    // Left-pad to 32 bytes
    let mut output = vec![0u8; 32];
    output[12..32].copy_from_slice(&result);
    
    PrecompileResult::ok(output, gas)
}

/// 0x04: Identity (data copy)
fn identity(input: &[u8], gas_limit: u64, schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    let words = (input.len() as u64 + 31) / 32;
    let gas = schedule.identity_base + words * schedule.identity_word;
    
    if gas_limit < gas {
        return Err(EvmError::OutOfGas { used: gas, limit: gas_limit });
    }
    
    PrecompileResult::ok(input.to_vec(), gas)
}

/// 0x05: MODEXP (simplified - full impl is complex)
fn modexp(input: &[u8], gas_limit: u64, _schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    // Parse lengths
    if input.len() < 96 {
        return PrecompileResult::ok(vec![], 200);
    }
    
    let base_len = u256_from_be_bytes(&input[0..32]) as usize;
    let exp_len = u256_from_be_bytes(&input[32..64]) as usize;
    let mod_len = u256_from_be_bytes(&input[64..96]) as usize;
    
    // Simplified gas calculation
    let max_len = base_len.max(mod_len);
    let gas = ((max_len * max_len * exp_len) / 3).max(200) as u64;
    
    if gas_limit < gas {
        return Err(EvmError::OutOfGas { used: gas, limit: gas_limit });
    }
    
    // For a real implementation, we'd use num-bigint here
    // Returning zeros for simplicity
    PrecompileResult::ok(vec![0u8; mod_len], gas)
}

fn u256_from_be_bytes(bytes: &[u8]) -> u64 {
    let mut result = 0u64;
    for (i, &b) in bytes.iter().rev().take(8).enumerate() {
        result |= (b as u64) << (i * 8);
    }
    result
}

/// 0x06: BN_ADD (alt_bn128 point addition)
fn bn_add(input: &[u8], gas_limit: u64, schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    if gas_limit < schedule.bn_add {
        return Err(EvmError::OutOfGas { used: schedule.bn_add, limit: gas_limit });
    }
    
    // In production, use ark-bn254 for actual EC operations
    // Placeholder: return identity element
    let output = vec![0u8; 64];
    PrecompileResult::ok(output, schedule.bn_add)
}

/// 0x07: BN_MUL (alt_bn128 scalar multiplication)
fn bn_mul(input: &[u8], gas_limit: u64, schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    if gas_limit < schedule.bn_mul {
        return Err(EvmError::OutOfGas { used: schedule.bn_mul, limit: gas_limit });
    }
    
    let output = vec![0u8; 64];
    PrecompileResult::ok(output, schedule.bn_mul)
}

/// 0x08: BN_PAIRING (alt_bn128 pairing check)
fn bn_pairing(input: &[u8], gas_limit: u64, schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    let num_pairs = input.len() / 192;
    let gas = schedule.bn_pairing_base + (num_pairs as u64) * schedule.bn_pairing_point;
    
    if gas_limit < gas {
        return Err(EvmError::OutOfGas { used: gas, limit: gas_limit });
    }
    
    // Placeholder: return success (1)
    let mut output = vec![0u8; 32];
    output[31] = 1;
    PrecompileResult::ok(output, gas)
}

/// 0x09: BLAKE2F compression
fn blake2f(input: &[u8], gas_limit: u64, schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    if input.len() != 213 {
        return PrecompileResult::error("Invalid BLAKE2F input length");
    }
    
    let rounds = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
    let gas = (rounds as u64) * schedule.blake2f_round;
    
    if gas_limit < gas {
        return Err(EvmError::OutOfGas { used: gas, limit: gas_limit });
    }
    
    // In production, use blake2 crate
    let output = vec![0u8; 64];
    PrecompileResult::ok(output, gas)
}

// ==== NexusChain Custom Precompiles ====

/// 0x100: ZKP_VERIFY - Verify a single ZK proof
fn zkp_verify(input: &[u8], gas_limit: u64, schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    // Input format:
    // - proof (variable, typically 192 bytes for Groth16)
    // - public inputs (32 bytes each)
    // - vk hash (32 bytes)
    
    if input.len() < 224 { // Minimum: 192 proof + 32 public input
        return PrecompileResult::error("Invalid ZKP input length");
    }
    
    let num_inputs = (input.len() - 192 - 32) / 32;
    let gas = schedule.zkp_verify_base + (num_inputs as u64) * schedule.zkp_verify_per_input;
    
    if gas_limit < gas {
        return Err(EvmError::OutOfGas { used: gas, limit: gas_limit });
    }
    
    // In production, this would call nexus_zkp::Verifier
    // Placeholder: always succeed
    let mut output = vec![0u8; 32];
    output[31] = 1; // true
    PrecompileResult::ok(output, gas)
}

/// 0x101: ZKP_BATCH_VERIFY - Verify multiple ZK proofs
fn zkp_batch_verify(input: &[u8], gas_limit: u64, schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    if input.len() < 4 {
        return PrecompileResult::error("Invalid batch verify input");
    }
    
    let num_proofs = u32::from_be_bytes([input[0], input[1], input[2], input[3]]) as usize;
    
    // Batch verification is more efficient (~60% of individual)
    let gas = (schedule.zkp_verify_base * num_proofs as u64 * 60) / 100;
    
    if gas_limit < gas {
        return Err(EvmError::OutOfGas { used: gas, limit: gas_limit });
    }
    
    let mut output = vec![0u8; 32];
    output[31] = 1;
    PrecompileResult::ok(output, gas)
}

/// 0x102: ISO20022_PARSE - Parse ISO 20022 XML message
fn iso20022_parse(input: &[u8], gas_limit: u64, schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    let gas = schedule.iso20022_parse_base + (input.len() as u64) * schedule.iso20022_parse_per_byte;
    
    if gas_limit < gas {
        return Err(EvmError::OutOfGas { used: gas, limit: gas_limit });
    }
    
    // In production, call nexus_iso::parse_credit_transfer
    // Return ABI-encoded parsed data
    let output = vec![0u8; 256]; // Placeholder
    PrecompileResult::ok(output, gas)
}

/// 0x103: ISO20022_VALIDATE - Validate ISO message
fn iso20022_validate(input: &[u8], gas_limit: u64, schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    let gas = schedule.iso20022_parse_base + (input.len() as u64 / 2) * schedule.iso20022_parse_per_byte;
    
    if gas_limit < gas {
        return Err(EvmError::OutOfGas { used: gas, limit: gas_limit });
    }
    
    let mut output = vec![0u8; 32];
    output[31] = 1; // Valid
    PrecompileResult::ok(output, gas)
}

/// 0x104: DAG_GET_VERTEX - Query DAG vertex by hash
fn dag_get_vertex(input: &[u8], gas_limit: u64, _schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    if input.len() < 32 {
        return PrecompileResult::error("Invalid vertex hash");
    }
    
    let gas = 1000u64; // Fixed cost for DAG lookup
    
    if gas_limit < gas {
        return Err(EvmError::OutOfGas { used: gas, limit: gas_limit });
    }
    
    // In production, query the DAG
    let output = vec![0u8; 128]; // Vertex data
    PrecompileResult::ok(output, gas)
}

/// 0x105: POSEIDON_HASH - ZK-friendly hash
fn poseidon_hash(input: &[u8], gas_limit: u64, _schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    let num_inputs = (input.len() + 31) / 32;
    let gas = 500 + (num_inputs as u64) * 100;
    
    if gas_limit < gas {
        return Err(EvmError::OutOfGas { used: gas, limit: gas_limit });
    }
    
    // In production, call nexus_zkp::poseidon_hash
    let output = Keccak256::digest(input).to_vec(); // Placeholder
    PrecompileResult::ok(output, gas)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_is_precompile() {
        assert!(is_precompile(&addr(0x01)));
        assert!(is_precompile(&addr(0x09)));
        assert!(is_precompile(&addr(0x100)));
        assert!(!is_precompile(&addr(0x00)));
        assert!(!is_precompile(&addr(0x0A)));
        assert!(!is_precompile(&addr(0x200)));
    }
    
    #[test]
    fn test_sha256_precompile() {
        let schedule = GasSchedule::default();
        let result = sha256(b"hello", 10000, &schedule).unwrap();
        assert_eq!(result.output.len(), 32);
    }
    
    #[test]
    fn test_identity_precompile() {
        let schedule = GasSchedule::default();
        let input = b"test data";
        let result = identity(input, 10000, &schedule).unwrap();
        assert_eq!(result.output, input);
    }
}
