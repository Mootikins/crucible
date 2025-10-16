// crates/crucible-mcp/src/service.rs
//
// rmcp-based MCP Server Service Layer
//
// This module provides the official rmcp-based implementation of the Crucible MCP server.
// It wraps existing tool implementations from tools::mod with rmcp's #[tool] macro.

use rmcp::{ErrorData as McpError, model::*, tool, tool_router, handler::server::{wrapper::Parameters, ServerHandler, tool::ToolRouter}};
use crate::database::EmbeddingDatabase;
use crate::embeddings::EmbeddingProvider;
use crate::rune_tools::ToolRegistry;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Crucible MCP Service using rmcp SDK
///
/// This service exposes 10 native Crucible MCP tools via the rmcp protocol,
/// plus dynamically loaded Rune-based tools from the tool registry.
#[derive(Clone)]
pub struct CrucibleMcpService {
    database: Arc<EmbeddingDatabase>,
    provider: Arc<dyn EmbeddingProvider>,
    tool_router: ToolRouter<Self>,
    rune_registry: Option<Arc<RwLock<ToolRegistry>>>,
}

#[tool_router]
impl CrucibleMcpService {
    /// Create a new Crucible MCP service instance without Rune tools
    pub fn new(database: Arc<EmbeddingDatabase>, provider: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            database,
            provider,
            tool_router: Self::tool_router(),
            rune_registry: None,
        }
    }

    /// Create a new Crucible MCP service instance with Rune tool support
    pub fn with_rune_tools(
        database: Arc<EmbeddingDatabase>,
        provider: Arc<dyn EmbeddingProvider>,
        rune_registry: ToolRegistry,
    ) -> Self {
        Self {
            database,
            provider,
            tool_router: Self::tool_router(),
            rune_registry: Some(Arc::new(RwLock::new(rune_registry))),
        }
    }

    /// Dynamically call a Rune tool by name
    ///
    /// This is a special tool that acts as a dispatcher for dynamically loaded Rune tools.
    /// Rune tools are discovered at runtime from .rn files and executed via the Rune VM.
    ///
    /// Available Rune tools can be queried by calling this tool with tool_name="__list"
    #[tool(description = "Execute a dynamically loaded Rune tool")]
    async fn __run_rune_tool(
        &self,
        Parameters(params): Parameters<crate::types::RuneToolParams>,
    ) -> Result<CallToolResult, McpError> {
        let registry = self.rune_registry.as_ref()
            .ok_or_else(|| McpError::internal_error("Rune tools not enabled".to_string(), None))?;

        // Get tool and context without holding lock
        let reg = registry.read().await;
        let tool = reg.get_tool(&params.tool_name)
            .ok_or_else(|| McpError::internal_error(format!("Rune tool '{}' not found", params.tool_name), None))?
            .clone();
        let context = reg.context.clone();
        drop(reg); // Explicitly drop lock

        // Execute the Rune tool on a blocking thread since Rune futures are !Send
        // This is necessary because Rune's VM uses thread-local storage
        let args = params.args;
        let result = tokio::task::spawn_blocking(move || {
            // Create a new tokio runtime for the Rune execution
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(tool.call(args, &context))
        })
        .await
        .map_err(|e| McpError::internal_error(format!("Task join error: {}", e), None))?
        .map_err(|e| McpError::internal_error(format!("Rune tool execution failed: {}", e), None))?;

        // Convert result to CallToolResult
        let content = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    /// Search notes by frontmatter properties
    #[tool(description = "[READ] Search vault notes by frontmatter property values (e.g., status:active)")]
    async fn search_by_properties(
        &self,
        Parameters(params): Parameters<crate::types::SearchByPropertiesParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: Some(params.properties),
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };
        let result = crate::tools::search_by_properties(&self.database, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Search notes by tags
    #[tool(description = "[READ] Search vault notes by tags (e.g., #project, #ai)")]
    async fn search_by_tags(
        &self,
        Parameters(params): Parameters<crate::types::SearchByTagsParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: None,
            tags: Some(params.tags),
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };
        let result = crate::tools::search_by_tags(&self.database, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// List notes in a specific folder
    #[tool(description = "[READ] List vault notes in a specific folder path")]
    async fn list_notes_in_folder(
        &self,
        Parameters(params): Parameters<crate::types::SearchByFolderParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: None,
            tags: None,
            path: Some(params.path),
            recursive: Some(params.recursive),
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };
        let result = crate::tools::search_by_folder(&self.database, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Search notes by filename pattern
    #[tool(description = "[READ] Find vault notes by filename or pattern (supports wildcards)")]
    async fn search_by_filename(
        &self,
        Parameters(params): Parameters<crate::types::SearchByFilenameParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: Some(params.pattern),
            query: None,
            top_k: None,
            force: None,
        };
        let result = crate::tools::search_by_filename(&self.database, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Full-text search in note contents
    #[tool(description = "[READ] Search vault notes by text content (keyword search, not semantic)")]
    async fn search_by_content(
        &self,
        Parameters(params): Parameters<crate::types::SearchByContentParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: Some(params.query),
            top_k: None,
            force: None,
        };
        let result = crate::tools::search_by_content(&self.database, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Semantic search using embeddings
    #[tool(description = "[READ] AI-powered semantic search of vault notes by meaning (requires embeddings)")]
    async fn semantic_search(
        &self,
        Parameters(params): Parameters<crate::types::SemanticSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: Some(params.query),
            top_k: Some(params.top_k),
            force: None,
        };
        let result = crate::tools::semantic_search(&self.database, &self.provider, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Build search index by generating embeddings
    #[tool(description = "[INTERNAL] Build search index - generates AI embeddings for semantic search. DO NOT use for adding notes.")]
    async fn build_search_index(
        &self,
        Parameters(params): Parameters<crate::types::IndexVaultParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: None,
            tags: None,
            path: Some(params.path),
            recursive: None,
            pattern: Some(params.pattern),
            query: None,
            top_k: None,
            force: Some(params.force),
        };
        let result = crate::tools::index_vault(&self.database, &self.provider, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Get metadata for a specific note
    #[tool(description = "[READ] Get metadata for a vault note (tags, properties, folder info)")]
    async fn get_note_metadata(
        &self,
        Parameters(params): Parameters<crate::types::GetNoteMetadataParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: None,
            tags: None,
            path: Some(params.path),
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };
        let result = crate::tools::get_note_metadata(&self.database, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Update frontmatter properties of a note
    #[tool(description = "[WRITE] Update frontmatter properties of an existing vault note")]
    async fn update_note_properties(
        &self,
        Parameters(params): Parameters<crate::types::UpdateNotePropertiesParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: Some(params.properties),
            tags: None,
            path: Some(params.path),
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };
        let result = crate::tools::update_note_properties(&self.database, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Get statistics about the vault
    #[tool(description = "[READ] Get vault statistics (total notes, embeddings, database info)")]
    async fn get_vault_stats(
        &self,
        Parameters(_params): Parameters<crate::types::GetDocumentStatsParams>,
    ) -> Result<CallToolResult, McpError> {
        let args = crate::types::ToolCallArgs {
            properties: None,
            tags: None,
            path: None,
            recursive: None,
            pattern: None,
            query: None,
            top_k: None,
            force: None,
        };
        let result = crate::tools::get_document_stats(&self.database, &args)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        self.convert_result(result)
    }

    /// Convert ToolCallResult to rmcp's CallToolResult
    ///
    /// CRITICAL: This method handles errors by returning successful tool results
    /// with isError=true. This is required for Claude Desktop compatibility.
    ///
    /// rmcp errors (Err returns) should only be used for protocol-level failures,
    /// not tool execution failures. Tool failures are returned as successful
    /// tool responses with error information in the content.
    fn convert_result(&self, result: crate::types::ToolCallResult) -> Result<CallToolResult, McpError> {
        if result.success {
            // Success case: return data as formatted JSON
            let content = if let Some(data) = result.data {
                serde_json::to_string_pretty(&data)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?
            } else {
                "Success".to_string()
            };
            Ok(CallToolResult::success(vec![Content::text(content)]))
        } else {
            // Error case: return as tool error (not protocol error)
            // This is critical for Claude Desktop - errors must be wrapped as tool results
            let error_message = result.error.unwrap_or_else(|| "Unknown error".to_string());

            // Include any partial data in the error response
            let error_content = if let Some(data) = result.data {
                format!("Error: {}\n\nPartial data:\n{}",
                    error_message,
                    serde_json::to_string_pretty(&data).unwrap_or_default())
            } else {
                error_message
            };

            Ok(CallToolResult::error(vec![Content::text(error_content)]))
        }
    }
}

// Implement ServerHandler to enable Service<RoleServer> trait for CrucibleMcpService
impl ServerHandler for CrucibleMcpService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "crucible-mcp".to_string(),
                version: "0.1.0".to_string(),
                title: Some("Crucible MCP Server".to_string()),
                icons: None,
                website_url: None,
            },
            instructions: Some("Crucible MCP server for Obsidian vault operations. Use search tools to find existing notes. Notes are managed in Obsidian - do not use build_search_index for adding notes. Semantic search requires embeddings (run build_search_index once).".to_string()),
        }
    }

    // Manually implement call_tool to use our tool router
    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        use rmcp::handler::server::tool::ToolCallContext;
        let tcc = ToolCallContext::new(self, request, context);
        self.tool_router.call(tcc).await
    }

    // Custom list_tools implementation to include dynamic Rune tools
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        use rmcp::model::Tool;
        use std::borrow::Cow;

        // DEBUG: Start debugging the list_tools process
        tracing::info!("🔍 DEBUG: list_tools() called");
        tracing::info!("🔍 DEBUG: rune_registry present: {}", self.rune_registry.is_some());

        // Get native tools from the router
        let mut all_tools = self.tool_router.list_all();
        tracing::info!("🔍 DEBUG: Native tools found: {}", all_tools.len());
        for (i, tool) in all_tools.iter().enumerate() {
            tracing::info!("🔍 DEBUG: Native tool {}: {}", i, tool.name);
        }

        // Add Rune tools if registry is available
        if let Some(registry) = &self.rune_registry {
            tracing::info!("🔍 DEBUG: Acquiring registry read lock");
            let reg = registry.read().await;
            tracing::info!("🔍 DEBUG: Registry read lock acquired");

            let rune_tools = reg.list_tools();
            tracing::info!("🔍 DEBUG: Rune tools from registry: {}", rune_tools.len());

            for (i, tool_meta) in rune_tools.iter().enumerate() {
                tracing::info!("🔍 DEBUG: Rune tool {}: name='{}', desc='{}'", i, tool_meta.name, tool_meta.description);
                tracing::info!("🔍 DEBUG: Rune tool {} input_schema type: {}", i,
                    if tool_meta.input_schema.is_object() { "object" } else { "other" });

                // Convert input_schema from Value to Map<String, Value>
                let input_schema = match &tool_meta.input_schema {
                    serde_json::Value::Object(map) => {
                        tracing::info!("🔍 DEBUG: Converting input_schema for tool '{}', {} properties",
                            tool_meta.name, map.len());
                        Arc::new(map.clone())
                    },
                    _ => {
                        tracing::warn!("Rune tool '{}' has non-object input_schema, using empty object", tool_meta.name);
                        Arc::new(serde_json::Map::new())
                    }
                };

                // Convert output_schema if present
                let output_schema = tool_meta.output_schema.as_ref().and_then(|schema| {
                    match schema {
                        serde_json::Value::Object(map) => {
                            tracing::info!("🔍 DEBUG: Converting output_schema for tool '{}'", tool_meta.name);
                            Some(Arc::new(map.clone()))
                        },
                        _ => {
                            tracing::warn!("Rune tool '{}' has non-object output_schema, ignoring", tool_meta.name);
                            None
                        }
                    }
                });

                // Convert ToolMetadata to rmcp::model::Tool
                let rune_tool = Tool {
                    name: Cow::Owned(tool_meta.name.clone()),
                    title: None,
                    description: Some(Cow::Owned(tool_meta.description.clone())),
                    input_schema,
                    output_schema,
                    annotations: None,
                    icons: None,
                };

                tracing::info!("🔍 DEBUG: Adding Rune tool '{}' to MCP tool list", tool_meta.name);
                all_tools.push(rune_tool);
            }

            tracing::info!("🔍 DEBUG: Releasing registry read lock");
            drop(reg);
        } else {
            tracing::warn!("🔍 DEBUG: No rune_registry available - Rune tools disabled");
        }

        tracing::info!("🔍 DEBUG: Final tool count: {} (native + rune)", all_tools.len());
        for (i, tool) in all_tools.iter().enumerate() {
            tracing::info!("🔍 DEBUG: Final tool {}: {}", i, tool.name);
        }

        Ok(ListToolsResult::with_all_items(all_tools))
    }
}

// The #[tool_router] macro generates the Service<RoleServer> implementation automatically

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use async_trait::async_trait;
    use crate::embeddings::{EmbeddingResponse, EmbeddingResult};
  
    // Mock embedding provider for testing
    struct TestEmbeddingProvider;

    #[async_trait]
    impl EmbeddingProvider for TestEmbeddingProvider {
        async fn embed(&self, _text: &str) -> EmbeddingResult<EmbeddingResponse> {
            Ok(EmbeddingResponse::new(vec![0.1; 384], "test-model".to_string()))
        }

        async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
            Ok(texts.iter().map(|_| EmbeddingResponse::new(vec![0.1; 384], "test-model".to_string())).collect())
        }

        fn model_name(&self) -> &str {
            "test-model"
        }

        fn dimensions(&self) -> usize {
            384
        }

        fn provider_name(&self) -> &str {
            "TestProvider"
        }

        async fn health_check(&self) -> EmbeddingResult<bool> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn test_service_creation() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap());

        let provider = Arc::new(TestEmbeddingProvider);

        let _service = CrucibleMcpService::new(db, provider);
        // If we get here, service was created successfully
    }

    #[tokio::test]
    async fn test_rune_tools_discovered_and_listed() {
        use std::fs;

        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap());

        let provider = Arc::new(TestEmbeddingProvider);

        // Create a test Rune tool
        let tools_dir = temp_dir.path().join("tools");
        fs::create_dir_all(&tools_dir).unwrap();

        let test_tool = r#"
            pub fn NAME() { "test_tool" }
            pub fn DESCRIPTION() { "A test tool for discovery" }
            pub fn INPUT_SCHEMA() {
                #{
                    type: "object",
                    properties: #{
                        message: #{ type: "string" }
                    },
                    required: ["message"]
                }
            }

            pub async fn call(args) {
                #{ success: true, message: args.message }
            }
        "#;

        fs::write(tools_dir.join("test_tool.rn"), test_tool).unwrap();

        // Create Rune context and registry
        let context = rune::Context::with_default_modules().unwrap();
        let registry = crate::rune_tools::ToolRegistry::new(
            tools_dir,
            Arc::new(context)
        ).unwrap();

        // Verify tool was loaded in registry
        assert_eq!(registry.tool_count(), 1);
        assert!(registry.has_tool("test_tool"));

        // Create service with Rune tools
        let service = CrucibleMcpService::with_rune_tools(
            db.clone(),
            provider.clone(),
            registry
        );

        // Get all tools from router (native tools)
        let native_tools = service.tool_router.list_all();
        let native_count = native_tools.len();

        // Verify we have the expected number of native tools (10)
        // search_by_properties, search_by_tags, list_notes_in_folder, search_by_filename,
        // search_by_content, semantic_search, build_search_index, get_note_metadata,
        // update_note_properties, get_vault_stats, __run_rune_tool
        assert_eq!(native_count, 11, "Expected 11 native tools (10 + __run_rune_tool), got {}", native_count);

        // Now verify that list_tools would include both native and Rune tools
        // We can't easily call list_tools directly without a RequestContext,
        // but we can verify the logic by checking the registry
        if let Some(reg) = &service.rune_registry {
            let reg = reg.read().await;
            let rune_tools = reg.list_tools();
            assert_eq!(rune_tools.len(), 1);
            assert_eq!(rune_tools[0].name, "test_tool");
            assert_eq!(rune_tools[0].description, "A test tool for discovery");
        } else {
            panic!("Rune registry should be Some");
        }
    }
}
