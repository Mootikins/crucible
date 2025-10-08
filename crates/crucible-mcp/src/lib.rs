// crates/crucible-mcp/src/lib.rs
pub mod tools;
pub mod types;
pub mod database;
pub mod protocol;
pub mod integration;
pub mod obsidian_client;

use anyhow::Result;
use database::EmbeddingDatabase;
use serde_json::Value;

// Re-export important types for external use
pub use protocol::{StdioMcpServer, McpProtocolHandler};
pub use types::*;
pub use integration::*;

pub struct McpServer {
    database: EmbeddingDatabase,
}

impl McpServer {
    pub async fn new(db_path: &str) -> Result<Self> {
        let database = EmbeddingDatabase::new(db_path).await?;
        
        Ok(Self { database })
    }

    /// Get all available MCP tools
    pub fn get_tools() -> Vec<McpTool> {
        vec![
            McpTool {
                name: "search_by_properties".to_string(),
                description: "Search notes by frontmatter properties".to_string(),
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
                description: "Search notes by tags".to_string(),
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
                description: "Search notes in a specific folder".to_string(),
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
                description: "Search notes by filename pattern".to_string(),
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
                description: "Full-text search in note contents".to_string(),
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
                description: "Semantic search using embeddings".to_string(),
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
                description: "Generate embeddings for all vault notes".to_string(),
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
                description: "Get metadata for a specific note".to_string(),
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
                description: "Update frontmatter properties of a note".to_string(),
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
                description: "Index a Crucible document for search".to_string(),
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
                description: "Search indexed Crucible documents".to_string(),
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
                description: "Get statistics about indexed documents".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            McpTool {
                name: "update_document_properties".to_string(),
                description: "Update properties of a Crucible document".to_string(),
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
            "search_by_properties" => {
                tools::search_by_properties(&self.database, &args).await
            }
            "search_by_tags" => {
                tools::search_by_tags(&self.database, &args).await
            }
            "search_by_folder" => {
                tools::search_by_folder(&self.database, &args).await
            }
            "search_by_filename" => {
                tools::search_by_filename(&self.database, &args).await
            }
            "search_by_content" => {
                tools::search_by_content(&self.database, &args).await
            }
            "semantic_search" => {
                tools::semantic_search(&self.database, &args).await
            }
            "index_vault" => {
                tools::index_vault(&self.database, &args).await
            }
            "get_note_metadata" => {
                tools::get_note_metadata(&self.database, &args).await
            }
            "update_note_properties" => {
                tools::update_note_properties(&self.database, &args).await
            }
            "index_document" => {
                tools::index_document(&self.database, &args).await
            }
            "search_documents" => {
                tools::search_documents(&self.database, &args).await
            }
            "get_document_stats" => {
                tools::get_document_stats(&self.database, &args).await
            }
            "update_document_properties" => {
                tools::update_document_properties(&self.database, &args).await
            }
            _ => {
                Ok(ToolCallResult {
                    success: false,
                    data: None,
                    error: Some(format!("Unknown tool: {}", name)),
                })
            }
        }
    }

    /// Start the MCP server
    pub async fn start(&self) -> Result<()> {
        tracing::info!("MCP Server started with {} tools", Self::get_tools().len());
        tracing::info!("MCP Server ready for tool calls");
        Ok(())
    }

    /// Start the MCP server over stdio with full protocol support
    pub async fn start_stdio(db_path: &str) -> Result<()> {
        let mut stdio_server = StdioMcpServer::new(
            "crucible-mcp".to_string(),
            "0.1.0".to_string(),
        );

        stdio_server.initialize(db_path).await?;
        stdio_server.run_stdio().await?;

        Ok(())
    }
}

