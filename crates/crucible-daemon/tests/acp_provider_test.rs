use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use crucible_daemon::test_support::{MockKnowledgeRepository, MockEmbeddingProvider};
use crucible_daemon::InProcessMcpHost;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_acp_agent_with_real_providers_semantic_search_succeeds() {
    let temp = TempDir::new().expect("temp dir");
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = start_mcp_host(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await;

    let payload = call_semantic_search_over_http(&host).await;
    host.shutdown().await;

    assert!(
        payload.get("result").is_some(),
        "ACP semantic_search should succeed when providers are wired; got: {payload}",
    );
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

async fn call_semantic_search_over_http(host: &InProcessMcpHost) -> serde_json::Value {
    let client = reqwest::Client::new();
    let url = host.mcp_url();

    let init_resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"acp-provider-test","version":"0.1.0"}}}"#)
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

    let call_resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Session-Id", &session_id)
        .body(r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"semantic_search","arguments":{"query":"acp-provider-bug","limit":5}}}"#)
        .send()
        .await
        .expect("tools/call semantic_search should return response");
    assert!(call_resp.status().is_success());

    let body = call_resp.text().await.expect("tools/call body");
    let payload = body
        .lines()
        .find(|line| line.starts_with("data: {"))
        .and_then(|line| line.strip_prefix("data: "))
        .unwrap_or(body.as_str());

    serde_json::from_str(payload).expect("valid JSON response")
}
