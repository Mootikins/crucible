use std::sync::Arc;

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
