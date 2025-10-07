use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// File information for local documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub title: Option<String>,
    pub folder: String,
    pub extension: String,
    pub size: u64,
    pub created: i64,
    pub modified: i64,
    pub tags: Vec<String>,
    pub properties: HashMap<String, serde_json::Value>,
}

/// Search result containing files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub files: Vec<FileInfo>,
}

/// Metadata for embeddings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingMetadata {
    pub file_path: String,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub folder: String,
    pub properties: HashMap<String, serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Embedding data with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingData {
    pub file_path: String,
    pub content: String,
    pub embedding: Vec<f32>,
    pub metadata: EmbeddingMetadata,
}

/// Search request for knowledge base
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub limit: Option<u32>,
}

/// Search result with similarity score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultWithScore {
    pub id: String,
    pub title: String,
    pub content: String,
    pub score: f64,
}

/// Tool call arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallArgs {
    pub properties: Option<HashMap<String, serde_json::Value>>,
    pub tags: Option<Vec<String>>,
    pub path: Option<String>,
    pub recursive: Option<bool>,
    pub pattern: Option<String>,
    pub query: Option<String>,
    pub top_k: Option<u32>,
    pub force: Option<bool>,
}

/// MCP Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Tool call result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub success: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
}
