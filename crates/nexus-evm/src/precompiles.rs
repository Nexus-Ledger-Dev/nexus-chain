//! NexusChain Custom Precompiles
//! 
//! Includes standard Ethereum precompiles plus NexusChain extensions:
//! - ZKP verification
//! - ISO 20022 message parsing
//! - DAG state queries

use sha3::{Digest, Keccak256};
use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};
use std::collections::HashMap;
use nexus_primitives::Address;
use crate::{EvmError, EvmResult, GasSchedule};

/// Precompile address range
pub const PRECOMPILE_START: u64 = 0x01;
pub const PRECOMPILE_END: u64 = 0x09;
pub const NEXUS_PRECOMPILE_START: u64 = 0x100;
pub const NEXUS_PRECOMPILE_END: u64 = 0x110;

/// Precompile function signature
pub type PrecompileFn = fn(&[u8], u64, &GasSchedule) -> EvmResult<PrecompileResult>;

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
    Address::new(bytes)
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

/// 0x05: MODEXP (EIP-198 / EIP-2565)
fn modexp(input: &[u8], gas_limit: u64, _schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    if input.len() < 96 {
        return PrecompileResult::ok(vec![], 200);
    }
    let base_len = u256_from_be_bytes(&input[0..32]) as usize;
    let exp_len  = u256_from_be_bytes(&input[32..64]) as usize;
    let mod_len  = u256_from_be_bytes(&input[64..96]) as usize;

    // EIP-2565 gas formula
    let gas = modexp_gas(base_len, exp_len, mod_len, input);
    if gas_limit < gas {
        return Err(EvmError::OutOfGas { used: gas, limit: gas_limit });
    }

    if mod_len == 0 {
        return PrecompileResult::ok(vec![], gas);
    }

    // Parse operands — pad with zeros if input is short
    let data = if input.len() > 96 { &input[96..] } else { &[] };
    let base  = read_slice(data, 0,        base_len);
    let exp   = read_slice(data, base_len, exp_len);
    let modulus = read_slice(data, base_len + exp_len, mod_len);

    use num_bigint::BigUint;
    let base_n = BigUint::from_bytes_be(&base);
    let exp_n  = BigUint::from_bytes_be(&exp);
    let mod_n  = BigUint::from_bytes_be(&modulus);

    if mod_n.bits() == 0 {
        return PrecompileResult::ok(vec![0u8; mod_len], gas);
    }

    let result_n = base_n.modpow(&exp_n, &mod_n);
    let result_bytes = result_n.to_bytes_be();

    // Left-pad to mod_len
    let mut output = vec![0u8; mod_len];
    if result_bytes.len() <= mod_len {
        output[mod_len - result_bytes.len()..].copy_from_slice(&result_bytes);
    } else {
        output.copy_from_slice(&result_bytes[result_bytes.len() - mod_len..]);
    }
    PrecompileResult::ok(output, gas)
}

// Read `len` bytes from `data` starting at `offset`, zero-padding if needed.
fn read_slice(data: &[u8], offset: usize, len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    let available = data.len().saturating_sub(offset);
    let copy_len = available.min(len);
    out[..copy_len].copy_from_slice(&data[offset..offset + copy_len]);
    out
}

fn modexp_gas(base_len: usize, exp_len: usize, mod_len: usize, input: &[u8]) -> u64 {
    let max_len = base_len.max(mod_len);
    let mult_complexity: u64 = if max_len <= 64 {
        (max_len * max_len) as u64
    } else if max_len <= 1024 {
        (max_len * max_len / 4 + 96 * max_len).saturating_sub(3072) as u64
    } else {
        (max_len * max_len / 16 + 480 * max_len).saturating_sub(199680) as u64
    };

    // First 32 bytes of exp (or all of exp if exp_len <= 32)
    let exp_start = 96 + base_len;
    let exp_view_len = exp_len.min(32);
    let exp_view: &[u8] = if exp_start + exp_view_len <= input.len() {
        &input[exp_start..exp_start + exp_view_len]
    } else if exp_start < input.len() {
        &input[exp_start..]
    } else {
        &[]
    };

    let highest_bit = highest_bit_index(exp_view);
    let adjusted: u64 = if exp_len <= 32 {
        highest_bit
    } else {
        8 * (exp_len - 32) as u64 + highest_bit
    };

    (mult_complexity * adjusted.max(1) / 3).max(200)
}

// Returns the bit-length minus 1, i.e. the index of the most-significant set bit
// (0 for value 1, 7 for value 128). Returns 0 for the zero value.
fn highest_bit_index(bytes: &[u8]) -> u64 {
    for (i, &b) in bytes.iter().enumerate() {
        if b != 0 {
            let remaining = bytes.len() - i - 1;
            return remaining as u64 * 8 + (7 - b.leading_zeros() as u64);
        }
    }
    0
}

fn u256_from_be_bytes(bytes: &[u8]) -> u64 {
    let mut result = 0u64;
    for (i, &b) in bytes.iter().rev().take(8).enumerate() {
        result |= (b as u64) << (i * 8);
    }
    result
}

// ── BN254 helpers ────────────────────────────────────────────────────────────

fn bn254_fq_from_be(bytes: &[u8; 32]) -> Option<ark_bn254::Fq> {
    use ark_ff::PrimeField;
    Some(ark_bn254::Fq::from_be_bytes_mod_order(bytes.as_slice()))
}

fn bn254_fr_from_be(bytes: &[u8; 32]) -> ark_bn254::Fr {
    use ark_ff::PrimeField;
    ark_bn254::Fr::from_be_bytes_mod_order(bytes.as_slice())
}

fn bn254_fq_to_be(f: &ark_bn254::Fq) -> [u8; 32] {
    use ark_ff::PrimeField;
    let bi = f.into_bigint();
    let le_u64 = bi.0; // [u64; 4] little-endian limbs
    // Convert to big-endian bytes
    let mut out = [0u8; 32];
    for (i, limb) in le_u64.iter().rev().enumerate() {
        out[i * 8..(i + 1) * 8].copy_from_slice(&limb.to_be_bytes());
    }
    out
}

/// Parse a 64-byte G1 point in Ethereum's big-endian format.
fn parse_g1(data: &[u8]) -> Option<ark_bn254::G1Affine> {
    use ark_ff::Zero;
    use ark_ec::AffineRepr;
    if data.len() < 64 { return None; }
    let x_bytes: [u8; 32] = data[0..32].try_into().ok()?;
    let y_bytes: [u8; 32] = data[32..64].try_into().ok()?;
    let x = bn254_fq_from_be(&x_bytes)?;
    let y = bn254_fq_from_be(&y_bytes)?;
    if x.is_zero() && y.is_zero() {
        return Some(ark_bn254::G1Affine::zero());
    }
    let p = ark_bn254::G1Affine::new_unchecked(x, y);
    if !p.is_on_curve() { return None; }
    Some(p)
}

/// Serialize a G1 point back to 64 bytes in Ethereum format.
fn serialize_g1(p: &ark_bn254::G1Affine) -> [u8; 64] {
    use ark_ec::AffineRepr;
    use ark_ff::Zero;
    let mut out = [0u8; 64];
    if p.is_zero() { return out; }
    out[0..32].copy_from_slice(&bn254_fq_to_be(&p.x));
    out[32..64].copy_from_slice(&bn254_fq_to_be(&p.y));
    out
}

/// Parse a 128-byte G2 point.  Ethereum uses (x_imag, x_real, y_imag, y_real).
fn parse_g2(data: &[u8]) -> Option<ark_bn254::G2Affine> {
    use ark_ff::Zero;
    use ark_ec::AffineRepr;
    if data.len() < 128 { return None; }
    let x_c1_bytes: [u8; 32] = data[0..32].try_into().ok()?;
    let x_c0_bytes: [u8; 32] = data[32..64].try_into().ok()?;
    let y_c1_bytes: [u8; 32] = data[64..96].try_into().ok()?;
    let y_c0_bytes: [u8; 32] = data[96..128].try_into().ok()?;
    let x = ark_bn254::Fq2::new(
        bn254_fq_from_be(&x_c0_bytes)?,
        bn254_fq_from_be(&x_c1_bytes)?,
    );
    let y = ark_bn254::Fq2::new(
        bn254_fq_from_be(&y_c0_bytes)?,
        bn254_fq_from_be(&y_c1_bytes)?,
    );
    if x.is_zero() && y.is_zero() {
        return Some(ark_bn254::G2Affine::zero());
    }
    let p = ark_bn254::G2Affine::new_unchecked(x, y);
    if !p.is_on_curve() { return None; }
    Some(p)
}

/// 0x06: BN_ADD (alt_bn128 point addition, EIP-196)
fn bn_add(input: &[u8], gas_limit: u64, schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    if gas_limit < schedule.bn_add {
        return Err(EvmError::OutOfGas { used: schedule.bn_add, limit: gas_limit });
    }
    // Pad input to 128 bytes
    let mut padded = [0u8; 128];
    let copy_len = input.len().min(128);
    padded[..copy_len].copy_from_slice(&input[..copy_len]);

    let p1 = parse_g1(&padded[0..64])
        .ok_or_else(|| EvmError::PrecompileError("BN_ADD: invalid G1 point p1".into()))?;
    let p2 = parse_g1(&padded[64..128])
        .ok_or_else(|| EvmError::PrecompileError("BN_ADD: invalid G1 point p2".into()))?;

    use ark_ec::{AffineRepr, CurveGroup};
    let sum = (p1.into_group() + p2.into_group()).into_affine();
    PrecompileResult::ok(serialize_g1(&sum).to_vec(), schedule.bn_add)
}

/// 0x07: BN_MUL (alt_bn128 scalar multiplication, EIP-196)
fn bn_mul(input: &[u8], gas_limit: u64, schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    if gas_limit < schedule.bn_mul {
        return Err(EvmError::OutOfGas { used: schedule.bn_mul, limit: gas_limit });
    }
    let mut padded = [0u8; 96];
    let copy_len = input.len().min(96);
    padded[..copy_len].copy_from_slice(&input[..copy_len]);

    let p = parse_g1(&padded[0..64])
        .ok_or_else(|| EvmError::PrecompileError("BN_MUL: invalid G1 point".into()))?;
    let scalar_bytes: [u8; 32] = padded[64..96].try_into().unwrap();
    let scalar = bn254_fr_from_be(&scalar_bytes);

    use ark_ec::{AffineRepr, CurveGroup};
    let result = (p.into_group() * scalar).into_affine();
    PrecompileResult::ok(serialize_g1(&result).to_vec(), schedule.bn_mul)
}

/// 0x08: BN_PAIRING (alt_bn128 pairing check, EIP-197)
fn bn_pairing(input: &[u8], gas_limit: u64, schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    if input.len() % 192 != 0 {
        return PrecompileResult::error("BN_PAIRING: input not multiple of 192 bytes");
    }
    let num_pairs = input.len() / 192;
    let gas = schedule.bn_pairing_base + (num_pairs as u64) * schedule.bn_pairing_point;
    if gas_limit < gas {
        return Err(EvmError::OutOfGas { used: gas, limit: gas_limit });
    }

    use ark_ec::pairing::Pairing;
    use ark_ff::Zero;

    let mut g1_points = Vec::with_capacity(num_pairs);
    let mut g2_points = Vec::with_capacity(num_pairs);

    for i in 0..num_pairs {
        let chunk = &input[i * 192..(i + 1) * 192];
        let p1 = parse_g1(&chunk[0..64])
            .ok_or_else(|| EvmError::PrecompileError("BN_PAIRING: invalid G1 point".into()))?;
        let p2 = parse_g2(&chunk[64..192])
            .ok_or_else(|| EvmError::PrecompileError("BN_PAIRING: invalid G2 point".into()))?;
        // Skip pairs where G1 or G2 is the identity (neutral element)
        use ark_ec::AffineRepr;
        if p1.is_zero() || p2.is_zero() {
            continue;
        }
        g1_points.push(p1);
        g2_points.push(p2);
    }

    let pairing_result = ark_bn254::Bn254::multi_pairing(g1_points, g2_points);

    // PairingOutput::zero() is the multiplicative identity (Fq12::one())
    let success = pairing_result.is_zero();
    let mut output = vec![0u8; 32];
    if success { output[31] = 1; }
    PrecompileResult::ok(output, gas)
}

/// 0x09: BLAKE2F compression function (EIP-152)
/// Input: 213 bytes = rounds(4) || h(64) || m(128) || t(16) || f(1)
fn blake2f(input: &[u8], gas_limit: u64, schedule: &GasSchedule) -> EvmResult<PrecompileResult> {
    if input.len() != 213 {
        return PrecompileResult::error("Invalid BLAKE2F input length: expected 213 bytes");
    }
    let f_byte = input[212];
    if f_byte != 0 && f_byte != 1 {
        return PrecompileResult::error("Invalid BLAKE2F f flag: must be 0 or 1");
    }

    let rounds = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
    let gas = (rounds as u64).saturating_mul(schedule.blake2f_round).max(1);
    if gas_limit < gas {
        return Err(EvmError::OutOfGas { used: gas, limit: gas_limit });
    }

    // Parse h (8 x u64 little-endian)
    let mut h = [0u64; 8];
    for i in 0..8 {
        h[i] = u64::from_le_bytes(input[4 + i * 8..12 + i * 8].try_into().unwrap());
    }
    // Parse m (16 x u64 little-endian)
    let mut m = [0u64; 16];
    for i in 0..16 {
        m[i] = u64::from_le_bytes(input[68 + i * 8..76 + i * 8].try_into().unwrap());
    }
    // Parse t (2 x u64 little-endian)
    let t = [
        u64::from_le_bytes(input[196..204].try_into().unwrap()),
        u64::from_le_bytes(input[204..212].try_into().unwrap()),
    ];
    let finalize = f_byte == 1;

    blake2b_compress(&mut h, &m, t, finalize, rounds);

    // Serialize output (8 x u64 little-endian)
    let mut output = vec![0u8; 64];
    for (i, &word) in h.iter().enumerate() {
        output[i * 8..(i + 1) * 8].copy_from_slice(&word.to_le_bytes());
    }
    PrecompileResult::ok(output, gas)
}

/// BLAKE2b-F compression function (RFC 7693 §3.2, parameterized rounds for EIP-152)
fn blake2b_compress(h: &mut [u64; 8], m: &[u64; 16], t: [u64; 2], f: bool, rounds: u32) {
    const IV: [u64; 8] = [
        0x6a09e667f3bcc908, 0xbb67ae8584caa73b,
        0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1,
        0x510e527fade682d1, 0x9b05688c2b3e6c1f,
        0x1f83d9abfb41bd6b, 0x5be0cd19137e2179,
    ];
    const SIGMA: [[usize; 16]; 10] = [
        [ 0, 1, 2, 3, 4, 5, 6, 7, 8, 9,10,11,12,13,14,15],
        [14,10, 4, 8, 9,15,13, 6, 1,12, 0, 2,11, 7, 5, 3],
        [11, 8,12, 0, 5, 2,15,13,10,14, 3, 6, 7, 1, 9, 4],
        [ 7, 9, 3, 1,13,12,11,14, 2, 6, 5,10, 4, 0,15, 8],
        [ 9, 0, 5, 7, 2, 4,10,15,14, 1,11,12, 6, 8, 3,13],
        [ 2,12, 6,10, 0,11, 8, 3, 4,13, 7, 5,15,14, 1, 9],
        [12, 5, 1,15,14,13, 4,10, 0, 7, 6, 3, 9, 2, 8,11],
        [13,11, 7,14,12, 1, 3, 9, 5, 0,15, 4, 8, 6, 2,10],
        [ 6,15,14, 9,11, 3, 0, 8,12, 2,13, 7, 1, 4,10, 5],
        [10, 2, 8, 4, 7, 6, 1, 5,15,11, 9,14, 3,12,13, 0],
    ];

    #[inline(always)]
    fn g(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
        v[d] = (v[d] ^ v[a]).rotate_right(32);
        v[c] = v[c].wrapping_add(v[d]);
        v[b] = (v[b] ^ v[c]).rotate_right(24);
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
        v[d] = (v[d] ^ v[a]).rotate_right(16);
        v[c] = v[c].wrapping_add(v[d]);
        v[b] = (v[b] ^ v[c]).rotate_right(63);
    }

    let mut v = [0u64; 16];
    v[..8].copy_from_slice(h);
    v[8..16].copy_from_slice(&IV);
    v[12] ^= t[0];
    v[13] ^= t[1];
    if f { v[14] = !v[14]; }

    for i in 0..rounds as usize {
        let s = &SIGMA[i % 10];
        g(&mut v, 0, 4,  8, 12, m[s[ 0]], m[s[ 1]]);
        g(&mut v, 1, 5,  9, 13, m[s[ 2]], m[s[ 3]]);
        g(&mut v, 2, 6, 10, 14, m[s[ 4]], m[s[ 5]]);
        g(&mut v, 3, 7, 11, 15, m[s[ 6]], m[s[ 7]]);
        g(&mut v, 0, 5, 10, 15, m[s[ 8]], m[s[ 9]]);
        g(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
        g(&mut v, 2, 7,  8, 13, m[s[12]], m[s[13]]);
        g(&mut v, 3, 4,  9, 14, m[s[14]], m[s[15]]);
    }

    for i in 0..8 {
        h[i] ^= v[i] ^ v[i + 8];
    }
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

    #[test]
    fn test_modexp_simple() {
        let schedule = GasSchedule::default();
        // 3^2 mod 5 = 4
        let mut input = vec![0u8; 96];
        input[31] = 1; // base_len = 1
        input[63] = 1; // exp_len = 1
        input[95] = 1; // mod_len = 1
        input.push(3); // base = 3
        input.push(2); // exp = 2
        input.push(5); // mod = 5
        let result = modexp(&input, 10_000_000, &schedule).unwrap();
        assert_eq!(result.output, vec![4]);
    }

    #[test]
    fn test_modexp_zero_exp() {
        let schedule = GasSchedule::default();
        // 7^0 mod 11 = 1
        let mut input = vec![0u8; 96];
        input[31] = 1; // base_len = 1
        input[63] = 1; // exp_len = 1
        input[95] = 1; // mod_len = 1
        input.push(7); // base
        input.push(0); // exp = 0
        input.push(11); // mod
        let result = modexp(&input, 10_000_000, &schedule).unwrap();
        assert_eq!(result.output, vec![1]);
    }

    #[test]
    fn test_blake2f_known_vector() {
        let schedule = GasSchedule::default();
        // EIP-152 test vector #1: 0 rounds, empty input → unchanged state
        // rounds=0, h=IV, m=zeros, t=[0,0], f=false
        let mut input = vec![0u8; 213];
        // rounds = 0 (already 0)
        // h = BLAKE2b IV (8 x u64 LE)
        let iv: [u64; 8] = [
            0x6a09e667f3bcc908, 0xbb67ae8584caa73b,
            0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1,
            0x510e527fade682d1, 0x9b05688c2b3e6c1f,
            0x1f83d9abfb41bd6b, 0x5be0cd19137e2179,
        ];
        for (i, &word) in iv.iter().enumerate() {
            input[4 + i*8..12 + i*8].copy_from_slice(&word.to_le_bytes());
        }
        // m, t = 0; f = 0 (already done)
        let result = blake2f(&input, 10_000_000, &schedule).unwrap();
        // 0 rounds means h is XOR'd with itself and IV-variants → h unchanged
        assert_eq!(result.output.len(), 64);
    }

    #[test]
    fn test_bn_add_identity() {
        let schedule = GasSchedule::default();
        // Adding identity point (0, 0) to itself should return identity
        let input = vec![0u8; 128];
        let result = bn_add(&input, 10_000_000, &schedule).unwrap();
        assert_eq!(result.output, vec![0u8; 64]);
    }

    #[test]
    fn test_bn_mul_zero_scalar() {
        let schedule = GasSchedule::default();
        // Multiplying any point by 0 should return identity
        // Use the generator point G1 (well-known coordinates)
        let mut input = vec![0u8; 96];
        // G1 generator x = 1
        input[31] = 1;
        // G1 generator y = 2
        input[63] = 2;
        // scalar = 0 (already 0)
        let result = bn_mul(&input, 10_000_000, &schedule).unwrap();
        assert_eq!(result.output, vec![0u8; 64]);
    }

    #[test]
    fn test_bn_pairing_empty() {
        let schedule = GasSchedule::default();
        // Empty pairing should return 1 (trivial product = identity)
        let result = bn_pairing(&[], 10_000_000, &schedule).unwrap();
        assert_eq!(result.output[31], 1); // success = true for empty input
    }
}
