//! NexusChain-specific RPC methods

use std::sync::Arc;
use serde_json::{json, Value};

use nexus_dag::Dag;
use nexus_evm::StateDb;
use nexus_consensus::ProofOfStake;
use nexus_iso::{ComplianceChecker, MessageContext};

use crate::{RpcResult, RpcError, types::*};

/// Nexus namespace handler (NexusChain extensions)
pub struct NexusHandler {
    dag: Arc<Dag>,
    state: Arc<StateDb>,
    pos: Arc<ProofOfStake>,
    compliance: ComplianceChecker,
    zkp: Arc<Box<dyn nexus_zkp::Verifier + Send + Sync>>,
}

impl NexusHandler {
    pub fn new(
        dag: Arc<Dag>,
        state: Arc<StateDb>,
        pos: Arc<ProofOfStake>,
        zkp: Arc<Box<dyn nexus_zkp::Verifier + Send + Sync>>,
    ) -> Self {
        Self {
            dag,
            state,
            pos,
            compliance: ComplianceChecker::default(),
            zkp,
        }
    }
    
    /// nexus_dagInfo - Get DAG statistics
    pub fn dag_info(&self) -> RpcResult<Value> {
        let stats = self.dag.stats();

        Ok(json!({
            "totalVertices": stats.total_vertices,
            "currentTips": stats.tips_count,
            "pendingVertices": stats.pending_count,
            "latestHeight": stats.latest_height,
            "finalizedHeight": stats.finalized_height,
        }))
    }
    
    /// nexus_getVertex - Get vertex by hash
    pub fn get_vertex(&self, params: &Value) -> RpcResult<Value> {
        let hash = params.get(0)
            .and_then(|v| v.as_str())
            .and_then(parse_hash)
            .ok_or_else(|| RpcError::InvalidParams("Invalid hash".into()))?;
        
        let vertex = self.dag.get_vertex(&hash)
            .ok_or_else(|| RpcError::NotFound("Vertex not found".into()))?;
        
        let status = self.dag.get_status(&hash);
        Ok(json!({
            "hash": format_hash(&vertex.hash),
            "height": vertex.height,
            "validator": format!("0x{}", hex::encode(&vertex.validator.0)),
            "parents": vertex.parents.iter().map(format_hash).collect::<Vec<_>>(),
            "transactions": vertex.transactions.iter().map(|tx| format_hash(&tx.hash())).collect::<Vec<_>>(),
            "timestamp": vertex.timestamp,
            "status": format!("{:?}", status),
        }))
    }
    
    /// nexus_getTips - Get current DAG tips
    pub fn get_tips(&self) -> RpcResult<Value> {
        let tips = self.dag.tips();
        Ok(json!(tips.iter().map(format_hash).collect::<Vec<_>>()))
    }
    
    /// nexus_getParents - Get vertex parents
    pub fn get_parents(&self, params: &Value) -> RpcResult<Value> {
        let hash = params.get(0)
            .and_then(|v| v.as_str())
            .and_then(parse_hash)
            .ok_or_else(|| RpcError::InvalidParams("Invalid hash".into()))?;
        
        let vertex = self.dag.get_vertex(&hash)
            .ok_or_else(|| RpcError::NotFound("Vertex not found".into()))?;
        Ok(json!(vertex.parents.iter().map(format_hash).collect::<Vec<_>>()))
    }
    
    /// nexus_getChildren - Get vertex children
    pub fn get_children(&self, params: &Value) -> RpcResult<Value> {
        let hash = params.get(0)
            .and_then(|v| v.as_str())
            .and_then(parse_hash)
            .ok_or_else(|| RpcError::InvalidParams("Invalid hash".into()))?;
        
        let children = self.dag.get_children(&hash);
        Ok(json!(children.iter().map(format_hash).collect::<Vec<_>>()))
    }
    
    /// nexus_isFinalized - Check if vertex is finalized
    pub fn is_finalized(&self, params: &Value) -> RpcResult<Value> {
        let hash = params.get(0)
            .and_then(|v| v.as_str())
            .and_then(parse_hash)
            .ok_or_else(|| RpcError::InvalidParams("Invalid hash".into()))?;
        
        let _vertex = self.dag.get_vertex(&hash)
            .ok_or_else(|| RpcError::NotFound("Vertex not found".into()))?;

        Ok(json!(self.dag.get_status(&hash) == Some(nexus_dag::VertexStatus::Finalized)))
    }
    
    /// nexus_validators - Get validator set
    pub fn validators(&self) -> RpcResult<Value> {
        let validators = self.pos.validator_set();
        
        let list: Vec<_> = validators.iter().map(|v| {
            json!({
                "address": format_address(&v.address),
                "stake": v.stake.to_string(),
                "commission": v.commission,
                "active": v.active,
                "uptime": v.uptime,
            })
        }).collect();
        
        Ok(json!({
            "epoch": validators.epoch,
            "totalStake": validators.total_stake.to_string(),
            "validators": list,
        }))
    }
    
    /// nexus_epoch - Get current epoch
    pub fn epoch(&self) -> RpcResult<Value> {
        Ok(json!(self.pos.epoch()))
    }
    
    /// nexus_compliance - Check compliance for a payment
    pub fn check_compliance(&self, params: &Value) -> RpcResult<Value> {
        let ctx: MessageContext = serde_json::from_value(params.get(0).cloned().unwrap_or_default())
            .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
        
        let result = self.compliance.check(&ctx);
        
        let checks: Vec<_> = result.checks.iter().map(|c| {
            json!({
                "name": c.name,
                "passed": c.passed,
                "message": c.message,
            })
        }).collect();
        
        Ok(json!({
            "passed": result.passed,
            "riskScore": result.risk_score,
            "checks": checks,
        }))
    }
    
    /// nexus_validateIban - Validate IBAN
    pub fn validate_iban(&self, params: &Value) -> RpcResult<Value> {
        let iban = params.get(0)
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::InvalidParams("Missing IBAN".into()))?;
        
        match nexus_iso::validate_iban(iban) {
            Ok(_) => Ok(json!({ "valid": true })),
            Err(e) => Ok(json!({ "valid": false, "error": e.to_string() })),
        }
    }
    
    /// nexus_validateBic - Validate BIC/SWIFT code
    pub fn validate_bic(&self, params: &Value) -> RpcResult<Value> {
        let bic = params.get(0)
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::InvalidParams("Missing BIC".into()))?;
        
        match nexus_iso::validate_bic(bic) {
            Ok(_) => Ok(json!({ "valid": true })),
            Err(e) => Ok(json!({ "valid": false, "error": e.to_string() })),
        }
    }
    
    /// nexus_zkpVerify - Verify a ZK proof
    pub fn zkp_verify(&self, params: &Value) -> RpcResult<Value> {
        let proof_hex = params.get("proof")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::InvalidParams("Missing proof".into()))?;
        let input_hex = params.get("input")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcError::InvalidParams("Missing input".into()))?;
        
        let proof = hex::decode(proof_hex)
            .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
        let input = hex::decode(input_hex)
            .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
        
        let verified = self.zkp.verify(&proof, &input);
        
        Ok(json!({ "verified": verified }))
    }
    
    /// nexus_getProof - Get account state proof
    pub fn get_proof(&self, params: &Value) -> RpcResult<Value> {
        let address = params.get(0)
            .and_then(|v| v.as_str())
            .and_then(parse_address)
            .ok_or_else(|| RpcError::InvalidParams("Invalid address".into()))?;

        if !self.state.account_exists(&address) {
            return Err(RpcError::NotFound("Account not found".into()));
        }

        Ok(json!({
            "address": format_address(&address),
            "balance": format_u256(&self.state.get_balance(&address)),
            "nonce": self.state.get_nonce(&address),
            "codeHash": format_hash(&self.state.get_code_hash(&address)),
            "stateRoot": format_hash(&self.state.state_root()),
        }))
    }
}
