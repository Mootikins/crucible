//! Tool-specific types and parameter structures
//!
//! This module contains the type definitions for all system tools,
//! providing clear schemas for AI models about required vs optional fields.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Serde default helper functions
fn default_direction_both() -> String {
    "both".to_string()
}

fn default_direction_outgoing() -> String {
    "outgoing".to_string()
}

fn default_damping_factor() -> Option<f64> {
    Some(0.85)
}

fn default_iterations() -> Option<u32> {
    Some(100)
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

/// Tool execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    pub success: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
    pub execution_time_ms: Option<u64>,
}

/// Tool definition and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub category: ToolCategory,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub deprecated: bool,
    pub version: String,
}

/// Tool categories for organization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    /// File system operations
    FileSystem,
    /// Database operations
    Database,
    /// Search and indexing
    Search,
    /// Vault management
    Vault,
    /// Semantic operations
    Semantic,
    /// System utilities
    System,
    /// External integrations
    Integration,
}

impl std::fmt::Display for ToolCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolCategory::FileSystem => write!(f, "file_system"),
            ToolCategory::Database => write!(f, "database"),
            ToolCategory::Search => write!(f, "search"),
            ToolCategory::Vault => write!(f, "vault"),
            ToolCategory::Semantic => write!(f, "semantic"),
            ToolCategory::System => write!(f, "system"),
            ToolCategory::Integration => write!(f, "integration"),
        }
    }
}

/// Tool execution context
#[derive(Debug, Clone)]
pub struct ToolExecutionContext {
    pub workspace_path: Option<String>,
    pub vault_path: Option<String>,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Tool registry metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRegistry {
    pub tools: HashMap<String, ToolDefinition>,
    pub categories: HashMap<ToolCategory, Vec<String>>,
    pub version: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            categories: HashMap::new(),
            version: "1.0.0".to_string(),
            updated_at: chrono::Utc::now(),
        }
    }

    pub fn register_tool(&mut self, tool: ToolDefinition) {
        let category = tool.category.clone();
        let tool_name = tool.name.clone();

        self.tools.insert(tool_name.clone(), tool);

        self.categories.entry(category).or_insert_with(Vec::new).push(tool_name);
        self.updated_at = chrono::Utc::now();
    }

    pub fn get_tool(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    pub fn list_tools(&self) -> Vec<&ToolDefinition> {
        self.tools.values().collect()
    }

    pub fn list_tools_by_category(&self, category: &ToolCategory) -> Vec<&ToolDefinition> {
        if let Some(tool_names) = self.categories.get(category) {
            tool_names.iter()
                .filter_map(|name| self.tools.get(name))
                .collect()
        } else {
            Vec::new()
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}