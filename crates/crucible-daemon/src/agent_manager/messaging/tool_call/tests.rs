//! Tests for tool-call dispatch — gate ordering and argument handling.
//!
//! Split out of `tool_call.rs`; the dispatch logic reads apart from the cases
//! that pin its gate order, and the module name is unchanged so no test path
//! moved.

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

/// A plugin WITHOUT the declaration may not take a tool call over, and an
/// unrecorded plugin is refused the same way.
///
/// This is the security-relevant half of the partition: a plugin is
/// third-party code, so `intercepts_tools` IS its boundary. The operator's own
/// sources sit on the other side of that boundary by trust root, not by
/// identity — `may_take_a_tool_call_over` carries the reasoning.
///
/// The list walks every source, so a new one cannot arrive without an answer.
#[test]
fn a_plugin_without_the_declaration_may_not_take_a_tool_call_over() {
    use crucible_lua::LuaSource;

    let lua = mlua::Lua::new();
    crucible_lua::record_plugin_intercept(&lua, "oci", true);
    crucible_lua::record_plugin_intercept(&lua, "quiet", false);

    let admitted: Vec<LuaSource> = [
        LuaSource::Plugin("oci".into()),
        // Declared `false`: the loader admitted it and it said no.
        LuaSource::Plugin("quiet".into()),
        // A plugin the loader never recorded. It must not gain the power by
        // being unknown.
        LuaSource::Plugin("stranger".into()),
        LuaSource::UserLua,
        LuaSource::Builtin,
        LuaSource::Eval,
    ]
    .into_iter()
    .filter(|source| super::may_take_a_tool_call_over(&lua, source))
    .collect();

    assert_eq!(
        admitted,
        vec![
            LuaSource::Plugin("oci".into()),
            LuaSource::UserLua,
            LuaSource::Builtin,
        ],
        "a plugin needs its declaration; the operator's own two sources do not, \
         and an eval is not the operator"
    );
}

/// An eval may NOT take a tool call over, whatever the socket caller asks for.
///
/// # Why the tempting reading is wrong
///
/// A human types `cru lua 'cru.config.set{…}'`, so an eval looks like the
/// operator, and this arm is the one a later reader is most likely to widen on
/// that ground. It is wrong: an eval is a SOCKET call, and the socket is the
/// surface an RPC client reaches. Reading it as the operator would let any
/// local caller that can open the daemon socket put a `pre_tool_call`
/// interception on a session it merely NAMES — somebody else's turn, taken
/// over by a caller that never held the session. That is the exact harm the
/// session scope exists to prevent, arriving through the scope itself.
///
/// The recorded table is keyed by plugin NAME, so the nearest thing to a
/// forgery available is recording a declaration under the name an eval
/// renders as. That must not reach it either.
#[test]
fn an_eval_may_not_take_a_tool_call_over_whatever_it_is_recorded_as() {
    use crucible_lua::LuaSource;

    let lua = mlua::Lua::new();
    // The string `Display` renders for an eval, plus the two an operator
    // source renders as — none of them names a plugin.
    for name in ["lua.eval", "init.lua", "builtin"] {
        crucible_lua::record_plugin_intercept(&lua, name, true);
    }

    assert!(
        !super::may_take_a_tool_call_over(&lua, &LuaSource::Eval),
        "an eval took a declaration recorded under the name it renders as"
    );
    assert_eq!(
        LuaSource::Eval.plugin_name(),
        None,
        "an eval must name no plugin, or the recorded table would reach it"
    );
}
