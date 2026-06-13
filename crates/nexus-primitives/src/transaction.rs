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
    pub logs_bloom: [u8; 256],
    
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
