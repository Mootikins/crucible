//! Extended MCP Server with Just and Rune tools
//!
//! This server combines:
//! - **CrucibleMcpServer** (12 tools): Note, Search, and Kiln operations
//! - **JustTools** (dynamic): Recipes from justfile in PWD
//! - **RuneTools** (dynamic): Scripts from configured runes/ directories
//!
//! All responses are formatted with TOON for token efficiency.
//!
//! ## Recipe Enrichment
//!
//! Just recipes are automatically enriched by Rune event handlers:
//! - Scripts in `runes/events/recipe_discovered/` are executed for each recipe
//! - Handlers can add category, tags, priority, and custom metadata
//! - Enrichment is visible in tool descriptions and schema annotations

use crate::toon_response::toon_success_smart;
use crate::CrucibleMcpServer;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use crucible_just::JustTools;
use crucible_rune::{
    ContentBlock, EnrichedRecipe, EventHandler, EventHandlerConfig, EventPipeline, PluginLoader,
    RuneDiscoveryConfig, RuneToolRegistry, ToolResultEvent,
};
use rmcp::model::{CallToolResult, Content, Tool};
use rmcp::service::RequestContext;
use rmcp::ServerHandler;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Extended MCP server exposing all Crucible tools plus Just and Rune
///
/// This server aggregates tools from multiple sources:
/// - **Kiln tools** (12): NoteTools, SearchTools, KilnTools via CrucibleMcpServer
/// - **Just tools** (dynamic): Recipes from justfile prefixed with `just_`
/// - **Rune tools** (dynamic): Scripts from runes/ directories prefixed with `rune_`
///
/// Just recipes are automatically enriched via event handlers before being exposed.
pub struct ExtendedMcpServer {
    /// Core Crucible MCP server with 12 kiln tools
    kiln_server: CrucibleMcpServer,
    /// Just recipe executor
    just_tools: Arc<JustTools>,
    /// Rune script registry
    rune_registry: Arc<RuneToolRegistry>,
    /// Event handler for recipe enrichment
    event_handler: Option<Arc<EventHandler>>,
    /// Event pipeline for filtering tool output
    event_pipeline: Option<EventPipeline>,
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
        let event_handler = match EventHandler::new(EventHandlerConfig::with_defaults(Some(
            &just_dir,
        ))) {
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
                            info!("Loaded {} plugin hooks from {}", hook_count, plugin_dir.display());
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

        Ok(Self {
            kiln_server,
            just_tools,
            rune_registry,
            event_handler,
            event_pipeline,
        })
    }

    /// Create server without Just or Rune tools (kiln only)
    pub fn kiln_only(
        kiln_path: String,
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        let kiln_server = CrucibleMcpServer::new(kiln_path, knowledge_repo, embedding_provider);
        let just_tools = Arc::new(JustTools::new("."));
        let rune_registry = Arc::new(
            RuneToolRegistry::new(RuneDiscoveryConfig::default())
                .expect("Failed to create empty Rune registry"),
        );

        Self {
            kiln_server,
            just_tools,
            rune_registry,
            event_handler: None,
            event_pipeline: None,
        }
    }

    /// Get reference to the kiln server
    pub fn kiln_server(&self) -> &CrucibleMcpServer {
        &self.kiln_server
    }

    /// Get reference to Just tools
    pub fn just_tools(&self) -> &JustTools {
        &self.just_tools
    }

    /// Get reference to Rune registry
    pub fn rune_registry(&self) -> &RuneToolRegistry {
        &self.rune_registry
    }

    /// List all available tools from all sources
    ///
    /// Just recipes are enriched via event handlers before being returned.
    pub async fn list_all_tools(&self) -> Vec<Tool> {
        let mut tools = self.kiln_server.list_tools();

        // Add Just tools (enriched via event handlers)
        if let Ok(just_tools) = self.just_tools.list_tools().await {
            // Enrich recipes via event handlers
            let enriched_tools = self.enrich_just_tools(just_tools).await;
            for jt in enriched_tools {
                tools.push(self.mcp_tool_from_just(&jt));
            }
        }

        // Add Rune tools
        for rt in self.rune_registry.list_tools().await {
            tools.push(self.mcp_tool_from_rune(&rt));
        }

        tools
    }

    /// Enrich Just tools via Rune event handlers
    ///
    /// Converts McpTools to EnrichedRecipes, processes through handlers,
    /// then updates the McpTools with enrichment data.
    async fn enrich_just_tools(
        &self,
        mut tools: Vec<crucible_just::McpTool>,
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
            Ok(enriched) => {
                // Update tools with enrichment data
                for (tool, recipe) in tools.iter_mut().zip(enriched.iter()) {
                    tool.category = recipe.category.clone();
                    tool.tags = recipe.tags.clone();
                    tool.priority = recipe.priority;
                }
                debug!(
                    "Enriched {} Just tools via event handlers",
                    tools.len()
                );
            }
            Err(e) => {
                warn!("Failed to enrich recipes: {}", e);
            }
        }

        tools
    }

    /// Convert crucible_just::McpTool to rmcp::model::Tool
    ///
    /// If enrichment data is present (category, tags), it's appended to the description.
    fn mcp_tool_from_just(&self, jt: &crucible_just::McpTool) -> Tool {
        let schema = jt
            .input_schema
            .as_object()
            .cloned()
            .unwrap_or_default();

        // Build description with enrichment metadata
        let mut description = jt.description.clone();

        // Append enrichment info if present
        let mut enrichment_parts = vec![];
        if let Some(cat) = &jt.category {
            enrichment_parts.push(format!("[{}]", cat));
        }
        if !jt.tags.is_empty() {
            enrichment_parts.push(format!("#{}", jt.tags.join(" #")));
        }
        if !enrichment_parts.is_empty() {
            description = format!("{} {}", description, enrichment_parts.join(" "));
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

    /// Convert crucible_rune::RuneTool to rmcp::model::Tool
    fn mcp_tool_from_rune(&self, rt: &crucible_rune::RuneTool) -> Tool {
        let schema = rt
            .input_schema
            .as_object()
            .cloned()
            .unwrap_or_default();
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

    /// Get total tool count
    pub async fn tool_count(&self) -> usize {
        let kiln = self.kiln_server.tool_count();
        let just = self.just_tools.tool_count().await.unwrap_or(0);
        let rune = self.rune_registry.tool_count().await;
        kiln + just + rune
    }

    /// Check if a tool name is handled by Just
    pub fn is_just_tool(name: &str) -> bool {
        name.starts_with("just_")
    }

    /// Check if a tool name is handled by Rune
    pub fn is_rune_tool(name: &str) -> bool {
        name.starts_with("rune_")
    }

    /// Execute a Just recipe and return TOON-formatted result
    ///
    /// If an event pipeline is configured, the result is processed through
    /// any matching hooks before being returned. Hooks can filter or transform
    /// the output (e.g., extracting test summaries).
    pub async fn call_just_tool(
        &self,
        name: &str,
        arguments: Value,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let recipe_name = name.strip_prefix("just_").unwrap_or(name);
        debug!("Executing Just recipe: {} with args: {:?}", recipe_name, arguments);

        match self.just_tools.execute(recipe_name, arguments.clone()).await {
            Ok(result) => {
                let is_error = result.exit_code != Some(0);

                // Combine stdout and stderr for the output
                let mut output = result.stdout.clone();
                if !result.stderr.is_empty() {
                    if !output.is_empty() {
                        output.push_str("\n--- stderr ---\n");
                    }
                    output.push_str(&result.stderr);
                }

                // Process through event pipeline if available
                let final_output = if let Some(pipeline) = &self.event_pipeline {
                    let event = ToolResultEvent {
                        tool_name: name.to_string(),
                        arguments: arguments.clone(),
                        is_error,
                        content: vec![ContentBlock::Text { text: output.clone() }],
                        duration_ms: 0, // TODO: track actual duration
                    };

                    match pipeline.process_tool_result(event).await {
                        Ok(processed) => {
                            // Extract text from processed event
                            processed.text_content()
                        }
                        Err(e) => {
                            warn!("Event pipeline error: {}, using original output", e);
                            output
                        }
                    }
                } else {
                    output
                };

                let response = json!({
                    "recipe": recipe_name,
                    "exit_code": result.exit_code,
                    "stdout": final_output,
                    "stderr": result.stderr,
                    "success": !is_error
                });
                Ok(toon_success_smart(response))
            }
            Err(e) => Err(rmcp::ErrorData::internal_error(
                format!("Just recipe '{}' failed: {}", recipe_name, e),
                None,
            )),
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
        debug!("Executing Rune tool: {} with args: {:?}", name, arguments);

        match self.rune_registry.execute(name, arguments).await {
            Ok(result) => {
                if result.success {
                    // Return result directly - only use TOON for structured data
                    match &result.result {
                        Some(Value::Object(_)) | Some(Value::Array(_)) => {
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
                    // Error - return as error message
                    let error_msg = result.error.unwrap_or_else(|| "Unknown error".to_string());
                    Err(rmcp::ErrorData::internal_error(
                        format!("Rune tool '{}' failed: {}", name, error_msg),
                        None,
                    ))
                }
            }
            Err(e) => Err(rmcp::ErrorData::internal_error(
                format!("Rune tool '{}' failed: {}", name, e),
                None,
            )),
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
}

/// Wrapper to make ExtendedMcpServer implement Clone (required by rmcp)
///
/// Since ExtendedMcpServer contains Arc fields, we wrap it in Arc for cloning.
#[derive(Clone)]
pub struct ExtendedMcpService {
    inner: Arc<ExtendedMcpServer>,
    /// Cached tools list (refreshed on demand)
    cached_tools: Arc<RwLock<Vec<Tool>>>,
}

impl ExtendedMcpService {
    /// Create from an ExtendedMcpServer
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
    pub fn server(&self) -> &ExtendedMcpServer {
        &self.inner
    }
}

impl ServerHandler for ExtendedMcpService {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::default(),
            capabilities: rmcp::model::ServerCapabilities {
                tools: Some(rmcp::model::ToolsCapability {
                    list_changed: None,
                }),
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
        let arguments = request
            .arguments
            .clone()
            .map(|m| Value::Object(m))
            .unwrap_or(Value::Null);

        debug!("Calling tool: {} with args: {:?}", name, arguments);

        // Route to appropriate handler based on prefix
        if ExtendedMcpServer::is_just_tool(name) {
            self.inner.call_just_tool(name, arguments).await
        } else if ExtendedMcpServer::is_rune_tool(name) {
            // Pass full name to registry (it stores tools with rune_ prefix)
            self.inner.call_rune_tool(name, arguments).await
        } else {
            // Delegate to kiln server for core tools
            self.inner
                .kiln_server
                .call_tool(request, context)
                .await
        }
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

        fn model_name(&self) -> &str {
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
        assert_eq!(tools.len(), 12); // 12 kiln tools, no just/rune
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
    }
}
