//! RPC error types and conversions

use crate::agent_manager::AgentError;
use crate::protocol::{RpcError, INTERNAL_ERROR, INVALID_PARAMS};

#[allow(dead_code)]
pub type RpcResult<T> = Result<T, RpcError>;

#[allow(dead_code)]
pub trait ToRpcError {
    fn to_rpc_error(&self) -> RpcError;
}

#[allow(dead_code)]
pub fn agent_error_to_rpc_error(e: AgentError) -> RpcError {
    use AgentError::*;
    match e {
        SessionNotFound(id) => RpcError {
            code: INVALID_PARAMS,
            message: format!("Session not found: {}", id),
            data: None,
        },
        NoAgentConfigured(id) => RpcError {
            code: INVALID_PARAMS,
            message: format!("No agent configured for session: {}", id),
            data: None,
        },
        ConcurrentRequest(id) => RpcError {
            code: INVALID_PARAMS,
            message: format!("Request already in progress for session: {}", id),
            data: None,
        },
        InvalidModelId(msg) => RpcError {
            code: INVALID_PARAMS,
            message: msg,
            data: None,
        },
        other => {
            tracing::error!("Internal agent error: {}", other);
            RpcError {
                code: INTERNAL_ERROR,
                message: "Internal server error".into(),
                data: None,
            }
        }
    }
}

#[allow(dead_code)]
pub fn anyhow_to_rpc_error(e: anyhow::Error) -> RpcError {
    tracing::error!("Internal error: {}", e);
    RpcError {
        code: INTERNAL_ERROR,
        message: "Internal server error".into(),
        data: None,
    }
}
