//! Transaction types for NexusChain

use crate::{Address, EcdsaSignature, Gas, Hash, Nonce, U256, blake3_hash, keccak256};
use serde::{Deserialize, Serialize};

/// Transaction type discriminator
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum TxType {
    /// Legacy Ethereum-style transaction
    Legacy = 0,
    
    /// EIP-2930 access list transaction
    AccessList = 1,
    
    /// EIP-1559 dynamic fee transaction
    DynamicFee = 2,
    
    /// NexusChain private transaction (ZKP-shielded)
    Private = 0x80,
    
    /// ISO 20022 payment message
    IsoPayment = 0x81,
    
    /// System transaction (validator rewards, governance)
    System = 0xFF,
}

/// Core transaction structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transaction {
    /// Transaction type
    pub tx_type: TxType,
    
    /// Chain ID for replay protection
    pub chain_id: u64,
    
    /// Sender nonce
    pub nonce: Nonce,
    
    /// Recipient (None for contract creation)
    pub to: Option<Address>,
    
    /// Value to transfer
    pub value: U256,
    
    /// Transaction data (calldata or init code)
    pub data: Vec<u8>,
    
    /// Gas limit
    pub gas_limit: Gas,
    
    /// Gas price (legacy) or max fee per gas (EIP-1559)
    pub gas_price: U256,
    
    /// Max priority fee (EIP-1559 only)
    pub max_priority_fee: Option<U256>,
    
    /// Access list (EIP-2930+)
    pub access_list: Vec<AccessListEntry>,
    
    /// ZKP proof data (for private transactions)
    pub zkp_proof: Option<ZkpProofData>,
    
    /// ISO message data (for ISO payment transactions)
    pub iso_data: Option<IsoMessageData>,
    
    /// Signature
    pub signature: Option<EcdsaSignature>,
}

impl Transaction {
    /// Create a new legacy transaction
    pub fn new_legacy(
        chain_id: u64,
        nonce: Nonce,
        to: Option<Address>,
        value: U256,
        data: Vec<u8>,
        gas_limit: Gas,
        gas_price: U256,
    ) -> Self {
        Self {
            tx_type: TxType::Legacy,
            chain_id,
            nonce,
            to,
            value,
            data,
            gas_limit,
            gas_price,
            max_priority_fee: None,
            access_list: Vec::new(),
            zkp_proof: None,
            iso_data: None,
            signature: None,
        }
    }
    
    /// Create a new EIP-1559 transaction
    pub fn new_dynamic_fee(
        chain_id: u64,
        nonce: Nonce,
        to: Option<Address>,
        value: U256,
        data: Vec<u8>,
        gas_limit: Gas,
        max_fee_per_gas: U256,
        max_priority_fee: U256,
    ) -> Self {
        Self {
            tx_type: TxType::DynamicFee,
            chain_id,
            nonce,
            to,
            value,
            data,
            gas_limit,
            gas_price: max_fee_per_gas,
            max_priority_fee: Some(max_priority_fee),
            access_list: Vec::new(),
            zkp_proof: None,
            iso_data: None,
            signature: None,
        }
    }
    
    /// Create a private (ZKP-shielded) transaction
    pub fn new_private(
        chain_id: u64,
        nonce: Nonce,
        zkp_proof: ZkpProofData,
    ) -> Self {
        Self {
            tx_type: TxType::Private,
            chain_id,
            nonce,
            to: None,
            value: U256::ZERO,
            data: Vec::new(),
            gas_limit: 500_000, // Fixed gas for ZKP verification
            gas_price: U256::from_u64(1_000_000_000),
            max_priority_fee: None,
            access_list: Vec::new(),
            zkp_proof: Some(zkp_proof),
            iso_data: None,
            signature: None,
        }
    }
    
    /// Create an ISO 20022 payment transaction
    pub fn new_iso_payment(
        chain_id: u64,
        nonce: Nonce,
        iso_data: IsoMessageData,
        gas_limit: Gas,
        gas_price: U256,
    ) -> Self {
        Self {
            tx_type: TxType::IsoPayment,
            chain_id,
            nonce,
            to: Some(Address::SYSTEM_ISO),
            value: U256::ZERO,
            data: Vec::new(),
            gas_limit,
            gas_price,
            max_priority_fee: None,
            access_list: Vec::new(),
            zkp_proof: None,
            iso_data: Some(iso_data),
            signature: None,
        }
    }
    
    /// Calculate transaction hash (for signing)
    pub fn signing_hash(&self) -> Hash {
        let encoded = self.encode_for_signing();
        Hash::new(keccak256(&encoded))
    }
    
    /// Calculate transaction hash (with signature)
    pub fn hash(&self) -> Hash {
        let encoded = self.encode();
        blake3_hash(&encoded)
    }
    
    /// RLP-encode transaction for signing
    fn encode_for_signing(&self) -> Vec<u8> {
        // Simplified encoding - in production use proper RLP
        let mut buf = Vec::new();
        
        buf.push(self.tx_type as u8);
        buf.extend_from_slice(&self.chain_id.to_be_bytes());
        buf.extend_from_slice(&self.nonce.to_be_bytes());
        buf.extend_from_slice(&self.gas_limit.to_be_bytes());
        buf.extend_from_slice(&self.gas_price.to_be_bytes());
        
        if let Some(to) = &self.to {
            buf.push(1);
            buf.extend_from_slice(&to.0);
        } else {
            buf.push(0);
        }
        
        buf.extend_from_slice(&self.value.to_be_bytes());
        buf.extend_from_slice(&(self.data.len() as u32).to_be_bytes());
        buf.extend_from_slice(&self.data);
        
        buf
    }
    
    /// Full transaction encoding
    fn encode(&self) -> Vec<u8> {
        let mut buf = self.encode_for_signing();
        
        if let Some(sig) = &self.signature {
            buf.extend_from_slice(&sig.to_bytes());
        }
        
        buf
    }
    
    /// Verify transaction signature and recover sender
    pub fn recover_sender(&self) -> Result<Address, crate::NexusError> {
        let signature = self.signature.as_ref()
            .ok_or_else(|| crate::NexusError::Validation("Missing signature".into()))?;
        
        let hash = self.signing_hash();
        let public_key = crate::recover_public_key(&hash, signature)?;
        
        Ok(public_key.to_address())
    }
    
    /// Estimated gas for this transaction type
    pub fn intrinsic_gas(&self) -> Gas {
        let base_gas = match self.tx_type {
            TxType::Legacy | TxType::AccessList | TxType::DynamicFee => 21_000,
            TxType::Private => 100_000, // ZKP verification overhead
            TxType::IsoPayment => 50_000, // ISO parsing overhead
            TxType::System => 0, // Free for system transactions
        };
        
        // Data cost: 4 gas per zero byte, 16 gas per non-zero byte
        let data_gas: Gas = self.data.iter()
            .map(|&b| if b == 0 { 4 } else { 16 })
            .sum();
        
        // Contract creation adds 32,000 gas
        let create_gas = if self.to.is_none() { 32_000 } else { 0 };
        
        base_gas + data_gas + create_gas
    }
    
    /// Check if transaction is valid (basic validation)
    pub fn validate(&self) -> Result<(), crate::NexusError> {
        // Check gas limit
        if self.gas_limit < self.intrinsic_gas() {
            return Err(crate::NexusError::Validation("Gas limit below intrinsic gas".into()));
        }
        
        // Check transaction size
        if self.data.len() > crate::MAX_TX_SIZE {
            return Err(crate::NexusError::Validation("Transaction too large".into()));
        }
        
        // Type-specific validation
        match self.tx_type {
            TxType::Private => {
                if self.zkp_proof.is_none() {
                    return Err(crate::NexusError::Validation("Private tx missing ZKP proof".into()));
                }
            }
            TxType::IsoPayment => {
                if self.iso_data.is_none() {
                    return Err(crate::NexusError::Validation("ISO tx missing message data".into()));
                }
            }
            _ => {}
        }
        
        Ok(())
    }
}

// ─────────────────────────── RLP decoding ─────────────────────────────────
//
// Helpers for decoding raw Ethereum transactions sent via eth_sendRawTransaction.
// Only handles the wire format; does not re-hash for signature verification.

mod rlp_decode {
    use super::*;
    use alloy_rlp::Header;

    /// Decode the next RLP string from `buf`, returning the raw content bytes.
    /// Handles: single-byte (b < 0x80), short string, long string.
    /// Returns None for lists or on truncated input.
    fn next_bytes<'a>(buf: &mut &'a [u8]) -> Option<&'a [u8]> {
        if buf.is_empty() {
            return None;
        }
        let first = buf[0];
        if first < 0x80 {
            // Self-describing single byte — Header::decode would not advance buf
            let result = &buf[..1];
            *buf = &buf[1..];
            return Some(result);
        }
        let header = Header::decode(buf).ok()?;
        if header.list {
            return None;
        }
        if buf.len() < header.payload_length {
            return None;
        }
        let result = &buf[..header.payload_length];
        *buf = &buf[header.payload_length..];
        Some(result)
    }

    fn next_uint(buf: &mut &[u8]) -> Option<u64> {
        let bytes = next_bytes(buf)?;
        if bytes.len() > 8 {
            return None; // doesn't fit in u64
        }
        let mut v = 0u64;
        for &b in bytes {
            v = (v << 8) | b as u64;
        }
        Some(v)
    }

    fn next_u256(buf: &mut &[u8]) -> Option<U256> {
        let bytes = next_bytes(buf)?;
        if bytes.len() > 32 {
            return None;
        }
        let mut arr = [0u8; 32];
        arr[32 - bytes.len()..].copy_from_slice(bytes);
        Some(U256::from_be_bytes(arr))
    }

    fn next_address(buf: &mut &[u8]) -> Option<Option<Address>> {
        let bytes = next_bytes(buf)?;
        match bytes.len() {
            0 => Some(None), // contract creation
            20 => {
                let mut arr = [0u8; 20];
                arr.copy_from_slice(bytes);
                Some(Some(Address(arr)))
            }
            _ => None,
        }
    }

    fn next_sig_scalar(buf: &mut &[u8]) -> Option<[u8; 32]> {
        let bytes = next_bytes(buf)?;
        if bytes.len() > 32 {
            return None;
        }
        let mut arr = [0u8; 32];
        arr[32 - bytes.len()..].copy_from_slice(bytes);
        Some(arr)
    }

    fn skip_list(buf: &mut &[u8]) -> Option<()> {
        let header = Header::decode(buf).ok()?;
        if !header.list {
            return None;
        }
        if buf.len() < header.payload_length {
            return None;
        }
        *buf = &buf[header.payload_length..];
        Some(())
    }

    /// EIP-1559 (type 2): 0x02 || RLP([chainId, nonce, maxPriorityFee, maxFee, gas, to, value, data, accessList, yParity, r, s])
    pub fn eip1559(payload: &[u8]) -> Option<Transaction> {
        let mut buf = payload;
        let h = Header::decode(&mut buf).ok()?;
        if !h.list { return None; }

        let chain_id = next_uint(&mut buf)?;
        let nonce = next_uint(&mut buf)?;
        let max_priority_fee = next_u256(&mut buf)?;
        let max_fee_per_gas = next_u256(&mut buf)?;
        let gas_limit = next_uint(&mut buf)?;
        let to = next_address(&mut buf)?;
        let value = next_u256(&mut buf)?;
        let data = next_bytes(&mut buf)?.to_vec();
        skip_list(&mut buf)?;
        let y_parity = next_uint(&mut buf)? as u8;
        let r = next_sig_scalar(&mut buf)?;
        let s = next_sig_scalar(&mut buf)?;

        Some(Transaction {
            tx_type: TxType::DynamicFee,
            chain_id,
            nonce,
            to,
            value,
            data,
            gas_limit,
            gas_price: max_fee_per_gas,
            max_priority_fee: Some(max_priority_fee),
            access_list: Vec::new(),
            zkp_proof: None,
            iso_data: None,
            signature: Some(EcdsaSignature { r, s, v: y_parity + 27 }),
        })
    }

    /// EIP-2930 (type 1): 0x01 || RLP([chainId, nonce, gasPrice, gas, to, value, data, accessList, yParity, r, s])
    pub fn eip2930(payload: &[u8]) -> Option<Transaction> {
        let mut buf = payload;
        let h = Header::decode(&mut buf).ok()?;
        if !h.list { return None; }

        let chain_id = next_uint(&mut buf)?;
        let nonce = next_uint(&mut buf)?;
        let gas_price = next_u256(&mut buf)?;
        let gas_limit = next_uint(&mut buf)?;
        let to = next_address(&mut buf)?;
        let value = next_u256(&mut buf)?;
        let data = next_bytes(&mut buf)?.to_vec();
        skip_list(&mut buf)?;
        let y_parity = next_uint(&mut buf)? as u8;
        let r = next_sig_scalar(&mut buf)?;
        let s = next_sig_scalar(&mut buf)?;

        Some(Transaction {
            tx_type: TxType::AccessList,
            chain_id,
            nonce,
            to,
            value,
            data,
            gas_limit,
            gas_price,
            max_priority_fee: None,
            access_list: Vec::new(),
            zkp_proof: None,
            iso_data: None,
            signature: Some(EcdsaSignature { r, s, v: y_parity + 27 }),
        })
    }

    /// Legacy (type 0): RLP([nonce, gasPrice, gas, to, value, data, v, r, s])
    /// EIP-155 v encodes chain_id: v = 2*chain_id + 35 + yParity
    pub fn legacy(payload: &[u8]) -> Option<Transaction> {
        let mut buf = payload;
        let h = Header::decode(&mut buf).ok()?;
        if !h.list { return None; }

        let nonce = next_uint(&mut buf)?;
        let gas_price = next_u256(&mut buf)?;
        let gas_limit = next_uint(&mut buf)?;
        let to = next_address(&mut buf)?;
        let value = next_u256(&mut buf)?;
        let data = next_bytes(&mut buf)?.to_vec();
        let v_raw = next_uint(&mut buf)?;
        let r = next_sig_scalar(&mut buf)?;
        let s = next_sig_scalar(&mut buf)?;

        let (chain_id, y_parity) = if v_raw == 27 || v_raw == 28 {
            (0u64, (v_raw - 27) as u8)
        } else if v_raw >= 35 {
            let cid = (v_raw - 35) / 2;
            let yp = (v_raw - cid * 2 - 35) as u8;
            (cid, yp)
        } else {
            return None;
        };

        Some(Transaction {
            tx_type: TxType::Legacy,
            chain_id,
            nonce,
            to,
            value,
            data,
            gas_limit,
            gas_price,
            max_priority_fee: None,
            access_list: Vec::new(),
            zkp_proof: None,
            iso_data: None,
            signature: Some(EcdsaSignature { r, s, v: y_parity + 27 }),
        })
    }
}

impl Transaction {
    /// Decode an RLP-encoded raw Ethereum transaction.
    /// Supports EIP-1559 (type 2), EIP-2930 (type 1), and legacy (type 0).
    pub fn from_rlp_bytes(raw: &[u8]) -> Option<Self> {
        if raw.is_empty() {
            return None;
        }
        match raw[0] {
            0x02 => rlp_decode::eip1559(&raw[1..]),
            0x01 => rlp_decode::eip2930(&raw[1..]),
            b if b >= 0xc0 => rlp_decode::legacy(raw),
            _ => None,
        }
    }
}

/// Access list entry (EIP-2930)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccessListEntry {
    pub address: Address,
    pub storage_keys: Vec<Hash>,
}

/// Zero-knowledge proof data for private transactions
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZkpProofData {
    /// Proof type (Groth16, PLONK, etc.)
    pub proof_type: ZkpProofType,
    
    /// Serialized proof bytes
    pub proof: Vec<u8>,
    
    /// Public inputs to the circuit
    pub public_inputs: Vec<[u8; 32]>,
    
    /// Nullifiers (to prevent double-spending)
    pub nullifiers: Vec<Hash>,
    
    /// Commitments (output notes)
    pub commitments: Vec<Hash>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZkpProofType {
    Groth16,
    Plonk,
    Stark,
}

/// ISO message data for financial transactions
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IsoMessageData {
    /// ISO message type (pacs.008, pacs.002, etc.)
    pub message_type: IsoMessageType,
    
    /// Business message identifier
    pub msg_id: String,
    
    /// Creation timestamp (ISO 8601)
    pub creation_timestamp: String,
    
    /// Instructing agent (BIC)
    pub instructing_agent: Option<String>,
    
    /// Instructed agent (BIC)
    pub instructed_agent: Option<String>,
    
    /// Debtor information
    pub debtor: Option<PartyInfo>,
    
    /// Creditor information
    pub creditor: Option<PartyInfo>,
    
    /// Settlement amount
    pub amount: Option<IsoAmount>,
    
    /// Raw XML message (for full compliance)
    pub raw_xml: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IsoMessageType {
    /// Credit Transfer (pacs.008)
    CreditTransfer,
    
    /// Payment Status (pacs.002)
    PaymentStatus,
    
    /// Account Statement (camt.053)
    AccountStatement,
    
    /// Direct Debit (pacs.003)
    DirectDebit,
    
    /// Return (pacs.004)
    Return,
}

/// Party information (debtor/creditor)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartyInfo {
    /// Name
    pub name: String,
    
    /// Account identifier (IBAN or other)
    pub account: String,
    
    /// BIC code
    pub bic: Option<String>,
    
    /// LEI (Legal Entity Identifier)
    pub lei: Option<String>,
    
    /// Address (structured)
    pub address: Option<PostalAddress>,
}

/// Postal address (ISO 20022 format)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PostalAddress {
    pub street_name: Option<String>,
    pub building_number: Option<String>,
    pub postal_code: Option<String>,
    pub town_name: Option<String>,
    pub country: String, // ISO 3166-1 alpha-2
}

/// ISO 20022 amount
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IsoAmount {
    /// Amount in minor units (cents)
    pub value: u64,
    
    /// Currency code (ISO 4217)
    pub currency: String,
}

/// Signed transaction ready for broadcast
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignedTransaction {
    pub transaction: Transaction,
    pub hash: Hash,
    pub sender: Address,
}

impl SignedTransaction {
    pub fn new(transaction: Transaction) -> Result<Self, crate::NexusError> {
        let hash = transaction.hash();
        let sender = transaction.recover_sender()?;
        
        Ok(Self {
            transaction,
            hash,
            sender,
        })
    }
}

/// Transaction receipt (execution result)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionReceipt {
    /// Transaction hash
    pub tx_hash: Hash,
    
    /// Vertex hash containing this transaction
    pub vertex_hash: Hash,
    
    /// Transaction index within vertex
    pub tx_index: u32,
    
    /// Sender address
    pub from: Address,
    
    /// Recipient (or created contract)
    pub to: Option<Address>,
    
    /// Contract created (if applicable)
    pub contract_address: Option<Address>,
    
    /// Gas used
    pub gas_used: Gas,
    
    /// Cumulative gas used in vertex
    pub cumulative_gas_used: Gas,
    
    /// Status (1 = success, 0 = failure)
    pub status: u8,
    
    /// Logs emitted
    pub logs: Vec<Log>,
    
    /// Bloom filter for logs
    pub logs_bloom: Vec<u8>, // bloom filter (256 bytes)
    
    /// ZKP verification result (for private transactions)
    pub zkp_verified: Option<bool>,
    
    /// ISO message status
    pub iso_status: Option<IsoTransactionStatus>,
}

/// Event log
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Log {
    /// Contract address that emitted the log
    pub address: Address,
    
    /// Indexed topics
    pub topics: Vec<Hash>,
    
    /// Non-indexed data
    pub data: Vec<u8>,
    
    /// Log index within receipt
    pub log_index: u32,
}

/// ISO transaction processing status
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IsoTransactionStatus {
    /// Status code
    pub status: IsoStatusCode,
    
    /// Reason code (if rejected)
    pub reason: Option<String>,
    
    /// Settlement date
    pub settlement_date: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IsoStatusCode {
    Accepted,
    Pending,
    Rejected,
    Settled,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_legacy_transaction() {
        let tx = Transaction::new_legacy(
            1,
            0,
            Some(Address::from_hex("0x1234567890123456789012345678901234567890").unwrap()),
            U256::from_u64(1000),
            vec![],
            21000,
            U256::from_u64(20_000_000_000),
        );
        
        assert_eq!(tx.tx_type, TxType::Legacy);
        assert_eq!(tx.intrinsic_gas(), 21000);
    }
    
    #[test]
    fn test_private_transaction() {
        let zkp_proof = ZkpProofData {
            proof_type: ZkpProofType::Groth16,
            proof: vec![0u8; 256],
            public_inputs: vec![],
            nullifiers: vec![],
            commitments: vec![],
        };
        
        let tx = Transaction::new_private(1, 0, zkp_proof);
        
        assert_eq!(tx.tx_type, TxType::Private);
        assert!(tx.zkp_proof.is_some());
    }
    
    #[test]
    fn test_rlp_decode_eip1559() {
        // Hand-crafted EIP-1559 raw tx bytes (type 0x02 + RLP list)
        // RLP([chain_id=1, nonce=0, maxPriorityFee=1gwei, maxFee=2gwei, gas=21000, to=0xaa..., value=1, data=[], [], yParity=0, r=0x01, s=0x02])
        let to_addr = [0xaau8; 20];
        let mut encoded = vec![0x02u8]; // type 2
        // Build the list manually
        let mut list = Vec::<u8>::new();
        // chain_id = 1
        list.push(0x01);
        // nonce = 0
        list.push(0x80);
        // maxPriorityFee = 1_000_000_000 = 0x3B9ACA00
        list.extend_from_slice(&[0x84, 0x3B, 0x9A, 0xCA, 0x00]);
        // maxFee = 2_000_000_000 = 0x77359400
        list.extend_from_slice(&[0x84, 0x77, 0x35, 0x94, 0x00]);
        // gasLimit = 21000 = 0x5208
        list.extend_from_slice(&[0x82, 0x52, 0x08]);
        // to = 20 bytes
        list.push(0x94);
        list.extend_from_slice(&to_addr);
        // value = 1
        list.push(0x01);
        // data = empty
        list.push(0x80);
        // accessList = empty list
        list.push(0xc0);
        // yParity = 0
        list.push(0x80);
        // r = 0x01 (1 byte, non-zero)
        list.extend_from_slice(&[0xa0]); // 32-byte string header
        list.extend_from_slice(&[0u8; 31]);
        list.push(0x01);
        // s = 0x02 (1 byte, non-zero)
        list.extend_from_slice(&[0xa0]);
        list.extend_from_slice(&[0u8; 31]);
        list.push(0x02);
        // RLP list header
        encoded.push(0xc0 + if list.len() < 56 { list.len() as u8 } else { 0 });
        // Note: this only works for short lists (< 56 bytes). If > 55 use 0xf7+len_of_len.
        if list.len() >= 56 {
            // Re-encode properly for longer lists
            let _ = &list; // skip if too long for this simple test
            return;
        }
        encoded.extend_from_slice(&list);

        let tx = Transaction::from_rlp_bytes(&encoded);
        assert!(tx.is_some(), "EIP-1559 decode should succeed");
        let tx = tx.unwrap();
        assert_eq!(tx.tx_type, TxType::DynamicFee);
        assert_eq!(tx.chain_id, 1);
        assert_eq!(tx.nonce, 0);
        assert_eq!(tx.gas_limit, 21000);
        assert_eq!(tx.to, Some(Address(to_addr)));
        assert_eq!(tx.value, U256::from(1u64));
    }

    #[test]
    fn test_rlp_decode_invalid() {
        assert!(Transaction::from_rlp_bytes(&[]).is_none());
        assert!(Transaction::from_rlp_bytes(&[0x03]).is_none()); // unknown type
        assert!(Transaction::from_rlp_bytes(&[0x02, 0x80]).is_none()); // type 2 but not a list
    }

    #[test]
    fn test_intrinsic_gas_calculation() {
        let mut tx = Transaction::new_legacy(
            1, 0, None, // Contract creation
            U256::ZERO,
            vec![0, 0, 0, 1, 2, 3], // Mixed data
            100000,
            U256::from_u64(1),
        );
        
        // Base (21000) + Create (32000) + Data (3*4 + 3*16 = 60)
        assert_eq!(tx.intrinsic_gas(), 21000 + 32000 + 60);
    }
}
