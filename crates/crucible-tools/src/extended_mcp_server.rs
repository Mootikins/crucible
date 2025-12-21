//! Extended MCP Server with Just and Rune tools
//!
//! This server combines:
//! - **`CrucibleMcpServer`** (12 tools): Note, Search, and Kiln operations
//! - **`JustTools`** (dynamic): Recipes from justfile in PWD
//! - **`RuneTools`** (dynamic): Scripts from configured runes/ directories
//!
//! All responses are formatted with TOON for token efficiency.
//!
//! ## Recipe Enrichment
//!
//! Just recipes are automatically enriched by Rune event handlers:
//! - Scripts in `runes/events/recipe_discovered/` are executed for each recipe
//! - Handlers can add category, tags, priority, and custom metadata
//! - Enrichment is visible in tool descriptions and schema annotations

use crate::clustering::ClusteringTools;
use crate::output_filter::{filter_test_output, FilterConfig};
use crate::toon_response::toon_success_smart;
use crate::CrucibleMcpServer;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::events::{SessionEvent, ToolSource as CoreToolSource};
use crucible_core::traits::KnowledgeRepository;
use crucible_just::JustTools;
use crucible_rune::{
    builtin_hooks::{create_test_filter_hook, BuiltinHooksConfig},
    event_bus::EventBus,
    mcp_gateway::McpGatewayManager,
    ContentBlock, EnrichedRecipe, EventHandler, EventHandlerConfig, EventPipeline, PluginLoader,
    RuneDiscoveryConfig, RuneToolRegistry, ToolResultEvent,
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

/// Extended MCP server exposing all Crucible tools plus Just and Rune
///
/// This server aggregates tools from multiple sources:
/// - **Kiln tools** (12): `NoteTools`, `SearchTools`, `KilnTools` via `CrucibleMcpServer`
/// - **Clustering tools** (3): `MoC` detection and document clustering tools
/// - **Just tools** (dynamic): Recipes from justfile prefixed with `just_`
/// - **Rune tools** (dynamic): Scripts from runes/ directories prefixed with `rune_`
/// - **Upstream MCP tools** (dynamic): Tools from external MCP servers via gateway
///
/// Just recipes are automatically enriched via event handlers before being exposed.
pub struct ExtendedMcpServer {
    /// Core Crucible MCP server with 12 kiln tools
    kiln_server: CrucibleMcpServer,
    /// Clustering tools for knowledge base organization
    clustering_tools: Arc<ClusteringTools>,
    /// Just recipe executor
    just_tools: Arc<JustTools>,
    /// Rune script registry
    rune_registry: Arc<RuneToolRegistry>,
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

impl ExtendedMcpServer {
    /// Create a new extended MCP server
    ///
    /// # Arguments
    ///
    /// * `kiln_path` - Path to the kiln directory
    /// * `knowledge_repo` - Repository for semantic search
    /// * `embedding_provider` - Provider for generating embeddings
    /// * `just_dir` - Directory containing justfile (usually PWD)
    /// * `rune_config` - Configuration for Rune tool discovery
    pub async fn new(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        just_dir: impl AsRef<Path>,
        rune_config: RuneDiscoveryConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Create core kiln server
        let kiln_server =
            CrucibleMcpServer::new(kiln_path.clone(), knowledge_repo, embedding_provider);

        // Create clustering tools
        let clustering_tools = Arc::new(ClusteringTools::new(PathBuf::from(kiln_path)));

        // Create Just tools wrapper
        let just_dir = just_dir.as_ref().to_path_buf();
        let just_tools = Arc::new(JustTools::new(&just_dir));
        if just_tools.has_justfile() {
            if let Err(e) = just_tools.refresh().await {
                warn!("Failed to load justfile: {}", e);
            } else {
                let count = just_tools.tool_count().await.unwrap_or(0);
                info!("Loaded {} Just recipes", count);
            }
        }

        // Create Rune registry
        let rune_registry = Arc::new(RuneToolRegistry::discover_from(rune_config).await?);
        let rune_count = rune_registry.tool_count().await;
        info!("Loaded {} Rune tools", rune_count);

        // Create event handler for recipe enrichment
        // Looks in ~/.crucible/runes/events/ and {just_dir}/runes/events/
        let event_handler =
            match EventHandler::new(EventHandlerConfig::with_defaults(Some(&just_dir))) {
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
        // Looks for plugins in {just_dir}/runes/plugins/
        let event_pipeline = {
            let plugin_dir = just_dir.join("runes").join("plugins");
            if plugin_dir.exists() {
                match PluginLoader::new(&plugin_dir) {
                    Ok(mut loader) => {
                        if let Err(e) = loader.load_plugins().await {
                            warn!("Failed to load plugins: {}", e);
                        }
                        let hook_count = loader.hooks().len();
                        if hook_count > 0 {
                            info!(
                                "Loaded {} plugin hooks from {}",
                                hook_count,
                                plugin_dir.display()
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
                debug!("No plugins directory at {}", plugin_dir.display());
                None
            }
        };

        // Create unified event bus and register built-in hooks
        let event_bus = {
            let mut bus = EventBus::new();

            // Register all built-in hooks
            let builtin_config = BuiltinHooksConfig::default();

            if builtin_config.test_filter.enabled {
                bus.register(create_test_filter_hook(&builtin_config.test_filter));
                info!("Registered builtin:test_filter hook");
            }

            if builtin_config.recipe_enrichment.enabled {
                bus.register(crucible_rune::builtin_hooks::create_recipe_enrichment_hook(
                    &builtin_config.recipe_enrichment,
                ));
                info!("Registered builtin:recipe_enrichment hook");
            }

            Arc::new(RwLock::new(bus))
        };

        Ok(Self {
            kiln_server,
            clustering_tools,
            just_tools,
            rune_registry,
            event_handler,
            event_pipeline,
            filter_config: FilterConfig::default(),
            event_bus,
            upstream_clients: None,
        })
    }

    /// Create server without Just or Rune tools (kiln only)
    pub fn kiln_only(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        let kiln_server =
            CrucibleMcpServer::new(kiln_path.clone(), knowledge_repo, embedding_provider);
        let clustering_tools = Arc::new(ClusteringTools::new(PathBuf::from(kiln_path)));
        let just_tools = Arc::new(JustTools::new("."));
        let rune_registry = Arc::new(
            RuneToolRegistry::new(RuneDiscoveryConfig::default())
                .expect("Failed to create empty Rune registry"),
        );

        Self {
            kiln_server,
            clustering_tools,
            just_tools,
            rune_registry,
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

    /// Get reference to clustering tools
    #[must_use]
    pub fn clustering_tools(&self) -> &ClusteringTools {
        &self.clustering_tools
    }

    /// Get reference to Just tools
    #[must_use]
    pub fn just_tools(&self) -> &JustTools {
        &self.just_tools
    }

    /// Get reference to Rune registry
    #[must_use]
    pub fn rune_registry(&self) -> &RuneToolRegistry {
        &self.rune_registry
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
    /// Just recipes are enriched via tool:discovered hooks before being returned.
    pub async fn list_all_tools(&self) -> Vec<Tool> {
        let mut tools = self.kiln_server.list_tools();

        // Add clustering tools
        let clustering_tools = self.clustering_tools.list_tools().await;
        tools.extend(clustering_tools);

        // Add Just tools (enriched via tool:discovered hooks)
        if let Ok(just_tools) = self.just_tools.list_tools().await {
            for jt in just_tools {
                // Emit tool:discovered event and apply enrichment from hooks
                let enriched_tool = self.emit_tool_discovered(&jt).await;
                tools.push(self.mcp_tool_from_just(&enriched_tool));
            }
        }

        // Add Rune tools
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

    /// Emit tool:discovered event and apply enrichment from hooks
    ///
    /// This replaces the old EventHandler-based enrichment with the new unified event system.
    async fn emit_tool_discovered(&self, tool: &crucible_just::McpTool) -> crucible_just::McpTool {
        let bus = self.event_bus.read().await;

        // Create tool schema from input_schema
        let schema = if tool.input_schema.is_object() {
            Some(tool.input_schema.clone())
        } else {
            None
        };

        // Emit SessionEvent::ToolDiscovered
        let event = SessionEvent::ToolDiscovered {
            name: tool.name.clone(),
            source: CoreToolSource::Rune, // Just tools use Rune source for now
            schema,
        };

        let (result_event, ctx, errors) = bus.emit_session(event);

        if !errors.is_empty() {
            for e in &errors {
                warn!("Hook error during tool:discovered: {}", e);
            }
        }

        // Extract enrichment from context metadata (handlers can set these)
        let mut enriched_tool = tool.clone();

        // Check context metadata for enrichment data
        if let Some(category) = ctx.get("category").and_then(|v| v.as_str()) {
            enriched_tool.category = Some(category.to_string());
        }
        if let Some(tags) = ctx.get("tags").and_then(|v| v.as_array()) {
            enriched_tool.tags = tags
                .iter()
                .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
                .collect();
        }
        if let Some(priority) = ctx.get("priority").and_then(serde_json::Value::as_i64) {
            enriched_tool.priority = Some(priority as i32);
        }

        // Also check if the result_event schema was modified (for backwards compat with old handlers)
        if let SessionEvent::ToolDiscovered {
            schema: Some(schema),
            ..
        } = &result_event
        {
            if let Some(obj) = schema.as_object() {
                if let Some(category) = obj.get("category").and_then(|v| v.as_str()) {
                    enriched_tool.category = Some(category.to_string());
                }
                if let Some(tags) = obj.get("tags").and_then(|v| v.as_array()) {
                    enriched_tool.tags = tags
                        .iter()
                        .filter_map(|v| v.as_str().map(std::string::ToString::to_string))
                        .collect();
                }
                if let Some(priority) = obj.get("priority").and_then(serde_json::Value::as_i64) {
                    enriched_tool.priority = Some(priority as i32);
                }
            }
        }

        enriched_tool
    }

    /// Enrich Just tools via Rune event handlers (DEPRECATED - kept for backward compatibility)
    ///
    /// This method is kept for backward compatibility with the old `EventHandler` system.
    /// New code should use the tool:discovered hook via `emit_tool_discovered()`.
    ///
    /// Converts `McpTools` to `EnrichedRecipes`, processes through handlers,
    /// then updates the `McpTools` with enrichment data.
    #[allow(dead_code)]
    async fn enrich_just_tools(
        &self,
        tools: Vec<crucible_just::McpTool>,
    ) -> Vec<crucible_just::McpTool> {
        let handler = match &self.event_handler {
            Some(h) => h,
            None => return tools, // No handler, return unchanged
        };

        // Convert McpTools to EnrichedRecipes for processing
        let recipes: Vec<EnrichedRecipe> = tools
            .iter()
            .map(|t| {
                // Extract original recipe name from tool name (strip just_ prefix, restore hyphens)
                let recipe_name = t
                    .name
                    .strip_prefix("just_")
                    .unwrap_or(&t.name)
                    .replace('_', "-");

                EnrichedRecipe::from_recipe(
                    recipe_name,
                    Some(t.description.clone()),
                    vec![], // Parameters not needed for enrichment
                    false,
                )
            })
            .collect();

        // Process through event handlers
        match handler.process_recipes(recipes).await {
            Ok(_enriched) => {
                // Update tools with enrichment data
                // Note: McpTool doesn't currently support enrichment fields
                // for (tool, recipe) in tools.iter_mut().zip(enriched.iter()) {
                //     tool.category = recipe.category.clone();
                //     tool.tags = recipe.tags.clone();
                //     tool.priority = recipe.priority;
                // }
                debug!("Enriched {} Just tools via event handlers", tools.len());
            }
            Err(e) => {
                warn!("Failed to enrich recipes: {}", e);
            }
        }

        tools
    }

    /// Convert `crucible_just::McpTool` to `rmcp::model::Tool`
    ///
    /// If enrichment data is present (category, tags), it's appended to the description.
    fn mcp_tool_from_just(&self, jt: &crucible_just::McpTool) -> Tool {
        let schema = jt.input_schema.as_object().cloned().unwrap_or_default();

        // Build description with enrichment metadata appended
        let mut description = jt.description.clone();

        // Append category if present
        if let Some(ref category) = jt.category {
            description = format!("{description} [{category}]");
        }

        // Append tags if present
        if !jt.tags.is_empty() {
            let tags_str = jt
                .tags
                .iter()
                .map(|t| format!("#{t}"))
                .collect::<Vec<_>>()
                .join(" ");
            description = format!("{description} {tags_str}");
        }

        Tool {
            name: jt.name.clone().into(),
            title: None,
            description: Some(description.into()),
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
            source: CoreToolSource::Mcp {
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
        let clustering = self.clustering_tools.list_tools().await.len();
        let just = self.just_tools.tool_count().await.unwrap_or(0);
        let rune = self.rune_registry.tool_count().await;
        kiln + clustering + just + rune
    }

    /// Check if a tool name is handled by Just
    #[must_use]
    pub fn is_just_tool(name: &str) -> bool {
        name.starts_with("just_")
    }

    /// Check if a tool name is handled by Rune
    #[must_use]
    pub fn is_rune_tool(name: &str) -> bool {
        name.starts_with("rune_")
    }

    /// Check if a tool name is handled by Clustering
    #[must_use]
    pub fn is_clustering_tool(name: &str) -> bool {
        matches!(
            name,
            "detect_mocs" | "cluster_documents" | "get_document_stats"
        )
    }

    /// Check if a tool name might be from an upstream MCP server
    ///
    /// Upstream tools have a prefix from their upstream config.
    /// Common prefixes: gh_, fs_, slack_, etc.
    /// We detect them by checking if they're NOT kiln/just/rune/clustering tools.
    #[must_use]
    pub fn is_upstream_tool(name: &str) -> bool {
        // If it's a known prefix, it's not upstream
        if Self::is_just_tool(name) || Self::is_rune_tool(name) || Self::is_clustering_tool(name) {
            return false;
        }
        // Otherwise, we need to check against the gateway manager at runtime
        true
    }

    /// Execute a Just recipe and return TOON-formatted result
    ///
    /// Output is filtered in two stages:
    /// 1. **Built-in filter**: Automatically extracts test summaries from cargo test,
    ///    pytest, jest, go test, etc. This is always applied for recognized test output.
    /// 2. **Rune pipeline**: If configured, custom plugins can further transform output.
    ///
    /// This makes test output much more useful for LLMs by removing verbose per-test
    /// lines and keeping only pass/fail summaries and error details.
    pub async fn call_just_tool(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let recipe_name = name.strip_prefix("just_").unwrap_or(name);
        let start = Instant::now();

        debug!(
            "Executing Just recipe: {} with args: {:?}",
            recipe_name, arguments
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
                    format!("Just recipe '{recipe_name}' execution cancelled by hook"),
                    None,
                ));
            }
        }

        match self
            .just_tools
            .execute(recipe_name, arguments.clone())
            .await
        {
            Ok(result) => {
                let duration_ms = start.elapsed().as_millis() as u64;
                let is_error = result.exit_code != Some(0);

                // Combine stdout and stderr for the output
                let mut output = result.stdout.clone();
                if !result.stderr.is_empty() {
                    if !output.is_empty() {
                        output.push_str("\n--- stderr ---\n");
                    }
                    output.push_str(&result.stderr);
                }

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

                let response = json!({
                    "recipe": recipe_name,
                    "exit_code": result.exit_code,
                    "stdout": final_output,
                    "stderr": result.stderr,
                    "success": !is_error
                });

                // Emit SessionEvent::ToolCompleted with the response
                {
                    let bus = self.event_bus.read().await;
                    let event = SessionEvent::ToolCompleted {
                        name: name.to_string(),
                        result: final_output.clone(),
                        error: if is_error {
                            Some("Tool returned non-zero exit code".to_string())
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
                let _duration_ms = start.elapsed().as_millis() as u64;

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
                    format!("Just recipe '{recipe_name}' failed: {e}"),
                    None,
                ))
            }
        }
    }

    /// Execute a clustering tool and return the result
    pub async fn call_clustering_tool(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        debug!(
            "Executing clustering tool: {} with args: {:?}",
            name, arguments
        );

        // Execute the appropriate clustering tool and convert to JSON
        let result = match name {
            "detect_mocs" => {
                let min_score = arguments
                    .get("min_score")
                    .and_then(serde_json::Value::as_f64);

                let mocs = self
                    .clustering_tools
                    .detect_mocs(min_score)
                    .await
                    .map_err(|e| {
                        rmcp::ErrorData::internal_error(format!("detect_mocs failed: {e}"), None)
                    })?;
                json!(mocs)
            }
            "cluster_documents" => {
                let min_similarity = arguments
                    .get("min_similarity")
                    .and_then(serde_json::Value::as_f64);
                let min_cluster_size = arguments
                    .get("min_cluster_size")
                    .and_then(serde_json::Value::as_u64)
                    .map(|v| v as usize);
                let link_weight = arguments
                    .get("link_weight")
                    .and_then(serde_json::Value::as_f64);
                let tag_weight = arguments
                    .get("tag_weight")
                    .and_then(serde_json::Value::as_f64);
                let title_weight = arguments
                    .get("title_weight")
                    .and_then(serde_json::Value::as_f64);

                let clusters = self
                    .clustering_tools
                    .cluster_documents(
                        min_similarity,
                        min_cluster_size,
                        link_weight,
                        tag_weight,
                        title_weight,
                    )
                    .await
                    .map_err(|e| {
                        rmcp::ErrorData::internal_error(
                            format!("cluster_documents failed: {e}"),
                            None,
                        )
                    })?;
                json!(clusters)
            }
            "get_document_stats" => {
                let stats = self
                    .clustering_tools
                    .get_document_stats()
                    .await
                    .map_err(|e| {
                        rmcp::ErrorData::internal_error(
                            format!("get_document_stats failed: {e}"),
                            None,
                        )
                    })?;
                json!(stats)
            }
            _ => {
                return Err(rmcp::ErrorData::internal_error(
                    format!("Unknown clustering tool: {name}"),
                    None,
                ));
            }
        };

        // Return result as TOON-formatted JSON
        Ok(toon_success_smart(result))
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

    /// Refresh Just tools (re-read justfile)
    pub async fn refresh_just(&self) -> Result<(), crucible_just::JustError> {
        self.just_tools.refresh().await
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

        // Route to appropriate handler based on prefix or name
        if ExtendedMcpServer::is_just_tool(name) {
            self.inner.call_just_tool(name, arguments).await
        } else if ExtendedMcpServer::is_rune_tool(name) {
            // Pass full name to registry (it stores tools with rune_ prefix)
            self.inner.call_rune_tool(name, arguments).await
        } else if ExtendedMcpServer::is_clustering_tool(name) {
            self.inner.call_clustering_tool(name, arguments).await
        } else {
            // Try upstream tools first if configured
            if let Some(gateway) = self.inner.upstream_clients() {
                // Check if this tool exists in any upstream client
                if gateway.find_client_for_tool(name).await.is_some() {
                    return self.inner.call_upstream_tool(name, arguments).await;
                }
            }

            // Delegate to kiln server for core tools
            self.inner.kiln_server.call_tool(request, context).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
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

        // Should have at least the 12 kiln tools + 3 clustering tools
        let count = server.tool_count().await;
        assert!(count >= 15);
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
        assert_eq!(tools.len(), 15); // 12 kiln + 3 clustering tools, no just/rune
    }

    #[test]
    fn test_tool_name_routing() {
        assert!(ExtendedMcpServer::is_just_tool("just_build"));
        assert!(ExtendedMcpServer::is_just_tool("just_test"));
        assert!(!ExtendedMcpServer::is_just_tool("rune_summarize"));
        assert!(!ExtendedMcpServer::is_just_tool("read_note"));

        assert!(ExtendedMcpServer::is_rune_tool("rune_summarize"));
        assert!(ExtendedMcpServer::is_rune_tool("rune_transform"));
        assert!(!ExtendedMcpServer::is_rune_tool("just_build"));
        assert!(!ExtendedMcpServer::is_rune_tool("read_note"));

        assert!(ExtendedMcpServer::is_clustering_tool("detect_mocs"));
        assert!(ExtendedMcpServer::is_clustering_tool("cluster_documents"));
        assert!(ExtendedMcpServer::is_clustering_tool("get_document_stats"));
        assert!(!ExtendedMcpServer::is_clustering_tool("just_build"));
        assert!(!ExtendedMcpServer::is_clustering_tool("rune_summarize"));
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
                "just_*",
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
            let event = Event::tool_after("just_test", json!({"result": "success"}));
            let (result, _ctx, errors) = bus.emit(event);

            assert_eq!(result.identifier, "just_test");
            assert!(errors.is_empty());
        }
    }

    #[tokio::test]
    #[ignore = "Requires `just` binary to be installed"]
    async fn test_tool_events_emitted() {
        use crucible_rune::event_bus::{EventType, Handler};
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

        // Track events emitted
        let before_count = Arc::new(AtomicUsize::new(0));
        let after_count = Arc::new(AtomicUsize::new(0));

        let before_count_clone = Arc::clone(&before_count);
        let after_count_clone = Arc::clone(&after_count);

        // Register handlers to count events
        {
            let mut bus = event_bus.write().await;

            bus.register(Handler::new(
                "count_before",
                EventType::ToolBefore,
                "*",
                move |_ctx, event| {
                    before_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(event)
                },
            ));

            bus.register(Handler::new(
                "count_after",
                EventType::ToolAfter,
                "*",
                move |_ctx, event| {
                    after_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(event)
                },
            ));
        }

        // Call a just tool (need to create a justfile first)
        // For this test, we'll create a simple justfile
        let justfile_path = temp.path().join("justfile");
        std::fs::write(&justfile_path, "hello:\n\techo 'Hello World'\n").unwrap();

        // Create a new server with the justfile
        let server = ExtendedMcpServer::new(
            temp.path().to_str().unwrap().to_string(),
            Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>,
            Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>,
            temp.path(),
            RuneDiscoveryConfig::default(),
        )
        .await
        .unwrap();

        // Copy the handlers to the new server's event bus
        let new_event_bus = server.event_bus();
        {
            let mut bus = new_event_bus.write().await;

            let before_count_clone2 = Arc::clone(&before_count);
            let after_count_clone2 = Arc::clone(&after_count);

            bus.register(Handler::new(
                "count_before",
                EventType::ToolBefore,
                "*",
                move |_ctx, event| {
                    before_count_clone2.fetch_add(1, Ordering::SeqCst);
                    Ok(event)
                },
            ));

            bus.register(Handler::new(
                "count_after",
                EventType::ToolAfter,
                "*",
                move |_ctx, event| {
                    after_count_clone2.fetch_add(1, Ordering::SeqCst);
                    Ok(event)
                },
            ));
        }

        // Execute a just tool
        let _ = server.call_just_tool("just_hello", json!({})).await;

        // Verify events were emitted
        assert_eq!(
            before_count.load(Ordering::SeqCst),
            1,
            "tool:before event should be emitted"
        );
        assert_eq!(
            after_count.load(Ordering::SeqCst),
            1,
            "tool:after event should be emitted"
        );
    }

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
                "just_*",
                move |_ctx, event| {
                    discovered_count_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(event)
                },
            ));
        }

        // List tools - should emit tool:discovered events
        let tools = server.list_all_tools().await;

        // Should have at least one just tool
        let just_tools: Vec<_> = tools
            .iter()
            .filter(|t| t.name.as_ref().starts_with("just_"))
            .collect();
        assert!(!just_tools.is_empty(), "Should have at least one just tool");

        // Should have emitted tool:discovered events for all just tools
        assert_eq!(
            discovered_count.load(Ordering::SeqCst),
            just_tools.len(),
            "Should emit tool:discovered event for each just tool"
        );
    }

    #[tokio::test]
    #[ignore = "Requires `just` binary to be installed"]
    async fn test_recipe_enrichment_via_hook() {
        use crucible_rune::event_bus::{EventType, Handler};

        let temp = TempDir::new().unwrap();

        // Create a justfile with a test recipe
        let justfile_path = temp.path().join("justfile");
        std::fs::write(
            &justfile_path,
            "test:\n\techo 'Running tests'\n\nbuild:\n\techo 'Building'\n",
        )
        .unwrap();

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

        // Register a hook to enrich just_test with category and tags
        {
            let mut bus = event_bus.write().await;
            bus.register(Handler::new(
                "test_enrichment",
                EventType::ToolDiscovered,
                "just_test",
                |_ctx, mut event| {
                    // Add enrichment data to the payload
                    if let Some(obj) = event.payload.as_object_mut() {
                        obj.insert("category".to_string(), json!("testing"));
                        obj.insert("tags".to_string(), json!(["ci", "quick"]));
                        obj.insert("priority".to_string(), json!(10));
                    }
                    Ok(event)
                },
            ));
        }

        // List tools - should apply enrichment via hooks
        let tools = server.list_all_tools().await;

        // Find the just_test tool
        let test_tool = tools.iter().find(|t| t.name.as_ref() == "just_test");
        assert!(test_tool.is_some(), "Should have just_test tool");

        let test_tool = test_tool.unwrap();

        // Verify enrichment is in the description
        let desc = test_tool.description.as_ref().unwrap().as_ref();
        assert!(
            desc.contains("[testing]"),
            "Description should contain category: {desc}"
        );
        assert!(
            desc.contains("#ci"),
            "Description should contain tags: {desc}"
        );
        assert!(
            desc.contains("#quick"),
            "Description should contain tags: {desc}"
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

        // Verify is_upstream_tool detection
        assert!(
            !ExtendedMcpServer::is_upstream_tool("just_build"),
            "just_ tools are not upstream"
        );
        assert!(
            !ExtendedMcpServer::is_upstream_tool("rune_test"),
            "rune_ tools are not upstream"
        );
        assert!(
            ExtendedMcpServer::is_upstream_tool("gh_search_repos"),
            "gh_ tools could be upstream"
        );

        // Note: We can't actually test tool execution here without implementing
        // the full rmcp transport, but we've verified the tool is discoverable
        // and the routing logic is in place
    }
}
