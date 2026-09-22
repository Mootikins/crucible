//! Bridge from ACP permission requests to the daemon's permission system.

use crucible_core::agent::ToolPolicy;
use crucible_core::config::components::permissions::{
    PermissionConfig, PermissionDecision, PermissionEngine,
};
use crucible_core::interaction::{PermAction, PermRequest, PermResponse};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::agent_manager::messaging::gate_decision::{decide_tool_gate, ToolGate};

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

impl DaemonPermissionGate {
    /// Decide one permission request for an agent action.
    ///
    /// `card_policy` is what the session's agent card says about this tool,
    /// when it says anything. The ACP path passes it here rather than
    /// answering ahead of the gate, so an agent that runs its own tools and
    /// an agent the daemon dispatches for get the same answer.
    pub async fn request_permission(
        &self,
        request: PermRequest,
        card_policy: Option<ToolPolicy>,
    ) -> PermResponse {
        let (tool_name, input) = Self::to_engine_input(&request);

        // The one tool policy: the card, the operator's rules and the
        // read-only exemption, decided in the one place both agent kinds read.
        match decide_tool_gate(card_policy, Some(&self.engine), tool_name, &input) {
            ToolGate::Refuse(reason) => return PermResponse::deny_with_reason(reason),
            ToolGate::Approve(_) => return PermResponse::allow(),
            ToolGate::Ask => {}
        }

        // This gate's own layers. The daemon's tool path has more of them —
        // the `--permissions` override, the saved patterns, the Lua hooks and
        // the mode stance — and runs them in `handle_permission_request`. An
        // ACP session has none of those: its override is already folded into
        // this engine's config by `resolve_effective_permission_config`.
        //
        // Always evaluated as interactive, and the interactivity decided here
        // instead: the engine folds `ask` into `deny` when non-interactive,
        // which is a statement about there being nobody to prompt, not about
        // what the rules say. Letting it answer for both collapses "the
        // operator denied this" into "we could not ask".
        match self.engine.evaluate(tool_name, &input, true) {
            PermissionDecision::Allow => PermResponse::allow(),
            PermissionDecision::Deny { reason } => PermResponse::deny_with_reason(reason),
            PermissionDecision::Ask { .. } => match &self.prompt_callback {
                Some(callback) if self.is_interactive => callback(request).await,
                _ => PermResponse::deny_with_reason(
                    "Permission requires user confirmation but no interactive bridge is configured",
                ),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// An operator's explicit `deny` outranks the read-only exemption.
    ///
    /// The exemption was checked *first*, so `deny = ["read_file:*"]` was
    /// silently ignored on this path — the one gate where a rule the operator
    /// wrote by hand lost to a hardcoded name list. The two sibling gates
    /// (`tools_bridge::refusal`, `agent_manager::messaging::permission`) both
    /// treat deny as absolute; this one now agrees.
    #[tokio::test]
    async fn an_operator_deny_outranks_the_read_only_exemption() {
        let config = PermissionConfig {
            deny: vec!["read_file:*".to_string()],
            ..Default::default()
        };
        let gate = DaemonPermissionGate::new(Some(config), true);
        let request = PermRequest::tool("read_file", json!({"path": "/etc/passwd"}));
        let response = gate.request_permission(request, None).await;
        assert!(
            !response.allowed,
            "an explicit deny must hold even for a read-only tool"
        );
    }

    /// `default = "deny"` means deny, read-only or not.
    ///
    /// Also a behaviour change from the old ordering, and the same one: a
    /// blanket deny is something the operator asked for.
    #[tokio::test]
    async fn a_deny_default_covers_read_only_tools_too() {
        use crucible_core::config::components::permissions::PermissionMode;
        let config = PermissionConfig {
            default: PermissionMode::Deny,
            ..Default::default()
        };
        let gate = DaemonPermissionGate::new(Some(config), true);
        let response = gate
            .request_permission(PermRequest::tool("read_file", json!({"path": "x"})), None)
            .await;
        assert!(!response.allowed, "default deny is not a suggestion");
    }

    /// …and a hardcoded deny does too. `is_safe("bash")` is false, but the
    /// ordering bug was general, so pin the other absolute source as well.
    #[tokio::test]
    async fn a_hardcoded_deny_outranks_everything() {
        let gate = DaemonPermissionGate::new(None, true);
        let request = PermRequest::bash(["rm", "-rf", "/"]);
        let response = gate.request_permission(request, None).await;
        assert!(!response.allowed, "rm -rf / is denied unconditionally");
    }

    #[tokio::test]
    async fn safe_tool_is_allowed() {
        let gate = DaemonPermissionGate::new(None, true);
        let request = PermRequest::tool("read_file", json!({"path": "/tmp/test.txt"}));
        let response = gate.request_permission(request, None).await;
        assert!(response.allowed);
    }

    #[tokio::test]
    async fn interactive_default_ask_without_prompt_callback_denies() {
        let gate = DaemonPermissionGate::new(None, true);
        let request = PermRequest::tool("dangerous_tool", json!({}));
        let response = gate.request_permission(request, None).await;
        assert!(!response.allowed);
    }

    #[tokio::test]
    async fn non_interactive_ask_becomes_deny() {
        let gate = DaemonPermissionGate::new(None, false);
        let request = PermRequest::tool("dangerous_tool", json!({}));
        let response = gate.request_permission(request, None).await;
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
        let response = gate.request_permission(request, None).await;
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
        let response = gate.request_permission(request, None).await;
        assert!(!response.allowed);
    }

    #[tokio::test]
    async fn read_action_defaults_to_ask_then_deny_without_prompt_callback() {
        let gate = DaemonPermissionGate::new(None, true);
        let request = PermRequest::read(["src", "main.rs"]);
        let response = gate.request_permission(request, None).await;
        assert!(!response.allowed);
    }

    #[tokio::test]
    async fn write_action_defaults_to_ask_then_deny_without_prompt_callback() {
        let gate = DaemonPermissionGate::new(None, true);
        let request = PermRequest::write(["src", "main.rs"]);
        let response = gate.request_permission(request, None).await;
        assert!(!response.allowed);
    }

    #[tokio::test]
    async fn gate_is_send_sync() {
        fn assert_send_sync<T: Send + Sync + 'static>() {}
        assert_send_sync::<DaemonPermissionGate>();
    }

    #[tokio::test]
    async fn permission_override_allow_approves_ask_tools() {
        use crucible_core::config::components::permissions::{PermissionConfig, PermissionMode};
        let config = PermissionConfig {
            default: PermissionMode::Allow,
            ..Default::default()
        };
        let gate = DaemonPermissionGate::new(Some(config), false);
        let request = PermRequest::tool("dangerous_tool", json!({}));
        let response = gate.request_permission(request, None).await;
        assert!(
            response.allowed,
            "with allow default, tool should be allowed"
        );
    }

    #[tokio::test]
    async fn permission_override_deny_blocks_all_tools() {
        use crucible_core::config::components::permissions::{PermissionConfig, PermissionMode};
        let config = PermissionConfig {
            default: PermissionMode::Deny,
            ..Default::default()
        };
        let gate = DaemonPermissionGate::new(Some(config), false);
        let request = PermRequest::tool("dangerous_tool", json!({}));
        let response = gate.request_permission(request, None).await;
        assert!(
            !response.allowed,
            "with deny default, tool should be denied"
        );
    }
}
