//! RPC error types and conversions

use crate::agent_manager::AgentError;
use crate::protocol::{RpcError, INTERNAL_ERROR, INVALID_PARAMS};

#[allow(dead_code)] // re-exported from rpc module; dispatch.rs uses its own copy
pub type RpcResult<T> = Result<T, RpcError>;

#[allow(dead_code)] // error conversion utility, exercised by tests
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

#[allow(dead_code)] // error conversion utility, exercised by tests
pub fn anyhow_to_rpc_error(e: anyhow::Error) -> RpcError {
    tracing::error!("Internal error: {}", e);
    RpcError {
        code: INTERNAL_ERROR,
        message: "Internal server error".into(),
        data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_manager::AgentError;
    use crate::protocol::{INTERNAL_ERROR, INVALID_PARAMS};

    #[test]
    fn agent_error_session_not_found_maps_to_invalid_params() {
        let err = AgentError::SessionNotFound("session-123".to_string());
        let rpc_err = agent_error_to_rpc_error(err);

        assert_eq!(rpc_err.code, INVALID_PARAMS);
        assert!(rpc_err.message.contains("Session not found"));
        assert!(rpc_err.message.contains("session-123"));
        assert_eq!(rpc_err.data, None);
    }

    #[test]
    fn agent_error_no_agent_configured_maps_to_invalid_params() {
        let err = AgentError::NoAgentConfigured("session-456".to_string());
        let rpc_err = agent_error_to_rpc_error(err);

        assert_eq!(rpc_err.code, INVALID_PARAMS);
        assert!(rpc_err.message.contains("No agent configured"));
        assert!(rpc_err.message.contains("session-456"));
        assert_eq!(rpc_err.data, None);
    }

    #[test]
    fn agent_error_concurrent_request_maps_to_invalid_params() {
        let err = AgentError::ConcurrentRequest("session-789".to_string());
        let rpc_err = agent_error_to_rpc_error(err);

        assert_eq!(rpc_err.code, INVALID_PARAMS);
        assert!(rpc_err.message.contains("Request already in progress"));
        assert!(rpc_err.message.contains("session-789"));
        assert_eq!(rpc_err.data, None);
    }

    #[test]
    fn agent_error_invalid_model_id_maps_to_invalid_params() {
        let err = AgentError::InvalidModelId("model-xyz is not valid".to_string());
        let rpc_err = agent_error_to_rpc_error(err);

        assert_eq!(rpc_err.code, INVALID_PARAMS);
        assert_eq!(rpc_err.message, "model-xyz is not valid");
        assert_eq!(rpc_err.data, None);
    }

    #[test]
    fn agent_error_permission_not_found_maps_to_internal_error() {
        let err = AgentError::PermissionNotFound("perm-xyz".to_string());
        let rpc_err = agent_error_to_rpc_error(err);

        assert_eq!(rpc_err.code, INTERNAL_ERROR);
        assert_eq!(rpc_err.message, "Internal server error");
        assert_eq!(rpc_err.data, None);
    }

    #[test]
    fn anyhow_error_maps_to_internal_error() {
        let err = anyhow::anyhow!("Something went wrong");
        let rpc_err = anyhow_to_rpc_error(err);

        assert_eq!(rpc_err.code, INTERNAL_ERROR);
        assert_eq!(rpc_err.message, "Internal server error");
        assert_eq!(rpc_err.data, None);
    }

    #[test]
    fn rpc_result_type_alias_works() {
        let ok_result: RpcResult<String> = Ok("success".to_string());
        assert!(ok_result.is_ok());

        let err_result: RpcResult<String> = Err(RpcError {
            code: INVALID_PARAMS,
            message: "test error".to_string(),
            data: None,
        });
        assert!(err_result.is_err());
    }

    #[test]
    fn error_messages_are_descriptive() {
        let session_id = "test-session-abc123";
        let err = AgentError::SessionNotFound(session_id.to_string());
        let rpc_err = agent_error_to_rpc_error(err);

        assert!(rpc_err.message.contains("Session not found"));
        assert!(rpc_err.message.contains(session_id));
    }
}
