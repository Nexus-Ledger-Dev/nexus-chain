//! Gas metering and EIP-1559 fee market

use serde::{Deserialize, Serialize};
use nexus_primitives::U256;

/// Gas costs for various operations
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GasSchedule {
    /// Base transaction cost
    pub tx_base: u64,
    /// Per byte of tx data (zero byte)
    pub tx_data_zero: u64,
    /// Per byte of tx data (non-zero)
    pub tx_data_nonzero: u64,
    /// Contract creation base cost
    pub tx_create: u64,
    /// Access list address cost (EIP-2930)
    pub access_list_address: u64,
    /// Access list storage key cost
    pub access_list_storage_key: u64,
    
    // Opcodes
    pub base: u64,
    pub verylow: u64,
    pub low: u64,
    pub mid: u64,
    pub high: u64,
    pub jumpdest: u64,
    
    // Memory
    pub memory: u64,
    pub copy: u64,
    
    // Storage (EIP-2200)
    pub sload_cold: u64,
    pub sload_warm: u64,
    pub sstore_set: u64,
    pub sstore_reset: u64,
    pub sstore_clear_refund: u64,
    
    // Account access (EIP-2929)
    pub cold_account_access: u64,
    pub warm_account_access: u64,
    pub cold_sload: u64,
    
    // Calls
    pub call: u64,
    pub call_value_transfer: u64,
    pub call_new_account: u64,
    pub selfdestruct: u64,
    
    // Precompiles
    pub ecrecover: u64,
    pub sha256_base: u64,
    pub sha256_word: u64,
    pub ripemd160_base: u64,
    pub ripemd160_word: u64,
    pub identity_base: u64,
    pub identity_word: u64,
    pub modexp_gquad_divisor: u64,
    pub bn_add: u64,
    pub bn_mul: u64,
    pub bn_pairing_base: u64,
    pub bn_pairing_point: u64,
    pub blake2f_round: u64,
    
    // NexusChain extensions
    pub zkp_verify_base: u64,
    pub zkp_verify_per_input: u64,
    pub iso20022_parse_base: u64,
    pub iso20022_parse_per_byte: u64,
}

impl Default for GasSchedule {
    fn default() -> Self {
        Self::berlin()
    }
}

impl GasSchedule {
    /// Berlin/London gas schedule
    pub fn berlin() -> Self {
        Self {
            tx_base: 21000,
            tx_data_zero: 4,
            tx_data_nonzero: 16,
            tx_create: 32000,
            access_list_address: 2400,
            access_list_storage_key: 1900,
            
            base: 2,
            verylow: 3,
            low: 5,
            mid: 8,
            high: 10,
            jumpdest: 1,
            
            memory: 3,
            copy: 3,
            
            sload_cold: 2100,
            sload_warm: 100,
            sstore_set: 20000,
            sstore_reset: 2900,
            sstore_clear_refund: 15000,
            
            cold_account_access: 2600,
            warm_account_access: 100,
            cold_sload: 2100,
            
            call: 100,
            call_value_transfer: 9000,
            call_new_account: 25000,
            selfdestruct: 5000,
            
            ecrecover: 3000,
            sha256_base: 60,
            sha256_word: 12,
            ripemd160_base: 600,
            ripemd160_word: 120,
            identity_base: 15,
            identity_word: 3,
            modexp_gquad_divisor: 3,
            bn_add: 150,
            bn_mul: 6000,
            bn_pairing_base: 45000,
            bn_pairing_point: 34000,
            blake2f_round: 1,
            
            // Custom precompiles (set to reasonable defaults)
            zkp_verify_base: 100000,      // ZKP verification is expensive
            zkp_verify_per_input: 5000,
            iso20022_parse_base: 5000,
            iso20022_parse_per_byte: 10,
        }
    }
    
    /// Calculate intrinsic gas for transaction
    pub fn intrinsic_gas(&self, data: &[u8], is_create: bool, access_list_len: usize) -> u64 {
        let mut gas = self.tx_base;
        
        if is_create {
            gas += self.tx_create;
        }
        
        // Data cost
        for byte in data {
            if *byte == 0 {
                gas += self.tx_data_zero;
            } else {
                gas += self.tx_data_nonzero;
            }
        }
        
        // Access list cost
        gas += access_list_len as u64 * self.access_list_address;
        
        gas
    }
    
    /// Calculate memory expansion cost
    pub fn memory_cost(&self, size: u64) -> u64 {
        let words = (size + 31) / 32;
        words * self.memory + (words * words) / 512
    }
    
    /// Calculate memory expansion delta
    pub fn memory_expansion_cost(&self, current_size: u64, new_size: u64) -> u64 {
        if new_size <= current_size {
            return 0;
        }
        self.memory_cost(new_size) - self.memory_cost(current_size)
    }
}

/// EIP-1559 fee market configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeeMarket {
    /// Base fee per gas (updated per block)
    pub base_fee_per_gas: U256,
    /// Maximum base fee increase factor per block
    pub max_change_denominator: u64,
    /// Target gas usage per block (50% of limit)
    pub target_gas_per_block: u64,
    /// Block gas limit
    pub gas_limit: u64,
    /// Minimum base fee
    pub min_base_fee: U256,
    /// Maximum base fee (optional cap)
    pub max_base_fee: Option<U256>,
}

impl Default for FeeMarket {
    fn default() -> Self {
        Self {
            base_fee_per_gas: U256::from(1_000_000_000u64), // 1 gwei
            max_change_denominator: 8,
            target_gas_per_block: 15_000_000,
            gas_limit: 30_000_000,
            min_base_fee: U256::from(1_000_000u64), // 0.001 gwei
            max_base_fee: None,
        }
    }
}

impl FeeMarket {
    /// Calculate next block's base fee based on gas used
    pub fn next_base_fee(&self, gas_used: u64) -> U256 {
        let target = self.target_gas_per_block;
        
        if gas_used == target {
            return self.base_fee_per_gas;
        }
        
        let delta = if gas_used > target {
            // Increase base fee
            let gas_delta = gas_used - target;
            let fee_delta = self.base_fee_per_gas * U256::from(gas_delta) 
                / U256::from(target)
                / U256::from(self.max_change_denominator);
            self.base_fee_per_gas + fee_delta.max(U256::from(1))
        } else {
            // Decrease base fee
            let gas_delta = target - gas_used;
            let fee_delta = self.base_fee_per_gas * U256::from(gas_delta)
                / U256::from(target)
                / U256::from(self.max_change_denominator);
            self.base_fee_per_gas.checked_sub(&fee_delta).unwrap_or(U256::ZERO)
        };
        
        // Apply bounds
        let mut result = delta.max(self.min_base_fee);
        if let Some(max) = &self.max_base_fee {
            result = result.min(*max);
        }
        result
    }
    
    /// Calculate effective gas price for EIP-1559 transaction
    pub fn effective_gas_price(&self, max_fee_per_gas: U256, max_priority_fee: U256) -> U256 {
        let priority_fee = max_priority_fee.min(max_fee_per_gas - self.base_fee_per_gas);
        self.base_fee_per_gas + priority_fee
    }
    
    /// Validate transaction fee parameters
    pub fn validate_fee(&self, max_fee_per_gas: U256, max_priority_fee: U256) -> bool {
        max_fee_per_gas >= self.base_fee_per_gas && max_priority_fee <= max_fee_per_gas
    }
}

/// Gas meter for transaction execution
#[derive(Clone, Debug)]
pub struct GasMeter {
    gas_limit: u64,
    gas_used: u64,
    gas_refunded: u64,
    schedule: GasSchedule,
}

impl GasMeter {
    pub fn new(gas_limit: u64, schedule: GasSchedule) -> Self {
        Self {
            gas_limit,
            gas_used: 0,
            gas_refunded: 0,
            schedule,
        }
    }
    
    /// Consume gas, return error if out of gas
    pub fn consume(&mut self, amount: u64) -> Result<(), ()> {
        let new_used = self.gas_used.saturating_add(amount);
        if new_used > self.gas_limit {
            return Err(());
        }
        self.gas_used = new_used;
        Ok(())
    }
    
    /// Add gas refund
    pub fn refund(&mut self, amount: u64) {
        self.gas_refunded = self.gas_refunded.saturating_add(amount);
    }
    
    /// Get remaining gas
    pub fn remaining(&self) -> u64 {
        self.gas_limit.saturating_sub(self.gas_used)
    }
    
    /// Get gas used
    pub fn used(&self) -> u64 {
        self.gas_used
    }
    
    /// Get gas refunded (capped at 50% of used, per EIP-3529)
    pub fn refunded(&self) -> u64 {
        let max_refund = self.gas_used / 5; // 20% cap post-London
        self.gas_refunded.min(max_refund)
    }
    
    /// Get final gas used after refunds
    pub fn final_used(&self) -> u64 {
        self.gas_used.saturating_sub(self.refunded())
    }
    
    /// Get gas schedule
    pub fn schedule(&self) -> &GasSchedule {
        &self.schedule
    }
    
    /// Record gas for SSTORE operation (EIP-2200)
    pub fn sstore_gas(
        &self,
        original: U256,
        current: U256,
        new: U256,
        is_cold: bool,
    ) -> (u64, i64) {
        let mut gas = if is_cold { self.schedule.cold_sload } else { 0 };
        let mut refund = 0i64;
        
        if current == new {
            // No-op
            gas += self.schedule.sload_warm;
        } else if original == current {
            if original == U256::ZERO {
                // 0 -> non-zero
                gas += self.schedule.sstore_set;
            } else if new == U256::ZERO {
                // non-zero -> 0
                gas += self.schedule.sstore_reset;
                refund = self.schedule.sstore_clear_refund as i64;
            } else {
                // non-zero -> non-zero
                gas += self.schedule.sstore_reset;
            }
        } else {
            // Dirty value
            gas += self.schedule.sload_warm;
            
            if original != U256::ZERO {
                if current == U256::ZERO {
                    // Restoring a cleared slot
                    refund -= self.schedule.sstore_clear_refund as i64;
                } else if new == U256::ZERO {
                    // Clearing a restored slot
                    refund += self.schedule.sstore_clear_refund as i64;
                }
            }
            
            if original == new {
                // Restoring to original
                if original == U256::ZERO {
                    refund += (self.schedule.sstore_set - self.schedule.sload_warm) as i64;
                } else {
                    refund += (self.schedule.sstore_reset - self.schedule.sload_warm) as i64;
                }
            }
        }
        
        (gas, refund)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_intrinsic_gas() {
        let schedule = GasSchedule::default();
        
        // Simple transfer
        assert_eq!(schedule.intrinsic_gas(&[], false, 0), 21000);
        
        // Contract creation
        assert_eq!(schedule.intrinsic_gas(&[], true, 0), 21000 + 32000);
        
        // With data
        let data = vec![0, 1, 2, 0, 3]; // 2 zeros, 3 non-zeros
        let expected = 21000 + 2 * 4 + 3 * 16;
        assert_eq!(schedule.intrinsic_gas(&data, false, 0), expected);
    }
    
    #[test]
    fn test_fee_market() {
        let market = FeeMarket::default();
        
        // At target, base fee stays same
        let next = market.next_base_fee(market.target_gas_per_block);
        assert_eq!(next, market.base_fee_per_gas);
        
        // Above target, base fee increases
        let next = market.next_base_fee(market.gas_limit);
        assert!(next > market.base_fee_per_gas);
        
        // Below target, base fee decreases
        let next = market.next_base_fee(0);
        assert!(next < market.base_fee_per_gas);
    }
    
    #[test]
    fn test_gas_meter() {
        let schedule = GasSchedule::default();
        let mut meter = GasMeter::new(100000, schedule);
        
        assert!(meter.consume(50000).is_ok());
        assert_eq!(meter.remaining(), 50000);
        
        assert!(meter.consume(60000).is_err()); // Out of gas
        
        meter.refund(10000);
        assert_eq!(meter.final_used(), 50000 - 10000);
    }
}
