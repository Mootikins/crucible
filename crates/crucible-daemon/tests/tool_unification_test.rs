use std::collections::HashSet;
use std::sync::Arc;

use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use crucible_daemon::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};
use crucible_daemon::InProcessMcpHost;
use tempfile::TempDir;

const EXPECTED_TOOL_NAMES: &[&str] = &[
    "semantic_search",
    "text_search",
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
];



fn to_set(names: &[&str]) -> HashSet<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

#[tokio::test]
async fn test_acp_mcp_server_tool_names() {
    let temp = TempDir::new().expect("temp dir");
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;
    let host = start_mcp_host(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await;

    let tool_names: HashSet<String> = list_tool_names_over_http(&host).await.into_iter().collect();
    host.shutdown().await;

    assert_eq!(tool_names, to_set(EXPECTED_TOOL_NAMES));
}

fn is_permission_denied(err: &crucible_acp::ClientError) -> bool {
    matches!(
        err,
        crucible_acp::ClientError::Connection(message) if message.contains("Operation not permitted")
    )
}

async fn start_mcp_host(
    kiln_path: std::path::PathBuf,
    knowledge_repo: Arc<dyn KnowledgeRepository>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
) -> InProcessMcpHost {
    match InProcessMcpHost::start(kiln_path, knowledge_repo, embedding_provider, None).await {
        Ok(host) => host,
        Err(err) => {
            if is_permission_denied(&err) {
                panic!(
                    "In-process MCP HTTP server requires localhost bind (sandbox denied): {}",
                    err
                );
            }
            panic!("failed to start in-process MCP host: {err:?}");
        }
    }
}

async fn list_tool_names_over_http(host: &InProcessMcpHost) -> Vec<String> {
    let client = reqwest::Client::new();
    let url = host.mcp_url();

    let init_resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"tool-unification-test","version":"0.1.0"}}}"#)
        .send()
        .await
        .expect("initialize should succeed");
    assert!(init_resp.status().is_success());

    let session_id = init_resp
        .headers()
        .get("mcp-session-id")
        .expect("initialize should return mcp-session-id")
        .to_str()
        .expect("session id should be valid header value")
        .to_string();

    let _ = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Session-Id", &session_id)
        .body(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
        .send()
        .await
        .expect("initialized notification should succeed");

    let tools_resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Session-Id", &session_id)
        .body(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#)
        .send()
        .await
        .expect("tools/list should succeed");
    assert!(tools_resp.status().is_success());

    let body = tools_resp.text().await.expect("tools/list body");
    let payload = body
        .lines()
        .find(|line| line.starts_with("data: {"))
        .and_then(|line| line.strip_prefix("data: "))
        .unwrap_or(body.as_str());

    let parsed: serde_json::Value = serde_json::from_str(payload).expect("valid JSON response");
    let tools = parsed["result"]["tools"]
        .as_array()
        .expect("tools/list response should include result.tools");

    tools
        .iter()
        .map(|tool| {
            tool["name"]
                .as_str()
                .expect("each tool should have a name")
                .to_string()
        })
        .collect()
}
