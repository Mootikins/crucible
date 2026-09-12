use crate::handlers::{
    execute_permission_hooks, register_permission_hook_api, LuaScriptHandlerRegistry,
    PermissionHookResult, PermissionRequest,
};
use mlua::Lua;

#[test]
fn test_permission_hook_registration() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_permission_hook_api(&lua, registry.clone()).unwrap();

    lua.load(
        r#"
        cru.permissions.on_request(function(request)
            return {allow=true}
        end)
    "#,
    )
    .exec()
    .unwrap();

    let hooks = registry.runtime_handlers_for(
        "permission:request",
        Some("bash"),
        crate::handlers::Firing::Sessionless,
    );
    assert_eq!(hooks.len(), 1);
    let _body: mlua::Function = lua.registry_value(hooks[0].body()).unwrap();
}

#[test]
fn test_permission_hook_returns_allow() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_permission_hook_api(&lua, registry.clone()).unwrap();

    lua.load(
        r#"
        cru.permissions.on_request(function(request)
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

    let result = execute_permission_hooks(
        &lua,
        &registry,
        &request,
        crate::handlers::Firing::Sessionless,
    );

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), PermissionHookResult::Allow);
}

#[test]
fn test_permission_hook_returns_deny() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_permission_hook_api(&lua, registry.clone()).unwrap();

    lua.load(
        r#"
        cru.permissions.on_request(function(request)
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

    let result = execute_permission_hooks(
        &lua,
        &registry,
        &request,
        crate::handlers::Firing::Sessionless,
    );

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), PermissionHookResult::Deny);
}

#[test]
fn test_permission_hook_returns_nil_for_prompt() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_permission_hook_api(&lua, registry.clone()).unwrap();

    lua.load(
        r#"
        cru.permissions.on_request(function(request)
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

    let result = execute_permission_hooks(
        &lua,
        &registry,
        &request,
        crate::handlers::Firing::Sessionless,
    );

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), PermissionHookResult::Prompt);
}

#[test]
fn test_permission_hook_no_hooks_returns_prompt() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    let request = PermissionRequest {
        tool_name: "bash".to_string(),
        args: serde_json::json!({}),
        file_path: None,
        mode: None,
        is_safe: false,
    };

    let result = execute_permission_hooks(
        &lua,
        &registry,
        &request,
        crate::handlers::Firing::Sessionless,
    );

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), PermissionHookResult::Prompt);
}

#[test]
fn test_permission_hook_receives_args() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_permission_hook_api(&lua, registry.clone()).unwrap();

    lua.load(
        r#"
        cru.permissions.on_request(function(request)
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

    let result = execute_permission_hooks(
        &lua,
        &registry,
        &request,
        crate::handlers::Firing::Sessionless,
    );

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), PermissionHookResult::Allow);
}

#[test]
fn test_permission_hook_receives_file_path() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_permission_hook_api(&lua, registry.clone()).unwrap();

    lua.load(
        r#"
        cru.permissions.on_request(function(request)
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

    let result = execute_permission_hooks(
        &lua,
        &registry,
        &request,
        crate::handlers::Firing::Sessionless,
    );

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), PermissionHookResult::Allow);
}

#[test]
fn test_permission_hook_first_decision_wins() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_permission_hook_api(&lua, registry.clone()).unwrap();

    lua.load(
        r#"
        cru.permissions.on_request(function(request)
            return {allow=true}  -- First hook allows
        end)
        cru.permissions.on_request(function(request)
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

    let result = execute_permission_hooks(
        &lua,
        &registry,
        &request,
        crate::handlers::Firing::Sessionless,
    );

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), PermissionHookResult::Allow);
}

/// Registration-time filtering, the option `cru.on` already had.
/// Without it every policy hook opens with `if request.tool_name == "bash"`.
#[test]
fn a_pattern_scopes_a_hook_to_matching_tools() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_permission_hook_api(&lua, registry.clone()).unwrap();

    lua.load(
        r#"cru.permissions.on_request(function(request)
             return { deny = true }
           end, { pattern = "bash" })"#,
    )
    .exec()
    .unwrap();

    let req = |tool: &str| PermissionRequest {
        tool_name: tool.to_string(),
        args: serde_json::json!({}),
        file_path: None,
        mode: None,
        is_safe: false,
    };

    assert_eq!(
        execute_permission_hooks(
            &lua,
            &registry,
            &req("bash"),
            crate::handlers::Firing::Sessionless
        )
        .unwrap(),
        PermissionHookResult::Deny,
        "the hook must fire for a matching tool"
    );
    assert_eq!(
        execute_permission_hooks(
            &lua,
            &registry,
            &req("read_file"),
            crate::handlers::Firing::Sessionless
        )
        .unwrap(),
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
    let registry = LuaScriptHandlerRegistry::new();
    register_permission_hook_api(&lua, registry.clone()).unwrap();

    lua.load(
        r#"cru.permissions.on_request(function(request)
             return { deny = true }
           end, { pattern = "{bash,edit}" })"#,
    )
    .exec()
    .unwrap();

    let req = |tool: &str| PermissionRequest {
        tool_name: tool.to_string(),
        args: serde_json::json!({}),
        file_path: None,
        mode: None,
        is_safe: false,
    };

    for tool in ["bash", "edit"] {
        assert_eq!(
            execute_permission_hooks(
                &lua,
                &registry,
                &req(tool),
                crate::handlers::Firing::Sessionless
            )
            .unwrap(),
            PermissionHookResult::Deny,
            "{tool} must match the alternation"
        );
    }
    assert_eq!(
        execute_permission_hooks(
            &lua,
            &registry,
            &req("read_file"),
            crate::handlers::Firing::Sessionless
        )
        .unwrap(),
        PermissionHookResult::Prompt
    );
}
