//! Integration tests for in-process MCP server hosting
//!
//! These tests verify that agents can discover and use Crucible tools
//! when connected via the in-process SSE MCP server.

use crucible_acp::InProcessMcpHost;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

fn is_permission_denied(err: &crucible_acp::ClientError) -> bool {
    matches!(
        err,
        crucible_acp::ClientError::Connection(message) if message.contains("Operation not permitted")
    )
}

async fn start_mcp_host(
    kiln_path: PathBuf,
    knowledge_repo: Arc<dyn KnowledgeRepository>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
) -> InProcessMcpHost {
    match InProcessMcpHost::start(kiln_path, knowledge_repo, embedding_provider).await {
        Ok(host) => host,
        Err(err) => {
            if is_permission_denied(&err) {
                panic!(
                    "In-process MCP SSE server requires binding to localhost (sandbox denied): {}",
                    err
                );
            }
            panic!("Should start MCP host: {:?}", err);
        }
    }
}

// Mock implementations for testing
struct MockKnowledgeRepository;
struct MockEmbeddingProvider;

#[async_trait::async_trait]
impl KnowledgeRepository for MockKnowledgeRepository {
    async fn get_note_by_name(
        &self,
        _name: &str,
    ) -> crucible_core::Result<Option<crucible_core::parser::ParsedNote>> {
        Ok(None)
    }

    async fn list_notes(
        &self,
        _path: Option<&str>,
    ) -> crucible_core::Result<Vec<crucible_core::traits::knowledge::NoteInfo>> {
        Ok(vec![])
    }

    async fn search_vectors(
        &self,
        _vector: Vec<f32>,
    ) -> crucible_core::Result<Vec<crucible_core::types::SearchResult>> {
        Ok(vec![])
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(vec![0.1; 384])
    }

    async fn embed_batch(&self, _texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(vec![vec![0.1; 384]; _texts.len()])
    }

    fn model_name(&self) -> &str {
        "mock-model"
    }

    fn dimensions(&self) -> usize {
        384
    }
}

/// Test that the in-process MCP host starts and provides a valid SSE URL
#[tokio::test]
async fn test_in_process_mcp_host_provides_valid_sse_url() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = start_mcp_host(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await;

    let url = host.sse_url();

    // Verify URL format
    assert!(
        url.starts_with("http://127.0.0.1:"),
        "URL should be localhost"
    );
    assert!(url.ends_with("/mcp"), "URL should end with /mcp path");

    // Verify port is non-zero (actually assigned)
    let port = host.address().port();
    assert!(port > 0, "Port should be non-zero");
    assert!(port > 1024, "Port should be unprivileged (>1024)");

    host.shutdown().await;
}

/// Test that the SSE endpoint is actually reachable
#[tokio::test]
async fn test_in_process_mcp_sse_endpoint_is_reachable() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = start_mcp_host(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await;

    let url = host.sse_url();

    // Try to connect to the streamable HTTP endpoint
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}"#)
        .send()
        .await;

    // The server should respond to a valid MCP initialize request
    assert!(response.is_ok(), "MCP endpoint should be reachable");

    let resp = response.unwrap();
    // Streamable HTTP MCP returns 200 OK for valid requests
    assert!(
        resp.status().is_success(),
        "MCP endpoint should return success status, got: {}",
        resp.status()
    );

    host.shutdown().await;
}

/// Test that McpServer::Sse can be constructed with the host's URL
#[tokio::test]
async fn test_mcp_server_sse_variant_with_host_url() {
    use agent_client_protocol::{McpServer, McpServerSse};

    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = start_mcp_host(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await;

    let url = host.sse_url();

    // Construct McpServer::Sse with the host's URL (this is what connect_with_sse_mcp does)
    let mcp_server = McpServer::Sse(McpServerSse::new("crucible", url.clone()));

    // Verify it serializes correctly for the ACP protocol
    let serialized = serde_json::to_value(&mcp_server).expect("Should serialize");

    assert_eq!(serialized["name"], "crucible");
    assert_eq!(serialized["url"], url);
    assert!(serialized["headers"].is_array());

    host.shutdown().await;
}

/// Test that the ACP NewSessionRequest can include SSE MCP server
#[tokio::test]
async fn test_new_session_request_with_sse_mcp() {
    use agent_client_protocol::{McpServer, McpServerSse, NewSessionRequest};
    use serde_json::json;

    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = start_mcp_host(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await;

    let url = host.sse_url();

    // This mirrors what connect_with_sse_mcp does
    let mcp_server = McpServer::Sse(McpServerSse::new("crucible", url.clone()));

    let request: NewSessionRequest = serde_json::from_value(json!({
        "cwd": "/test",
        "mcpServers": [mcp_server],
        "_meta": null
    }))
    .expect("Failed to create NewSessionRequest");

    // Verify the request structure
    assert_eq!(request.mcp_servers.len(), 1);

    match &request.mcp_servers[0] {
        McpServer::Sse(sse) => {
            assert_eq!(&sse.name, "crucible");
            assert_eq!(&sse.url, &url);
            assert!(sse.headers.is_empty());
        }
        _ => panic!("Expected McpServer::Sse variant"),
    }

    // Verify full serialization for protocol transmission
    let json = serde_json::to_string(&request).expect("Should serialize");
    assert!(json.contains("crucible"));
    assert!(json.contains(&url));

    host.shutdown().await;
}

/// Test graceful shutdown of MCP host
#[tokio::test]
async fn test_in_process_mcp_host_graceful_shutdown() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = start_mcp_host(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await;

    let url = host.sse_url();

    // Verify endpoint works before shutdown
    let client = reqwest::Client::new();
    let before = client
        .get(&url)
        .header("Accept", "text/event-stream")
        .send()
        .await;
    assert!(before.is_ok(), "Endpoint should work before shutdown");

    // Shutdown
    host.shutdown().await;

    // Give the server time to shut down
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Endpoint should no longer be reachable
    let after = client
        .get(&url)
        .header("Accept", "text/event-stream")
        .timeout(tokio::time::Duration::from_millis(500))
        .send()
        .await;

    // Connection should fail or timeout
    assert!(
        after.is_err(),
        "Endpoint should not be reachable after shutdown"
    );
}
