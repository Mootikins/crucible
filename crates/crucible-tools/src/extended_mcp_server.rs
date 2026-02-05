//! Extended MCP Server with Lua tools
//!
//! This server combines:
//! - **`CrucibleMcpServer`** (12 tools): Note, Search, and Kiln operations
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
//! Lua handlers use `@handler` doc comments to register event handlers.

use crate::mcp_gateway::McpGatewayManager;
use crate::toon_response::toon_success_smart;
use crate::CrucibleMcpServer;
use crucible_core::discovery::DiscoveryPaths;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::events::{Reactor, ReactorEmitResult, SessionEvent};
use crucible_core::traits::KnowledgeRepository;
use crucible_lua::{LuaScriptHandlerRegistry, LuaToolRegistry};
use rmcp::model::{CallToolResult, Content, Tool};
use rmcp::service::RequestContext;
use rmcp::ServerHandler;
use serde_json::{json, Value};
use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Extended MCP server exposing Crucible kiln tools plus Lua plugins.
///
/// This server aggregates tools from multiple sources:
/// - **Kiln tools** (12): `NoteTools`, `SearchTools`, `KilnTools` via `CrucibleMcpServer`
/// - **Lua tools** (dynamic): Scripts from plugins/ directories prefixed with `lua_`
/// - **Gateway tools** (dynamic): Tools from upstream MCP servers with configured prefixes
///
/// ## Event Handling
///
/// Events are processed through the unified `Reactor` from crucible-core.
pub struct ExtendedMcpServer {
    kiln_server: CrucibleMcpServer,
    lua_registry: Arc<RwLock<LuaToolRegistry>>,
    reactor: Arc<RwLock<Reactor>>,
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
    /// Create a new extended MCP server with full plugin discovery.
    pub async fn new(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        plugin_dir: impl AsRef<Path>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let kiln_server =
            CrucibleMcpServer::new(kiln_path.clone(), knowledge_repo, embedding_provider);

        let plugin_dir = plugin_dir.as_ref().to_path_buf();
        let kiln_path_ref = Path::new(&kiln_path);

        let lua_registry = {
            let plugin_paths = DiscoveryPaths::new("plugins", Some(kiln_path_ref));
            let existing_plugin_paths = plugin_paths.existing_paths();

            match LuaToolRegistry::new() {
                Ok(mut registry) => {
                    for path in &existing_plugin_paths {
                        if let Err(e) = registry.discover_from(path).await {
                            warn!(
                                "Failed to discover Lua tools from {}: {}",
                                path.display(),
                                e
                            );
                        } else {
                            let count = registry.list_tools().len();
                            if count > 0 {
                                info!("Loaded {} Lua tools from {}", count, path.display());
                            }
                        }
                    }
                    if plugin_dir.exists()
                        && !existing_plugin_paths.iter().any(|p| p == &plugin_dir)
                    {
                        if let Err(e) = registry.discover_from(&plugin_dir).await {
                            warn!(
                                "Failed to discover Lua tools from {}: {}",
                                plugin_dir.display(),
                                e
                            );
                        }
                    }
                    Arc::new(RwLock::new(registry))
                }
                Err(e) => {
                    warn!("Failed to create Lua registry: {}", e);
                    Arc::new(RwLock::new(
                        LuaToolRegistry::new().expect("Lua registry must be creatable"),
                    ))
                }
            }
        };

        let reactor = {
            let mut reactor = Reactor::new();
            let handler_paths = DiscoveryPaths::new("handlers", Some(kiln_path_ref));
            let existing_handler_paths = handler_paths.existing_paths();

            if !existing_handler_paths.is_empty() {
                match LuaScriptHandlerRegistry::discover(&existing_handler_paths) {
                    Ok(registry) => match registry.to_core_handlers() {
                        Ok(handlers) => {
                            let mut loaded = 0;
                            for handler in handlers {
                                let name = handler.name().to_string();
                                if let Err(e) = reactor.register(handler) {
                                    warn!("Failed to register Lua handler {}: {}", name, e);
                                } else {
                                    loaded += 1;
                                    debug!("Registered Lua handler: {}", name);
                                }
                            }
                            if loaded > 0 {
                                info!("Loaded {} Lua handlers for MCP reactor", loaded);
                            }
                        }
                        Err(e) => warn!("Failed to create core handlers from Lua: {}", e),
                    },
                    Err(e) => warn!("Failed to discover Lua handlers: {}", e),
                }
            }
            Arc::new(RwLock::new(reactor))
        };

        Ok(Self {
            kiln_server,
            lua_registry,
            reactor,
            gateway: None,
        })
    }

    pub fn kiln_only(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        let kiln_server = CrucibleMcpServer::new(kiln_path, knowledge_repo, embedding_provider);
        let lua_registry = Arc::new(RwLock::new(
            LuaToolRegistry::new().expect("Failed to create Lua registry"),
        ));

        Self {
            kiln_server,
            lua_registry,
            reactor: Arc::new(RwLock::new(Reactor::new())),
            gateway: None,
        }
    }

    #[must_use]
    pub fn kiln_server(&self) -> &CrucibleMcpServer {
        &self.kiln_server
    }

    #[must_use]
    pub fn reactor(&self) -> Arc<RwLock<Reactor>> {
        Arc::clone(&self.reactor)
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

        let lua_registry = self.lua_registry.read().await;
        for lua_tool in lua_registry.list_tools() {
            tools.push(self.mcp_tool_from_lua(lua_tool));
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
        Tool {
            name: Cow::Owned(tool.prefixed_name.clone()),
            title: None,
            description: tool.description.clone().map(Cow::Owned),
            input_schema: Arc::new(schema),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }

    fn discovery_tools() -> Vec<Tool> {
        use std::borrow::Cow;
        use std::sync::Arc;

        vec![
            Tool {
                name: Cow::Borrowed("discover_tools"),
                description: Some(Cow::Borrowed(
                    "Search available tools by name, description, or source. \
                     Use to find tools before calling them.",
                )),
                input_schema: Arc::new(serde_json::Map::from_iter([
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
                annotations: None,
                title: None,
                output_schema: None,
                icons: None,
                meta: None,
            },
            Tool {
                name: Cow::Borrowed("get_tool_schema"),
                description: Some(Cow::Borrowed(
                    "Get the full JSON Schema for a specific tool's input parameters.",
                )),
                input_schema: Arc::new(serde_json::Map::from_iter([
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
                annotations: None,
                title: None,
                output_schema: None,
                icons: None,
                meta: None,
            },
        ]
    }

    fn mcp_tool_from_lua(&self, lt: &crucible_lua::LuaTool) -> Tool {
        let schema = lt
            .params
            .iter()
            .fold(serde_json::Map::new(), |mut map, param| {
                let mut prop = serde_json::Map::new();
                prop.insert("type".to_string(), json!(param.param_type));
                if !param.description.is_empty() {
                    prop.insert("description".to_string(), json!(param.description));
                }
                map.insert(param.name.clone(), Value::Object(prop));
                map
            });

        let mut full_schema = serde_json::Map::new();
        full_schema.insert("type".to_string(), json!("object"));
        full_schema.insert("properties".to_string(), Value::Object(schema));

        let required: Vec<_> = lt
            .params
            .iter()
            .filter(|p| p.required)
            .map(|p| p.name.clone())
            .collect();
        if !required.is_empty() {
            full_schema.insert("required".to_string(), json!(required));
        }

        Tool {
            name: format!("lua_{}", lt.name).into(),
            title: None,
            description: Some(lt.description.clone().into()),
            input_schema: Arc::new(full_schema),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }

    async fn emit_event(&self, event: SessionEvent) -> (SessionEvent, bool) {
        let mut reactor = self.reactor.write().await;
        match reactor.emit(event.clone()).await {
            Ok(ReactorEmitResult::Completed {
                event: modified, ..
            }) => (modified, false),
            Ok(ReactorEmitResult::Cancelled { .. }) => (event, true),
            Ok(ReactorEmitResult::Failed { .. }) => (event, false),
            Err(e) => {
                warn!("Reactor error: {}", e);
                (event, false)
            }
        }
    }

    pub async fn tool_count(&self) -> usize {
        let kiln = self.kiln_server.tool_count();
        let discovery = Self::discovery_tools().len();
        let lua = self.lua_registry.read().await.list_tools().len();
        let gateway = if let Some(gw) = &self.gateway {
            gw.read().await.tool_count()
        } else {
            0
        };
        kiln + discovery + lua + gateway
    }

    #[must_use]
    pub fn is_lua_tool(name: &str) -> bool {
        name.starts_with("lua_")
    }

    pub async fn has_lua_tool(&self, name: &str) -> bool {
        let registry = self.lua_registry.read().await;
        registry.get_tool(name).is_some()
    }

    pub async fn call_lua_tool(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let _start = Instant::now();

        debug!("Executing Lua tool: {} with args: {:?}", name, arguments);

        let pre_event = SessionEvent::ToolCalled {
            name: name.to_string(),
            args: arguments.clone(),
        };
        let (modified_event, cancelled) = self.emit_event(pre_event).await;

        if cancelled {
            return Err(rmcp::ErrorData::internal_error(
                format!("Lua tool '{name}' execution cancelled by hook"),
                None,
            ));
        }

        let effective_args = match modified_event {
            SessionEvent::ToolCalled { args, .. } => args,
            _ => arguments,
        };

        let registry = self.lua_registry.read().await;
        match registry.execute(name, effective_args).await {
            Ok(result) => {
                if result.success {
                    let result_text = serde_json::to_string(&result.content).unwrap_or_default();

                    let post_event = SessionEvent::ToolCompleted {
                        name: name.to_string(),
                        result: result_text,
                        error: None,
                    };
                    drop(registry);
                    let (modified_result, _) = self.emit_event(post_event).await;

                    let final_content = match modified_result {
                        SessionEvent::ToolCompleted { result: r, .. } => {
                            serde_json::from_str(&r).unwrap_or(result.content)
                        }
                        _ => result.content,
                    };

                    match &final_content {
                        Value::Object(_) | Value::Array(_) => Ok(toon_success_smart(final_content)),
                        Value::String(s) => {
                            Ok(CallToolResult::success(vec![Content::text(s.clone())]))
                        }
                        Value::Number(n) => {
                            Ok(CallToolResult::success(vec![Content::text(n.to_string())]))
                        }
                        Value::Bool(b) => {
                            Ok(CallToolResult::success(vec![Content::text(b.to_string())]))
                        }
                        Value::Null => Ok(CallToolResult::success(vec![])),
                    }
                } else {
                    let error_msg = result.error.unwrap_or_else(|| "Unknown error".to_string());

                    let event = SessionEvent::ToolCompleted {
                        name: name.to_string(),
                        result: String::new(),
                        error: Some(error_msg.clone()),
                    };
                    drop(registry);
                    self.emit_event(event).await;

                    Err(rmcp::ErrorData::internal_error(
                        format!("Lua tool '{name}' failed: {error_msg}"),
                        None,
                    ))
                }
            }
            Err(e) => {
                let event = SessionEvent::ToolCompleted {
                    name: name.to_string(),
                    result: String::new(),
                    error: Some(e.to_string()),
                };
                drop(registry);
                self.emit_event(event).await;

                Err(rmcp::ErrorData::internal_error(
                    format!("Lua tool '{name}' failed: {e}"),
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
                };
                drop(gateway);
                self.emit_event(post_event).await;

                Ok(CallToolResult {
                    content: result
                        .content
                        .into_iter()
                        .filter_map(|c| c.as_text().map(|t| Content::text(t.to_string())))
                        .collect(),
                    is_error: Some(result.is_error),
                    structured_content: None,
                    meta: None,
                })
            }
            Err(e) => {
                let event = SessionEvent::ToolCompleted {
                    name: name.to_string(),
                    result: String::new(),
                    error: Some(e.to_string()),
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
        rmcp::model::ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::default(),
            capabilities: rmcp::model::ServerCapabilities {
                tools: Some(rmcp::model::ToolsCapability { list_changed: None }),
                ..Default::default()
            },
            server_info: rmcp::model::Implementation {
                name: "crucible-mcp-server".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                title: Some("Crucible MCP Server".into()),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Crucible MCP server exposing kiln tools (notes, search, metadata) \
                and Lua plugins for knowledge management."
                    .into(),
            ),
        }
    }

    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParam>,
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
        request: rmcp::model::CallToolRequestParam,
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
        } else if ExtendedMcpServer::is_lua_tool(name) || self.inner.has_lua_tool(name).await {
            self.inner.call_lua_tool(name, arguments).await
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
    use crate::tool_discovery::{DiscoverToolsParams, GetToolSchemaParams, ToolDiscovery};

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
            let params: GetToolSchemaParams = serde_json::from_value(arguments)
                .map_err(|e| rmcp::ErrorData::invalid_params(e.to_string(), None))?;
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
    use async_trait::async_trait;
    use tempfile::TempDir;

    struct MockKnowledgeRepository;
    struct MockEmbeddingProvider;

    #[async_trait]
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

    #[async_trait]
    impl EmbeddingProvider for MockEmbeddingProvider {
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![0.1; 384])
        }

        async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(vec![vec![0.1; 384]; texts.len()])
        }

        fn model_name(&self) -> &'static str {
            "mock-model"
        }

        fn dimensions(&self) -> usize {
            384
        }
    }

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
            temp.path(),
        )
        .await
        .unwrap();

        // Should have at least the 12 kiln tools
        let count = server.tool_count().await;
        assert!(count >= 12);
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
        assert_eq!(tools.len(), 14); // 12 kiln tools + 2 discovery tools
    }

    #[test]
    fn test_tool_name_routing() {
        // Lua tools use lua_ prefix
        assert!(ExtendedMcpServer::is_lua_tool("lua_summarize"));
        assert!(ExtendedMcpServer::is_lua_tool("lua_transform"));
        assert!(!ExtendedMcpServer::is_lua_tool("just_build"));
        assert!(!ExtendedMcpServer::is_lua_tool("read_note"));
    }

    #[test]
    fn test_reactor_accessible() {
        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let server = ExtendedMcpServer::kiln_only(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );

        let reactor = server.reactor();
        assert!(Arc::strong_count(&reactor) >= 2);
    }
}
