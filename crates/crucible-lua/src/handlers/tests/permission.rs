use crate::handlers::{
    execute_permission_hooks, register_permission_hook_api, PermissionHook, PermissionHookResult,
    PermissionRequest,
};
use mlua::{Lua, RegistryKey};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[test]
fn test_permission_hook_registration() {
    let lua = Lua::new();
    let hooks = Arc::new(Mutex::new(Vec::new()));
    let functions = Arc::new(Mutex::new(HashMap::new()));

    register_permission_hook_api(&lua, hooks.clone(), functions.clone()).unwrap();

    lua.load(
        r#"
        crucible.permissions.on_request(function(request)
            return {allow=true}
        end)
    "#,
    )
    .exec()
    .unwrap();

    let guard = hooks.lock().unwrap();
    assert_eq!(guard.len(), 1);
    assert_eq!(guard[0].name, "permission_hook_0");

    let func_guard = functions.lock().unwrap();
    assert!(func_guard.contains_key("permission_hook_0"));
}

#[test]
fn test_permission_hook_returns_allow() {
    let lua = Lua::new();
    let hooks = Arc::new(Mutex::new(Vec::new()));
    let functions = Arc::new(Mutex::new(HashMap::new()));

    register_permission_hook_api(&lua, hooks.clone(), functions.clone()).unwrap();

    lua.load(
        r#"
        crucible.permissions.on_request(function(request)
            if request.tool_name == "bash" then
                return {allow=true}
            end
            return nil
        end)
    "#,
    )
    .exec()
    .unwrap();

    let request = PermissionRequest {
        tool_name: "bash".to_string(),
        args: serde_json::json!({"command": "npm install"}),
        file_path: None,
        mode: None,
        is_safe: false,
    };

    let hooks_guard = hooks.lock().unwrap();
    let functions_guard = functions.lock().unwrap();
    let result = execute_permission_hooks(&lua, &hooks_guard, &functions_guard, &request);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), PermissionHookResult::Allow);
}

#[test]
fn test_permission_hook_returns_deny() {
    let lua = Lua::new();
    let hooks = Arc::new(Mutex::new(Vec::new()));
    let functions = Arc::new(Mutex::new(HashMap::new()));

    register_permission_hook_api(&lua, hooks.clone(), functions.clone()).unwrap();

    lua.load(
        r#"
        crucible.permissions.on_request(function(request)
            if request.tool_name == "delete" then
                return {deny=true}
            end
            return nil
        end)
    "#,
    )
    .exec()
    .unwrap();

    let request = PermissionRequest {
        tool_name: "delete".to_string(),
        args: serde_json::json!({"path": "/important/file"}),
        file_path: Some("/important/file".to_string()),
        mode: None,
        is_safe: false,
    };

    let hooks_guard = hooks.lock().unwrap();
    let functions_guard = functions.lock().unwrap();
    let result = execute_permission_hooks(&lua, &hooks_guard, &functions_guard, &request);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), PermissionHookResult::Deny);
}

#[test]
fn test_permission_hook_returns_nil_for_prompt() {
    let lua = Lua::new();
    let hooks = Arc::new(Mutex::new(Vec::new()));
    let functions = Arc::new(Mutex::new(HashMap::new()));

    register_permission_hook_api(&lua, hooks.clone(), functions.clone()).unwrap();

    lua.load(
        r#"
        crucible.permissions.on_request(function(request)
            return nil  -- Show normal prompt
        end)
    "#,
    )
    .exec()
    .unwrap();

    let request = PermissionRequest {
        tool_name: "write".to_string(),
        args: serde_json::json!({"path": "test.txt"}),
        file_path: Some("test.txt".to_string()),
        mode: None,
        is_safe: false,
    };

    let hooks_guard = hooks.lock().unwrap();
    let functions_guard = functions.lock().unwrap();
    let result = execute_permission_hooks(&lua, &hooks_guard, &functions_guard, &request);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), PermissionHookResult::Prompt);
}

#[test]
fn test_permission_hook_no_hooks_returns_prompt() {
    let lua = Lua::new();
    let hooks: Vec<PermissionHook> = Vec::new();
    let functions: HashMap<String, RegistryKey> = HashMap::new();

    let request = PermissionRequest {
        tool_name: "bash".to_string(),
        args: serde_json::json!({}),
        file_path: None,
        mode: None,
        is_safe: false,
    };

    let result = execute_permission_hooks(&lua, &hooks, &functions, &request);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), PermissionHookResult::Prompt);
}

#[test]
fn test_permission_hook_receives_args() {
    let lua = Lua::new();
    let hooks = Arc::new(Mutex::new(Vec::new()));
    let functions = Arc::new(Mutex::new(HashMap::new()));

    register_permission_hook_api(&lua, hooks.clone(), functions.clone()).unwrap();

    lua.load(
        r#"
        crucible.permissions.on_request(function(request)
            if request.args.command and string.match(request.args.command, "^npm ") then
                return {allow=true}
            end
            return nil
        end)
    "#,
    )
    .exec()
    .unwrap();

    let request = PermissionRequest {
        tool_name: "bash".to_string(),
        args: serde_json::json!({"command": "npm install express"}),
        file_path: None,
        mode: None,
        is_safe: false,
    };

    let hooks_guard = hooks.lock().unwrap();
    let functions_guard = functions.lock().unwrap();
    let result = execute_permission_hooks(&lua, &hooks_guard, &functions_guard, &request);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), PermissionHookResult::Allow);
}

#[test]
fn test_permission_hook_receives_file_path() {
    let lua = Lua::new();
    let hooks = Arc::new(Mutex::new(Vec::new()));
    let functions = Arc::new(Mutex::new(HashMap::new()));

    register_permission_hook_api(&lua, hooks.clone(), functions.clone()).unwrap();

    lua.load(
        r#"
        crucible.permissions.on_request(function(request)
            if request.file_path and string.match(request.file_path, "%.test%.") then
                return {allow=true}
            end
            return nil
        end)
    "#,
    )
    .exec()
    .unwrap();

    let request = PermissionRequest {
        tool_name: "write".to_string(),
        args: serde_json::json!({"path": "src/foo.test.ts"}),
        file_path: Some("src/foo.test.ts".to_string()),
        mode: None,
        is_safe: false,
    };

    let hooks_guard = hooks.lock().unwrap();
    let functions_guard = functions.lock().unwrap();
    let result = execute_permission_hooks(&lua, &hooks_guard, &functions_guard, &request);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), PermissionHookResult::Allow);
}

#[test]
fn test_permission_hook_first_decision_wins() {
    let lua = Lua::new();
    let hooks = Arc::new(Mutex::new(Vec::new()));
    let functions = Arc::new(Mutex::new(HashMap::new()));

    register_permission_hook_api(&lua, hooks.clone(), functions.clone()).unwrap();

    lua.load(
        r#"
        crucible.permissions.on_request(function(request)
            return {allow=true}  -- First hook allows
        end)
        crucible.permissions.on_request(function(request)
            return {deny=true}  -- Second hook denies (should not be reached)
        end)
    "#,
    )
    .exec()
    .unwrap();

    let request = PermissionRequest {
        tool_name: "bash".to_string(),
        args: serde_json::json!({}),
        file_path: None,
        mode: None,
        is_safe: false,
    };

    let hooks_guard = hooks.lock().unwrap();
    let functions_guard = functions.lock().unwrap();
    let result = execute_permission_hooks(&lua, &hooks_guard, &functions_guard, &request);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), PermissionHookResult::Allow);
}

/// Registration-time filtering, the option `crucible.on` already had.
/// Without it every policy hook opens with `if request.tool_name == "bash"`.
#[test]
fn a_pattern_scopes_a_hook_to_matching_tools() {
    let lua = Lua::new();
    let hooks = Arc::new(Mutex::new(Vec::new()));
    let functions = Arc::new(Mutex::new(HashMap::new()));
    register_permission_hook_api(&lua, hooks.clone(), functions.clone()).unwrap();

    lua.load(
        r#"crucible.permissions.on_request(function(request)
             return { deny = true }
           end, { pattern = "bash" })"#,
    )
    .exec()
    .unwrap();

    let hooks_guard = hooks.lock().unwrap();
    let functions_guard = functions.lock().unwrap();

    let req = |tool: &str| PermissionRequest {
        tool_name: tool.to_string(),
        args: serde_json::json!({}),
        file_path: None,
        mode: None,
        is_safe: false,
    };

    assert_eq!(
        execute_permission_hooks(&lua, &hooks_guard, &functions_guard, &req("bash")).unwrap(),
        PermissionHookResult::Deny,
        "the hook must fire for a matching tool"
    );
    assert_eq!(
        execute_permission_hooks(&lua, &hooks_guard, &functions_guard, &req("read_file")).unwrap(),
        PermissionHookResult::Prompt,
        "and must not be consulted for a non-matching one"
    );
}

/// The assertion that distinguishes the shared matcher from the `*`-only one
/// this crate used to hand-roll: `{bash,edit}` matches `edit` under
/// `crucible_core::utils::glob_match` and matches nothing under a `*`-only
/// implementation. Without this, the test above passes under either.
#[test]
fn a_pattern_uses_the_same_glob_syntax_as_crucible_on() {
    let lua = Lua::new();
    let hooks = Arc::new(Mutex::new(Vec::new()));
    let functions = Arc::new(Mutex::new(HashMap::new()));
    register_permission_hook_api(&lua, hooks.clone(), functions.clone()).unwrap();

    lua.load(
        r#"crucible.permissions.on_request(function(request)
             return { deny = true }
           end, { pattern = "{bash,edit}" })"#,
    )
    .exec()
    .unwrap();

    let hooks_guard = hooks.lock().unwrap();
    let functions_guard = functions.lock().unwrap();
    let req = |tool: &str| PermissionRequest {
        tool_name: tool.to_string(),
        args: serde_json::json!({}),
        file_path: None,
        mode: None,
        is_safe: false,
    };

    for tool in ["bash", "edit"] {
        assert_eq!(
            execute_permission_hooks(&lua, &hooks_guard, &functions_guard, &req(tool)).unwrap(),
            PermissionHookResult::Deny,
            "{tool} must match the alternation"
        );
    }
    assert_eq!(
        execute_permission_hooks(&lua, &hooks_guard, &functions_guard, &req("read_file")).unwrap(),
        PermissionHookResult::Prompt
    );
}
