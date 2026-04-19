//! Lua shell module integration tests.

use crucible_lua::{register_shell_module, LuaExecutor, ShellPolicy};
use serde_json::json;

// NOTE: Shell module uses async functions which can't be called from synchronous
// Lua handlers via execute_source. The shell module is tested in its own unit tests.
// This test verifies the shell module loads correctly.
#[tokio::test]
async fn test_lua_shell_module_registration() {
    let executor = LuaExecutor::new().unwrap();

    // Verify shell module can be registered
    register_shell_module(executor.lua(), ShellPolicy::permissive()).unwrap();

    // Verify shell.which works (synchronous function)
    let source = r#"
function handler(args)
    -- shell.which is synchronous and should work
    local echo_path = shell.which("echo")
    return {
        found = echo_path ~= nil,
        is_string = type(echo_path) == "string"
    }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();
    assert!(result.success);
    let content = result.content.unwrap();
    assert!(content["found"].as_bool().unwrap());
    assert!(content["is_string"].as_bool().unwrap());
}

#[tokio::test]
async fn test_lua_shell_policy_blocks_dangerous_commands() {
    let executor = LuaExecutor::new().unwrap();

    // Use default policy which blocks rm, sudo, etc.
    register_shell_module(executor.lua(), ShellPolicy::default()).unwrap();

    let source = r#"
function handler(args)
    local result = shell.exec("rm", {"-rf", "/"}, {})
    return { executed = true }
end
"#;

    let result = executor
        .execute_source(source, false, json!({}))
        .await
        .unwrap();

    // The shell.exec should fail due to policy
    assert!(!result.success);
    assert!(result.error.unwrap().contains("not allowed"));
}
