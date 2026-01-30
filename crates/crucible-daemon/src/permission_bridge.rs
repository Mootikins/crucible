//! Bridge from ACP permission requests to the daemon's permission system.

use async_trait::async_trait;
use crucible_core::interaction::{PermAction, PermRequest, PermResponse};
use crucible_core::traits::PermissionGate;

use crate::agent_manager::is_safe;

/// Permission gate bridging to the daemon's permission system.
///
/// Currently implements Layer 1 (static safety check via `is_safe`).
/// Future layers: PatternStore matching, Lua hooks, interactive user prompt.
pub struct DaemonPermissionGate {
    /// Whether to auto-allow tools not recognized by `is_safe`.
    fallback_allow: bool,
}

impl DaemonPermissionGate {
    pub fn new() -> Self {
        Self {
            fallback_allow: true,
        }
    }

    pub fn with_fallback(fallback_allow: bool) -> Self {
        Self { fallback_allow }
    }
}

impl Default for DaemonPermissionGate {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PermissionGate for DaemonPermissionGate {
    async fn request_permission(&self, request: PermRequest) -> PermResponse {
        let tool_name = match &request.action {
            PermAction::Tool { name, .. } => name.as_str(),
            PermAction::Bash { tokens } => tokens.first().map(|s| s.as_str()).unwrap_or("bash"),
            PermAction::Read { .. } => "read_file",
            PermAction::Write { .. } => "write_file",
        };

        if is_safe(tool_name) {
            return PermResponse::allow();
        }

        if self.fallback_allow {
            PermResponse::allow()
        } else {
            PermResponse::deny()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn safe_tool_is_allowed() {
        let gate = DaemonPermissionGate::new();
        let request = PermRequest::tool("read_file", json!({"path": "/tmp/test.txt"}));
        let response = gate.request_permission(request).await;
        assert!(response.allowed);
    }

    #[tokio::test]
    async fn unsafe_tool_allowed_with_fallback() {
        let gate = DaemonPermissionGate::new();
        let request = PermRequest::tool("dangerous_tool", json!({}));
        let response = gate.request_permission(request).await;
        assert!(response.allowed);
    }

    #[tokio::test]
    async fn unsafe_tool_denied_without_fallback() {
        let gate = DaemonPermissionGate::with_fallback(false);
        let request = PermRequest::tool("dangerous_tool", json!({}));
        let response = gate.request_permission(request).await;
        assert!(!response.allowed);
    }

    #[tokio::test]
    async fn bash_command_not_safe() {
        let gate = DaemonPermissionGate::with_fallback(false);
        let request = PermRequest::bash(["rm", "-rf", "/tmp/test"]);
        let response = gate.request_permission(request).await;
        assert!(!response.allowed);
    }

    #[tokio::test]
    async fn read_action_maps_to_read_file() {
        let gate = DaemonPermissionGate::new();
        let request = PermRequest::read(["src", "main.rs"]);
        let response = gate.request_permission(request).await;
        assert!(response.allowed);
    }

    #[tokio::test]
    async fn write_action_denied_without_fallback() {
        let gate = DaemonPermissionGate::with_fallback(false);
        let request = PermRequest::write(["src", "main.rs"]);
        let response = gate.request_permission(request).await;
        assert!(!response.allowed);
    }

    #[tokio::test]
    async fn gate_is_send_sync() {
        fn assert_send_sync<T: Send + Sync + 'static>() {}
        assert_send_sync::<DaemonPermissionGate>();
    }
}
