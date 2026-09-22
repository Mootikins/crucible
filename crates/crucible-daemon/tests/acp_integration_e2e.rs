//! ACP integration E2E tests
//!
//! Verifies that ACP plumbing remains intact after crate absorptions:
//! - Tool dispatch routing via DaemonToolDispatcher
//! - DaemonToolsBridge wiring to DaemonToolsApi

use crucible_daemon::tool_dispatch::{DaemonToolDispatcher, ToolDispatcher};
use crucible_daemon::tools::workspace::WorkspaceTools;
use crucible_daemon::tools_bridge::DaemonToolsBridge;
use crucible_lua::DaemonToolsApi;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

// ============================================================================
// Test 2: Tool dispatch routes to daemon tools via DaemonToolDispatcher
// ============================================================================

#[test]
fn test_tool_dispatch_routes_to_daemon_tools() {
    let workspace_tools = Arc::new(WorkspaceTools::new(PathBuf::from("/tmp")));
    let dispatcher = DaemonToolDispatcher::new(vec![
        workspace_tools as Arc<dyn crucible_core::traits::tools::ToolExecutor>,
    ]);

    // Verify known workspace tools are recognized
    assert!(
        dispatcher.has_tool("read_file"),
        "Dispatcher should route read_file"
    );
    assert!(
        dispatcher.has_tool("edit_file"),
        "Dispatcher should route edit_file"
    );
    assert!(
        dispatcher.has_tool("write_file"),
        "Dispatcher should route write_file"
    );
    assert!(dispatcher.has_tool("bash"), "Dispatcher should route bash");
    assert!(dispatcher.has_tool("glob"), "Dispatcher should route glob");
    assert!(dispatcher.has_tool("grep"), "Dispatcher should route grep");

    // Verify unknown tools are rejected
    assert!(
        !dispatcher.has_tool("nonexistent_tool"),
        "Dispatcher should not route nonexistent_tool"
    );
    assert!(
        !dispatcher.has_tool(""),
        "Dispatcher should not route empty string"
    );
}

#[tokio::test]
async fn test_tool_dispatch_executes_read_file() {
    let temp = TempDir::new().expect("temp dir");
    let test_file = temp.path().join("test.txt");
    std::fs::write(&test_file, "hello world").expect("write test file");

    let workspace_tools = Arc::new(WorkspaceTools::new(temp.path()));
    let dispatcher = DaemonToolDispatcher::new(vec![
        workspace_tools as Arc<dyn crucible_core::traits::tools::ToolExecutor>,
    ]);

    // Dispatch a read_file call
    let result: Result<serde_json::Value, String> = dispatcher
        .dispatch_tool(
            "read_file",
            serde_json::json!({ "path": test_file.to_string_lossy() }),
            Default::default(),
        )
        .await;

    assert!(
        result.is_ok(),
        "read_file dispatch should succeed: {:?}",
        result.err()
    );

    let output = result.unwrap();
    // The output should contain the file content
    let output_str = output.to_string();
    assert!(
        output_str.contains("hello world"),
        "read_file output should contain file content, got: {}",
        output_str
    );
}

// ============================================================================
// Test 5: DaemonToolsBridge wires to DaemonToolsApi
// ============================================================================

#[tokio::test]
async fn test_tools_bridge_list_tools() {
    let temp = TempDir::new().expect("temp dir");
    let workspace_tools = Arc::new(WorkspaceTools::new(temp.path()));
    let bridge = DaemonToolsBridge::new(workspace_tools, None);

    // DaemonToolsApi::list_tools should return tool definitions as JSON
    let tools = bridge
        .list_tools()
        .await
        .expect("list_tools should succeed");

    assert!(
        !tools.is_empty(),
        "Bridge should expose workspace tools, got empty list"
    );

    // Verify each tool has a name field
    for tool_json in &tools {
        assert!(
            tool_json.get("name").is_some(),
            "Each tool should have a name field: {:?}",
            tool_json
        );
    }
}

#[tokio::test]
async fn test_tools_bridge_call_tool_routes_correctly() {
    let temp = TempDir::new().expect("temp dir");
    let test_file = temp.path().join("bridge_test.txt");
    std::fs::write(&test_file, "bridge content").expect("write test file");

    let workspace_tools = Arc::new(WorkspaceTools::new(temp.path()));
    let bridge = DaemonToolsBridge::new(workspace_tools, None);

    // Call read_file through the bridge
    let result = bridge
        .call_tool(
            "read_file".to_string(),
            serde_json::json!({ "path": test_file.to_string_lossy() }),
            None,
        )
        .await;

    assert!(
        result.is_ok(),
        "Bridge call_tool(read_file) should succeed: {:?}",
        result.err()
    );

    let output = result.unwrap();
    let output_str = output.to_string();
    assert!(
        output_str.contains("bridge content"),
        "Bridge output should contain file content, got: {}",
        output_str
    );
}
