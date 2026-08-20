//! Extended MCP Server with Lua tools
//!
//! This server combines:
//! - **`CrucibleMcpServer`** (16 tools): Note, Search, Kiln, delegation, and job management operations
//! - **`LuaTools`** (dynamic): Scripts from configured plugins/ directories
//!
//! All responses are formatted with TOON for token efficiency.
//!
//! ## Plugin Discovery
//!
//! Plugins are discovered from (using `DiscoveryPaths`):
//! - Global personal: `~/.config/crucible/plugins/`
//! - Kiln personal: `KILN/.crucible/plugins/` (gitignored)
//! - Kiln shared: `KILN/plugins/` (version-controlled)
//!
//! ## Handler Discovery
//!
//! Event handlers are discovered from:
//! - Global personal: `~/.config/crucible/handlers/`
//! - Kiln personal: `KILN/.crucible/handlers/` (gitignored)
//! - Kiln shared: `KILN/handlers/` (version-controlled)
//!
//! Lua plugins use `@tool` doc comments to register tools.
//! Lua handlers register with `crucible.on` in a plugin; this server does
//! not scan for them.

use super::helpers::{make_server_info, text_success, McpResultExt};
use super::mcp_gateway::McpGatewayManager;
use super::toon_response::toon_success_smart;
use super::CrucibleMcpServer;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::events::SessionEvent;
use crucible_core::traits::KnowledgeRepository;
use rmcp::model::{CallToolResult, ContentBlock, Tool};
use rmcp::service::RequestContext;
use rmcp::ServerHandler;
use serde_json::{json, Value};
use std::borrow::Cow;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::debug;

/// Extended MCP server exposing Crucible kiln tools plus Lua plugins.
///
/// This server aggregates tools from multiple sources:
/// - **Kiln tools** (11): `NoteTools`, `SearchTools`, `KilnTools`, delegation via `CrucibleMcpServer`
/// - **Lua tools** (dynamic): Scripts from plugins/ directories prefixed with `lua_`
/// - **Gateway tools** (dynamic): Tools from upstream MCP servers with configured prefixes
///
/// ## Event Handling
///
pub struct ExtendedMcpServer {
    kiln_server: CrucibleMcpServer,
    /// Tools contributed by loaded plugins — the same registry the agent
    /// dispatches through, so `cru mcp` and an internal agent see one set.
    plugin_tools: Option<Arc<crate::plugin_tools::PluginRegistry>>,
    /// Optional gateway for upstream MCP servers
    gateway: Option<Arc<RwLock<McpGatewayManager>>>,
}

#[allow(
    clippy::missing_errors_doc,
    clippy::too_many_lines,
    clippy::missing_panics_doc,
    clippy::unused_self,
    clippy::cast_possible_truncation,
    missing_docs
)]
impl ExtendedMcpServer {
    /// Create a new extended MCP server.
    ///
    /// `plugin_tools` is the daemon's plugin registry — the same one the
    /// internal agent dispatches through. `cru mcp` used to serve a *separate*
    /// set, scraped out of `<kiln>/.crucible/plugins/*.lua` by `@tool` doc
    /// comments, so the two surfaces advertised different tools from different
    /// files by different rules. One registry now.
    pub async fn new(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        plugin_tools: Option<Arc<crate::plugin_tools::PluginRegistry>>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let kiln_server = CrucibleMcpServer::new_with_delegation(
            kiln_path.clone(),
            knowledge_repo,
            embedding_provider,
            None,
        );

        Ok(Self {
            kiln_server,
            plugin_tools,
            // Empty. Handlers used to be scanned out of the kiln here via
            // `-- @handler` doc comments; a plugin's `crucible.on` is the one
            // registration route now.
            gateway: None,
        })
    }

    pub fn kiln_only(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        let kiln_server = CrucibleMcpServer::new_with_delegation(
            kiln_path,
            knowledge_repo,
            embedding_provider,
            None,
        );
        Self {
            kiln_server,
            plugin_tools: None,
            gateway: None,
        }
    }

    #[must_use]
    pub fn kiln_server(&self) -> &CrucibleMcpServer {
        &self.kiln_server
    }

    /// Attach an MCP gateway for upstream server tools.
    #[must_use]
    pub fn with_gateway(mut self, gateway: McpGatewayManager) -> Self {
        self.gateway = Some(Arc::new(RwLock::new(gateway)));
        self
    }

    /// Check if a tool belongs to the gateway (has a registered prefix).
    pub async fn is_gateway_tool(&self, name: &str) -> bool {
        if let Some(gw) = &self.gateway {
            gw.read().await.has_tool(name)
        } else {
            false
        }
    }

    pub async fn list_all_tools(&self) -> Vec<Tool> {
        let mut tools = self.kiln_server.list_tools();
        tools.extend(Self::discovery_tools());

        if let Some(plugins) = &self.plugin_tools {
            for def in plugins.tool_definitions() {
                tools.push(Self::mcp_tool_from_plugin(&def));
            }
        }

        if let Some(gw) = &self.gateway {
            let gateway = gw.read().await;
            for gw_tool in gateway.all_tools() {
                tools.push(self.mcp_tool_from_gateway(&gw_tool));
            }
        }

        tools
    }

    fn mcp_tool_from_gateway(&self, tool: &crucible_core::traits::mcp::McpToolInfo) -> Tool {
        let schema = match &tool.input_schema {
            Value::Object(map) => map.clone(),
            _ => serde_json::Map::new(),
        };
        Tool::new_with_raw(
            Cow::Owned(tool.prefixed_name.clone()),
            tool.description.clone().map(Cow::Owned),
            Arc::new(schema),
        )
    }

    fn discovery_tools() -> Vec<Tool> {
        use std::sync::Arc;

        vec![
            Tool::new(
                "discover_tools",
                "Search available tools by name, description, or source. \
                 Use to find tools before calling them.",
                Arc::new(serde_json::Map::from_iter([
                    ("type".to_string(), json!("object")),
                    (
                        "properties".to_string(),
                        json!({
                            "query": {
                                "type": "string",
                                "description": "Search query to filter by name or description"
                            },
                            "source": {
                                "type": "string",
                                "enum": ["builtin", "lua"],
                                "description": "Filter by tool source"
                            },
                            "limit": {
                                "type": "integer",
                                "default": 50,
                                "description": "Maximum results to return"
                            }
                        }),
                    ),
                ])),
            ),
            Tool::new(
                "get_tool_schema",
                "Get the full JSON Schema for a specific tool's input parameters.",
                Arc::new(serde_json::Map::from_iter([
                    ("type".to_string(), json!("object")),
                    (
                        "properties".to_string(),
                        json!({
                            "name": {
                                "type": "string",
                                "description": "The name of the tool to get schema for"
                            }
                        }),
                    ),
                    ("required".to_string(), json!(["name"])),
                ])),
            ),
        ]
    }

    /// A plugin's `ToolDefinition` as an MCP `Tool`.
    ///
    /// The name is passed through unprefixed. The annotation path used to emit
    /// `lua_<name>`, which meant the same tool was `greet` to an internal agent
    /// and `lua_greet` over MCP.
    fn mcp_tool_from_plugin(def: &crucible_core::traits::tools::ToolDefinition) -> Tool {
        let schema = match &def.parameters {
            Some(Value::Object(map)) => map.clone(),
            _ => {
                let mut empty = serde_json::Map::new();
                empty.insert("type".to_string(), json!("object"));
                empty.insert("properties".to_string(), json!({}));
                empty
            }
        };

        Tool::new_with_raw(
            Cow::Owned(def.name.clone()),
            Some(Cow::Owned(def.description.clone())),
            Arc::new(schema),
        )
    }

    /// The tool-event seam: returns the event and whether it was cancelled.
    ///
    /// Currently identity. It dispatched through a `Reactor` whose `Handler`
    /// trait had no production implementation and could not acquire one —
    /// nothing outside its own tests ever called `register` — so every call
    /// returned the event unmodified with `cancelled = false`. The Reactor is
    /// gone; this keeps the shape its six callers read, and inlining it is a
    /// separate, mechanical change.
    async fn emit_event(&self, event: SessionEvent) -> (SessionEvent, bool) {
        (event, false)
    }

    pub async fn tool_count(&self) -> usize {
        let kiln = self.kiln_server.tool_count();
        let discovery = Self::discovery_tools().len();
        let lua = self
            .plugin_tools
            .as_ref()
            .map(|p| p.tool_definitions().len())
            .unwrap_or(0);
        let gateway = if let Some(gw) = &self.gateway {
            gw.read().await.tool_count()
        } else {
            0
        };
        kiln + discovery + lua + gateway
    }

    pub async fn has_plugin_tool(&self, name: &str) -> bool {
        self.plugin_tools
            .as_ref()
            .is_some_and(|p| p.tool_names().contains(name))
    }

    pub async fn call_plugin_tool(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let _start = Instant::now();

        debug!("Executing plugin tool: {} with args: {:?}", name, arguments);

        let pre_event = SessionEvent::ToolCalled {
            name: name.to_string(),
            args: arguments.clone(),
            description: None,
            source: None,
        };
        let (modified_event, cancelled) = self.emit_event(pre_event).await;

        if cancelled {
            return Err(rmcp::ErrorData::internal_error(
                format!("Plugin tool '{name}' execution cancelled by hook"),
                None,
            ));
        }

        let effective_args = match modified_event {
            SessionEvent::ToolCalled { args, .. } => args,
            _ => arguments,
        };

        let Some(plugins) = self.plugin_tools.clone() else {
            return Err(rmcp::ErrorData::internal_error(
                format!("No plugin registry available for tool '{name}'"),
                None,
            ));
        };

        let executor = crate::plugin_tools::PluginToolExecutor::new(plugins);
        let ctx = crucible_core::traits::tools::ExecutionContext::default();
        let outcome = {
            use crucible_core::traits::tools::ToolExecutor;
            executor.execute_tool(name, effective_args, &ctx).await
        };

        match outcome {
            Ok(content) => {
                let result_text = serde_json::to_string(&content).unwrap_or_default();
                let post_event = SessionEvent::ToolCompleted {
                    name: name.to_string(),
                    result: result_text,
                    error: None,
                    terminate: false,
                };
                let (modified_result, _) = self.emit_event(post_event).await;

                let final_content = match modified_result {
                    SessionEvent::ToolCompleted { result: r, .. } => {
                        serde_json::from_str(&r).unwrap_or(content)
                    }
                    _ => content,
                };

                match &final_content {
                    Value::Object(_) | Value::Array(_) => Ok(toon_success_smart(&final_content)),
                    Value::String(s) => Ok(text_success(s.clone())),
                    Value::Number(n) => Ok(text_success(n.to_string())),
                    Value::Bool(b) => Ok(text_success(b.to_string())),
                    Value::Null => Ok(CallToolResult::success(vec![])),
                }
            }
            Err(e) => {
                let event = SessionEvent::ToolCompleted {
                    name: name.to_string(),
                    result: String::new(),
                    error: Some(e.to_string()),
                    terminate: false,
                };
                self.emit_event(event).await;

                Err(rmcp::ErrorData::internal_error(
                    format!("Plugin tool '{name}' failed: {e}"),
                    None,
                ))
            }
        }
    }

    /// Call a tool on an upstream MCP server via the gateway.
    pub async fn call_gateway_tool(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let gw = self
            .gateway
            .as_ref()
            .ok_or_else(|| rmcp::ErrorData::internal_error("No gateway configured", None))?;

        debug!(
            "Executing gateway tool: {} with args: {:?}",
            name, arguments
        );

        let pre_event = SessionEvent::ToolCalled {
            name: name.to_string(),
            args: arguments.clone(),
            description: None,
            source: None,
        };
        let (modified_event, cancelled) = self.emit_event(pre_event).await;

        if cancelled {
            return Err(rmcp::ErrorData::internal_error(
                format!("Gateway tool '{name}' execution cancelled by hook"),
                None,
            ));
        }

        let effective_args = match modified_event {
            SessionEvent::ToolCalled { args, .. } => args,
            _ => arguments,
        };

        let gateway = gw.read().await;
        match gateway.call_tool(name, effective_args).await {
            Ok(result) => {
                let result_text = result
                    .content
                    .iter()
                    .filter_map(|c| c.as_text().map(str::to_string))
                    .collect::<Vec<_>>()
                    .join("\n");

                let post_event = SessionEvent::ToolCompleted {
                    name: name.to_string(),
                    result: result_text,
                    error: None,
                    terminate: false,
                };
                drop(gateway);
                self.emit_event(post_event).await;

                let content_vec: Vec<ContentBlock> = result
                    .content
                    .into_iter()
                    .filter_map(|c| c.as_text().map(|t| ContentBlock::text(t.to_string())))
                    .collect();
                Ok(if result.is_error {
                    CallToolResult::error(content_vec)
                } else {
                    CallToolResult::success(content_vec)
                })
            }
            Err(e) => {
                let event = SessionEvent::ToolCompleted {
                    name: name.to_string(),
                    result: String::new(),
                    error: Some(e.to_string()),
                    terminate: false,
                };
                drop(gateway);
                self.emit_event(event).await;

                Err(rmcp::ErrorData::internal_error(
                    format!("Gateway tool '{name}' failed: {e}"),
                    None,
                ))
            }
        }
    }
}

/// Wrapper to make `ExtendedMcpServer` implement Clone (required by rmcp)
///
/// Since `ExtendedMcpServer` contains Arc fields, we wrap it in Arc for cloning.
#[derive(Clone)]
pub struct ExtendedMcpService {
    inner: Arc<ExtendedMcpServer>,
    /// Cached tools list (refreshed on demand)
    cached_tools: Arc<RwLock<Vec<Tool>>>,
}

#[allow(clippy::missing_errors_doc)]
impl ExtendedMcpService {
    /// Create from an `ExtendedMcpServer`
    pub async fn new(server: ExtendedMcpServer) -> Self {
        let tools = server.list_all_tools().await;
        Self {
            inner: Arc::new(server),
            cached_tools: Arc::new(RwLock::new(tools)),
        }
    }

    /// Refresh the cached tools list
    pub async fn refresh_tools(&self) {
        let tools = self.inner.list_all_tools().await;
        *self.cached_tools.write().await = tools;
    }

    /// Get inner server reference
    #[must_use]
    pub fn server(&self) -> &ExtendedMcpServer {
        &self.inner
    }

    /// Serve via stdio transport (stdin/stdout)
    ///
    /// This blocks until the connection is closed.
    pub async fn serve_stdio(self) -> Result<(), anyhow::Error> {
        use rmcp::ServiceExt;

        let _service = self
            .serve((tokio::io::stdin(), tokio::io::stdout()))
            .await?;

        // Wait forever - the service will handle requests until EOF or error
        std::future::pending::<()>().await;
        Ok(())
    }

    /// Serve via streamable HTTP transport on the specified address.
    pub async fn serve_sse(self, addr: std::net::SocketAddr) -> Result<(), anyhow::Error> {
        use rmcp::transport::streamable_http_server::{
            session::local::LocalSessionManager, tower::StreamableHttpService,
        };
        use rmcp::transport::StreamableHttpServerConfig;

        let service = StreamableHttpService::new(
            move || Ok(self.clone()),
            LocalSessionManager::default().into(),
            StreamableHttpServerConfig::default(),
        );

        let router = axum::Router::new().nest_service("/mcp", service);
        let listener = tokio::net::TcpListener::bind(addr).await?;

        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                tokio::signal::ctrl_c().await.ok();
            })
            .await?;

        Ok(())
    }
}

impl ServerHandler for ExtendedMcpService {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        make_server_info(
            "Crucible MCP server exposing kiln tools (notes, search, metadata) \
            and Lua plugins for knowledge management.",
        )
    }

    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, rmcp::ErrorData> {
        let tools = self.cached_tools.read().await.clone();
        debug!("Listing {} tools", tools.len());
        Ok(rmcp::model::ListToolsResult {
            tools,
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let name = request.name.as_ref();
        let arguments = request.arguments.clone().map_or(Value::Null, Value::Object);

        debug!("Calling tool: {} with args: {:?}", name, arguments);

        if name == "discover_tools" || name == "get_tool_schema" {
            let tools = self.cached_tools.read().await.clone();
            return handle_discovery_tool(name, arguments, tools);
        }

        if self.inner.is_gateway_tool(name).await {
            self.inner.call_gateway_tool(name, arguments).await
        } else if self.inner.has_plugin_tool(name).await {
            self.inner.call_plugin_tool(name, arguments).await
        } else {
            self.inner.kiln_server.call_tool(request, context).await
        }
    }
}

fn handle_discovery_tool(
    name: &str,
    arguments: Value,
    tools: Vec<Tool>,
) -> Result<CallToolResult, rmcp::ErrorData> {
    use super::tool_discovery::{DiscoverToolsParams, GetToolSchemaParams, ToolDiscovery};

    let discovery = ToolDiscovery::new(tools);

    match name {
        "discover_tools" => {
            let params: DiscoverToolsParams =
                serde_json::from_value(arguments).unwrap_or(DiscoverToolsParams {
                    query: None,
                    source: None,
                    limit: 50,
                });
            discovery.discover_tools(&params)
        }
        "get_tool_schema" => {
            let params: GetToolSchemaParams =
                serde_json::from_value(arguments).mcp_invalid("Invalid params")?;
            discovery.get_tool_schema(&params)
        }
        _ => Err(rmcp::ErrorData::new(
            rmcp::model::ErrorCode::METHOD_NOT_FOUND,
            "Unknown discovery tool",
            None,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};
    use tempfile::TempDir;

    #[test]
    fn test_kiln_only_server_creation() {
        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let _server = ExtendedMcpServer::kiln_only(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );
    }

    #[tokio::test]
    async fn test_extended_server_creation() {
        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let server = ExtendedMcpServer::new(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
            None,
        )
        .await
        .unwrap();

        // Should have at least the 13 kiln tools
        let count = server.tool_count().await;
        assert!(count >= 13);
    }

    #[tokio::test]
    async fn test_list_all_tools() {
        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let server = ExtendedMcpServer::kiln_only(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );

        let tools = server.list_all_tools().await;
        // 11 kiln tools (delegate_session filtered without context; incl.
        // skill_view) + 3 job tools + 2 discovery tools. No workspace tools:
        // the MCP surface serves the kiln.
        assert_eq!(tools.len(), 16);
    }

    /// A plugin tool routes by registry membership, not by a name prefix.
    ///
    /// The annotation path published `lua_<name>`, so `is_lua_tool` sniffed
    /// that prefix. Plugin tools carry the same name everywhere — the internal
    /// agent's `greet` and MCP's `greet` are one tool — so membership is the
    /// only test, and a server with no registry claims nothing.
    #[tokio::test]
    async fn a_server_without_a_plugin_registry_claims_no_plugin_tools() {
        let temp = TempDir::new().unwrap();
        let server = ExtendedMcpServer::kiln_only(
            temp.path().to_str().unwrap().to_string(),
            Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>,
            Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>,
        );
        assert!(!server.has_plugin_tool("greet").await);
        assert!(!server.has_plugin_tool("lua_greet").await);
    }
}
