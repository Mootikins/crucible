//! Type definitions for Obsidian API responses

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// File information returned by the Obsidian API
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileInfo {
    pub path: String,
    pub name: String,
    pub folder: String,
    pub extension: String,
    pub size: u64,
    pub created: i64,
    pub modified: i64,
}

/// Response from listing files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListFilesResponse {
    pub files: Vec<FileInfo>,
}

/// Response from getting file content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContentResponse {
    pub content: String,
    pub path: String,
}

/// File metadata including properties, tags, links, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub path: String,
    pub properties: HashMap<String, serde_json::Value>,
    pub tags: Vec<String>,
    pub folder: String,
    pub links: Vec<String>,
    pub backlinks: Vec<String>,
    pub stats: FileStats,
}

/// File statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStats {
    pub size: u64,
    pub created: i64,
    pub modified: i64,
    #[serde(rename = "wordCount")]
    pub word_count: u32,
}

/// Response from search endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub files: Vec<FileInfo>,
}

/// Request to update file properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePropertiesRequest {
    pub properties: HashMap<String, serde_json::Value>,
}

/// Response from property update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePropertiesResponse {
    pub success: bool,
}

/// Response from settings update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSettingsResponse {
    pub success: bool,
}

/// Embedding provider settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingSettings {
    pub provider: String,
    #[serde(rename = "apiUrl")]
    pub api_url: String,
    #[serde(rename = "apiKey")]
    pub api_key: Option<String>,
    pub model: String,
}

/// Response from embedding models endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingModelsResponse {
    pub models: Vec<String>,
}

/// Error response from the Obsidian API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
