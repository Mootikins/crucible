//! In-Process MCP Server Host
//!
//! This module hosts an MCP server within the same process as the ACP client,
//! using SSE transport on localhost. This avoids DB lock contention that would
//! occur with subprocess-based MCP servers.
//!
//! ## Architecture
//!
//! When `cru chat` starts, it:
//! 1. Creates an `InProcessMcpHost` which starts an SSE server on `127.0.0.1:0`
//! 2. Gets the bound address and constructs an SSE URL
//! 3. Passes that URL to the agent via `McpServer::Sse` in `NewSessionRequest`
//! 4. The agent connects to the SSE endpoint and discovers Crucible's tools
//!
//! This keeps all tool execution in-process, sharing the same database connection.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use crate::{ClientError, Result};
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use crucible_tools::CrucibleMcpServer;

/// Hosts an MCP server in-process using SSE transport
///
/// This struct manages the lifecycle of an SSE-based MCP server that runs
/// within the same process as the chat session.
pub struct InProcessMcpHost {
    /// Handle to the server task
    _server_handle: JoinHandle<()>,
    /// Bound address of the SSE server
    address: SocketAddr,
    /// Cancellation token for graceful shutdown
    shutdown: CancellationToken,
}

impl InProcessMcpHost {
    /// Start an in-process MCP server on localhost
    ///
    /// # Arguments
    ///
    /// * `kiln_path` - Path to the kiln directory
    /// * `knowledge_repo` - Repository for semantic search
    /// * `embedding_provider` - Provider for generating embeddings
    ///
    /// # Returns
    ///
    /// An `InProcessMcpHost` that can be used to get the SSE URL
    pub async fn start(
        kiln_path: PathBuf,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
    ) -> Result<Self> {
        use rmcp::transport::sse_server::{SseServer, SseServerConfig};
        use tracing::Instrument;

        info!(
            "Starting in-process MCP server for kiln: {}",
            kiln_path.display()
        );

        // Create cancellation token for shutdown
        let shutdown = CancellationToken::new();

        // Bind to localhost with random port - we need to get the actual address
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| ClientError::Connection(format!("Failed to bind SSE server: {}", e)))?;

        let actual_addr = listener
            .local_addr()
            .map_err(|e| ClientError::Connection(format!("Failed to get local address: {}", e)))?;

        info!("MCP SSE server bound to {}", actual_addr);

        // Create the SSE server config with the actual bound address
        let config = SseServerConfig {
            bind: actual_addr,
            sse_path: "/sse".to_string(),
            post_path: "/message".to_string(),
            ct: shutdown.clone(),
            // 30s keepalive prevents silent connection drops on long-lived SSE streams
            sse_keep_alive: Some(std::time::Duration::from_secs(30)),
        };

        // Create the SSE server and router
        let (mut sse_server, router) = SseServer::new(config);

        // Start the axum server with our listener
        let ct = shutdown.child_token();
        let server = axum::serve(listener, router).with_graceful_shutdown(async move {
            ct.cancelled().await;
            info!("SSE server cancelled");
        });

        tokio::spawn(
            async move {
                if let Err(e) = server.await {
                    error!(error = %e, "SSE server shutdown with error");
                }
            }
            .instrument(tracing::info_span!("sse-server", bind_address = %actual_addr)),
        );

        // Create the Crucible MCP server
        let mcp_server = CrucibleMcpServer::new(
            kiln_path.to_string_lossy().to_string(),
            knowledge_repo,
            embedding_provider,
        );

        // Spawn the transport handler task - it will handle incoming SSE connections
        // and serve the MCP protocol using our CrucibleMcpServer
        let server_handle = {
            use rmcp::ServiceExt;

            let ct = shutdown.child_token();
            tokio::spawn(async move {
                while let Some(transport) = sse_server.next_transport().await {
                    let service = mcp_server.clone();
                    let ct = ct.child_token();
                    tokio::spawn(async move {
                        if let Ok(server) = service.serve_with_ct(transport, ct).await {
                            let _ = server.waiting().await;
                        }
                    });
                }
            })
        };

        info!("In-process MCP server started with 12 tools");

        Ok(Self {
            _server_handle: server_handle,
            address: actual_addr,
            shutdown,
        })
    }

    /// Get the SSE endpoint URL that agents should connect to
    ///
    /// # Returns
    ///
    /// The full URL to the SSE endpoint (e.g., "http://127.0.0.1:12345/sse")
    pub fn sse_url(&self) -> String {
        format!("http://{}/sse", self.address)
    }

    /// Get the bound address of the SSE server
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Shut down the MCP server gracefully
    pub async fn shutdown(self) {
        info!("Shutting down in-process MCP server");
        self.shutdown.cancel();
        // The server handle will complete when cancelled
    }
}

impl Drop for InProcessMcpHost {
    fn drop(&mut self) {
        // Trigger shutdown on drop if not already cancelled
        if !self.shutdown.is_cancelled() {
            debug!("InProcessMcpHost dropped - triggering shutdown");
            self.shutdown.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ClientError;
    use tempfile::TempDir;

    // Mock implementations for testing
    struct MockKnowledgeRepository;
    struct MockEmbeddingProvider;

    #[async_trait::async_trait]
    impl crucible_core::traits::KnowledgeRepository for MockKnowledgeRepository {
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
    impl crucible_core::enrichment::EmbeddingProvider for MockEmbeddingProvider {
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

    #[tokio::test]
    async fn test_mcp_host_starts_and_binds() {
        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let host = match InProcessMcpHost::start(
            temp.path().to_path_buf(),
            knowledge_repo,
            embedding_provider,
        )
        .await
        {
            Ok(host) => host,
            Err(err) => {
                if is_permission_denied(&err) {
                    eprintln!("Skipping MCP host startup test: {}", err);
                    return;
                }
                panic!("Should start MCP host successfully: {:?}", err);
            }
        };

        let url = host.sse_url();

        // URL should be localhost with some port
        assert!(
            url.starts_with("http://127.0.0.1:"),
            "URL should be localhost: {}",
            url
        );
        assert!(url.ends_with("/sse"), "URL should end with /sse: {}", url);

        // Port should be non-zero
        let port = host.address().port();
        assert!(port > 0, "Port should be assigned: {}", port);

        // Clean shutdown
        host.shutdown().await;
    }

    #[tokio::test]
    async fn test_mcp_host_shutdown_on_drop() {
        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let host = match InProcessMcpHost::start(
            temp.path().to_path_buf(),
            knowledge_repo,
            embedding_provider,
        )
        .await
        {
            Ok(host) => host,
            Err(err) => {
                if is_permission_denied(&err) {
                    eprintln!("Skipping MCP host shutdown test: {}", err);
                    return;
                }
                panic!("Should start MCP host successfully: {:?}", err);
            }
        };

        let _url = host.sse_url();

        // Drop should trigger shutdown
        drop(host);

        // Give it a moment to shut down
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    fn is_permission_denied(err: &ClientError) -> bool {
        matches!(
            err,
            ClientError::Connection(message) if message.contains("Operation not permitted")
        )
    }
}
