// crates/crucible-mcp/src/lib.rs
pub mod database;
pub mod embeddings;
pub mod integration;
pub mod obsidian_client;
pub mod protocol;
pub mod service;
pub mod tools;
pub mod types;

use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;

// Re-export important types for external use
pub use database::EmbeddingDatabase;
pub use embeddings::{EmbeddingConfig, EmbeddingProvider, create_provider};
pub use integration::*;
pub use protocol::{McpProtocolHandler, StdioMcpServer};
pub use service::CrucibleMcpService;
pub use types::*;

pub struct McpServer {
    database: EmbeddingDatabase,
    provider: Arc<dyn EmbeddingProvider>,
}

impl McpServer {
    pub async fn new(db_path: &str, provider: Arc<dyn EmbeddingProvider>) -> Result<Self> {
        let database = EmbeddingDatabase::new(db_path).await?;

        Ok(Self { database, provider })
    }

    /// Get all available MCP tools
    pub fn get_tools() -> Vec<McpTool> {
        vec![
            McpTool {
                name: "search_by_properties".to_string(),
                description: "[READ] Find notes by YAML properties".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "properties": {
                            "type": "object",
                            "description": "Property key-value pairs to match"
                        }
                    },
                    "required": ["properties"]
                }),
            },
            McpTool {
                name: "search_by_tags".to_string(),
                description: "[READ] Find notes by tags".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "tags": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Tags to search for"
                        }
                    },
                    "required": ["tags"]
                }),
            },
            McpTool {
                name: "search_by_folder".to_string(),
                description: "[READ] List notes in folder (recursive)".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Folder path"},
                        "recursive": {
                            "type": "boolean",
                            "description": "Search subfolders",
                            "default": true
                        }
                    },
                    "required": ["path"]
                }),
            },
            McpTool {
                name: "search_by_filename".to_string(),
                description: "[READ] Find notes matching filename".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Filename pattern to match"
                        }
                    },
                    "required": ["pattern"]
                }),
            },
            McpTool {
                name: "search_by_content".to_string(),
                description: "[READ] Full-text search in note contents".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "Search query"}
                    },
                    "required": ["query"]
                }),
            },
            McpTool {
                name: "semantic_search".to_string(),
                description: "[READ] Semantic search (needs index_vault first)".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "Search query"},
                        "top_k": {
                            "type": "integer",
                            "description": "Number of results",
                            "default": 10
                        }
                    },
                    "required": ["query"]
                }),
            },
            McpTool {
                name: "index_vault".to_string(),
                description: "[INDEX] Generate embeddings for notes (slow)".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "force": {
                            "type": "boolean",
                            "description": "Re-index existing files",
                            "default": false
                        }
                    }
                }),
            },
            McpTool {
                name: "get_note_metadata".to_string(),
                description: "[READ] Get note metadata and frontmatter".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Note file path"}
                    },
                    "required": ["path"]
                }),
            },
            McpTool {
                name: "update_note_properties".to_string(),
                description: "[WRITE] Update note frontmatter properties".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Note file path"},
                        "properties": {
                            "type": "object",
                            "description": "Properties to update"
                        }
                    },
                    "required": ["path", "properties"]
                }),
            },
            McpTool {
                name: "index_document".to_string(),
                description: "[INDEX] Index document for search".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "document": {
                            "type": "object",
                            "description": "Document to index"
                        }
                    },
                    "required": ["document"]
                }),
            },
            McpTool {
                name: "search_documents".to_string(),
                description: "[READ] Search indexed documents".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "Search query"},
                        "top_k": {
                            "type": "integer",
                            "description": "Number of results",
                            "default": 10
                        }
                    },
                    "required": ["query"]
                }),
            },
            McpTool {
                name: "get_document_stats".to_string(),
                description: "[READ] Get indexing statistics".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            McpTool {
                name: "update_document_properties".to_string(),
                description: "[WRITE] Update document properties".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "document_id": {"type": "string", "description": "Document ID"},
                        "properties": {
                            "type": "object",
                            "description": "Properties to update"
                        }
                    },
                    "required": ["document_id", "properties"]
                }),
            },
        ]
    }

    /// Handle a tool call
    pub async fn handle_tool_call(&self, name: &str, arguments: Value) -> Result<ToolCallResult> {
        let args: ToolCallArgs = serde_json::from_value(arguments)?;

        match name {
            "search_by_properties" => tools::search_by_properties(&self.database, &args).await,
            "search_by_tags" => tools::search_by_tags(&self.database, &args).await,
            "search_by_folder" => tools::search_by_folder(&self.database, &args).await,
            "search_by_filename" => tools::search_by_filename(&self.database, &args).await,
            "search_by_content" => tools::search_by_content(&self.database, &args).await,
            "semantic_search" => tools::semantic_search(&self.database, &self.provider, &args).await,
            "index_vault" => tools::index_vault(&self.database, &self.provider, &args).await,
            "get_note_metadata" => tools::get_note_metadata(&self.database, &args).await,
            "update_note_properties" => tools::update_note_properties(&self.database, &args).await,
            "index_document" => tools::index_document(&self.database, &self.provider, &args).await,
            "search_documents" => tools::search_documents(&self.database, &self.provider, &args).await,
            "get_document_stats" => tools::get_document_stats(&self.database, &args).await,
            "update_document_properties" => {
                tools::update_document_properties(&self.database, &args).await
            }
            _ => Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some(format!("Unknown tool: {}", name)),
            }),
        }
    }

    /// Start the MCP server
    pub async fn start(&self) -> Result<()> {
        tracing::info!("MCP Server started with {} tools", Self::get_tools().len());
        tracing::info!("MCP Server ready for tool calls");
        Ok(())
    }

    /// Start the MCP server over stdio with full protocol support
    pub async fn start_stdio(
        db_path: &str,
        provider: Arc<dyn EmbeddingProvider>,
    ) -> Result<()> {
        let mut stdio_server = StdioMcpServer::new("crucible-mcp".to_string(), "0.1.0".to_string());

        stdio_server.initialize(db_path, provider).await?;
        stdio_server.run_stdio().await?;

        Ok(())
    }
}
