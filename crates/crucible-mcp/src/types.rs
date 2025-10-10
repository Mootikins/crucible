// crates/crucible-mcp/src/types.rs
//
// Type System Structure:
// ======================
//
// DOMAIN TYPES (Keep - these are crucible-specific business logic types):
// - FileInfo, SearchResult, SearchResultWithScore
// - EmbeddingMetadata, EmbeddingData, SearchRequest
// - ToolCallArgs, ToolCallResult, McpTool
// - ServerCapabilities, InitializeRequest/Response, etc.
//
// PROTOCOL TYPES (Remove in Phase 5 - replaced by rmcp):
// - JsonRpcError (marked with TODO comments below)
// - JsonRpcRequest, JsonRpcResponse, JsonRpcNotification (in protocol.rs)
//
// All domain types derive Debug, Clone, Serialize, Deserialize for:
// - JSON serialization (serde_json)
// - rmcp compatibility (requires Serialize + Deserialize)
// - Thread safety (all types are Send + Sync by default via std types)

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

/// Legacy tool call arguments (deprecated - use specific types below)
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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

// Specific parameter types for each tool
// These provide clear schemas to AI models about required vs optional fields

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SearchByPropertiesParams {
    /// Property key-value pairs to match
    pub properties: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SearchByTagsParams {
    /// Tags to search for
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SearchByFolderParams {
    /// Folder path to search in
    pub path: String,
    /// Search recursively in subfolders (default: true)
    #[serde(default = "default_recursive")]
    pub recursive: bool,
}

fn default_recursive() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SearchByFilenameParams {
    /// Filename pattern to match
    pub pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SearchByContentParams {
    /// Search query text
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SemanticSearchParams {
    /// Search query text
    pub query: String,
    /// Number of results to return (default: 10)
    #[serde(default = "default_top_k")]
    pub top_k: u32,
}

fn default_top_k() -> u32 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IndexVaultParams {
    /// Vault path to index (default: current directory)
    #[serde(default = "default_path")]
    pub path: String,
    /// File pattern to match (default: "**/*.md")
    #[serde(default = "default_pattern")]
    pub pattern: String,
    /// Re-index existing files (default: false)
    #[serde(default)]
    pub force: bool,
}

fn default_path() -> String {
    ".".to_string()
}

fn default_pattern() -> String {
    "**/*.md".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetNoteMetadataParams {
    /// Note file path
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateNotePropertiesParams {
    /// Note file path
    pub path: String,
    /// Properties to update
    pub properties: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct IndexDocumentParams {
    /// Document to index
    pub document: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SearchDocumentsParams {
    /// Search query text
    pub query: String,
    /// Number of results to return (default: 10)
    #[serde(default = "default_top_k")]
    pub top_k: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GetDocumentStatsParams {}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateDocumentPropertiesParams {
    /// Document ID
    pub document_id: String,
    /// Properties to update
    pub properties: HashMap<String, serde_json::Value>,
}

/// MCP Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

// MCP Protocol Types

/// MCP Protocol capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    pub tools: Option<ToolsCapability>,
}

/// Tool capability structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCapability {
    /// Whether the server supports listing available tools
    pub list_changed: Option<bool>,
}

/// MCP initialization request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeRequest {
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    pub client_info: ClientInfo,
}

/// Client capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCapabilities {
    pub experimental: Option<serde_json::Value>,
}

/// Client information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

/// MCP initialization response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResponse {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: ServerInfo,
}

/// Server information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// MCP tool list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListToolsResponse {
    pub tools: Vec<McpTool>,
}

/// MCP tool call request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolRequest {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// MCP tool call response content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolResponse {
    pub content: Vec<McpContent>,
    #[serde(rename = "isError")]
    pub is_error: Option<bool>,
}

/// MCP content types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    #[serde(rename = "resource")]
    Resource { resource: ResourceContent },
}

/// Resource content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContent {
    pub uri: String,
    pub mime_type: Option<String>,
    pub text: Option<String>,
}

// ==============================================================================
// JSON-RPC Protocol Types
// ==============================================================================
// TODO: Phase 5 - Remove this type, replaced by rmcp's error handling
// This is a legacy JSON-RPC protocol type that will be superseded by rmcp

/// JSON-RPC Error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}
