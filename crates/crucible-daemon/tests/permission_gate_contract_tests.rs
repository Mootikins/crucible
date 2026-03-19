use std::sync::Arc;

use crucible_config::components::permissions::{PermissionConfig, PermissionMode};
use crucible_core::interaction::{PermRequest, PermissionScope};
use crucible_core::traits::PermissionGate;
use crucible_daemon::DaemonPermissionGate;
use serde_json::json;

#[tokio::test]
async fn contract_safe_actions_are_allowed_without_prompting() {
    let gate = DaemonPermissionGate::new(None, true);
    let request = PermRequest::tool("read_file", json!({"path": "README.md"}));

    let response = gate.request_permission(request).await;

    assert!(response.allowed, "safe actions should be auto-allowed");
}

#[tokio::test]
async fn contract_unsafe_actions_are_denied_without_interactive_callback() {
    let gate = DaemonPermissionGate::new(None, true);
    let request = PermRequest::tool("dangerous_tool", json!({}));

    let response = gate.request_permission(request).await;

    assert!(
        !response.allowed,
        "unsafe actions should not be auto-allowed without callback"
    );
}

#[tokio::test]
async fn contract_prompt_callback_decision_is_respected() {
    let gate = DaemonPermissionGate::new(None, true).with_prompt_callback(Arc::new(|_| {
        Box::pin(async {
            crucible_core::interaction::PermResponse::allow_pattern(
                "dangerous_tool",
                PermissionScope::Session,
            )
        })
    }));
    let request = PermRequest::tool("dangerous_tool", json!({"target": "workspace"}));

    let response = gate.request_permission(request).await;

    assert!(response.allowed, "callback allow should allow the action");
    assert_eq!(response.pattern.as_deref(), Some("dangerous_tool"));
    assert_eq!(response.scope, PermissionScope::Session);
}

#[tokio::test]
async fn contract_non_interactive_skips_callback_returns_deny() {
    let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let called_clone = called.clone();
    let gate = DaemonPermissionGate::new(None, false).with_prompt_callback(Arc::new(move |_| {
        called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async { crucible_core::interaction::PermResponse::allow() })
    }));

    let request = PermRequest::tool("dangerous_tool", json!({}));
    let response = gate.request_permission(request).await;

    assert!(!response.allowed, "non-interactive should deny ask tools");
    assert!(
        !called.load(std::sync::atomic::Ordering::SeqCst),
        "callback must not be invoked in non-interactive mode"
    );
}

#[tokio::test]
async fn contract_permission_override_allow_bypasses_ask() {
    let mut config = PermissionConfig::default();
    config.default = PermissionMode::Allow;

    let gate = DaemonPermissionGate::new(Some(config), false);
    let request = PermRequest::tool("dangerous_tool", json!({}));
    let response = gate.request_permission(request).await;

    assert!(response.allowed, "allow override should permit the tool");
}

#[tokio::test]
async fn contract_permission_override_deny_blocks_even_safe_patterns() {
    let mut config = PermissionConfig::default();
    config.default = PermissionMode::Deny;
    config.deny = vec!["*:*".to_string()];

    let gate = DaemonPermissionGate::new(Some(config), true);
    let request = PermRequest::tool("dangerous_tool", json!({"target": "workspace"}));
    let response = gate.request_permission(request).await;

    assert!(!response.allowed, "deny override should block the tool");
}

#[tokio::test]
async fn contract_non_interactive_safe_actions_still_allowed() {
    let gate = DaemonPermissionGate::new(None, false);
    let request = PermRequest::tool("read_file", json!({"path": "README.md"}));
    let response = gate.request_permission(request).await;

    assert!(
        response.allowed,
        "safe actions should bypass permission engine entirely"
    );
}
