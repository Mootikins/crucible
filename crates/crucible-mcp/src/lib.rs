pub mod tools;
pub mod types;
pub mod database;

use anyhow::Result;
use database::EmbeddingDatabase;
use types::{McpTool, ToolCallArgs, ToolCallResult};
use serde_json::Value;

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
        
        // TODO: Implement full MCP protocol communication over stdio
        // This would involve:
        // 1. Setting up stdio streams for reading/writing JSON-RPC messages
        // 2. Handling MCP protocol messages (initialize, list_tools, call_tool)
        // 3. Processing tool calls and returning results
        // 4. Error handling and logging
        
        // For now, just log that the server is ready
        tracing::info!("MCP Server ready for tool calls");
        
        // In a real implementation, this would be an infinite loop
        // that processes stdio input/output
        Ok(())
    }
}

