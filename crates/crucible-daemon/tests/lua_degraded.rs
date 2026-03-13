//! Integration tests for Lua degraded mode.
//!
//! The daemon has intentional error-swallowing patterns where Lua failures
//! are logged but not propagated. These tests verify:
//!
//! 1. Daemon starts successfully even when Lua plugin loading fails
//! 2. The degraded state is detectable via RPC
//! 3. Core functionality (sessions, kilns) continues to work without plugins
//!
//! ## Error-swallowing sites verified (server.rs + daemon_plugins.rs):
//!
//! - `DaemonPluginLoader::new()` failure → `plugin_loader = None`
//! - `loader.upgrade_with_sessions()` failure → warn, Lua sessions stubs remain
//! - `loader.upgrade_with_tools()` failure → warn, Lua tools stubs remain
//! - `loader.load_plugins()` failure → warn, no plugins loaded
//! - `loader.upgrade_with_storage()` failure → warn, graph/vault stubs remain
//!
//! These are all intentionally soft-fail: the daemon should run in degraded
//! mode rather than crash when Lua subsystem has issues.

mod common;

use common::TestDaemon;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// Send a JSON-RPC request and return the parsed response.
async fn rpc_call(stream: &mut UnixStream, request: &str) -> serde_json::Value {
    stream
        .write_all(format!("{}\n", request).as_bytes())
        .await
        .expect("Failed to write request");

    let mut response = Vec::new();
    loop {
        let mut chunk = [0u8; 1024];
        let n = stream
            .read(&mut chunk)
            .await
            .expect("Failed to read response");
        assert!(n > 0, "Connection closed before full response");

        response.extend_from_slice(&chunk[..n]);
        if response.contains(&b'\n') {
            break;
        }
    }

    let line_end = response
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(response.len());
    serde_json::from_slice(&response[..line_end]).expect("Failed to parse JSON response")
}

/// Daemon starts and runs with a broken Lua plugin file.
///
/// This test creates a syntactically invalid .lua plugin and points
/// CRUCIBLE_PLUGIN_PATH at it. The daemon should:
/// - Start successfully (error swallowed during plugin loading)
/// - Respond to ping (core RPC functional)
/// - Return empty plugin list (broken plugin was not loaded)
///
/// Verifies error-swallowing in:
/// - `DaemonPluginLoader::load_plugins()` → warns on bad plugin, continues
/// - `load_plugin_spec()` → warns on extraction failure, skips plugin
#[tokio::test]
async fn test_e2e_lua_degraded_daemon_starts_with_broken_plugin() {
    let plugin_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let broken_plugin = plugin_dir.path().join("broken.lua");
    std::fs::write(
        &broken_plugin,
        concat!(
            "-- @tool name=\"broken_tool\" description=\"This plugin has invalid syntax\"\n",
            "-- @param query string \"Search query\"\n",
            "function broken_tool(args\n",
            "  -- syntax error: unclosed function\n",
        ),
    )
    .expect("Failed to write broken plugin");

    // Isolate from host's real plugins by overriding XDG_CONFIG_HOME
    let isolated_config = tempfile::tempdir().expect("Failed to create config dir");
    let config_path = isolated_config.path().to_string_lossy().to_string();
    let plugin_path = plugin_dir.path().to_string_lossy().to_string();
    let mut daemon = TestDaemon::start_with_env(vec![
        ("CRUCIBLE_PLUGIN_PATH", &plugin_path),
        ("XDG_CONFIG_HOME", &config_path),
    ])
    .await
    .expect("Daemon should start despite broken Lua plugin");

    // Verify daemon is running
    assert!(
        daemon.is_running(),
        "Daemon should be running in degraded mode"
    );

    // Connect and verify core RPC works
    let mut stream = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("Failed to connect to daemon");

    // 1. Ping should work — core daemon is functional
    let response = rpc_call(&mut stream, r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#).await;
    assert_eq!(
        response.get("result").and_then(|v| v.as_str()),
        Some("pong"),
        "Daemon should respond to ping in degraded mode"
    );

    // 2. plugin.list should return empty — broken plugin was not loaded
    let response = rpc_call(
        &mut stream,
        r#"{"jsonrpc":"2.0","id":2,"method":"plugin.list"}"#,
    )
    .await;
    let result = response.get("result").expect("Should have result");
    let plugins = result
        .get("plugins")
        .and_then(|v| v.as_array())
        .expect("Should have plugins array");
    assert!(
        plugins.is_empty(),
        "Broken plugin should not appear in plugin list, got: {:?}",
        plugins
    );

    // 3. Session creation should still work — Lua failure doesn't block core
    let kiln_dir = daemon.socket_path.parent().unwrap().join("kiln");
    std::fs::create_dir_all(&kiln_dir).expect("Failed to create kiln dir");
    let kiln_path = kiln_dir.to_string_lossy();

    let create_req = format!(
        r#"{{"jsonrpc":"2.0","id":3,"method":"session.create","params":{{"type":"chat","kiln":"{}"}}}}"#,
        kiln_path
    );
    let response = rpc_call(&mut stream, &create_req).await;
    let result = response
        .get("result")
        .expect("Session create should succeed");
    assert!(
        result.get("session_id").is_some(),
        "Should get session_id even in degraded Lua mode"
    );

    daemon.stop().await.expect("Failed to stop daemon");
}

/// Degraded state is detectable via plugin RPC and core functionality persists.
///
/// When the daemon starts without any Lua plugins (normal case for many
/// deployments), the degraded state is observable:
/// - `plugin.list` returns an empty array
/// - `plugin.reload` returns an error for nonexistent plugins
///
/// Meanwhile, all core functionality continues to work:
/// - Session create/list/pause/resume/end
/// - Kiln management
///
/// This documents what functionality is lost when Lua plugins don't load:
/// - No plugin-provided tools (custom tools defined in .lua files)
/// - No plugin-provided services (background Lua tasks)
/// - No plugin-provided handlers (event handlers from Lua)
/// - All built-in Rust tools and RPC methods remain available
#[tokio::test]
async fn test_e2e_lua_degraded_state_detectable_via_rpc() {
    let empty_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let isolated_config = tempfile::tempdir().expect("Failed to create config dir");
    let empty_path = empty_dir.path().to_string_lossy().to_string();
    let config_path = isolated_config.path().to_string_lossy().to_string();
    let mut daemon = TestDaemon::start_with_env(vec![
        ("CRUCIBLE_PLUGIN_PATH", &empty_path),
        ("XDG_CONFIG_HOME", &config_path),
    ])
    .await
    .expect("Daemon should start without any plugins");

    let mut stream = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("Failed to connect to daemon");

    // 1. Detect degraded state: plugin.list returns empty
    let response = rpc_call(
        &mut stream,
        r#"{"jsonrpc":"2.0","id":1,"method":"plugin.list"}"#,
    )
    .await;
    let result = response.get("result").expect("Should have result");
    let plugins = result
        .get("plugins")
        .and_then(|v| v.as_array())
        .expect("Should have plugins array");
    assert!(
        plugins.is_empty(),
        "No plugins should be loaded: {:?}",
        plugins
    );

    // 2. Detect degraded state: plugin.reload returns error for any plugin name
    //    When plugin_loader is Some but plugin not found, this returns internal error.
    //    This is the key signal that Lua plugin functionality is unavailable.
    let response = rpc_call(
        &mut stream,
        r#"{"jsonrpc":"2.0","id":2,"method":"plugin.reload","params":{"name":"nonexistent_plugin"}}"#,
    )
    .await;
    assert!(
        response.get("error").is_some(),
        "plugin.reload should return error for nonexistent plugin: {:?}",
        response
    );

    // 3. Core session lifecycle works without plugins
    let kiln_dir = daemon.socket_path.parent().unwrap().join("kiln");
    std::fs::create_dir_all(&kiln_dir).expect("Failed to create kiln dir");
    let kiln_path = kiln_dir.to_string_lossy();

    // Create session
    let create_req = format!(
        r#"{{"jsonrpc":"2.0","id":3,"method":"session.create","params":{{"type":"chat","kiln":"{}"}}}}"#,
        kiln_path
    );
    let response = rpc_call(&mut stream, &create_req).await;
    let result = response
        .get("result")
        .expect("Session create should succeed");
    let session_id = result
        .get("session_id")
        .and_then(|v| v.as_str())
        .expect("Should have session_id");

    // Pause session
    let pause_req = format!(
        r#"{{"jsonrpc":"2.0","id":4,"method":"session.pause","params":{{"session_id":"{}"}}}}"#,
        session_id
    );
    let response = rpc_call(&mut stream, &pause_req).await;
    let result = response
        .get("result")
        .expect("Session pause should succeed");
    let state = result.get("state").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        state.to_lowercase().contains("paused"),
        "Session should be paused, got state: {}",
        state
    );

    // Resume session
    let resume_req = format!(
        r#"{{"jsonrpc":"2.0","id":5,"method":"session.resume","params":{{"session_id":"{}"}}}}"#,
        session_id
    );
    let response = rpc_call(&mut stream, &resume_req).await;
    let result = response
        .get("result")
        .expect("Session resume should succeed");
    let state = result.get("state").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        state.to_lowercase().contains("active"),
        "Session should be active after resume, got state: {}",
        state
    );

    // List sessions — should show our session
    let list_req = format!(
        r#"{{"jsonrpc":"2.0","id":6,"method":"session.list","params":{{"kiln":"{}"}}}}"#,
        kiln_path
    );
    let response = rpc_call(&mut stream, &list_req).await;
    let result = response.get("result").expect("Session list should succeed");
    let total = result.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
    assert_eq!(total, 1, "Should have exactly 1 session");

    // End session
    let end_req = format!(
        r#"{{"jsonrpc":"2.0","id":7,"method":"session.end","params":{{"session_id":"{}"}}}}"#,
        session_id
    );
    let response = rpc_call(&mut stream, &end_req).await;
    let result = response.get("result").expect("Session end should succeed");
    let state = result.get("state").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        state.to_lowercase().contains("ended"),
        "Session should be ended, got state: {}",
        state
    );

    daemon.stop().await.expect("Failed to stop daemon");
}

/// Verify daemon with broken plugin still handles kiln operations.
///
/// Lua storage upgrade (`upgrade_with_storage()`) is called during kiln.open.
/// If plugins failed to load, this upgrade either skips or handles errors
/// gracefully. This test verifies kiln operations work in degraded mode.
#[tokio::test]
async fn test_e2e_lua_degraded_kiln_operations_work() {
    let plugin_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let broken_plugin = plugin_dir.path().join("bad_kiln_plugin.lua");
    std::fs::write(
        &broken_plugin,
        concat!(
            "-- @tool name=\"kiln_tool\" description=\"Broken kiln plugin\"\n",
            "local x = {[} -- invalid table constructor\n",
        ),
    )
    .expect("Failed to write broken plugin");
    let isolated_config = tempfile::tempdir().expect("Failed to create config dir");
    let config_path = isolated_config.path().to_string_lossy().to_string();
    let plugin_path = plugin_dir.path().to_string_lossy().to_string();
    let mut daemon = TestDaemon::start_with_env(vec![
        ("CRUCIBLE_PLUGIN_PATH", &plugin_path),
        ("XDG_CONFIG_HOME", &config_path),
    ])
    .await
    .expect("Daemon should start with broken plugin");

    let mut stream = UnixStream::connect(&daemon.socket_path)
        .await
        .expect("Failed to connect to daemon");

    // Create a kiln directory
    let kiln_dir = daemon.socket_path.parent().unwrap().join("test_kiln");
    std::fs::create_dir_all(&kiln_dir).expect("Failed to create kiln dir");

    // Open kiln — this triggers upgrade_with_storage() for Lua
    let kiln_path = kiln_dir.to_string_lossy();
    let open_req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"kiln.open","params":{{"path":"{}"}}}}"#,
        kiln_path
    );
    let response = rpc_call(&mut stream, &open_req).await;
    let result = response.get("result");
    assert!(
        result.is_some(),
        "kiln.open should succeed in degraded mode: {:?}",
        response
    );

    // List kilns — opened kiln should appear
    let response = rpc_call(
        &mut stream,
        r#"{"jsonrpc":"2.0","id":2,"method":"kiln.list"}"#,
    )
    .await;
    let result = response.get("result").and_then(|v| v.as_array());
    assert!(
        result.is_some() && !result.unwrap().is_empty(),
        "kiln.list should show opened kiln: {:?}",
        response
    );

    // Close kiln
    let close_req = format!(
        r#"{{"jsonrpc":"2.0","id":3,"method":"kiln.close","params":{{"path":"{}"}}}}"#,
        kiln_path
    );
    let response = rpc_call(&mut stream, &close_req).await;
    assert!(
        response.get("result").is_some(),
        "kiln.close should succeed: {:?}",
        response
    );

    daemon.stop().await.expect("Failed to stop daemon");
}
