//! RPC HTTP Server

use std::net::SocketAddr;
use std::sync::Arc;
use axum::{
    Router,
    routing::post,
    extract::State,
    http::StatusCode,
    Json,
};
use tower_http::cors::CorsLayer;
use tracing::info;

use crate::{MethodDispatcher, JsonRpcRequest, JsonRpcResponse};

/// RPC server configuration
#[derive(Clone, Debug)]
pub struct RpcConfig {
    /// Listen address
    pub listen_addr: SocketAddr,
    /// Enable CORS
    pub cors: bool,
    /// Max request size
    pub max_request_size: usize,
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:8545".parse().unwrap(),
            cors: true,
            max_request_size: 1024 * 1024, // 1 MB
        }
    }
}

/// Shared server state
struct ServerState {
    dispatcher: MethodDispatcher,
}

/// RPC Server
pub struct RpcServer {
    config: RpcConfig,
    dispatcher: MethodDispatcher,
}

impl RpcServer {
    pub fn new(config: RpcConfig, dispatcher: MethodDispatcher) -> Self {
        Self { config, dispatcher }
    }
    
    /// Start the server
    pub async fn run(self) -> Result<(), std::io::Error> {
        let state = Arc::new(ServerState {
            dispatcher: self.dispatcher,
        });
        
        let mut app = Router::new()
            .route("/", post(handle_rpc))
            .with_state(state);
        
        if self.config.cors {
            app = app.layer(CorsLayer::permissive());
        }
        
        info!("Starting RPC server on {}", self.config.listen_addr);
        
        let listener = tokio::net::TcpListener::bind(self.config.listen_addr).await?;
        axum::serve(listener, app).await
    }
}

/// Handle JSON-RPC request
async fn handle_rpc(
    State(state): State<Arc<ServerState>>,
    Json(request): Json<JsonRpcRequest>,
) -> (StatusCode, Json<JsonRpcResponse>) {
    // Validate JSON-RPC version
    if request.jsonrpc != "2.0" {
        return (
            StatusCode::OK,
            Json(JsonRpcResponse::error(
                request.id,
                -32600,
                "Invalid JSON-RPC version".to_string(),
            )),
        );
    }
    
    // Convert params to array if needed
    let params = if request.params.is_null() {
        serde_json::json!([])
    } else if request.params.is_object() {
        // Some methods expect params as array
        serde_json::json!([request.params])
    } else {
        request.params
    };
    
    // Dispatch method
    match state.dispatcher.dispatch(&request.method, &params) {
        Ok(result) => (
            StatusCode::OK,
            Json(JsonRpcResponse::success(request.id, result)),
        ),
        Err(e) => (
            StatusCode::OK,
            Json(JsonRpcResponse::error(request.id, e.code(), e.to_string())),
        ),
    }
}

#[allow(dead_code)]
async fn handle_batch_rpc(
    State(state): State<Arc<ServerState>>,
    Json(requests): Json<Vec<JsonRpcRequest>>,
) -> (StatusCode, Json<Vec<JsonRpcResponse>>) {
    let responses: Vec<JsonRpcResponse> = requests
        .into_iter()
        .map(|req| {
            let params = if req.params.is_null() {
                serde_json::json!([])
            } else {
                req.params
            };
            
            match state.dispatcher.dispatch(&req.method, &params) {
                Ok(result) => JsonRpcResponse::success(req.id, result),
                Err(e) => JsonRpcResponse::error(req.id, e.code(), e.to_string()),
            }
        })
        .collect();
    
    (StatusCode::OK, Json(responses))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_dag::{Dag, DagConfig};
    use nexus_evm::StateDb;
    use nexus_consensus::ProofOfStake;
    use crate::RpcError;

    fn setup_dispatcher() -> MethodDispatcher {
        use crate::mempool::Mempool;
        use std::sync::Arc;
        use std::sync::atomic::AtomicUsize;
        let dag = Arc::new(Dag::new(DagConfig::default()));
        let state = Arc::new(StateDb::new());
        let pos = Arc::new(ProofOfStake::default());
        let mempool = Arc::new(Mempool::new(1000));
        let peer_count = Arc::new(AtomicUsize::new(0));
        MethodDispatcher::new(dag, state, pos, Arc::new(Box::new(nexus_zkp::DefaultVerifier::default())), 1337, mempool, peer_count)
    }
    
    #[test]
    fn test_chain_id() {
        let dispatcher = setup_dispatcher();
        let result = dispatcher.dispatch("eth_chainId", &serde_json::json!([]));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), serde_json::json!("0x539")); // 1337
    }
    
    #[test]
    fn test_client_version() {
        let dispatcher = setup_dispatcher();
        let result = dispatcher.dispatch("web3_clientVersion", &serde_json::json!([]));
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_unknown_method() {
        let dispatcher = setup_dispatcher();
        let result = dispatcher.dispatch("unknown_method", &serde_json::json!([]));
        assert!(matches!(result, Err(RpcError::MethodNotFound(_))));
    }
}
