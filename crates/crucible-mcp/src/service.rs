// crates/crucible-mcp/src/service.rs
//
// rmcp-based MCP Server Service Layer
//
// This module provides the official rmcp-based implementation of the Crucible MCP server.
// It wraps existing tool implementations from tools::mod with rmcp's #[tool] macro.

use rmcp::{ErrorData as McpError, model::*, tool, tool_router, handler::server::{wrapper::Parameters, ServerHandler, tool::ToolRouter}};
use crate::database::EmbeddingDatabase;
use crate::embeddings::EmbeddingProvider;
use crate::rune_tools::AsyncToolRegistry;
use crate::errors::{CrucibleError, ErrorSeverity};
use crucible_surrealdb::SurrealClient;
use crucible_core::{DocumentDB, RelationalDB, GraphDB};
use std::sync::Arc;
use std::collections::HashMap;

/// Convert CrucibleError to McpError with appropriate error handling
fn crucible_error_to_mcp(error: CrucibleError) -> McpError {
    match error.severity {
        ErrorSeverity::Critical => McpError::internal_error(error.details(), None),
        ErrorSeverity::Error => McpError::internal_error(error.message, None),
        ErrorSeverity::Warning => McpError::internal_error(error.message, None),
        ErrorSeverity::Info => McpError::internal_error(error.message, None),
    }
}

/// Crucible MCP Service using rmcp SDK
///
/// This service exposes native Crucible MCP tools via the rmcp protocol,
/// multi-model database tools via SurrealClient, plus dynamically loaded
/// Rune-based tools from the tool registry.
#[derive(Clone)]
pub struct CrucibleMcpService {
    database: Arc<EmbeddingDatabase>,
    provider: Arc<dyn EmbeddingProvider>,
    surreal_client: Option<Arc<SurrealClient>>,
    tool_router: ToolRouter<Self>,
    rune_registry: Option<Arc<AsyncToolRegistry>>,
}

#[tool_router]
impl CrucibleMcpService {
    /// Create a new Crucible MCP service instance without Rune tools
    pub fn new(database: Arc<EmbeddingDatabase>, provider: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            database,
            provider,
            surreal_client: None,
            tool_router: Self::tool_router(),
            rune_registry: None,
        }
    }

    /// Create a new Crucible MCP service instance with multi-model database support
    pub fn with_multi_model(
        database: Arc<EmbeddingDatabase>,
        provider: Arc<dyn EmbeddingProvider>,
        surreal_client: Arc<SurrealClient>,
    ) -> Self {
        Self {
            database,
            provider,
            surreal_client: Some(surreal_client),
            tool_router: Self::tool_router(),
            rune_registry: None,
        }
    }

    /// Create a new Crucible MCP service instance with Rune tool support
    pub fn with_rune_tools(
        database: Arc<EmbeddingDatabase>,
        provider: Arc<dyn EmbeddingProvider>,
        rune_registry: Arc<AsyncToolRegistry>,
    ) -> Self {
        Self {
            database,
            provider,
            surreal_client: None,
            tool_router: Self::tool_router(),
            rune_registry: Some(rune_registry),
        }
    }

    /// Create a new Crucible MCP service instance with async Rune tool loading
    ///
    /// This constructor uses enhanced discovery for better tool discovery and schema generation.
    pub async fn with_enhanced_rune_tools(
        database: Arc<EmbeddingDatabase>,
        provider: Arc<dyn EmbeddingProvider>,
        tool_dir: std::path::PathBuf,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Create Obsidian client for stdlib
        let obsidian_client = crate::obsidian_client::ObsidianClient::new()?;

        // Create AsyncToolRegistry with enhanced discovery
        let async_registry = AsyncToolRegistry::new_with_stdlib(
            tool_dir,
            Arc::clone(&database),
            Arc::new(obsidian_client),
        ).await?;

        Ok(Self {
            database,
            provider,
            surreal_client: None,
            tool_router: Self::tool_router(),
            rune_registry: Some(Arc::new(async_registry)),
        })
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

        // Get tool and context from AsyncToolRegistry
        let tool = registry.get_tool(&params.tool_name).await
            .ok_or_else(|| McpError::internal_error(format!("Rune tool '{}' not found", params.tool_name), None))?;
        let context = registry.context().await;

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

    // =============================================================================
    // Multi-Model Database Tools (Relational, Graph, Document)
    // =============================================================================

    // Relational Database Tools

    /// Create a new relational table
    #[tool(description = "[RELATIONAL] Create a new table with specified columns")]
    async fn relational_create_table(
        &self,
        Parameters(params): Parameters<crate::types::RelationalCreateTableParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.surreal_client.as_ref()
            .ok_or_else(|| McpError::internal_error("Multi-model database not enabled".to_string(), None))?;

        // Convert parameter types to core types
        let columns: Vec<crucible_core::ColumnDefinition> = params.columns.into_iter().map(|col| {
            crucible_core::ColumnDefinition {
                name: col.name,
                data_type: self.convert_data_type(&col.data_type),
                nullable: col.nullable,
                default_value: col.default_value,
                unique: col.unique,
            }
        }).collect();

        let schema = crucible_core::TableSchema {
            name: params.table.clone(),
            columns,
            primary_key: params.primary_key,
            foreign_keys: vec![],
            indexes: vec![],
        };

        client.create_table(&params.table, schema)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(format!("Table '{}' created successfully", params.table))]))
    }

    /// Insert records into a relational table
    #[tool(description = "[RELATIONAL] Insert records into a table")]
    async fn relational_insert(
        &self,
        Parameters(params): Parameters<crate::types::RelationalInsertParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.surreal_client.as_ref()
            .ok_or_else(|| McpError::internal_error("Multi-model database not enabled".to_string(), None))?;

        // Convert JSON records to Record structs
        let records: Vec<crucible_core::Record> = params.records.into_iter().map(|record_json| {
            crucible_core::Record {
                id: None, // Will auto-generate
                data: if let serde_json::Value::Object(map) = record_json {
                    map.into_iter().collect()
                } else {
                    HashMap::new()
                },
            }
        }).collect();

        let result = client.insert_batch(&params.table, records)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let content = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    /// Select records from a relational table
    #[tool(description = "[RELATIONAL] Query records from a table with filters")]
    async fn relational_select(
        &self,
        Parameters(params): Parameters<crate::types::RelationalSelectParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.surreal_client.as_ref()
            .ok_or_else(|| McpError::internal_error("Multi-model database not enabled".to_string(), None))?;

        // Build SelectQuery from parameters
        let query = crucible_core::SelectQuery {
            table: params.table.clone(),
            columns: if params.columns.is_empty() { None } else { Some(params.columns) },
            filter: params.filter.as_ref().and_then(|f| self.convert_filter(f)),
            order_by: if params.order_by.is_empty() { None } else { Some(self.convert_order_clauses(&params.order_by)) },
            limit: params.limit,
            offset: params.offset,
            joins: None,
        };

        let result = client.select(query)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let content = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    /// Update records in a relational table
    #[tool(description = "[RELATIONAL] Update records in a table")]
    async fn relational_update(
        &self,
        Parameters(params): Parameters<crate::types::RelationalUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.surreal_client.as_ref()
            .ok_or_else(|| McpError::internal_error("Multi-model database not enabled".to_string(), None))?;

        let filter = self.convert_filter(&params.filter)
            .ok_or_else(|| McpError::internal_error("Invalid filter format".to_string(), None))?;

        let updates = self.convert_updates(&params.updates);

        let result = client.update(&params.table, filter, updates)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let content = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    /// Delete records from a relational table
    #[tool(description = "[RELATIONAL] Delete records from a table")]
    async fn relational_delete(
        &self,
        Parameters(params): Parameters<crate::types::RelationalDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.surreal_client.as_ref()
            .ok_or_else(|| McpError::internal_error("Multi-model database not enabled".to_string(), None))?;

        let filter = self.convert_filter(&params.filter)
            .ok_or_else(|| McpError::internal_error("Invalid filter format".to_string(), None))?;

        let result = client.delete(&params.table, filter)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let content = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    // Graph Database Tools

    /// Create a new graph node
    #[tool(description = "[GRAPH] Create a new node with specified label and properties")]
    async fn graph_create_node(
        &self,
        Parameters(params): Parameters<crate::types::GraphCreateNodeParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.surreal_client.as_ref()
            .ok_or_else(|| McpError::internal_error("Multi-model database not enabled".to_string(), None))?;

        let node_id = client.create_node(&params.label, params.properties.clone())
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(format!("Node '{}' created successfully", node_id))]))
    }

    /// Create a new graph edge
    #[tool(description = "[GRAPH] Create an edge between two nodes")]
    async fn graph_create_edge(
        &self,
        Parameters(params): Parameters<crate::types::GraphCreateEdgeParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.surreal_client.as_ref()
            .ok_or_else(|| McpError::internal_error("Multi-model database not enabled".to_string(), None))?;

        let from_node = crucible_core::NodeId(params.from_node);
        let to_node = crucible_core::NodeId(params.to_node);

        let edge_id = client.create_edge(&from_node, &to_node, &params.label, params.properties.clone())
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(format!("Edge '{}' created successfully", edge_id))]))
    }

    /// Get neighbors of a graph node
    #[tool(description = "[GRAPH] Get neighboring nodes and edges")]
    async fn graph_get_neighbors(
        &self,
        Parameters(params): Parameters<crate::types::GraphGetNeighborsParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.surreal_client.as_ref()
            .ok_or_else(|| McpError::internal_error("Multi-model database not enabled".to_string(), None))?;

        // Convert string node ID to NodeId
        let node_id = crucible_core::NodeId(params.node_id);

        // Convert direction string to Direction enum
        let direction = match params.direction.as_str() {
            "incoming" => crucible_core::Direction::Incoming,
            "both" => crucible_core::Direction::Both,
            _ => crucible_core::Direction::Outgoing, // default
        };

        // Convert edge filter parameters
        let edge_filter = if params.edge_labels.is_some() || params.edge_properties.is_some() {
            Some(crucible_core::EdgeFilter {
                labels: params.edge_labels,
                properties: params.edge_properties,
            })
        } else {
            None
        };

        let result = client.get_neighbors(&node_id, direction, edge_filter)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let content = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    /// Perform graph traversal
    #[tool(description = "[GRAPH] Traverse graph following specified patterns")]
    async fn graph_traversal(
        &self,
        Parameters(params): Parameters<crate::types::GraphTraversalParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.surreal_client.as_ref()
            .ok_or_else(|| McpError::internal_error("Multi-model database not enabled".to_string(), None))?;

        // Convert start node ID
        let start_node = crucible_core::NodeId(params.start_node);

        // Convert traversal pattern - simplified version
        let pattern = crucible_core::TraversalPattern {
            steps: params.pattern.steps.into_iter().map(|step| {
                crucible_core::TraversalStep {
                    direction: match step.direction.as_str() {
                        "incoming" => crucible_core::Direction::Incoming,
                        "both" => crucible_core::Direction::Both,
                        _ => crucible_core::Direction::Outgoing,
                    },
                    edge_filter: if step.edge_labels.is_some() || step.edge_properties.is_some() {
                        Some(crucible_core::EdgeFilter {
                            labels: step.edge_labels,
                            properties: step.edge_properties,
                        })
                    } else {
                        None
                    },
                    node_filter: None,
                    min_hops: step.min_hops,
                    max_hops: step.max_hops,
                }
            }).collect(),
        };

        let result = client.traverse(&start_node, pattern, params.max_depth)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let content = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    /// Perform graph analytics
    #[tool(description = "[GRAPH] Analyze graph structure (centrality, page rank, etc.)")]
    async fn graph_analytics(
        &self,
        Parameters(params): Parameters<crate::types::GraphAnalyticsParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.surreal_client.as_ref()
            .ok_or_else(|| McpError::internal_error("Multi-model database not enabled".to_string(), None))?;

        // Convert node IDs
        let node_ids = params.node_ids.map(|ids| {
            ids.into_iter().map(|id| crucible_core::NodeId(id)).collect()
        });

        // Convert analysis operation
        let analysis = match &params.analysis {
            crate::types::GraphAnalyticsOperation::DegreeCentrality { direction } => {
                crucible_core::GraphAnalysis::DegreeCentrality {
                    direction: match direction.as_str() {
                        "incoming" => crucible_core::Direction::Incoming,
                        "both" => crucible_core::Direction::Both,
                        _ => crucible_core::Direction::Outgoing,
                    },
                }
            },
            crate::types::GraphAnalyticsOperation::PageRank { damping_factor, iterations } => {
                crucible_core::GraphAnalysis::PageRank {
                    damping_factor: *damping_factor,
                    iterations: *iterations,
                }
            },
        };

        let result = client.graph_analytics(node_ids, analysis)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let content = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    // Document Database Tools

    /// Create a new document collection
    #[tool(description = "[DOCUMENT] Create a new collection with optional schema")]
    async fn document_create_collection(
        &self,
        Parameters(params): Parameters<crate::types::DocumentCreateCollectionParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.surreal_client.as_ref()
            .ok_or_else(|| McpError::internal_error("Multi-model database not enabled".to_string(), None))?;

        // Convert schema if present
        let schema = params.schema.as_ref().map(|schema| {
            crucible_core::DocumentSchema {
                fields: schema.fields.iter().map(|field| {
                    crucible_core::FieldDefinition {
                        name: field.name.clone(),
                        field_type: match field.field_type.as_str() {
                            "string" => crucible_core::DocumentFieldType::String,
                            "integer" => crucible_core::DocumentFieldType::Integer,
                            "float" => crucible_core::DocumentFieldType::Float,
                            "boolean" => crucible_core::DocumentFieldType::Boolean,
                            "array" => crucible_core::DocumentFieldType::Array,
                            "object" => crucible_core::DocumentFieldType::Object,
                            "datetime" => crucible_core::DocumentFieldType::DateTime,
                            "text" => crucible_core::DocumentFieldType::Text,
                            _ => crucible_core::DocumentFieldType::String,
                        },
                        required: field.required,
                        index: Some(false), // Default to not indexed
                    }
                }).collect(),
                validation: None, // No validation rules by default
            }
        });

        client.create_collection(&params.name, schema)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(format!("Collection '{}' created successfully", params.name))]))
    }

    /// Create a new document
    #[tool(description = "[DOCUMENT] Create a new document in a collection")]
    async fn document_create(
        &self,
        Parameters(params): Parameters<crate::types::DocumentCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.surreal_client.as_ref()
            .ok_or_else(|| McpError::internal_error("Multi-model database not enabled".to_string(), None))?;

        // Convert metadata if present
        let metadata = params.metadata.as_ref().map(|meta| {
            crucible_core::DocumentMetadata {
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                version: 1,
                content_type: meta.content_type.clone(),
                tags: meta.tags.clone(),
                collection: Some(params.collection.clone()),
            }
        });

        let document = crucible_core::Document {
            id: None,
            content: params.content.clone(),
            metadata: metadata.unwrap_or_else(|| crucible_core::DocumentMetadata {
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                version: 1,
                content_type: Some("application/json".to_string()),
                tags: vec![],
                collection: Some(params.collection.clone()),
            }),
        };

        let doc_id = client.create_document(&params.collection, document)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(format!("Document '{}' created successfully", doc_id))]))
    }

    /// Query documents
    #[tool(description = "[DOCUMENT] Query documents with filters and projections")]
    async fn document_query(
        &self,
        Parameters(params): Parameters<crate::types::DocumentQueryParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.surreal_client.as_ref()
            .ok_or_else(|| McpError::internal_error("Multi-model database not enabled".to_string(), None))?;

        // Build DocumentQuery from parameters
        let query = crucible_core::DocumentQuery {
            collection: params.collection.clone(),
            filter: params.filter.as_ref().and_then(|f| self.convert_document_filter(f)),
            projection: if params.projection.is_empty() { None } else { Some(params.projection) },
            sort: if params.sort.is_empty() { None } else { Some(
                params.sort.iter().map(|s| crucible_core::DocumentSort {
                    field: s.field.clone(),
                    direction: if s.direction == "desc" {
                        crucible_core::OrderDirection::Desc
                    } else {
                        crucible_core::OrderDirection::Asc
                    },
                }).collect()
            )},
            limit: params.limit,
            skip: params.skip,
        };

        let result = client.query_documents(&params.collection, query)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let content = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    /// Search documents
    #[tool(description = "[DOCUMENT] Full-text search in documents")]
    async fn document_search(
        &self,
        Parameters(params): Parameters<crate::types::DocumentSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.surreal_client.as_ref()
            .ok_or_else(|| McpError::internal_error("Multi-model database not enabled".to_string(), None))?;

        // Convert search options
        let options = crucible_core::SearchOptions {
            fields: params.options.fields.clone(),
            fuzzy: params.options.fuzzy,
            boost_fields: None, // No field boosting by default
            limit: params.options.limit,
            highlight: params.options.highlight,
        };

        let result = client.full_text_search(&params.collection, &params.query, options)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let content = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    /// Aggregate documents
    #[tool(description = "[DOCUMENT] Aggregate documents using pipeline stages")]
    async fn document_aggregate(
        &self,
        Parameters(params): Parameters<crate::types::DocumentAggregateParams>,
    ) -> Result<CallToolResult, McpError> {
        let client = self.surreal_client.as_ref()
            .ok_or_else(|| McpError::internal_error("Multi-model database not enabled".to_string(), None))?;

        // Convert aggregation pipeline
        let pipeline = crucible_core::AggregationPipeline {
            stages: params.pipeline.into_iter().map(|stage| {
                match stage {
                    crate::types::DocumentAggregationStage::Match { filter } => {
                        crucible_core::AggregationStage::Match {
                            filter: self.convert_document_filter(&filter).unwrap_or_else(|| {
                                crucible_core::DocumentFilter::And(vec![])
                            })
                        }
                    },
                    crate::types::DocumentAggregationStage::Group { id, operations } => {
                        crucible_core::AggregationStage::Group {
                            id,
                            operations: operations.into_iter().map(|op| {
                                crucible_core::GroupOperation {
                                    field: op.field,
                                    operation: match op.operation.as_str() {
                                        "count" => crucible_core::AggregateType::Count,
                                        "sum" => crucible_core::AggregateType::Sum,
                                        "avg" | "average" => crucible_core::AggregateType::Average,
                                        "min" => crucible_core::AggregateType::Min,
                                        "max" => crucible_core::AggregateType::Max,
                                        _ => crucible_core::AggregateType::Count, // Default
                                    },
                                    alias: op.alias,
                                }
                            }).collect(),
                        }
                    },
                    crate::types::DocumentAggregationStage::Sort { sort } => {
                        crucible_core::AggregationStage::Sort {
                            sort: sort.into_iter().map(|s| crucible_core::DocumentSort {
                                field: s.field,
                                direction: if s.direction == "desc" {
                                    crucible_core::OrderDirection::Desc
                                } else {
                                    crucible_core::OrderDirection::Asc
                                },
                            }).collect(),
                        }
                    },
                    crate::types::DocumentAggregationStage::Limit { limit } => {
                        crucible_core::AggregationStage::Limit { limit }
                    },
                    crate::types::DocumentAggregationStage::Skip { skip } => {
                        crucible_core::AggregationStage::Skip { skip }
                    },
                    crate::types::DocumentAggregationStage::Project { projection } => {
                        crucible_core::AggregationStage::Project { projection }
                    },
                }
            }).collect(),
        };

        let result = client.aggregate_documents(&params.collection, pipeline)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let content = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(content)]))
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

    // =============================================================================
    // Helper Methods for Multi-Model Operations
    // =============================================================================

    /// Convert string data type to core DataType
    fn convert_data_type(&self, type_str: &str) -> crucible_core::DataType {
        match type_str.to_lowercase().as_str() {
            "string" | "text" => crucible_core::DataType::String,
            "integer" | "int" => crucible_core::DataType::Integer,
            "float" | "double" | "number" => crucible_core::DataType::Float,
            "boolean" | "bool" => crucible_core::DataType::Boolean,
            "array" => crucible_core::DataType::Array(Box::new(crucible_core::DataType::String)),
            "json" | "object" => crucible_core::DataType::Json,
            "datetime" | "timestamp" => crucible_core::DataType::DateTime,
            _ => crucible_core::DataType::String, // Default fallback
        }
    }

    /// Convert JSON filter to core FilterClause
    fn convert_filter(&self, filter: &serde_json::Value) -> Option<crucible_core::FilterClause> {
        if let serde_json::Value::Object(map) = filter {
            if map.len() == 1 {
                if let Some((field, value)) = map.iter().next() {
                    return Some(crucible_core::FilterClause::Equals {
                        column: field.clone(),
                        value: value.clone(),
                    });
                }
            }
        }
        None // Simplified - would handle complex filters in production
    }

    /// Convert order clauses to core OrderClause
    fn convert_order_clauses(&self, clauses: &[crate::types::RelationalOrderClause]) -> Vec<crucible_core::OrderClause> {
        clauses.iter().map(|clause| {
            crucible_core::OrderClause {
                column: clause.column.clone(),
                direction: if clause.direction == "desc" {
                    crucible_core::OrderDirection::Desc
                } else {
                    crucible_core::OrderDirection::Asc
                },
            }
        }).collect()
    }

    /// Convert JSON updates to core UpdateClause
    fn convert_updates(&self, updates: &HashMap<String, serde_json::Value>) -> crucible_core::UpdateClause {
        crucible_core::UpdateClause {
            assignments: updates.clone(),
        }
    }

    /// Convert JSON document filter to core DocumentFilter
    fn convert_document_filter(&self, filter: &serde_json::Value) -> Option<crucible_core::DocumentFilter> {
        if let serde_json::Value::Object(map) = filter {
            if map.len() == 1 {
                if let Some((field, value)) = map.iter().next() {
                    return Some(crucible_core::DocumentFilter::Equals {
                        field: field.clone(),
                        value: value.clone(),
                    });
                }
            }
        }
        None // Simplified - would handle complex filters in production
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
            tracing::info!("🔍 Adding Rune tools to MCP tool list");

            let rune_tools = registry.list_tools().await;
            tracing::info!("🔍 Discovered {} Rune tools", rune_tools.len());

            // Check if enhanced discovery is being used
            let enhanced_mode = registry.is_enhanced_mode().await;
            tracing::info!("🔍 Enhanced discovery mode: {}", enhanced_mode);

            for (i, tool_meta) in rune_tools.iter().enumerate() {
                tracing::info!("🔍 Rune tool {}: name='{}', desc='{}'", i, tool_meta.name, tool_meta.description);

                // Convert input_schema from Value to Map<String, Value>
                let input_schema = match &tool_meta.input_schema {
                    serde_json::Value::Object(map) => {
                        tracing::info!("🔍 Converting input_schema for tool '{}', {} properties",
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
                            tracing::info!("🔍 Converting output_schema for tool '{}'", tool_meta.name);
                            Some(Arc::new(map.clone()))
                        },
                        _ => {
                            tracing::warn!("Rune tool '{}' has non-object output_schema, ignoring", tool_meta.name);
                            None
                        }
                    }
                });

                // Add enhanced annotations for enhanced discovery tools
                let annotations = if enhanced_mode {
                    Some(ToolAnnotations {
                        title: Some(format!("{} (Enhanced Discovery)", tool_meta.name)),
                        read_only_hint: Some(true), // Most Rune tools are read-only
                        destructive_hint: Some(false),
                        idempotent_hint: Some(false),
                        open_world_hint: Some(true), // Rune tools can interact with external systems
                    })
                } else {
                    None
                };

                // Convert ToolMetadata to rmcp::model::Tool
                let rune_tool = Tool {
                    name: Cow::Owned(tool_meta.name.clone()),
                    title: None,
                    description: Some(Cow::Owned(tool_meta.description.clone())),
                    input_schema,
                    output_schema,
                    annotations,
                    icons: None,
                };

                tracing::info!("🔍 Adding Rune tool '{}' to MCP tool list", tool_meta.name);
                all_tools.push(rune_tool);
            }
        } else {
            tracing::warn!("🔍 No rune_registry available - Rune tools disabled");
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

        // Create Rune context and async registry
        let context = rune::Context::with_default_modules().unwrap();
        let async_registry = crate::rune_tools::AsyncToolRegistry::new(
            tools_dir,
            Arc::new(context)
        ).await.unwrap();

        // Verify tool was loaded in registry
        assert_eq!(async_registry.tool_count().await, 1);
        assert!(async_registry.has_tool("test_tool").await);

        // Create service with Rune tools
        let service = CrucibleMcpService::with_rune_tools(
            db.clone(),
            provider.clone(),
            Arc::new(async_registry)
        );

        // Get all tools from router (native tools)
        let native_tools = service.tool_router.list_all();
        let native_count = native_tools.len();

        // Verify we have the expected number of native tools
        // Original 10: search_by_properties, search_by_tags, list_notes_in_folder, search_by_filename,
        // search_by_content, semantic_search, build_search_index, get_note_metadata,
        // update_note_properties, get_vault_stats
        // Plus: __run_rune_tool (1)
        // Plus multi-model database tools (15):
        //   Relational (5): relational_create_table, relational_insert_records, relational_select, relational_update, relational_delete
        //   Graph (3): graph_create_node, graph_create_edge, graph_get_neighbors, graph_traverse, graph_analytics
        //   Document (5): document_create_collection, document_create, document_query, document_search, document_aggregate
        // Total: 10 + 1 + 15 = 26
        assert_eq!(native_count, 26, "Expected 26 native tools (10 vault + 1 rune + 15 multi-model), got {}", native_count);

        // Now verify that list_tools would include both native and Rune tools
        // We can't easily call list_tools directly without a RequestContext,
        // but we can verify the logic by checking the registry
        if let Some(async_reg) = &service.rune_registry {
            let rune_tools = async_reg.list_tools().await;
            assert_eq!(rune_tools.len(), 1);
            assert_eq!(rune_tools[0].name, "test_tool");
            assert_eq!(rune_tools[0].description, "A test tool for discovery");
        } else {
            panic!("Rune registry should be Some");
        }
    }

    #[tokio::test]
    async fn test_convert_result_success_with_data() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap());
        let provider = Arc::new(TestEmbeddingProvider);
        let service = CrucibleMcpService::new(db, provider);

        let result = crate::types::ToolCallResult {
            success: true,
            data: Some(serde_json::json!({"message": "test data"})),
            error: None,
        };

        let converted = service.convert_result(result);
        assert!(converted.is_ok());
        let call_result = converted.unwrap();
        assert!(!call_result.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn test_convert_result_success_without_data() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap());
        let provider = Arc::new(TestEmbeddingProvider);
        let service = CrucibleMcpService::new(db, provider);

        let result = crate::types::ToolCallResult {
            success: true,
            data: None,
            error: None,
        };

        let converted = service.convert_result(result);
        assert!(converted.is_ok());
        let call_result = converted.unwrap();
        assert!(!call_result.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn test_convert_result_error_with_message() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap());
        let provider = Arc::new(TestEmbeddingProvider);
        let service = CrucibleMcpService::new(db, provider);

        let result = crate::types::ToolCallResult {
            success: false,
            data: None,
            error: Some("Test error message".to_string()),
        };

        let converted = service.convert_result(result);
        assert!(converted.is_ok());
        let call_result = converted.unwrap();
        assert!(call_result.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn test_convert_result_error_with_partial_data() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap());
        let provider = Arc::new(TestEmbeddingProvider);
        let service = CrucibleMcpService::new(db, provider);

        let result = crate::types::ToolCallResult {
            success: false,
            data: Some(serde_json::json!({"partial": "data"})),
            error: Some("Partial failure".to_string()),
        };

        let converted = service.convert_result(result);
        assert!(converted.is_ok());
        let call_result = converted.unwrap();
        assert!(call_result.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn test_convert_result_error_without_message() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap());
        let provider = Arc::new(TestEmbeddingProvider);
        let service = CrucibleMcpService::new(db, provider);

        let result = crate::types::ToolCallResult {
            success: false,
            data: None,
            error: None, // Error without message
        };

        let converted = service.convert_result(result);
        assert!(converted.is_ok());
        let call_result = converted.unwrap();
        assert!(call_result.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn test_get_info() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap());
        let provider = Arc::new(TestEmbeddingProvider);
        let service = CrucibleMcpService::new(db, provider);

        let info = service.get_info();
        assert_eq!(info.server_info.name, "crucible-mcp");
        assert_eq!(info.server_info.version, "0.1.0");
        assert!(info.instructions.is_some());
        assert!(info.capabilities.tools.is_some());
    }

    #[tokio::test]
    async fn test_service_without_rune_tools() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap());
        let provider = Arc::new(TestEmbeddingProvider);

        let service = CrucibleMcpService::new(db, provider);
        assert!(service.rune_registry.is_none());
    }

    #[tokio::test]
    async fn test_service_with_rune_tools() {
        use std::fs;

        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap());
        let provider = Arc::new(TestEmbeddingProvider);

        let tools_dir = temp_dir.path().join("tools");
        fs::create_dir_all(&tools_dir).unwrap();

        let context = rune::Context::with_default_modules().unwrap();
        let async_registry = crate::rune_tools::AsyncToolRegistry::new(
            tools_dir,
            Arc::new(context)
        ).await.unwrap();

        let service = CrucibleMcpService::with_rune_tools(db, provider, Arc::new(async_registry));
        assert!(service.rune_registry.is_some());
    }

    #[tokio::test]
    async fn test_tool_router_lists_all_native_tools() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(EmbeddingDatabase::new(db_path.to_str().unwrap()).await.unwrap());
        let provider = Arc::new(TestEmbeddingProvider);

        let service = CrucibleMcpService::new(db, provider);
        let tools = service.tool_router.list_all();

        // Should have 11 native tools (10 Crucible tools + __run_rune_tool)
        assert_eq!(tools.len(), 11);
    }
}
