//! REVM database adapter — bridges nexus StateDb to revm's Database trait.

use std::collections::HashMap;
use std::sync::Arc;

use revm::{
    Database,
    primitives::{
        AccountInfo, Bytecode, B256, U256 as RevmU256,
        Address as RevmAddress, Account as RevmAccount, KECCAK_EMPTY,
        Bytes as RevmBytes,
    },
};

use nexus_primitives::{Address, Hash, U256};
use crate::{EvmError, state::StateDb};

// ───────────────────────── type conversions ─────────────────────────

pub fn to_revm_addr(addr: &Address) -> RevmAddress {
    RevmAddress::from_slice(addr.as_bytes())
}

pub fn from_revm_addr(addr: RevmAddress) -> Address {
    Address::from_slice(addr.as_slice()).unwrap_or(Address::ZERO)
}

/// nexus U256 (`[u64; 4]` little-endian) ↔ revm U256 (`ruint::Uint<256,4>` little-endian).
/// The limb order is identical, so this is a direct bit-copy.
pub fn to_revm_u256(v: U256) -> RevmU256 {
    RevmU256::from_limbs(v.0)
}

pub fn from_revm_u256(v: RevmU256) -> U256 {
    U256(v.into_limbs())
}

pub fn to_revm_b256(h: &Hash) -> B256 {
    B256::from_slice(h.as_bytes())
}

pub fn from_revm_b256(b: B256) -> Hash {
    Hash::new(b.0)
}

// ─────────────────────── NexusDatabase (read-only) ──────────────────

/// Wraps `StateDb` to satisfy revm's `Database` trait.
/// All mutations happen through `apply_revm_state` after `transact()` returns.
pub struct NexusDatabase {
    pub state: Arc<StateDb>,
}

impl NexusDatabase {
    pub fn new(state: Arc<StateDb>) -> Self {
        Self { state }
    }
}

impl Database for NexusDatabase {
    type Error = EvmError;

    fn basic(&mut self, address: RevmAddress) -> Result<Option<AccountInfo>, Self::Error> {
        let addr = from_revm_addr(address);
        let account = self.state.get_account(&addr);
        if account.is_empty() {
            return Ok(None);
        }
        let (code, code_hash) = if account.code_hash == Hash::ZERO {
            // EOA / no-code account: use canonical empty-code hash.
            (None, KECCAK_EMPTY)
        } else {
            let code_bytes = self.state.get_code(&addr);
            let hash = to_revm_b256(&account.code_hash);
            let bytecode = Bytecode::new_raw(RevmBytes::from(code_bytes));
            (Some(bytecode), hash)
        };
        Ok(Some(AccountInfo {
            balance: to_revm_u256(account.balance),
            nonce: account.nonce,
            code_hash,
            code,
        }))
    }

    fn code_by_hash(&mut self, _code_hash: B256) -> Result<Bytecode, Self::Error> {
        // Code is embedded in basic(); this path is a safe fallback for cold-code scenarios.
        Ok(Bytecode::new_raw(RevmBytes::new()))
    }

    fn storage(&mut self, address: RevmAddress, index: RevmU256) -> Result<RevmU256, Self::Error> {
        let addr = from_revm_addr(address);
        let key = from_revm_u256(index);
        Ok(to_revm_u256(self.state.get_storage(&addr, &key)))
    }

    fn block_hash(&mut self, _number: RevmU256) -> Result<B256, Self::Error> {
        // The DAG doesn't expose sequential block hashes; return zero.
        Ok(B256::ZERO)
    }
}

// ─────────────────── post-execution state application ───────────────

/// Write the state diff produced by `evm.transact()` back into `StateDb`.
///
/// This covers:
/// - balance and nonce changes (gas transfers, value transfers, nested CALLs)
/// - storage slot mutations (SSTORE)
/// - new contract code (CREATE / CREATE2)
/// - account deletion (SELFDESTRUCT)
pub fn apply_revm_state(state: &StateDb, changes: HashMap<RevmAddress, RevmAccount>) {
    for (revm_addr, account) in changes {
        // Only process accounts that REVM actually touched.
        if !account.is_touched() {
            continue;
        }

        let addr = from_revm_addr(revm_addr);

        if account.is_selfdestructed() {
            state.delete_account(&addr);
            continue;
        }

        // Sync balance and nonce.
        {
            let mut nexus_acc = state.get_account(&addr);
            nexus_acc.balance = from_revm_u256(account.info.balance);
            nexus_acc.nonce = account.info.nonce;
            state.set_account(addr.clone(), nexus_acc);
        }

        // Sync code — only when REVM produced new code (CREATE path).
        if let Some(ref code) = account.info.code {
            let code_bytes = code.bytes_slice().to_vec();
            if !code_bytes.is_empty() {
                let current_hash = state.get_account(&addr).code_hash;
                let new_hash = from_revm_b256(account.info.code_hash);
                if new_hash != current_hash {
                    state.set_code(&addr, code_bytes);
                }
            }
        }

        // Sync changed storage slots.
        for (slot, slot_val) in &account.storage {
            if slot_val.is_changed() {
                state.set_storage(
                    &addr,
                    from_revm_u256(*slot),
                    from_revm_u256(slot_val.present_value()),
                );
            }
        }
    }
}
