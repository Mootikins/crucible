//! MCP Server Manager for the daemon
//!
//! Manages the lifecycle of an MCP server (start/stop/status) via RPC.
//! This replaces the CLI's `cru mcp` command with daemon-managed lifecycle,
//! allowing clients to start/stop MCP servers through JSON-RPC.

use crate::empty_providers::{EmptyEmbeddingProvider, EmptyKnowledgeRepository};
use crate::kiln_manager::KilnManager;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use crucible_tools::{ExtendedMcpServer, ExtendedMcpService};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{info, warn};

/// State of the MCP server
enum McpServerState {
    /// Server is not running
    Stopped,
    /// Server is running
    Running {
        /// Transport type: "sse" or "stdio"
        transport: String,
        /// Port for SSE transport (None for stdio)
        port: Option<u16>,
        /// Kiln path the server is serving
        kiln_path: String,
        /// Handle to the spawned server task
        handle: JoinHandle<()>,
    },
}

/// Manages MCP server lifecycle for the daemon.
///
/// Supports starting/stopping an MCP server that exposes Crucible's tools
/// via SSE or stdio transport, mirroring the CLI's `cru mcp` command.
pub struct McpServerManager {
    state: Arc<Mutex<McpServerState>>,
}

impl McpServerManager {
    /// Create a new manager with no running server.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(McpServerState::Stopped)),
        }
    }

    /// Start the MCP server.
    ///
    /// Creates an `ExtendedMcpServer` with the given kiln path and spawns
    /// a tokio task to serve via the specified transport.
    pub async fn start(
        &self,
        kiln_manager: &KilnManager,
        transport: &str,
        port: u16,
        kiln_path: &str,
        no_just: bool,
        just_dir: Option<&str>,
    ) -> Result<serde_json::Value, String> {
        let mut state = self.state.lock().await;

        // Check if already running
        if matches!(*state, McpServerState::Running { .. }) {
            return Err("MCP server is already running".to_string());
        }

        // Get or open the kiln to obtain knowledge_repo
        let kiln_path_ref = Path::new(kiln_path);
        let (knowledge_repo, embedding_provider): (
            Arc<dyn KnowledgeRepository>,
            Arc<dyn EmbeddingProvider>,
        ) = match kiln_manager.get_or_open(kiln_path_ref).await {
            Ok(handle) => {
                let kr = handle.as_knowledge_repository();
                // Try to get an embedding provider from daemon config;
                // fall back to empty provider if unavailable
                let ep: Arc<dyn EmbeddingProvider> = Arc::new(EmptyEmbeddingProvider);
                (kr, ep)
            }
            Err(e) => {
                warn!("Failed to open kiln for MCP server, using empty providers: {}", e);
                (
                    Arc::new(EmptyKnowledgeRepository) as Arc<dyn KnowledgeRepository>,
                    Arc::new(EmptyEmbeddingProvider) as Arc<dyn EmbeddingProvider>,
                )
            }
        };

        // Create the ExtendedMcpServer
        let server = if no_just {
            ExtendedMcpServer::kiln_only(
                kiln_path.to_string(),
                knowledge_repo,
                embedding_provider,
            )
        } else {
            let just_path = just_dir
                .map(|d| std::path::PathBuf::from(d))
                .unwrap_or_else(|| {
                    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
                });
            match ExtendedMcpServer::new(
                kiln_path.to_string(),
                knowledge_repo,
                embedding_provider,
                &just_path,
            )
            .await
            {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to create ExtendedMcpServer with Just, falling back to kiln-only: {}", e);
                    // Re-open kiln for fallback (previous knowledge_repo was consumed)
                    let (kr, ep) = match kiln_manager.get_or_open(kiln_path_ref).await {
                        Ok(handle) => (
                            handle.as_knowledge_repository(),
                            Arc::new(EmptyEmbeddingProvider) as Arc<dyn EmbeddingProvider>,
                        ),
                        Err(_) => (
                            Arc::new(EmptyKnowledgeRepository) as Arc<dyn KnowledgeRepository>,
                            Arc::new(EmptyEmbeddingProvider) as Arc<dyn EmbeddingProvider>,
                        ),
                    };
                    ExtendedMcpServer::kiln_only(kiln_path.to_string(), kr, ep)
                }
            }
        };

        let tool_count = server.tool_count().await;
        info!("MCP server initialized with {} tools", tool_count);

        let service = ExtendedMcpService::new(server).await;
        let transport_str = transport.to_string();
        let kiln_path_owned = kiln_path.to_string();

        let handle = match transport {
            "stdio" => {
                info!("Starting MCP server via stdio transport");
                tokio::spawn(async move {
                    if let Err(e) = service.serve_stdio().await {
                        warn!("MCP stdio server error: {}", e);
                    }
                })
            }
            _ => {
                // Default to SSE
                let addr: SocketAddr = match format!("127.0.0.1:{}", port).parse() {
                    Ok(a) => a,
                    Err(e) => return Err(format!("Invalid port: {}", e)),
                };
                info!("Starting MCP SSE server on http://{}", addr);
                tokio::spawn(async move {
                    if let Err(e) = service.serve_sse(addr).await {
                        warn!("MCP SSE server error: {}", e);
                    }
                })
            }
        };

        let actual_port = if transport_str == "stdio" {
            None
        } else {
            Some(port)
        };

        *state = McpServerState::Running {
            transport: transport_str.clone(),
            port: actual_port,
            kiln_path: kiln_path_owned,
            handle,
        };

        Ok(serde_json::json!({
            "status": "started",
            "transport": transport_str,
            "port": actual_port,
            "tool_count": tool_count,
        }))
    }

    /// Stop the running MCP server.
    pub async fn stop(&self) -> Result<serde_json::Value, String> {
        let mut state = self.state.lock().await;

        match std::mem::replace(&mut *state, McpServerState::Stopped) {
            McpServerState::Running { handle, transport, port, .. } => {
                handle.abort();
                info!("MCP server stopped (was {} on port {:?})", transport, port);
                Ok(serde_json::json!({
                    "status": "stopped",
                }))
            }
            McpServerState::Stopped => {
                *state = McpServerState::Stopped;
                Err("MCP server is not running".to_string())
            }
        }
    }

    /// Get the current status of the MCP server.
    pub async fn status(&self) -> serde_json::Value {
        let state = self.state.lock().await;

        match &*state {
            McpServerState::Stopped => {
                serde_json::json!({
                    "running": false,
                })
            }
            McpServerState::Running {
                transport,
                port,
                kiln_path,
                handle,
            } => {
                serde_json::json!({
                    "running": true,
                    "transport": transport,
                    "port": port,
                    "kiln_path": kiln_path,
                    "finished": handle.is_finished(),
                })
            }
        }
    }
}

impl Default for McpServerManager {
    fn default() -> Self {
        Self::new()
    }
}
