use crate::{Hash, NexusError, Result};
use k256::ecdsa::{self, signature::hazmat::PrehashVerifier, SigningKey, VerifyingKey, RecoveryId};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

/// ECDSA signature (secp256k1, Ethereum compatible)
#[derive(Clone, Serialize, Deserialize)]
pub struct EcdsaSignature {
    pub r: [u8; 32],
    pub s: [u8; 32],
    pub v: u8, // Recovery ID
}

impl EcdsaSignature {
    pub fn to_bytes(&self) -> [u8; 65] {
        let mut bytes = [0u8; 65];
        bytes[0..32].copy_from_slice(&self.r);
        bytes[32..64].copy_from_slice(&self.s);
        bytes[64] = self.v;
        bytes
    }

    pub fn from_bytes(bytes: &[u8; 65]) -> Self {
        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&bytes[0..32]);
        s.copy_from_slice(&bytes[32..64]);
        Self { r, s, v: bytes[64] }
    }
}

impl std::fmt::Debug for EcdsaSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sig(r={}, v={})", hex::encode(&self.r[..4]), self.v)
    }
}

/// Public key (uncompressed secp256k1)
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKey(#[serde(with = "serde_bytes")] pub [u8; 64]); // Uncompressed without prefix

impl PublicKey {
    pub fn from_secret(secret: &SecretKey) -> Result<Self> {
        let verifying_key = secret.0.verifying_key();
        let encoded = verifying_key.to_encoded_point(false);
        let bytes = encoded.as_bytes();

        // Skip the 0x04 prefix for uncompressed keys
        if bytes.len() == 65 {
            let mut pubkey = [0u8; 64];
            pubkey.copy_from_slice(&bytes[1..]);
            Ok(Self(pubkey))
        } else {
            Err(NexusError::Crypto("Invalid public key length".into()))
        }
    }

    /// Derive Ethereum-style address from public key
    pub fn to_address(&self) -> crate::Address {
        let hash = keccak256(&self.0);
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&hash[12..32]);
        crate::Address(addr)
    }

    pub fn to_verifying_key(&self) -> Result<VerifyingKey> {
        // Add the 0x04 prefix back
        let mut bytes = [0u8; 65];
        bytes[0] = 0x04;
        bytes[1..].copy_from_slice(&self.0);

        VerifyingKey::from_sec1_bytes(&bytes)
            .map_err(|e| NexusError::Crypto(format!("Invalid public key: {}", e)))
    }
}

impl std::fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PubKey({}...)", hex::encode(&self.0[..8]))
    }
}

/// Secret key (secp256k1)
pub struct SecretKey(SigningKey);

impl SecretKey {
    /// Generate a new random secret key
    pub fn generate() -> Self {
        Self(SigningKey::random(&mut OsRng))
    }

    /// Create from raw bytes
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self> {
        SigningKey::from_bytes(bytes.into())
            .map(Self)
            .map_err(|e| NexusError::Crypto(format!("Invalid secret key: {}", e)))
    }

    /// Export as raw bytes
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes().into()
    }

    /// Sign a message hash
    pub fn sign(&self, message_hash: &Hash) -> Result<EcdsaSignature> {
        let (signature, recovery_id) = self.0
            .sign_prehash_recoverable(message_hash.as_bytes())
            .map_err(|e| NexusError::Crypto(format!("Signing failed: {}", e)))?;

        let sig_bytes = signature.to_bytes();
        let mut r = [0u8; 32];
        let mut s = [0u8; 32];
        r.copy_from_slice(&sig_bytes[0..32]);
        s.copy_from_slice(&sig_bytes[32..64]);

        Ok(EcdsaSignature {
            r,
            s,
            v: recovery_id.to_byte() + 27, // Ethereum convention
        })
    }
}

/// Verify an ECDSA signature
pub fn verify_signature(
    message_hash: &Hash,
    signature: &EcdsaSignature,
    public_key: &PublicKey,
) -> Result<bool> {
    let verifying_key = public_key.to_verifying_key()?;

    let mut sig_bytes = [0u8; 64];
    sig_bytes[0..32].copy_from_slice(&signature.r);
    sig_bytes[32..64].copy_from_slice(&signature.s);

    let ecdsa_sig = ecdsa::Signature::from_bytes((&sig_bytes).into())
        .map_err(|e| NexusError::Crypto(format!("Invalid signature format: {}", e)))?;

    // Parse recovery id to validate it (not needed for signature verification itself)
    let _recovery_id = RecoveryId::try_from(signature.v.saturating_sub(27))
        .map_err(|e| NexusError::Crypto(format!("Invalid recovery id: {}", e)))?;

    Ok(verifying_key.verify_prehash(message_hash.as_bytes(), &ecdsa_sig).is_ok())
}

/// Recover public key from signature (used for Ethereum tx compatibility)
pub fn recover_public_key(
    message_hash: &Hash,
    signature: &EcdsaSignature,
) -> Result<PublicKey> {
    use k256::ecdsa::RecoveryId;

    let mut sig_bytes = [0u8; 64];
    sig_bytes[0..32].copy_from_slice(&signature.r);
    sig_bytes[32..64].copy_from_slice(&signature.s);

    let ecdsa_sig = ecdsa::Signature::from_bytes((&sig_bytes).into())
        .map_err(|e| NexusError::Crypto(format!("Invalid signature: {}", e)))?;

    // Recover the recovery id from the original EcdsaSignature's v field (Ethereum offset 27/28)
    let recovery_id = RecoveryId::try_from(signature.v.saturating_sub(27))
        .map_err(|e| NexusError::Crypto(format!("Invalid recovery id: {}", e)))?;

    let verifying_key = VerifyingKey::recover_from_prehash(
        message_hash.as_bytes(),
        &ecdsa_sig,
        recovery_id,
    ).map_err(|e| NexusError::Crypto(format!("Recovery failed: {}", e)))?;

    let encoded = verifying_key.to_encoded_point(false);
    let bytes = encoded.as_bytes();

    if bytes.len() == 65 {
        let mut pubkey = [0u8; 64];
        pubkey.copy_from_slice(&bytes[1..]);
        Ok(PublicKey(pubkey))
    } else {
        Err(NexusError::Crypto("Invalid recovered key length".into()))
    }
}

/// Keccak-256 hash (Ethereum compatible)
pub fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Blake3 hash (faster, used for internal operations)
pub fn blake3_hash(data: &[u8]) -> Hash {
    Hash::new(*blake3::hash(data).as_bytes())
}

/// Hash for DAG vertex ID (combines multiple inputs)
pub fn vertex_hash(
    parents: &[Hash],
    transactions: &[Hash],
    timestamp: u64,
    validator: &[u8; 32],
) -> Hash {
    let mut hasher = blake3::Hasher::new();

    // Hash parents
    hasher.update(&(parents.len() as u32).to_le_bytes());
    for parent in parents {
        hasher.update(parent.as_bytes());
    }

    // Hash transactions
    hasher.update(&(transactions.len() as u32).to_le_bytes());
    for tx in transactions {
        hasher.update(tx.as_bytes());
    }

    // Hash metadata
    hasher.update(&timestamp.to_le_bytes());
    hasher.update(validator);

    Hash::new(*hasher.finalize().as_bytes())
}

/// Merkle root computation
pub fn compute_merkle_root(hashes: &[Hash]) -> Hash {
    if hashes.is_empty() {
        return Hash::ZERO;
    }
    if hashes.len() == 1 {
        return hashes[0];
    }

    let mut current_level: Vec<Hash> = hashes.to_vec();

    while current_level.len() > 1 {
        let mut next_level = Vec::with_capacity((current_level.len() + 1) / 2);

        for chunk in current_level.chunks(2) {
            let combined = if chunk.len() == 2 {
                let mut data = [0u8; 64];
                data[0..32].copy_from_slice(chunk[0].as_bytes());
                data[32..64].copy_from_slice(chunk[1].as_bytes());
                blake3_hash(&data)
            } else {
                // Odd number of elements, hash with itself
                let mut data = [0u8; 64];
                data[0..32].copy_from_slice(chunk[0].as_bytes());
                data[32..64].copy_from_slice(chunk[0].as_bytes());
                blake3_hash(&data)
            };
            next_level.push(combined);
        }

        current_level = next_level;
    }

    current_level[0]
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_generation_and_signing() {
        let secret = SecretKey::generate();
        let public = PublicKey::from_secret(&secret).unwrap();
        let message_hash = blake3_hash(b"test message");
        let signature = secret.sign(&message_hash).unwrap();
        assert!(verify_signature(&message_hash, &signature, &public).unwrap());
    }


    #[test]
    fn test_address_derivation() {
        let secret = SecretKey::generate();
        let public = PublicKey::from_secret(&secret).unwrap();
        let address = public.to_address();

        assert_eq!(address.0.len(), 20);
    }

    #[test]
    fn test_merkle_root() {
        let hashes = vec![
            blake3_hash(b"tx1"),
            blake3_hash(b"tx2"),
            blake3_hash(b"tx3"),
        ];

        let root = compute_merkle_root(&hashes);
        assert_ne!(root, Hash::ZERO);

        // Same inputs should give same root
        let root2 = compute_merkle_root(&hashes);
        assert_eq!(root, root2);
    }
}

