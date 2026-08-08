//! Tests for tool-call dispatch — gate ordering and argument handling.
//!
//! Split out of `tool_call.rs` to keep that file under the 1000-line budget;
//! the module name is unchanged so no test path moved.

use super::AgentManager;
use crucible_core::traits::chat::ChatToolCall;

fn invoke(args: serde_json::Value) -> ChatToolCall {
    ChatToolCall {
        name: "invoke_tool".to_string(),
        arguments: Some(args),
        id: Some("call-42".to_string()),
    }
}

#[test]
fn unwrap_rewrites_to_inner_tool_and_preserves_call_id() {
    let call = invoke(serde_json::json!({
        "name": "gh_search_repos",
        "args": {"query": "rust"}
    }));
    let inner = AgentManager::unwrap_invoke_tool("auto", &call, "call-42")
        .expect("valid invoke_tool must unwrap");
    assert_eq!(inner.name, "gh_search_repos");
    assert_eq!(inner.id.as_deref(), Some("call-42"));
    assert_eq!(
        inner
            .arguments
            .unwrap()
            .get("query")
            .and_then(|v| v.as_str()),
        Some("rust")
    );
}

#[test]
fn unwrap_defaults_missing_args_to_empty_object() {
    let call = invoke(serde_json::json!({ "name": "list_jobs" }));
    let inner = AgentManager::unwrap_invoke_tool("auto", &call, "call-42").unwrap();
    assert!(inner.arguments.unwrap().is_object());
}

#[test]
fn unwrap_rejects_recursion() {
    let call = invoke(serde_json::json!({ "name": "invoke_tool", "args": {} }));
    let err = AgentManager::unwrap_invoke_tool("auto", &call, "call-42")
        .expect_err("recursive invoke_tool must be denied");
    assert_eq!(err.call_id.as_deref(), Some("call-42"));
    assert!(err.error.unwrap().contains("itself"));
}

#[test]
fn unwrap_rejects_missing_name_without_panicking() {
    let call = invoke(serde_json::json!({ "args": {"x": 1} }));
    let err = AgentManager::unwrap_invoke_tool("auto", &call, "call-42")
        .expect_err("missing name must yield an error result");
    assert!(err.error.unwrap().contains("name"));
}

#[test]
fn unwrap_denies_write_tool_in_plan_mode() {
    let call = invoke(serde_json::json!({
        "name": "edit_file",
        "args": {"path": "x", "content": "y"}
    }));
    let err = AgentManager::unwrap_invoke_tool("plan", &call, "call-42")
        .expect_err("plan mode must deny non-plan tools via the bridge");
    assert!(err.error.unwrap().contains("plan mode"));
}

#[test]
fn unwrap_allows_plan_tool_in_plan_mode() {
    let call = invoke(serde_json::json!({
        "name": "semantic_search",
        "args": {"query": "notes"}
    }));
    let inner = AgentManager::unwrap_invoke_tool("plan", &call, "call-42")
        .expect("plan-allowed tools remain callable via the bridge");
    assert_eq!(inner.name, "semantic_search");
}

#[test]
fn missing_tool_after_unwrap_yields_error_result_not_stall() {
    // invoke_tool named a tool the dispatcher doesn't know: must return an
    // error result (so the turn completes) rather than None (which stalls
    // the turn waiting for a result that never arrives).
    let result = AgentManager::missing_tool_result(true, "bogus_tool", "call-42")
        .expect("unwrapped unknown tool must yield an error result");
    assert_eq!(result.name, "bogus_tool");
    assert_eq!(result.call_id.as_deref(), Some("call-42"));
    let err = result.error.expect("must carry an error");
    assert!(err.contains("bogus_tool"));
    assert!(err.contains("discover_tools"));
}

#[test]
fn missing_tool_without_unwrap_returns_none_for_external_agent() {
    // A genuine ACP tool call (not unwrapped) still defers to the external
    // agent — no synthetic error result.
    assert!(AgentManager::missing_tool_result(false, "acp_tool", "call-42").is_none());
}
