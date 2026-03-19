//! Bridge from ACP permission requests to the daemon's permission system.

use async_trait::async_trait;
use crucible_config::components::permissions::{
    PermissionConfig, PermissionDecision, PermissionEngine,
};
use crucible_core::interaction::{PermAction, PermRequest, PermResponse};
use crucible_core::traits::PermissionGate;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::agent_manager::is_safe;

pub type PermissionPromptFuture = Pin<Box<dyn Future<Output = PermResponse> + Send>>;
pub type PermissionPromptCallback =
    Arc<dyn Fn(PermRequest) -> PermissionPromptFuture + Send + Sync>;

pub struct DaemonPermissionGate {
    engine: PermissionEngine,
    is_interactive: bool,
    prompt_callback: Option<PermissionPromptCallback>,
}

impl DaemonPermissionGate {
    pub fn new(permission_config: Option<PermissionConfig>, is_interactive: bool) -> Self {
        Self {
            engine: PermissionEngine::new(permission_config.as_ref()),
            is_interactive,
            prompt_callback: None,
        }
    }

    pub fn with_prompt_callback(mut self, callback: PermissionPromptCallback) -> Self {
        self.prompt_callback = Some(callback);
        self
    }

    fn to_engine_input(request: &PermRequest) -> (&str, String) {
        match &request.action {
            PermAction::Tool { name, args } => (name.as_str(), args.to_string()),
            PermAction::Bash { tokens } => ("bash", tokens.join(" ")),
            PermAction::Read { segments } => ("read", segments.join("/")),
            PermAction::Write { segments } => ("write", segments.join("/")),
        }
    }
}

impl Default for DaemonPermissionGate {
    fn default() -> Self {
        Self::new(None, false)
    }
}

#[async_trait]
impl PermissionGate for DaemonPermissionGate {
    async fn request_permission(&self, request: PermRequest) -> PermResponse {
        let (tool_name, input) = Self::to_engine_input(&request);

        if is_safe(tool_name) {
            return PermResponse::allow();
        }

        match self.engine.evaluate(tool_name, &input, self.is_interactive) {
            PermissionDecision::Allow => PermResponse::allow(),
            PermissionDecision::Deny { reason } => PermResponse::deny_with_reason(reason),
            PermissionDecision::Ask => {
                if let Some(callback) = &self.prompt_callback {
                    callback(request).await
                } else {
                    PermResponse::deny_with_reason(
                        "Permission requires user confirmation but no interactive bridge is configured",
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn safe_tool_is_allowed() {
        let gate = DaemonPermissionGate::new(None, true);
        let request = PermRequest::tool("read_file", json!({"path": "/tmp/test.txt"}));
        let response = gate.request_permission(request).await;
        assert!(response.allowed);
    }

    #[tokio::test]
    async fn interactive_default_ask_without_prompt_callback_denies() {
        let gate = DaemonPermissionGate::new(None, true);
        let request = PermRequest::tool("dangerous_tool", json!({}));
        let response = gate.request_permission(request).await;
        assert!(!response.allowed);
    }

    #[tokio::test]
    async fn non_interactive_ask_becomes_deny() {
        let gate = DaemonPermissionGate::new(None, false);
        let request = PermRequest::tool("dangerous_tool", json!({}));
        let response = gate.request_permission(request).await;
        assert!(!response.allowed);
    }

    #[tokio::test]
    async fn non_interactive_ask_with_callback_never_calls_callback() {
        let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_clone = called.clone();
        let callback: PermissionPromptCallback = Arc::new(move |_| {
            called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async { PermResponse::allow() })
        });
        let gate = DaemonPermissionGate::new(None, false).with_prompt_callback(callback);
        let request = PermRequest::tool("dangerous_tool", serde_json::json!({}));
        let response = gate.request_permission(request).await;
        assert!(!response.allowed, "should be denied");
        assert!(
            !called.load(std::sync::atomic::Ordering::SeqCst),
            "callback must NOT be called"
        );
    }

    #[tokio::test]
    async fn bash_command_not_safe() {
        let gate = DaemonPermissionGate::new(None, false);
        let request = PermRequest::bash(["rm", "-rf", "/tmp/test"]);
        let response = gate.request_permission(request).await;
        assert!(!response.allowed);
    }

    #[tokio::test]
    async fn read_action_defaults_to_ask_then_deny_without_prompt_callback() {
        let gate = DaemonPermissionGate::new(None, true);
        let request = PermRequest::read(["src", "main.rs"]);
        let response = gate.request_permission(request).await;
        assert!(!response.allowed);
    }

    #[tokio::test]
    async fn write_action_defaults_to_ask_then_deny_without_prompt_callback() {
        let gate = DaemonPermissionGate::new(None, true);
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
