//! Integration test for the progressive tool disclosure discovery bridge.
//!
//! Exercises the dispatcher-level flow an agent uses when tools are deferred:
//! `discover_tools` to find a tool, `get_tool_schema` to inspect it, then the
//! tool call itself (the target an `invoke_tool` bridge call unwraps to).
//! Uses the public `WorkspaceTools` provider over a real temp directory, so
//! the whole chain runs against real files.

use std::sync::Arc;

use crucible_core::traits::tools::ToolExecutor;
use crucible_daemon::tool_dispatch::{DaemonToolDispatcher, ToolDispatcher};
use crucible_daemon::tools::workspace::WorkspaceTools;
use serde_json::json;
use tempfile::TempDir;

fn dispatcher_over(dir: &std::path::Path) -> DaemonToolDispatcher {
    let workspace = Arc::new(WorkspaceTools::new(dir.to_path_buf())) as Arc<dyn ToolExecutor>;
    DaemonToolDispatcher::new(vec![workspace])
}

#[tokio::test]
async fn discovery_bridge_finds_inspects_and_invokes_a_tool() {
    let temp = TempDir::new().expect("tempdir");
    std::fs::write(temp.path().join("hello.md"), "# hello\nworld\n").expect("seed file");
    let dispatcher = dispatcher_over(temp.path());

    // 1. discover_tools surfaces the workspace tools (as if deferred).
    let discovered = dispatcher
        .dispatch_tool(
            "discover_tools",
            json!({ "query": "glob" }),
            Default::default(),
        )
        .await
        .expect("discover_tools should succeed");
    assert!(
        discovered.to_string().contains("glob"),
        "discovery should surface the glob tool: {discovered}"
    );

    // 2. get_tool_schema returns the tool's input schema.
    let schema = dispatcher
        .dispatch_tool(
            "get_tool_schema",
            json!({ "name": "glob" }),
            Default::default(),
        )
        .await
        .expect("get_tool_schema should succeed");
    let schema_str = schema.to_string();
    assert!(
        schema_str.contains("glob"),
        "schema names the tool: {schema_str}"
    );
    assert!(
        schema_str.contains("pattern"),
        "glob schema should describe its pattern parameter: {schema_str}"
    );

    // 3. The discovered tool executes — this is the target an invoke_tool
    //    bridge call unwraps to before dispatch.
    let result = dispatcher
        .dispatch_tool("glob", json!({ "pattern": "**/*.md" }), Default::default())
        .await
        .expect("the discovered tool should execute");
    assert!(
        result.to_string().contains("hello.md"),
        "glob should find the seeded file: {result}"
    );
}

#[tokio::test]
async fn discovery_bridge_reports_unknown_tool_schema_as_error() {
    let temp = TempDir::new().expect("tempdir");
    let dispatcher = dispatcher_over(temp.path());

    let result = dispatcher
        .dispatch_tool(
            "get_tool_schema",
            json!({ "name": "no_such_tool" }),
            Default::default(),
        )
        .await;

    assert!(result.is_err(), "unknown tool schema lookup should error");
}
