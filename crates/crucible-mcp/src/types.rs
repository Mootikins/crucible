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

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RuneToolParams {
    /// Name of the Rune tool to execute
    pub tool_name: String,
    /// Arguments to pass to the tool
    pub args: serde_json::Value,
}

// ==============================================================================
// Multi-Model Database Parameter Types
// ==============================================================================

// Relational Database Parameters

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RelationalCreateTableParams {
    /// Table name to create
    pub table: String,
    /// Column definitions for the table
    pub columns: Vec<RelationalColumnDefinition>,
    /// Primary key column name
    pub primary_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RelationalColumnDefinition {
    /// Column name
    pub name: String,
    /// Column data type
    pub data_type: String,
    /// Whether column can be null
    #[serde(default)]
    pub nullable: bool,
    /// Default value for the column
    pub default_value: Option<serde_json::Value>,
    /// Whether column values must be unique
    #[serde(default)]
    pub unique: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RelationalInsertParams {
    /// Table name to insert into
    pub table: String,
    /// Record data to insert
    pub records: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RelationalSelectParams {
    /// Table name to query
    pub table: String,
    /// Column names to select (empty for all)
    #[serde(default)]
    pub columns: Vec<String>,
    /// Filter conditions
    pub filter: Option<serde_json::Value>,
    /// Sort order
    #[serde(default)]
    pub order_by: Vec<RelationalOrderClause>,
    /// Maximum number of records to return
    pub limit: Option<u32>,
    /// Number of records to skip
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RelationalOrderClause {
    /// Column to sort by
    pub column: String,
    /// Sort direction
    #[serde(default = "asc")]
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RelationalUpdateParams {
    /// Table name to update
    pub table: String,
    /// Filter conditions for records to update
    pub filter: serde_json::Value,
    /// New values to set
    pub updates: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RelationalDeleteParams {
    /// Table name to delete from
    pub table: String,
    /// Filter conditions for records to delete
    pub filter: serde_json::Value,
}

// Graph Database Parameters

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphCreateNodeParams {
    /// Node label
    pub label: String,
    /// Node properties
    pub properties: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphCreateEdgeParams {
    /// Source node ID
    pub from_node: String,
    /// Target node ID
    pub to_node: String,
    /// Edge label
    pub label: String,
    /// Edge properties
    pub properties: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphGetNeighborsParams {
    /// Node ID to get neighbors for
    pub node_id: String,
    /// Direction: "outgoing", "incoming", or "both"
    #[serde(default = "outgoing")]
    pub direction: String,
    /// Edge filter by labels
    pub edge_labels: Option<Vec<String>>,
    /// Edge filter by properties
    pub edge_properties: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphTraversalParams {
    /// Starting node ID
    pub start_node: String,
    /// Traversal pattern
    pub pattern: GraphTraversalPattern,
    /// Maximum traversal depth
    #[serde(default)]
    pub max_depth: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphTraversalPattern {
    /// Traversal steps
    pub steps: Vec<GraphTraversalStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphTraversalStep {
    /// Direction: "outgoing", "incoming", or "both"
    #[serde(default = "outgoing")]
    pub direction: String,
    /// Edge labels to filter by
    pub edge_labels: Option<Vec<String>>,
    /// Edge properties to filter by
    pub edge_properties: Option<HashMap<String, serde_json::Value>>,
    /// Minimum hops
    #[serde(default)]
    pub min_hops: Option<u32>,
    /// Maximum hops
    #[serde(default)]
    pub max_hops: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphAnalyticsParams {
    /// Nodes to analyze (empty for all nodes)
    pub node_ids: Option<Vec<String>>,
    /// Analytics operation to perform
    pub analysis: GraphAnalyticsOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type")]
pub enum GraphAnalyticsOperation {
    #[serde(rename = "degree_centrality")]
    DegreeCentrality {
        /// Direction: "outgoing", "incoming", or "both"
        #[serde(default = "both")]
        direction: String,
    },
    #[serde(rename = "page_rank")]
    PageRank {
        /// Damping factor (default: 0.85)
        #[serde(default = "0.85")]
        damping_factor: Option<f64>,
        /// Number of iterations (default: 100)
        #[serde(default = "100")]
        iterations: Option<u32>,
    },
}

// Document Database Parameters

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DocumentCreateCollectionParams {
    /// Collection name
    pub name: String,
    /// Optional collection schema
    pub schema: Option<DocumentCollectionSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DocumentCollectionSchema {
    /// Field definitions
    pub fields: Vec<DocumentFieldDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DocumentFieldDefinition {
    /// Field name
    pub name: String,
    /// Field type
    pub field_type: String,
    /// Whether field is required
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DocumentCreateParams {
    /// Collection name
    pub collection: String,
    /// Document content (JSON)
    pub content: serde_json::Value,
    /// Document metadata
    pub metadata: Option<DocumentMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DocumentMetadata {
    /// Document tags
    #[serde(default)]
    pub tags: Vec<String>,
    /// Content type
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DocumentQueryParams {
    /// Collection name
    pub collection: String,
    /// Filter conditions
    pub filter: Option<serde_json::Value>,
    /// Fields to return
    #[serde(default)]
    pub projection: Vec<String>,
    /// Sort order
    #[serde(default)]
    pub sort: Vec<DocumentSortClause>,
    /// Maximum number of documents
    pub limit: Option<u32>,
    /// Number of documents to skip
    pub skip: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DocumentSortClause {
    /// Field to sort by
    pub field: String,
    /// Sort direction: "asc" or "desc"
    #[serde(default = "asc")]
    pub direction: String,
}

fn asc() -> String {
    "asc".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DocumentSearchParams {
    /// Collection name
    pub collection: String,
    /// Search query text
    pub query: String,
    /// Search options
    #[serde(default)]
    pub options: DocumentSearchOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, Default)]
pub struct DocumentSearchOptions {
    /// Fields to search in
    pub fields: Option<Vec<String>>,
    /// Enable fuzzy matching
    #[serde(default)]
    pub fuzzy: Option<bool>,
    /// Maximum number of results
    pub limit: Option<u32>,
    /// Highlight matching text
    #[serde(default)]
    pub highlight: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DocumentAggregateParams {
    /// Collection name
    pub collection: String,
    /// Aggregation pipeline stages
    pub pipeline: Vec<DocumentAggregationStage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type")]
pub enum DocumentAggregationStage {
    #[serde(rename = "match")]
    Match {
        /// Filter conditions
        filter: serde_json::Value,
    },
    #[serde(rename = "group")]
    Group {
        /// Group by field
        id: serde_json::Value,
        /// Aggregation operations
        operations: Vec<DocumentGroupOperation>,
    },
    #[serde(rename = "sort")]
    Sort {
        /// Sort order
        sort: Vec<DocumentSortClause>,
    },
    #[serde(rename = "limit")]
    Limit {
        /// Limit results
        limit: u32,
    },
    #[serde(rename = "skip")]
    Skip {
        /// Skip results
        skip: u32,
    },
    #[serde(rename = "project")]
    Project {
        /// Fields to project
        projection: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DocumentGroupOperation {
    /// Field to operate on
    pub field: String,
    /// Operation type
    pub operation: String,
    /// Result field alias
    pub alias: Option<String>,
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
