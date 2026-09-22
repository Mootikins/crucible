#[path = "acp_support/mcp_http.rs"]
mod mcp_http;

use std::collections::HashSet;

use crucible_daemon::InProcessMcpHost;
use mcp_http::{mcp_http_open_session, mcp_http_request, start_host};
use serde_json::json;
use tempfile::TempDir;

/// The kiln-and-delegation surface Crucible serves over MCP.
///
/// Workspace tools are deliberately absent — any harness speaking MCP already
/// has `bash`/`read_file`/`edit_file`, Crucible enforced no permissions on the
/// copies it served, and `agent_factory` added the same six separately so a
/// kiln session advertised each of them to the model twice.
const EXPECTED_TOOL_NAMES: &[&str] = &[
    "semantic_search",
    "grep_notes",
    "property_search",
    "list_notes",
    "read_note",
    "read_metadata",
    "get_kiln_info",
    "list_jobs",
    "create_note",
    "update_note",
    "delete_note",
    "delegate_session",
    "get_job_result",
    "cancel_job",
    "skill_view",
];

fn to_set(names: &[&str]) -> HashSet<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

#[tokio::test]
async fn test_acp_mcp_server_tool_names() {
    let temp = TempDir::new().expect("temp dir");
    let host = start_host(temp.path(), None).await;

    let tool_names: HashSet<String> = list_tool_names_over_http(&host).await.into_iter().collect();
    host.shutdown().await;

    assert_eq!(tool_names, to_set(EXPECTED_TOOL_NAMES));
}

/// The tool names that `tools/list` returns over HTTP.
async fn list_tool_names_over_http(host: &InProcessMcpHost) -> Vec<String> {
    let client = reqwest::Client::new();
    let url = host.mcp_url();
    let session_id = mcp_http_open_session(&client, &url).await;
    let parsed = mcp_http_request(&client, &url, &session_id, 2, "tools/list", json!({})).await;
    parsed["result"]["tools"]
        .as_array()
        .expect("tools/list response should include result.tools")
        .iter()
        .map(|tool| {
            tool["name"]
                .as_str()
                .expect("each tool should have a name")
                .to_string()
        })
        .collect()
}
