//! E2E integration tests for platform/infrastructure RPC methods
//!
//! Tests lua.*, plugin.*, project.*, storage.*, mcp.*, skills.*, and agents.*
//! RPC methods through a real daemon server with DaemonClient.

mod common;

use crucible_core::protocol::RpcMethod;
use crucible_daemon::DaemonClient;

/// In-process test server (same pattern as rpc_integration.rs)
async fn start_server() -> common::InProcessDaemon {
    common::InProcessDaemonBuilder::new()
        .expect("a test daemon builder")
        .with_kiln("kiln")
        .start()
        .await
        .expect("Failed to start server")
}

// =============================================================================
// Lua RPC tests
// =============================================================================

/// Test lua.init_session + lua.shutdown_session lifecycle
#[tokio::test]
async fn test_lua_session_lifecycle() {
    let server = start_server().await;

    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("Failed to connect");

    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    // Init a Lua session
    let init_result = client
        .call(
            RpcMethod::LuaInitSession,
            serde_json::json!({
                "session_id": "test-lua-session-1",
                "kiln_path": kiln_dir.path().to_string_lossy(),
            }),
        )
        .await
        .expect("lua.init_session RPC failed");

    // Verify response shape
    assert_eq!(
        init_result["session_id"].as_str(),
        Some("test-lua-session-1"),
        "Should return the session_id"
    );
    assert!(
        init_result["commands"].is_array(),
        "Should return a commands array"
    );

    // Shutdown the Lua session
    let shutdown_result = client
        .call(
            RpcMethod::LuaShutdownSession,
            serde_json::json!({
                "session_id": "test-lua-session-1",
            }),
        )
        .await
        .expect("lua.shutdown_session RPC failed");

    assert_eq!(
        shutdown_result["shutdown"].as_bool(),
        Some(true),
        "Should confirm session was shutdown"
    );

    // Shutting down again should return false (already removed)
    let shutdown_again = client
        .call(
            RpcMethod::LuaShutdownSession,
            serde_json::json!({
                "session_id": "test-lua-session-1",
            }),
        )
        .await
        .expect("lua.shutdown_session (second) RPC failed");

    assert_eq!(
        shutdown_again["shutdown"].as_bool(),
        Some(false),
        "Second shutdown should return false"
    );

    server.shutdown().await;
}

// =============================================================================
// Plugin RPC tests
// =============================================================================

/// Test plugin.list returns a list (may be empty or populated)
#[tokio::test]
async fn test_plugin_list_returns_list() {
    let server = start_server().await;

    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("Failed to connect");

    let result = client
        .call(RpcMethod::PluginList, serde_json::json!({}))
        .await
        .expect("plugin.list RPC failed");

    // plugins field should be an array
    let plugins = result["plugins"]
        .as_array()
        .expect("plugins should be an array");

    // Verify shape: each plugin should be a string name
    for plugin in plugins {
        assert!(
            plugin.is_string(),
            "Each plugin entry should be a string name, got: {:?}",
            plugin
        );
    }

    server.shutdown().await;
}

// =============================================================================
// Project RPC tests
// =============================================================================

/// Test project.list returns a list (empty in test environment)
#[tokio::test]
async fn test_project_list_returns_list() {
    let server = start_server().await;

    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("Failed to connect");

    let result = client
        .call(RpcMethod::ProjectList, serde_json::json!({}))
        .await
        .expect("project.list RPC failed");

    // project.list returns a serialized Vec — should be an array
    assert!(
        result.is_array(),
        "project.list should return an array, got: {:?}",
        result
    );

    server.shutdown().await;
}

// =============================================================================
// Storage RPC tests
// =============================================================================

/// Test storage.verify returns expected response shape
#[tokio::test]
async fn test_storage_verify_returns_status() {
    let server = start_server().await;

    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("Failed to connect");

    let result = client
        .call(RpcMethod::StorageVerify, serde_json::json!({}))
        .await
        .expect("storage.verify RPC failed");

    // storage.verify returns {status, message}
    assert!(
        result["status"].is_string(),
        "Should have a status field, got: {:?}",
        result
    );
    assert!(
        result["message"].is_string(),
        "Should have a message field, got: {:?}",
        result
    );

    server.shutdown().await;
}

// =============================================================================
// MCP RPC tests
// =============================================================================

/// Test mcp.status returns server status
#[tokio::test]
async fn test_mcp_status_returns_status() {
    let server = start_server().await;

    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("Failed to connect");

    let result = client
        .call(RpcMethod::McpStatus, serde_json::json!({}))
        .await
        .expect("mcp.status RPC failed");

    // mcp.status returns a JSON value with server status info
    // In test env with no MCP server running, it should still return valid JSON
    assert!(
        result.is_object() || result.is_null(),
        "mcp.status should return an object or null, got: {:?}",
        result
    );

    server.shutdown().await;
}

// =============================================================================
// Skills RPC tests
// =============================================================================

/// Test skills.list returns a skills array
#[tokio::test]
async fn test_skills_list_returns_list() {
    let server = start_server().await;

    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("Failed to connect");

    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let result = client
        .call(
            RpcMethod::SkillsList,
            serde_json::json!({
                "kiln_path": kiln_dir.path().to_string_lossy(),
            }),
        )
        .await
        .expect("skills.list RPC failed");

    // skills.list returns {skills: [...]}
    let skills = result["skills"]
        .as_array()
        .expect("skills should be an array");

    // May be empty in test environment — that's fine, we just verify the shape
    assert!(
        skills.is_empty() || skills.iter().all(|s| s["name"].is_string()),
        "Each skill should have a name field"
    );

    server.shutdown().await;
}

// =============================================================================
// Agents RPC tests
// =============================================================================

/// Test agents.list_profiles returns a profiles array
#[tokio::test]
async fn test_agents_list_profiles_returns_list() {
    let server = start_server().await;

    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("Failed to connect");

    let result = client
        .call(RpcMethod::AgentsListProfiles, serde_json::json!({}))
        .await
        .expect("agents.list_profiles RPC failed");

    // agents.list_profiles returns {profiles: [...]}
    let profiles = result["profiles"]
        .as_array()
        .expect("profiles should be an array");

    // Built-in profiles (claude, opencode, gemini, etc.) should be present
    // Each profile should have name and description fields
    for profile in profiles {
        assert!(
            profile["name"].is_string(),
            "Each profile should have a name: {:?}",
            profile
        );
        assert!(
            profile["available"].is_boolean(),
            "Each profile should carry probed availability: {:?}",
            profile
        );
    }

    server.shutdown().await;
}

// =============================================================================
// Additional platform tests
// =============================================================================

/// Test agents.resolve_profile for a known built-in agent
#[tokio::test]
async fn test_agents_resolve_profile_builtin() {
    let server = start_server().await;

    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("Failed to connect");

    // Resolve a known built-in profile
    let result = client
        .call(
            RpcMethod::AgentsResolveProfile,
            serde_json::json!({
                "name": "claude",
            }),
        )
        .await
        .expect("agents.resolve_profile RPC failed");

    // Should resolve to a profile (or null if not available)
    if !result.is_null() {
        assert_eq!(
            result["name"].as_str(),
            Some("claude"),
            "Resolved profile name should match"
        );
        assert!(
            result["is_builtin"].as_bool().unwrap_or(false),
            "Claude should be a built-in profile"
        );
    }

    // Non-existent profile should return null
    let missing = client
        .call(
            RpcMethod::AgentsResolveProfile,
            serde_json::json!({
                "name": "nonexistent-agent-xyz",
            }),
        )
        .await
        .expect("agents.resolve_profile RPC failed");

    assert!(missing.is_null(), "Non-existent profile should return null");

    server.shutdown().await;
}

/// Test storage.cleanup returns expected response
#[tokio::test]
async fn test_storage_cleanup_returns_status() {
    let server = start_server().await;

    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .expect("Failed to connect");

    let result = client
        .call(RpcMethod::StorageCleanup, serde_json::json!({}))
        .await
        .expect("storage.cleanup RPC failed");

    assert!(result["status"].is_string(), "Should have a status field");

    server.shutdown().await;
}
