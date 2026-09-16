//! MCP Server Manager for the daemon
//!
//! Manages the lifecycle of an MCP server (start/stop/status) via RPC.
//! This replaces the CLI's `cru mcp` command with daemon-managed lifecycle,
//! allowing clients to start/stop MCP servers through JSON-RPC.

use crate::empty_providers::EmptyKnowledgeRepository;
use crate::kiln_manager::KilnManager;
use crate::tools::mcp_gateway::McpGatewayManager;
use crate::tools::{ExtendedMcpServer, ExtendedMcpService};
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tracing::{info, warn};

/// What `mcp.status` answers: the server is up, or it is not.
///
/// Untagged, because the two arms are told apart by `running` and the wire
/// has always spelled them that way. The stopped arm carries `running` ALONE:
/// a stopped server has no transport, no port and no kiln, and writing those
/// keys as null would say it has them and they are empty.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(untagged)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum McpStatus {
    /// A server is serving a kiln. Listed first so a payload that carries the
    /// running keys never reads as the stopped arm, which ignores them.
    Running(McpRunning),
    /// No server is running.
    Stopped(McpStopped),
}

/// The running arm of [`McpStatus`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct McpRunning {
    /// Always `true`.
    pub running: bool,
    /// Transport type: `sse` or `stdio`.
    pub transport: String,
    /// The SSE port, or `null` under stdio. Always written.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub port: Option<u16>,
    /// The kiln path the server serves.
    pub kiln_path: String,
    /// Whether the server task has already finished, which is how a crashed
    /// server reads while the manager still calls itself running.
    pub finished: bool,
}

/// The stopped arm of [`McpStatus`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct McpStopped {
    /// Always `false`.
    pub running: bool,
}

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
    /// The daemon's gateway, so a served MCP surface also lists upstream tools.
    gateway: Option<Arc<RwLock<McpGatewayManager>>>,
}

impl McpServerManager {
    /// Create a new manager with no running server and no gateway.
    pub fn new() -> Self {
        Self::new_with_gateway(None)
    }

    /// Create a new manager that attaches `gateway` to each server it starts.
    pub fn new_with_gateway(gateway: Option<Arc<RwLock<McpGatewayManager>>>) -> Self {
        Self {
            state: Arc::new(Mutex::new(McpServerState::Stopped)),
            gateway,
        }
    }

    /// Start the MCP server.
    ///
    /// Creates an `ExtendedMcpServer` with the given kiln path and spawns
    /// a tokio task to serve via the specified transport.
    ///
    /// The caller resolves `embedding_provider` from the daemon's enrichment
    /// config, so `semantic_search` on the served surface uses the same
    /// provider as an internal agent.
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        &self,
        kiln_manager: &KilnManager,
        transport: &str,
        port: u16,
        kiln_path: &str,
        no_just: bool,
        plugin_tools: Option<Arc<crate::plugin_tools::PluginRegistry>>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
    ) -> Result<serde_json::Value, String> {
        let mut state = self.state.lock().await;

        // Check if already running
        if matches!(*state, McpServerState::Running { .. }) {
            return Err("MCP server is already running".to_string());
        }

        // Get or open the kiln to obtain knowledge_repo
        let kiln_path_ref = Path::new(kiln_path);
        let knowledge_repo: Arc<dyn KnowledgeRepository> =
            match kiln_manager.get_or_open(kiln_path_ref).await {
                Ok(handle) => handle.as_knowledge_repository(),
                Err(e) => {
                    warn!(
                        "Failed to open kiln for MCP server, using empty knowledge repository: {}",
                        e
                    );
                    Arc::new(EmptyKnowledgeRepository)
                }
            };

        // Create the ExtendedMcpServer
        let server = if no_just {
            ExtendedMcpServer::kiln_only(kiln_path.to_string(), knowledge_repo, embedding_provider)
        } else {
            match ExtendedMcpServer::new(
                kiln_path.to_string(),
                knowledge_repo.clone(),
                embedding_provider.clone(),
                plugin_tools.clone(),
            )
            .await
            {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to create ExtendedMcpServer with Just, falling back to kiln-only: {}", e);
                    ExtendedMcpServer::kiln_only(
                        kiln_path.to_string(),
                        knowledge_repo,
                        embedding_provider,
                    )
                }
            }
        };

        let server = match self.gateway.clone() {
            Some(gateway) => server.with_gateway(gateway),
            None => server,
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
            McpServerState::Running {
                handle,
                transport,
                port,
                ..
            } => {
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
    pub async fn status(&self) -> McpStatus {
        let state = self.state.lock().await;

        match &*state {
            McpServerState::Stopped => McpStatus::Stopped(McpStopped { running: false }),
            McpServerState::Running {
                transport,
                port,
                kiln_path,
                handle,
            } => McpStatus::Running(McpRunning {
                running: true,
                transport: transport.clone(),
                port: *port,
                kiln_path: kiln_path.clone(),
                finished: handle.is_finished(),
            }),
        }
    }
}

impl Default for McpServerManager {
    fn default() -> Self {
        Self::new()
    }
}
