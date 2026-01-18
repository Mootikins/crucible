//! Extended MCP Server with Rune tools
//!
//! This server combines:
//! - **`CrucibleMcpServer`** (12 tools): Note, Search, and Kiln operations
//! - **`RuneTools`** (dynamic): Scripts from configured plugins/ directories
//! - **`StructPlugins`** (dynamic): Struct-based plugins like `just.rn`
//!
//! All responses are formatted with TOON for token efficiency.
//!
//! ## Plugin Discovery
//!
//! Plugins are discovered from:
//! - Global personal: `~/.config/crucible/plugins/`
//! - Kiln personal: `KILN/.crucible/plugins/` (gitignored)
//! - Kiln shared: `KILN/plugins/` (version-controlled)
//!
//! Plugins use `#[tool(...)]` and `#[handler(...)]` attributes to register
//! tools and event handlers respectively. Struct-based plugins use
//! `#[plugin(...)]` for stateful tools with file watching.

use crate::output_filter::{filter_test_output, FilterConfig};
use crate::toon_response::toon_success_smart;
use crate::CrucibleMcpServer;
use crucible_config::ConfigResolver;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::events::{SessionEvent, ToolProvider};
use crucible_core::traits::KnowledgeRepository;
use crucible_rune::{
    builtin_handlers::{create_test_filter_handler, BuiltinHandlersConfig},
    event_bus::EventBus,
    mcp_gateway::McpGatewayManager,
    ContentBlock, EventHandler, EventHandlerConfig, EventPipeline, PluginLoader,
    RuneDiscoveryConfig, RuneToolRegistry, StructPluginHandle, ToolResultEvent,
};
use rmcp::model::{CallToolResult, Content, Tool};
use rmcp::service::RequestContext;
use rmcp::ServerHandler;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Extended MCP server exposing all Crucible tools plus Rune
///
/// This server aggregates tools from multiple sources:
/// - **Kiln tools** (12): `NoteTools`, `SearchTools`, `KilnTools` via `CrucibleMcpServer`
/// - **Rune tools** (dynamic): Scripts from runes/ directories prefixed with `rune_`
/// - **Struct plugins** (dynamic): Struct-based plugins like `just.rn` for `just_*` tools
/// - **Upstream MCP tools** (dynamic): Tools from external MCP servers via gateway
///
/// Struct-based plugins (e.g., `just.rn`) provide tools that integrate with file watching.
pub struct ExtendedMcpServer {
    /// Core Crucible MCP server with 12 kiln tools
    kiln_server: CrucibleMcpServer,
    /// Rune script registry (hook-based plugins with `#[tool]` attribute)
    rune_registry: Arc<RuneToolRegistry>,
    /// Struct-based plugin handle (plugins with `#[plugin]` attribute)
    /// Thread-safe handle to the Rune plugin thread
    struct_plugins: Arc<StructPluginHandle>,
    /// Event handler for recipe enrichment (DEPRECATED: use `event_bus` with tool:discovered hook)
    #[allow(dead_code)]
    event_handler: Option<Arc<EventHandler>>,
    /// Event pipeline for filtering tool output (Rune plugins)
    event_pipeline: Option<EventPipeline>,
    /// Configuration for built-in output filtering
    filter_config: FilterConfig,
    /// Unified event bus for all events
    event_bus: Arc<RwLock<EventBus>>,
    /// Upstream MCP server connections (None until explicitly configured)
    upstream_clients: Option<Arc<McpGatewayManager>>,
}

#[allow(
    clippy::missing_errors_doc,
    clippy::too_many_lines,
    clippy::missing_panics_doc,
    clippy::unused_self,
    clippy::cast_possible_truncation
)]
impl ExtendedMcpServer {
    /// Create a new extended MCP server
    ///
    /// # Arguments
    ///
    /// * `kiln_path` - Path to the kiln directory
    /// * `knowledge_repo` - Repository for semantic search
    /// * `embedding_provider` - Provider for generating embeddings
    /// * `plugin_dir` - Directory containing plugins (usually PWD/plugins or kiln/plugins)
    /// * `rune_config` - Configuration for Rune tool discovery
    pub async fn new(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        plugin_dir: impl AsRef<Path>,
        rune_config: RuneDiscoveryConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Create core kiln server
        let kiln_server =
            CrucibleMcpServer::new(kiln_path.clone(), knowledge_repo, embedding_provider);

        let plugin_dir = plugin_dir.as_ref().to_path_buf();

        // Load configuration and get shell policy for plugin security
        // Use kiln_path as workspace root for three-tier config resolution
        let shell_policy = match ConfigResolver::for_workspace(&kiln_path) {
            Ok(resolver) => {
                let policy = resolver.shell_policy();
                info!(
                    "Loaded shell policy: {} whitelisted commands, {} blacklisted",
                    policy.whitelist.len(),
                    policy.blacklist.len()
                );
                policy
            }
            Err(e) => {
                warn!("Failed to load workspace config, using defaults: {}", e);
                crucible_config::ShellPolicy::with_defaults()
            }
        };

        // Create struct-based plugin handle (for plugins with #[plugin] attribute)
        // The handle spawns a dedicated Rune thread for thread-safe plugin execution
        let struct_plugins = {
            let handle = StructPluginHandle::new(shell_policy)
                .map_err(|e| format!("Failed to create struct plugin handle: {e}"))?;

            // Load plugins from kiln/plugins/ directory
            let kiln_plugins = PathBuf::from(&kiln_path).join("plugins");
            if kiln_plugins.exists() {
                if let Err(e) = handle.load_from_directory(&kiln_plugins).await {
                    warn!(
                        "Failed to load plugins from {}: {}",
                        kiln_plugins.display(),
                        e
                    );
                } else {
                    let tool_count = handle.all_tools().await.len();
                    if tool_count > 0 {
                        info!(
                            "Loaded {} struct plugin tools from {}",
                            tool_count,
                            kiln_plugins.display()
                        );
                    }
                }
            }

            // Also try the provided plugin_dir if different
            if plugin_dir.exists() && plugin_dir != kiln_plugins {
                if let Err(e) = handle.load_from_directory(&plugin_dir).await {
                    warn!(
                        "Failed to load plugins from {}: {}",
                        plugin_dir.display(),
                        e
                    );
                } else {
                    let tool_count = handle.all_tools().await.len();
                    info!(
                        "Loaded {} struct plugin tools from {}",
                        tool_count,
                        plugin_dir.display()
                    );
                }
            }

            Arc::new(handle)
        };

        // Create Rune registry (for hook-based plugins with #[tool] attribute)
        let rune_registry = Arc::new(RuneToolRegistry::discover_from(rune_config).await?);
        let rune_count = rune_registry.tool_count().await;
        info!("Loaded {} Rune tools", rune_count);

        // Create event handler for recipe enrichment
        let event_handler =
            match EventHandler::new(EventHandlerConfig::with_defaults(Some(&plugin_dir))) {
                Ok(handler) => {
                    // Ensure event directories exist
                    if let Err(e) = handler.ensure_event_directories(&["recipe_discovered"]) {
                        warn!("Failed to ensure event directories: {}", e);
                    }
                    info!("Recipe event handler initialized");
                    Some(Arc::new(handler))
                }
                Err(e) => {
                    warn!("Failed to create event handler: {}", e);
                    None
                }
            };

        // Create event pipeline for filtering tool output
        let event_pipeline = {
            let runes_plugins = plugin_dir.join("runes").join("plugins");
            if runes_plugins.exists() {
                match PluginLoader::new(&runes_plugins) {
                    Ok(mut loader) => {
                        if let Err(e) = loader.load_plugins().await {
                            warn!("Failed to load plugins: {}", e);
                        }
                        let hook_count = loader.hooks().len();
                        if hook_count > 0 {
                            info!(
                                "Loaded {} plugin hooks from {}",
                                hook_count,
                                runes_plugins.display()
                            );
                        }
                        Some(EventPipeline::new(Arc::new(RwLock::new(loader))))
                    }
                    Err(e) => {
                        warn!("Failed to create plugin loader: {}", e);
                        None
                    }
                }
            } else {
                debug!("No plugins directory at {}", runes_plugins.display());
                None
            }
        };

        // Create unified event bus and register built-in handlers
        let event_bus = {
            let mut bus = EventBus::new();

            // Register all built-in handlers
            let builtin_config = BuiltinHandlersConfig::default();

            if builtin_config.test_filter.enabled {
                bus.register(create_test_filter_handler(&builtin_config.test_filter));
                info!("Registered builtin:test_filter handler");
            }

            if builtin_config.recipe_enrichment.enabled {
                bus.register(
                    crucible_rune::builtin_handlers::create_recipe_enrichment_handler(
                        &builtin_config.recipe_enrichment,
                    ),
                );
                info!("Registered builtin:recipe_enrichment handler");
            }

            Arc::new(RwLock::new(bus))
        };

        Ok(Self {
            kiln_server,
            rune_registry,
            struct_plugins,
            event_handler,
            event_pipeline,
            filter_config: FilterConfig::default(),
            event_bus,
            upstream_clients: None,
        })
    }

    /// Create server without Rune tools (kiln only)
    pub fn kiln_only(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        let kiln_server = CrucibleMcpServer::new(kiln_path, knowledge_repo, embedding_provider);
        let rune_registry = Arc::new(
            RuneToolRegistry::new(RuneDiscoveryConfig::default())
                .expect("Failed to create empty Rune registry"),
        );
        let struct_plugins = Arc::new(
            StructPluginHandle::new(crucible_config::ShellPolicy::with_defaults())
                .expect("Failed to create empty struct plugin handle"),
        );

        Self {
            kiln_server,
            rune_registry,
            struct_plugins,
            event_handler: None,
            event_pipeline: None,
            filter_config: FilterConfig::default(),
            event_bus: Arc::new(RwLock::new(EventBus::new())),
            upstream_clients: None,
        }
    }

    /// Get reference to the kiln server
    #[must_use]
    pub fn kiln_server(&self) -> &CrucibleMcpServer {
        &self.kiln_server
    }

    /// Get reference to Rune registry
    #[must_use]
    pub fn rune_registry(&self) -> &RuneToolRegistry {
        &self.rune_registry
    }

    /// Get reference to struct-based plugin handle
    #[must_use]
    pub fn struct_plugins(&self) -> Arc<StructPluginHandle> {
        Arc::clone(&self.struct_plugins)
    }

    /// Get reference to the event bus
    #[must_use]
    pub fn event_bus(&self) -> Arc<RwLock<EventBus>> {
        Arc::clone(&self.event_bus)
    }

    /// Get reference to upstream MCP clients
    pub fn upstream_clients(&self) -> Option<Arc<McpGatewayManager>> {
        self.upstream_clients.as_ref().map(Arc::clone)
    }

    /// Set upstream MCP clients (builder pattern)
    ///
    /// This allows adding upstream MCP server connections after creation.
    /// The manager should be configured to use the same event bus as this server.
    #[must_use]
    pub fn with_upstream_clients(mut self, clients: Arc<McpGatewayManager>) -> Self {
        self.upstream_clients = Some(clients);
        self
    }

    /// List all available tools from all sources
    ///
    /// Struct plugin tools are enriched via tool:discovered hooks before being returned.
    pub async fn list_all_tools(&self) -> Vec<Tool> {
        let mut tools = self.kiln_server.list_tools();

        tools.extend(Self::discovery_tools());

        // Add struct plugin tools (just.rn and similar plugins)
        for tool_def in self.struct_plugins.all_tools().await {
            // Convert ToolDefinition to rmcp Tool
            tools.push(self.mcp_tool_from_struct_plugin(&tool_def));
        }

        // Add Rune tools (hook-based plugins)
        for rt in self.rune_registry.list_tools().await {
            tools.push(self.mcp_tool_from_rune(&rt));
        }

        // Add upstream MCP tools if available
        if let Some(gateway) = &self.upstream_clients {
            for upstream_tool in gateway.all_tools().await {
                // Emit tool:discovered event for upstream tools
                self.emit_upstream_tool_discovered(&upstream_tool).await;
                tools.push(self.mcp_tool_from_upstream(&upstream_tool));
            }
        }

        tools
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
                                "enum": ["builtin", "rune", "just", "upstream"],
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

    /// Convert a struct plugin `ToolDefinition` to `rmcp::model::Tool`
    fn mcp_tool_from_struct_plugin(&self, td: &crucible_rune::ToolDefinition) -> Tool {
        // Build JSON schema from tool parameters
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for param in &td.parameters {
            properties.insert(
                param.name.clone(),
                json!({
                    "type": "string",
                    "description": param.description.clone()
                }),
            );
            if param.required {
                required.push(param.name.clone());
            }
        }

        let schema = serde_json::Map::from_iter([
            ("type".to_string(), json!("object")),
            ("properties".to_string(), Value::Object(properties)),
            ("required".to_string(), json!(required)),
        ]);

        Tool {
            name: td.name.clone().into(),
            title: None,
            description: Some(td.description.clone().into()),
            input_schema: Arc::new(schema),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }

    /// Convert `crucible_rune::RuneTool` to `rmcp::model::Tool`
    fn mcp_tool_from_rune(&self, rt: &crucible_rune::RuneTool) -> Tool {
        let schema = rt.input_schema.as_object().cloned().unwrap_or_default();
        Tool {
            name: format!("rune_{}", rt.name).into(),
            title: None,
            description: Some(rt.description.clone().into()),
            input_schema: Arc::new(schema),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }

    /// Convert `crucible_rune::mcp_gateway::UpstreamTool` to `rmcp::model::Tool`
    fn mcp_tool_from_upstream(&self, ut: &crucible_rune::mcp_gateway::UpstreamTool) -> Tool {
        let schema = ut.input_schema.as_object().cloned().unwrap_or_default();
        Tool {
            name: ut.prefixed_name.clone().into(),
            title: None,
            description: ut.description.clone().map(std::convert::Into::into),
            input_schema: Arc::new(schema),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }

    /// Emit tool:discovered event for upstream tool
    async fn emit_upstream_tool_discovered(&self, tool: &crucible_rune::mcp_gateway::UpstreamTool) {
        let bus = self.event_bus.read().await;

        // Emit SessionEvent::ToolDiscovered for upstream tool
        let event = SessionEvent::ToolDiscovered {
            name: tool.prefixed_name.clone(),
            source: ToolProvider::Mcp {
                server: tool.upstream.clone(),
            },
            schema: Some(tool.input_schema.clone()),
        };

        let (_result_event, _ctx, errors) = bus.emit_session(event);

        if !errors.is_empty() {
            for e in &errors {
                warn!("Hook error during tool:discovered for upstream tool: {}", e);
            }
        }
    }

    /// Get total tool count
    pub async fn tool_count(&self) -> usize {
        let kiln = self.kiln_server.tool_count();
        let discovery = Self::discovery_tools().len();
        let struct_plugins = self.struct_plugins.all_tools().await.len();
        let rune = self.rune_registry.tool_count().await;
        kiln + discovery + struct_plugins + rune
    }

    /// Check if a tool name is handled by Rune
    #[must_use]
    pub fn is_rune_tool(name: &str) -> bool {
        name.starts_with("rune_")
    }

    /// Check if a tool is provided by struct plugins
    ///
    /// This checks the `struct_plugins` handle for the tool by name.
    pub async fn has_struct_plugin_tool(&self, name: &str) -> bool {
        self.struct_plugins.has_tool(name).await
    }

    /// Execute a struct plugin tool (e.g., just_* tools from just.rn)
    ///
    /// Output is filtered in two stages:
    /// 1. **Built-in filter**: Automatically extracts test summaries from cargo test,
    ///    pytest, jest, go test, etc. This is always applied for recognized test output.
    /// 2. **Rune pipeline**: If configured, custom plugins can further transform output.
    ///
    /// This makes test output much more useful for LLMs by removing verbose per-test
    /// lines and keeping only pass/fail summaries and error details.
    pub async fn call_struct_plugin_tool(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let start = Instant::now();

        debug!(
            "Executing struct plugin tool: {} with args: {:?}",
            name, arguments
        );

        // Emit SessionEvent::ToolCalled (before tool execution)
        {
            let bus = self.event_bus.read().await;
            let event = SessionEvent::ToolCalled {
                name: name.to_string(),
                args: arguments.clone(),
            };
            let (_result_event, ctx, errors) = bus.emit_session(event);

            if !errors.is_empty() {
                for e in &errors {
                    warn!("Hook error during tool:called: {}", e);
                }
            }

            // Check if execution was cancelled via context
            if ctx.is_cancelled() {
                return Err(rmcp::ErrorData::internal_error(
                    format!("Struct plugin tool '{name}' execution cancelled by hook"),
                    None,
                ));
            }
        }

        // Dispatch to struct plugin handle
        let result = self.struct_plugins.dispatch(name, arguments.clone()).await;

        match result {
            Ok(result_value) => {
                let duration_ms = start.elapsed().as_millis() as u64;

                // Check for error in result
                let is_error = result_value.get("error").is_some_and(|v| !v.is_null());

                let exit_code = result_value
                    .get("exit_code")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0);
                let is_error = is_error || exit_code != 0;

                // Get output for filtering
                let output = result_value
                    .get("stdout")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                // Stage 1: Apply built-in test output filter
                let filtered_output = if self.filter_config.filter_test_output {
                    if let Some(filtered) = filter_test_output(&output) {
                        debug!(
                            "Built-in filter reduced output from {} to {} chars",
                            output.len(),
                            filtered.len()
                        );
                        filtered
                    } else {
                        output.clone()
                    }
                } else {
                    output.clone()
                };

                // Stage 2: Process through Rune event pipeline if available
                let final_output = if let Some(pipeline) = &self.event_pipeline {
                    let event = ToolResultEvent {
                        tool_name: name.to_string(),
                        arguments: arguments.clone(),
                        is_error,
                        content: vec![ContentBlock::Text {
                            text: filtered_output.clone(),
                        }],
                        duration_ms,
                    };

                    match pipeline.process_tool_result(event).await {
                        Ok(processed) => processed.text_content(),
                        Err(e) => {
                            warn!("Event pipeline error: {}, using filtered output", e);
                            filtered_output
                        }
                    }
                } else {
                    filtered_output
                };

                // Build response with filtered output
                let mut response = result_value.clone();
                if let Some(obj) = response.as_object_mut() {
                    obj.insert("stdout".to_string(), json!(final_output));
                }

                // Emit SessionEvent::ToolCompleted with the response
                {
                    let bus = self.event_bus.read().await;
                    let event = SessionEvent::ToolCompleted {
                        name: name.to_string(),
                        result: final_output.clone(),
                        error: if is_error {
                            Some("Tool returned error".to_string())
                        } else {
                            None
                        },
                    };

                    let (_result_event, _ctx, errors) = bus.emit_session(event);

                    if !errors.is_empty() {
                        for e in &errors {
                            warn!("Hook error during tool:completed: {}", e);
                        }
                    }
                }

                Ok(toon_success_smart(response))
            }
            Err(e) => {
                // Emit SessionEvent::ToolCompleted with error
                {
                    let bus = self.event_bus.read().await;
                    let event = SessionEvent::ToolCompleted {
                        name: name.to_string(),
                        result: String::new(),
                        error: Some(e.to_string()),
                    };

                    let (_result_event, _ctx, _errors) = bus.emit_session(event);
                }

                Err(rmcp::ErrorData::internal_error(
                    format!("Struct plugin tool '{name}' failed: {e}"),
                    None,
                ))
            }
        }
    }

    /// Execute a Rune tool and return the result directly
    ///
    /// Returns the raw result value for simple types (strings, numbers, bools).
    /// Only uses TOON formatting for structured JSON objects/arrays.
    pub async fn call_rune_tool(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let start = Instant::now();

        debug!("Executing Rune tool: {} with args: {:?}", name, arguments);

        // Emit SessionEvent::ToolCalled (before tool execution)
        {
            let bus = self.event_bus.read().await;
            let event = SessionEvent::ToolCalled {
                name: name.to_string(),
                args: arguments.clone(),
            };
            let (_result_event, ctx, errors) = bus.emit_session(event);

            if !errors.is_empty() {
                for e in &errors {
                    warn!("Hook error during tool:called: {}", e);
                }
            }

            // Check if execution was cancelled via context
            if ctx.is_cancelled() {
                return Err(rmcp::ErrorData::internal_error(
                    format!("Rune tool '{name}' execution cancelled by hook"),
                    None,
                ));
            }
        }

        match self.rune_registry.execute(name, arguments).await {
            Ok(result) => {
                let _duration_ms = start.elapsed().as_millis() as u64;

                if result.success {
                    // Emit SessionEvent::ToolCompleted
                    let result_text = match &result.result {
                        Some(v) => serde_json::to_string(v).unwrap_or_default(),
                        None => String::new(),
                    };

                    {
                        let bus = self.event_bus.read().await;
                        let event = SessionEvent::ToolCompleted {
                            name: name.to_string(),
                            result: result_text.clone(),
                            error: None,
                        };

                        let (_result_event, _ctx, errors) = bus.emit_session(event);

                        if !errors.is_empty() {
                            for e in &errors {
                                warn!("Hook error during tool:completed: {}", e);
                            }
                        }
                    }

                    // Return result directly - only use TOON for structured data
                    match &result.result {
                        Some(Value::Object(_) | Value::Array(_)) => {
                            // Structured data - use TOON
                            Ok(toon_success_smart(result.result.unwrap_or(Value::Null)))
                        }
                        Some(Value::String(s)) => {
                            // Plain string - return as-is
                            Ok(CallToolResult::success(vec![Content::text(s.clone())]))
                        }
                        Some(Value::Number(n)) => {
                            // Number - return as string representation
                            Ok(CallToolResult::success(vec![Content::text(n.to_string())]))
                        }
                        Some(Value::Bool(b)) => {
                            // Bool - return as string representation
                            Ok(CallToolResult::success(vec![Content::text(b.to_string())]))
                        }
                        Some(Value::Null) | None => {
                            // Null/empty - return empty success
                            Ok(CallToolResult::success(vec![]))
                        }
                    }
                } else {
                    let error_msg = result.error.unwrap_or_else(|| "Unknown error".to_string());

                    // Emit SessionEvent::ToolCompleted with error
                    {
                        let bus = self.event_bus.read().await;
                        let event = SessionEvent::ToolCompleted {
                            name: name.to_string(),
                            result: String::new(),
                            error: Some(error_msg.clone()),
                        };

                        let (_result_event, _ctx, _errors) = bus.emit_session(event);
                    }

                    // Error - return as error message
                    Err(rmcp::ErrorData::internal_error(
                        format!("Rune tool '{name}' failed: {error_msg}"),
                        None,
                    ))
                }
            }
            Err(e) => {
                // Emit SessionEvent::ToolCompleted with error
                {
                    let bus = self.event_bus.read().await;
                    let event = SessionEvent::ToolCompleted {
                        name: name.to_string(),
                        result: String::new(),
                        error: Some(e.to_string()),
                    };

                    let (_result_event, _ctx, _errors) = bus.emit_session(event);
                }

                Err(rmcp::ErrorData::internal_error(
                    format!("Rune tool '{name}' failed: {e}"),
                    None,
                ))
            }
        }
    }

    /// Refresh struct plugins (reload plugins and rediscover tools)
    pub async fn refresh_struct_plugins(
        &self,
        plugin_dir: &Path,
    ) -> Result<(), crucible_rune::RuneError> {
        self.struct_plugins.load_from_directory(plugin_dir).await
    }

    /// Refresh Rune tools (re-discover scripts)
    pub async fn refresh_rune(&self) -> Result<usize, crucible_rune::RuneError> {
        self.rune_registry.discover().await
    }

    /// Execute an upstream MCP tool and return the result
    pub async fn call_upstream_tool(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let gateway = self.upstream_clients.as_ref().ok_or_else(|| {
            rmcp::ErrorData::internal_error("No upstream MCP clients configured".to_string(), None)
        })?;

        match gateway.call_tool(name, arguments).await {
            Ok(result) => {
                // Convert ToolCallResult to CallToolResult
                let content_blocks: Vec<Content> = result
                    .content
                    .into_iter()
                    .map(|block| match block {
                        crucible_rune::mcp_gateway::ContentBlock::Text { text } => {
                            Content::text(text)
                        }
                        crucible_rune::mcp_gateway::ContentBlock::Image { data, mime_type } => {
                            Content::image(data, mime_type)
                        }
                        crucible_rune::mcp_gateway::ContentBlock::Resource { uri, text } => {
                            // ResourceContents needs text and uri
                            use rmcp::model::ResourceContents;
                            let resource_contents = if let Some(text_content) = text {
                                ResourceContents::text(text_content, uri)
                            } else {
                                ResourceContents::text("", uri)
                            };
                            Content::resource(resource_contents)
                        }
                    })
                    .collect();

                if result.is_error {
                    Ok(CallToolResult::error(content_blocks))
                } else {
                    Ok(CallToolResult::success(content_blocks))
                }
            }
            Err(e) => Err(rmcp::ErrorData::internal_error(
                format!("Upstream tool '{name}' failed: {e}"),
                None,
            )),
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

    /// Serve via SSE transport on the specified address
    ///
    /// Returns when the server receives a shutdown signal (Ctrl+C).
    pub async fn serve_sse(self, addr: std::net::SocketAddr) -> Result<(), anyhow::Error> {
        use rmcp::transport::SseServer;

        let sse_server = SseServer::serve(addr).await?;
        let _ct = sse_server.with_service(move || self.clone());

        // Wait for shutdown signal
        tokio::signal::ctrl_c().await?;
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
                "Crucible MCP server exposing kiln tools (notes, search, metadata), \
                Just recipes, and Rune scripts for knowledge management."
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

        if self.inner.has_struct_plugin_tool(name).await {
            self.inner.call_struct_plugin_tool(name, arguments).await
        } else if ExtendedMcpServer::is_rune_tool(name) {
            self.inner.call_rune_tool(name, arguments).await
        } else {
            if let Some(gateway) = self.inner.upstream_clients() {
                if gateway.find_client_for_tool(name).await.is_some() {
                    return self.inner.call_upstream_tool(name, arguments).await;
                }
            }
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
        let rune_config = RuneDiscoveryConfig::default();

        let server = ExtendedMcpServer::new(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
            temp.path(),
            rune_config,
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
        // Rune tools use rune_ prefix
        assert!(ExtendedMcpServer::is_rune_tool("rune_summarize"));
        assert!(ExtendedMcpServer::is_rune_tool("rune_transform"));
        assert!(!ExtendedMcpServer::is_rune_tool("just_build"));
        assert!(!ExtendedMcpServer::is_rune_tool("read_note"));

        // Note: struct plugin tools (like just_*) are now dynamically resolved
        // via has_struct_plugin_tool(), not static prefix matching
    }

    #[test]
    fn test_extended_server_has_event_bus() {
        use crucible_rune::event_bus::EventType;

        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let server = ExtendedMcpServer::kiln_only(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );

        // Verify event_bus field exists and is accessible
        let event_bus = server.event_bus();
        assert!(Arc::strong_count(&event_bus) >= 2); // At least server + this reference

        // Verify we can read the bus (it should be initially empty)
        let bus = event_bus.blocking_read();
        assert_eq!(bus.count_handlers(EventType::ToolBefore), 0);
        assert_eq!(bus.count_handlers(EventType::ToolAfter), 0);
    }

    #[tokio::test]
    async fn test_event_bus_can_register_handlers() {
        use crucible_rune::event_bus::{Event, EventType, Handler};
        use serde_json::json;

        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let server = ExtendedMcpServer::kiln_only(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );

        let event_bus = server.event_bus();

        // Register a handler
        {
            let mut bus = event_bus.write().await;
            bus.register(Handler::new(
                "test_handler",
                EventType::ToolAfter,
                "test_*",
                |_ctx, event| Ok(event),
            ));
        }

        // Verify handler was registered
        {
            let bus = event_bus.read().await;
            assert_eq!(bus.count_handlers(EventType::ToolAfter), 1);

            let handler = bus.get_handler("test_handler");
            assert!(handler.is_some());
            assert_eq!(handler.unwrap().name, "test_handler");
        }

        // Verify we can emit events through the bus
        {
            let bus = event_bus.read().await;
            let event = Event::tool_after("test_tool", json!({"result": "success"}));
            let (result, _ctx, errors) = bus.emit(event);

            assert_eq!(result.identifier, "test_tool");
            assert!(errors.is_empty());
        }
    }

    // Note: test_tool_events_emitted removed - requires RequestContext which can't be
    // easily constructed in tests. Event emission is tested via integration tests.

    #[tokio::test]
    async fn test_test_filter_hook_registered() {
        use crucible_rune::event_bus::EventType;

        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let server = ExtendedMcpServer::new(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
            temp.path(),
            RuneDiscoveryConfig::default(),
        )
        .await
        .unwrap();

        let event_bus = server.event_bus();

        // Check if test_filter hook is registered
        {
            let bus = event_bus.read().await;
            let handler = bus.get_handler("builtin:test_filter");
            assert!(
                handler.is_some(),
                "builtin:test_filter hook should be registered"
            );
            assert_eq!(handler.unwrap().event_type, EventType::ToolAfter);
        }
    }

    #[tokio::test]
    #[ignore = "Requires `just` binary to be installed"]
    async fn test_tool_discovered_event_emitted() {
        use crucible_rune::event_bus::{EventType, Handler};
        use std::sync::atomic::{AtomicUsize, Ordering};

        let temp = TempDir::new().unwrap();

        // Create a justfile with a test recipe
        let justfile_path = temp.path().join("justfile");
        std::fs::write(&justfile_path, "test:\n\techo 'Running tests'\n").unwrap();

        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let server = ExtendedMcpServer::new(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
            temp.path(),
            RuneDiscoveryConfig::default(),
        )
        .await
        .unwrap();

        let event_bus = server.event_bus();

        // Track tool:discovered events
        let discovered_count = Arc::new(AtomicUsize::new(0));
        let discovered_count_clone = Arc::clone(&discovered_count);

        {
            let mut bus = event_bus.write().await;
            bus.register(Handler::new(
                "test_tool_discovered_counter",
                EventType::ToolDiscovered,
                "rune_*",
                move |_ctx, event| {
                    discovered_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(event)
                },
            ));
        }

        // List tools - should emit tool:discovered events
        let tools = server.list_all_tools().await;

        // Should have at least one rune tool
        let rune_tools: Vec<_> = tools
            .iter()
            .filter(|t| t.name.as_ref().starts_with("rune_"))
            .collect();
        assert!(!rune_tools.is_empty(), "Should have at least one rune tool");

        // Should have emitted tool:discovered events for all rune tools
        assert_eq!(
            discovered_count.load(Ordering::SeqCst),
            rune_tools.len(),
            "Should emit tool:discovered event for each rune tool"
        );
    }

    #[test]
    fn test_upstream_clients_initially_none() {
        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let server = ExtendedMcpServer::kiln_only(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );

        assert!(
            server.upstream_clients().is_none(),
            "upstream_clients should be None initially"
        );
    }

    #[test]
    fn test_with_upstream_clients_sets_manager() {
        use crucible_rune::mcp_gateway::McpGatewayManager;

        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let server = ExtendedMcpServer::kiln_only(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );

        // Create a manager with a new event bus
        let bus = EventBus::new();
        let manager = Arc::new(McpGatewayManager::new(bus));

        // Use the builder method to set upstream clients
        let server = server.with_upstream_clients(Arc::clone(&manager));

        // Verify the manager is set
        assert!(
            server.upstream_clients().is_some(),
            "upstream_clients should be set"
        );

        // Verify it's the same manager (Arc pointer equality)
        let retrieved = server.upstream_clients().unwrap();
        assert!(
            Arc::ptr_eq(&retrieved, &manager),
            "Should return the same Arc instance"
        );
    }

    #[tokio::test]
    async fn test_list_all_tools_includes_upstream() {
        use crucible_rune::mcp_gateway::{
            McpGatewayManager, TransportConfig, UpstreamConfig, UpstreamTool,
        };
        use serde_json::json;

        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let server = ExtendedMcpServer::kiln_only(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );

        // Create a manager with a mock upstream client
        let bus = EventBus::new();
        let mut manager = McpGatewayManager::new(bus);

        // Add a mock upstream config
        let config = UpstreamConfig {
            name: "test_upstream".to_string(),
            transport: TransportConfig::Stdio {
                command: "echo".to_string(),
                args: vec![],
                env: vec![],
            },
            prefix: Some("upstream_".to_string()),
            allowed_tools: None,
            blocked_tools: None,
            auto_reconnect: false,
        };

        let client = manager.add_client(config);

        // Manually add some mock tools to the client
        let mock_tool = UpstreamTool {
            name: "test_tool".to_string(),
            prefixed_name: "upstream_test_tool".to_string(),
            description: Some("Test upstream tool".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "input": {"type": "string"}
                }
            }),
            upstream: "test_upstream".to_string(),
        };

        client.update_tools(vec![mock_tool]).await;

        // Set the manager on the server
        let server = server.with_upstream_clients(Arc::new(manager));

        // List all tools
        let tools = server.list_all_tools().await;

        // Should have kiln tools + upstream tools
        let upstream_tools: Vec<_> = tools
            .iter()
            .filter(|t| t.name.as_ref().starts_with("upstream_"))
            .collect();
        assert_eq!(upstream_tools.len(), 1, "Should have 1 upstream tool");
        assert_eq!(upstream_tools[0].name.as_ref(), "upstream_test_tool");
    }

    #[tokio::test]
    async fn test_upstream_tool_routing() {
        use crucible_rune::mcp_gateway::{
            McpGatewayManager, TransportConfig, UpstreamConfig, UpstreamTool,
        };
        use serde_json::json;

        let temp = TempDir::new().unwrap();
        let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
        let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

        let server = ExtendedMcpServer::kiln_only(
            temp.path().to_str().unwrap().to_string(),
            knowledge_repo,
            embedding_provider,
        );

        // Create a manager with a mock upstream client
        let bus = EventBus::new();
        let mut manager = McpGatewayManager::new(bus);

        let config = UpstreamConfig {
            name: "test_upstream".to_string(),
            transport: TransportConfig::Stdio {
                command: "echo".to_string(),
                args: vec![],
                env: vec![],
            },
            prefix: Some("gh_".to_string()),
            allowed_tools: None,
            blocked_tools: None,
            auto_reconnect: false,
        };

        let client = manager.add_client(config);

        // Add a mock tool
        let mock_tool = UpstreamTool {
            name: "search_repos".to_string(),
            prefixed_name: "gh_search_repos".to_string(),
            description: Some("Search repositories".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"}
                }
            }),
            upstream: "test_upstream".to_string(),
        };

        client.update_tools(vec![mock_tool]).await;

        let server = server.with_upstream_clients(Arc::new(manager));

        // Verify the tool is discoverable
        let tools = server.list_all_tools().await;
        let gh_tool = tools.iter().find(|t| t.name.as_ref() == "gh_search_repos");
        assert!(gh_tool.is_some(), "Should find gh_search_repos tool");

        // Verify upstream tool can be found via gateway
        let gateway = server.upstream_clients().unwrap();
        assert!(
            gateway
                .find_client_for_tool("gh_search_repos")
                .await
                .is_some(),
            "gh_search_repos should be routable via gateway"
        );

        // Non-upstream tools should not be found in gateway
        assert!(
            gateway.find_client_for_tool("rune_test").await.is_none(),
            "rune_ tools should not be in gateway"
        );
        assert!(
            gateway
                .find_client_for_tool("search_by_content")
                .await
                .is_none(),
            "kiln tools should not be in gateway"
        );
    }
}
