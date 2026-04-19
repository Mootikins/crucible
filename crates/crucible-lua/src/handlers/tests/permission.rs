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
    };

    let hooks_guard = hooks.lock().unwrap();
    let functions_guard = functions.lock().unwrap();
    let result = execute_permission_hooks(&lua, &hooks_guard, &functions_guard, &request);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), PermissionHookResult::Allow);
}
