//! EVM Executor
//! 
//! Main execution engine integrating REVM with NexusChain extensions.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use sha3::{Digest, Keccak256};
use tracing::{debug, trace, warn};

use nexus_primitives::{Address, Hash, Transaction, TransactionType, U256};
use crate::{
    EvmError, EvmResult,
    state::{StateDb, JournaledState, Account},
    gas::{GasSchedule, FeeMarket, GasMeter},
    precompiles::{get_precompiles, is_precompile, PrecompileFn},
    receipt::{Receipt, ExecutionResult, Log, ExecutionStatus},
};

/// EVM execution environment
pub struct EvmEnv {
    /// Chain ID
    pub chain_id: u64,
    /// Current "block" number (vertex height for DAG)
    pub number: u64,
    /// Timestamp
    pub timestamp: u64,
    /// Coinbase (validator address)
    pub coinbase: Address,
    /// Gas limit
    pub gas_limit: u64,
    /// Base fee per gas (EIP-1559)
    pub base_fee_per_gas: U256,
    /// Previous block/vertex hash
    pub prev_hash: Hash,
    /// Difficulty (for DIFFICULTY opcode, 0 post-merge)
    pub difficulty: U256,
    /// Random value (for PREVRANDAO post-merge)
    pub prevrandao: Hash,
}

impl Default for EvmEnv {
    fn default() -> Self {
        Self {
            chain_id: 1337, // NexusChain testnet
            number: 0,
            timestamp: 0,
            coinbase: Address::ZERO,
            gas_limit: 30_000_000,
            base_fee_per_gas: U256::from(1_000_000_000u64), // 1 gwei
            prev_hash: Hash::ZERO,
            difficulty: U256::ZERO,
            prevrandao: Hash::ZERO,
        }
    }
}

/// EVM Executor
pub struct Executor {
    /// State database
    state: Arc<StateDb>,
    /// Environment
    env: EvmEnv,
    /// Gas schedule
    gas_schedule: GasSchedule,
    /// Fee market configuration
    fee_market: FeeMarket,
    /// Precompiled contracts
    precompiles: HashMap<Address, PrecompileFn>,
    /// Access list (EIP-2929)
    accessed_addresses: HashSet<Address>,
    accessed_storage: HashSet<(Address, U256)>,
}

impl Executor {
    /// Create new executor
    pub fn new(state: Arc<StateDb>, env: EvmEnv) -> Self {
        Self {
            state,
            env,
            gas_schedule: GasSchedule::default(),
            fee_market: FeeMarket::default(),
            precompiles: get_precompiles(),
            accessed_addresses: HashSet::new(),
            accessed_storage: HashSet::new(),
        }
    }
    
    /// Execute a transaction
    pub fn execute_tx(&mut self, tx: &Transaction) -> EvmResult<Receipt> {
        debug!("Executing transaction: {:?}", tx.hash());
        
        // Validate transaction
        self.validate_tx(tx)?;
        
        // Get sender
        let sender = tx.recover_sender()
            .ok_or(EvmError::InvalidSignature)?;
        
        // Initialize access list with sender and recipient
        self.accessed_addresses.clear();
        self.accessed_storage.clear();
        self.accessed_addresses.insert(sender.clone());
        if let Some(to) = &tx.to {
            self.accessed_addresses.insert(to.clone());
        }
        
        // Calculate gas costs
        let is_create = tx.to.is_none();
        let intrinsic_gas = self.gas_schedule.intrinsic_gas(&tx.data, is_create, 0);
        
        if tx.gas_limit < intrinsic_gas {
            return Err(EvmError::OutOfGas { 
                used: intrinsic_gas, 
                limit: tx.gas_limit 
            });
        }
        
        // Calculate effective gas price
        let effective_gas_price = self.calculate_gas_price(tx)?;
        let max_fee = effective_gas_price * U256::from(tx.gas_limit);
        
        // Check sender has enough balance
        let sender_balance = self.state.get_balance(&sender);
        let total_cost = max_fee + tx.value;
        if sender_balance < total_cost {
            return Err(EvmError::InsufficientBalance {
                required: format!("{:?}", total_cost),
                available: format!("{:?}", sender_balance),
            });
        }
        
        // Check and increment nonce
        let expected_nonce = self.state.get_nonce(&sender);
        if tx.nonce != expected_nonce {
            return Err(EvmError::NonceMismatch { 
                expected: expected_nonce, 
                got: tx.nonce 
            });
        }
        
        // Create journaled state for potential rollback
        let mut journal = JournaledState::new(self.state.clone());
        
        // Deduct max gas fee upfront
        journal.set_balance(&sender, sender_balance - max_fee);
        self.state.increment_nonce(&sender);
        
        // Create checkpoint
        let checkpoint = journal.checkpoint();
        
        // Execute
        let result = if is_create {
            self.execute_create(&mut journal, &sender, tx)
        } else {
            let to = tx.to.as_ref().unwrap();
            self.execute_call(&mut journal, &sender, to, tx.value, &tx.data, tx.gas_limit - intrinsic_gas)
        };
        
        // Handle result
        let (receipt, gas_used) = match result {
            Ok(exec_result) => {
                if exec_result.success {
                    journal.commit_checkpoint(checkpoint);
                    
                    let gas_used = intrinsic_gas + exec_result.gas_used - exec_result.gas_refunded;
                    
                    let mut receipt = Receipt::success(
                        tx.hash(),
                        self.env.prev_hash.clone(),
                        gas_used,
                        effective_gas_price,
                        exec_result.logs,
                    );
                    receipt.contract_address = exec_result.created_address;
                    
                    (receipt, gas_used)
                } else {
                    journal.revert_checkpoint(checkpoint);
                    
                    let reason = exec_result.revert_reason().unwrap_or_default();
                    let receipt = Receipt::revert(
                        tx.hash(),
                        self.env.prev_hash.clone(),
                        intrinsic_gas + exec_result.gas_used,
                        effective_gas_price,
                        reason,
                    );
                    
                    (receipt, intrinsic_gas + exec_result.gas_used)
                }
            }
            Err(e) => {
                journal.revert_checkpoint(checkpoint);
                
                let receipt = Receipt::failure(
                    tx.hash(),
                    self.env.prev_hash.clone(),
                    tx.gas_limit, // Consume all gas on failure
                    effective_gas_price,
                );
                
                warn!("Transaction execution failed: {:?}", e);
                (receipt, tx.gas_limit)
            }
        };
        
        // Refund unused gas
        let gas_refund = tx.gas_limit - gas_used;
        let refund_amount = effective_gas_price * U256::from(gas_refund);
        self.state.add_balance(&sender, refund_amount)?;
        
        // Pay coinbase
        let fee = effective_gas_price * U256::from(gas_used);
        let priority_fee = fee - (self.env.base_fee_per_gas * U256::from(gas_used));
        self.state.add_balance(&self.env.coinbase, priority_fee)?;
        
        // Burn base fee (EIP-1559)
        // In production, this would be tracked separately
        
        Ok(receipt)
    }
    
    /// Validate transaction
    fn validate_tx(&self, tx: &Transaction) -> EvmResult<()> {
        // Check gas limit
        if tx.gas_limit > self.env.gas_limit {
            return Err(EvmError::OutOfGas { 
                used: tx.gas_limit, 
                limit: self.env.gas_limit 
            });
        }
        
        // Check chain ID
        if tx.chain_id != self.env.chain_id {
            return Err(EvmError::StateError(format!(
                "Chain ID mismatch: expected {}, got {}",
                self.env.chain_id, tx.chain_id
            )));
        }
        
        Ok(())
    }
    
    /// Calculate effective gas price
    fn calculate_gas_price(&self, tx: &Transaction) -> EvmResult<U256> {
        match tx.tx_type {
            TransactionType::Legacy => Ok(tx.gas_price),
            TransactionType::Eip2930 => Ok(tx.gas_price),
            TransactionType::Eip1559 => {
                if tx.max_fee_per_gas < self.env.base_fee_per_gas {
                    return Err(EvmError::StateError(
                        "Max fee per gas below base fee".into()
                    ));
                }
                
                let priority_fee = tx.max_priority_fee_per_gas
                    .min(tx.max_fee_per_gas - self.env.base_fee_per_gas);
                
                Ok(self.env.base_fee_per_gas + priority_fee)
            }
            _ => Ok(tx.gas_price),
        }
    }
    
    /// Execute contract creation
    fn execute_create(
        &mut self,
        journal: &mut JournaledState,
        sender: &Address,
        tx: &Transaction,
    ) -> EvmResult<ExecutionResult> {
        // Calculate contract address
        let nonce = self.state.get_nonce(sender) - 1; // Nonce was already incremented
        let contract_addr = self.calculate_create_address(sender, nonce);
        
        debug!("Creating contract at {:?}", contract_addr);
        
        // Check contract doesn't exist
        let existing = self.state.get_account(&contract_addr);
        if !existing.is_empty() {
            return Err(EvmError::CreateFailed(
                "Contract address already exists".into()
            ));
        }
        
        // Initialize contract account
        self.state.set_account(contract_addr.clone(), Account {
            nonce: 1, // EIP-161
            balance: tx.value,
            code_hash: Hash::ZERO,
            storage_root: Hash::ZERO,
        });
        
        // Transfer value
        if tx.value > U256::ZERO {
            self.state.sub_balance(sender, tx.value)?;
            self.state.add_balance(&contract_addr, tx.value)?;
        }
        
        // Execute init code
        let gas_available = tx.gas_limit - self.gas_schedule.tx_create;
        let result = self.execute_code(
            journal,
            sender,
            &contract_addr,
            &tx.data,
            gas_available,
        )?;
        
        if result.success {
            // Store returned code
            let code = &result.output;
            
            // Check code size (EIP-170: max 24KB)
            if code.len() > 24576 {
                return Err(EvmError::CreateFailed(
                    "Contract code size exceeds limit".into()
                ));
            }
            
            // Check code doesn't start with 0xEF (EIP-3541)
            if code.first() == Some(&0xEF) {
                return Err(EvmError::CreateFailed(
                    "Contract code starts with 0xEF".into()
                ));
            }
            
            // Store code
            self.state.set_code(&contract_addr, code.clone());
            
            // Gas cost for code storage
            let code_deposit_cost = code.len() as u64 * 200;
            
            Ok(ExecutionResult {
                success: true,
                gas_used: result.gas_used + code_deposit_cost,
                gas_refunded: result.gas_refunded,
                output: vec![],
                logs: result.logs,
                created_address: Some(contract_addr),
            })
        } else {
            Ok(result)
        }
    }
    
    /// Execute contract call
    fn execute_call(
        &mut self,
        journal: &mut JournaledState,
        sender: &Address,
        to: &Address,
        value: U256,
        data: &[u8],
        gas: u64,
    ) -> EvmResult<ExecutionResult> {
        // Check if precompile
        if let Some(precompile) = self.precompiles.get(to) {
            trace!("Calling precompile at {:?}", to);
            let result = precompile(data, gas, &self.gas_schedule)?;
            return Ok(ExecutionResult::success(
                result.gas_used,
                0,
                result.output,
                vec![],
            ));
        }
        
        // Transfer value
        if value > U256::ZERO {
            self.state.sub_balance(sender, value)?;
            self.state.add_balance(to, value)?;
        }
        
        // Get contract code
        let code = self.state.get_code(to);
        
        if code.is_empty() {
            // EOA or empty contract
            return Ok(ExecutionResult::success(0, 0, vec![], vec![]));
        }
        
        // Execute code
        self.execute_code(journal, sender, to, &code, gas)
    }
    
    /// Execute EVM bytecode
    fn execute_code(
        &mut self,
        _journal: &mut JournaledState,
        _caller: &Address,
        _address: &Address,
        code: &[u8],
        gas_limit: u64,
    ) -> EvmResult<ExecutionResult> {
        // In production, this would use REVM for actual execution
        // This is a simplified placeholder
        
        let mut gas_meter = GasMeter::new(gas_limit, self.gas_schedule.clone());
        let mut logs = Vec::new();
        let mut output = Vec::new();
        let mut success = true;
        
        // Basic bytecode interpretation would go here
        // For now, we just consume some gas based on code size
        let base_cost = code.len() as u64 * 3;
        if gas_meter.consume(base_cost).is_err() {
            return Ok(ExecutionResult::revert(gas_limit, vec![]));
        }
        
        // Simulate some execution
        // In reality, this would be a full EVM interpreter loop
        
        Ok(ExecutionResult {
            success,
            gas_used: gas_meter.used(),
            gas_refunded: gas_meter.refunded(),
            output,
            logs,
            created_address: None,
        })
    }
    
    /// Calculate CREATE address
    fn calculate_create_address(&self, sender: &Address, nonce: u64) -> Address {
        let mut rlp = Vec::new();
        
        // RLP encode [sender, nonce]
        let sender_bytes = sender.as_bytes();
        
        // Total length
        let nonce_bytes = if nonce == 0 {
            vec![0x80] // RLP empty
        } else if nonce < 128 {
            vec![nonce as u8]
        } else {
            // Multi-byte nonce
            let bytes: Vec<u8> = nonce.to_be_bytes().into_iter()
                .skip_while(|&b| b == 0)
                .collect();
            let mut result = vec![0x80 + bytes.len() as u8];
            result.extend(bytes);
            result
        };
        
        let list_len = 21 + nonce_bytes.len(); // 20 bytes address + 1 byte prefix + nonce
        if list_len < 56 {
            rlp.push(0xc0 + list_len as u8);
        } else {
            // Would need length encoding for larger lists
            rlp.push(0xf7 + 1);
            rlp.push(list_len as u8);
        }
        
        rlp.push(0x80 + 20); // Address prefix
        rlp.extend_from_slice(sender_bytes);
        rlp.extend(nonce_bytes);
        
        let hash = Keccak256::digest(&rlp);
        Address::from_slice(&hash[12..])
    }
    
    /// Calculate CREATE2 address
    pub fn calculate_create2_address(
        sender: &Address,
        salt: &[u8; 32],
        init_code_hash: &[u8; 32],
    ) -> Address {
        let mut data = Vec::with_capacity(85);
        data.push(0xff);
        data.extend_from_slice(sender.as_bytes());
        data.extend_from_slice(salt);
        data.extend_from_slice(init_code_hash);
        
        let hash = Keccak256::digest(&data);
        Address::from_slice(&hash[12..])
    }
    
    /// Execute batch of transactions (for parallel DAG processing)
    pub fn execute_batch(&mut self, txs: Vec<Transaction>) -> Vec<EvmResult<Receipt>> {
        // In a DAG, we can potentially execute independent transactions in parallel
        // For now, execute sequentially
        txs.iter().map(|tx| self.execute_tx(tx)).collect()
    }
    
    /// Get access list gas cost
    fn access_address(&mut self, address: &Address) -> u64 {
        if self.accessed_addresses.insert(address.clone()) {
            self.gas_schedule.cold_account_access
        } else {
            self.gas_schedule.warm_account_access
        }
    }
    
    fn access_storage(&mut self, address: &Address, key: &U256) -> u64 {
        if self.accessed_storage.insert((address.clone(), key.clone())) {
            self.gas_schedule.cold_sload
        } else {
            self.gas_schedule.sload_warm
        }
    }
    
    /// Get current environment
    pub fn env(&self) -> &EvmEnv {
        &self.env
    }
    
    /// Update environment for new block/vertex
    pub fn set_env(&mut self, env: EvmEnv) {
        self.env = env;
    }
    
    /// Get state reference
    pub fn state(&self) -> &Arc<StateDb> {
        &self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_address() {
        let state = Arc::new(StateDb::new());
        let executor = Executor::new(state, EvmEnv::default());
        
        let sender = Address::from([0x42u8; 20]);
        let addr = executor.calculate_create_address(&sender, 0);
        
        // Address should be 20 bytes
        assert_eq!(addr.as_bytes().len(), 20);
    }
    
    #[test]
    fn test_create2_address() {
        let sender = Address::from([0x42u8; 20]);
        let salt = [0u8; 32];
        let init_code_hash = Keccak256::digest(b"init code").into();
        
        let addr = Executor::calculate_create2_address(&sender, &salt, &init_code_hash);
        
        assert_eq!(addr.as_bytes().len(), 20);
    }
}
