//! RPC method dispatcher

use std::sync::Arc;
use serde_json::Value;
use tracing::debug;

use nexus_dag::Dag;
use nexus_evm::StateDb;
use nexus_consensus::ProofOfStake;

use crate::{RpcResult, RpcError, EthHandler, NexusHandler};

/// RPC method dispatcher
pub struct MethodDispatcher {
    eth: EthHandler,
    nexus: NexusHandler,
}

impl MethodDispatcher {
    pub fn new(
        dag: Arc<Dag>,
        state: Arc<StateDb>,
        pos: Arc<ProofOfStake>,
        zkp: Arc<Box<dyn nexus_zkp::Verifier + Send + Sync>>,
        chain_id: u64,
    ) -> Self {
        Self {
            eth: EthHandler::new(dag.clone(), state.clone(), pos.clone(), chain_id),
            nexus: NexusHandler::new(dag, state, pos, zkp),
        }
    }
    
    /// Dispatch method call
    pub fn dispatch(&self, method: &str, params: &Value) -> RpcResult<Value> {
        debug!("RPC call: {} params: {:?}", method, params);
        
        match method {
            // Ethereum methods
            "eth_chainId" => self.eth.chain_id(),
            "eth_blockNumber" => self.eth.block_number(),
            "eth_gasPrice" => self.eth.gas_price(),
            "eth_getBalance" => self.eth.get_balance(params),
            "eth_getTransactionCount" => self.eth.get_transaction_count(params),
            "eth_getCode" => self.eth.get_code(params),
            "eth_getStorageAt" => self.eth.get_storage_at(params),
            "eth_sendRawTransaction" => self.eth.send_raw_transaction(params),
            "eth_call" => self.eth.call(params),
            "eth_estimateGas" => self.eth.estimate_gas(params),
            "eth_getBlockByNumber" => self.eth.get_block_by_number(params),
            "eth_getBlockByHash" => self.eth.get_block_by_hash(params),
            "eth_syncing" => self.eth.syncing(),
            "eth_coinbase" => self.eth.coinbase(),
            "eth_mining" => self.eth.mining(),
            "eth_hashrate" => self.eth.hashrate(),
            "eth_accounts" => self.eth.accounts(),
            
            // Network methods
            "net_version" => self.eth.net_version(),
            "net_listening" => self.eth.net_listening(),
            "net_peerCount" => self.eth.net_peer_count(),
            
            // Web3 methods
            "web3_clientVersion" => self.eth.client_version(),
            "web3_sha3" => self.eth.sha3(params),
            
            // NexusChain extensions
            "nexus_dagInfo" => self.nexus.dag_info(),
            "nexus_getVertex" => self.nexus.get_vertex(params),
            "nexus_getTips" => self.nexus.get_tips(),
            "nexus_getParents" => self.nexus.get_parents(params),
            "nexus_getChildren" => self.nexus.get_children(params),
            "nexus_isFinalized" => self.nexus.is_finalized(params),
            "nexus_validators" => self.nexus.validators(),
            "nexus_epoch" => self.nexus.epoch(),
            "nexus_checkCompliance" => self.nexus.check_compliance(params),
            "nexus_validateIban" => self.nexus.validate_iban(params),
            "nexus_validateBic" => self.nexus.validate_bic(params),
            "nexus_zkpVerify" => self.nexus.zkp_verify(params),
            "nexus_getProof" => self.nexus.get_proof(params),
            
            _ => Err(RpcError::MethodNotFound(method.to_string())),
        }
    }
}

pub use crate::eth::EthHandler;
pub use crate::nexus::NexusHandler;
