//! Vault operation tools
//!
//! This module provides tools for interacting with Obsidian vaults,
//! including file operations, metadata management, and indexing.

use crate::system_tools::{schemas, Tool};
use crate::types::*;
use crucible_services::types::tool::{ToolDefinition, ToolExecutionContext, ToolExecutionResult, ContextRef};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::Path;
use tracing::{info, warn};

/// Search notes by frontmatter properties
pub struct SearchByPropertiesTool;

impl SearchByPropertiesTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SearchByPropertiesTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SearchByPropertiesTool {
    fn definition(&self) -> &ToolDefinition {
        lazy_static::lazy_static! {
            static ref DEFINITION: ToolDefinition = ToolDefinition {
                name: "search_by_properties".to_string(),
                description: "Search notes by frontmatter properties".to_string(),
                category: ToolCategory::Vault,
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "properties": {
                            "type": "object",
                            "description": "Property key-value pairs to match",
                            "additionalProperties": true
                        }
                    },
                    "required": ["properties"]
                }),
                output_schema: schemas::success_response(Some(json!({
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "name": {"type": "string"},
                            "folder": {"type": "string"},
                            "properties": {"type": "object"}
                        }
                    }
                }))),
                deprecated: false,
                version: Some("1.0.0".to_string()),
            };
        }
        &DEFINITION
    }

    async fn execute(
        &self,
        params: Value,
        context: &ToolExecutionContext,
    ) -> Result<ToolExecutionResult> {
        let properties = match params.get("properties").and_then(|p| p.as_object()) {
            Some(props) => props,
            None => {
                return Ok(ToolExecutionResult {
                    success: false,
                    result: None,
                    error: Some("Missing properties".to_string()),
                    execution_time: std::time::Duration::from_millis(0),
                    tool_name: "search_by_properties".to_string(),
                    context_ref: Some(ContextRef::new()),
                });
            }
        };

        info!("Searching for files with properties: {:?}", properties);

        // This would integrate with the actual search service
        // For now, return a mock result
        let matching_files = vec![
            json!({
                "path": "projects/project1.md",
                "name": "Project 1",
                "folder": "projects",
                "properties": {
                    "status": "active",
                    "priority": "high"
                }
            }),
        ];

        Ok(ToolExecutionResult {
            success: true,
            result: Some(json!(matching_files)),
            error: None,
            execution_time: std::time::Duration::from_millis(0),
            tool_name: "search_by_properties".to_string(),
            context_ref: Some(ContextRef::new()),
        })
    }
}

/// Search notes by tags
pub struct SearchByTagsTool;

impl SearchByTagsTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SearchByTagsTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SearchByTagsTool {
    fn definition(&self) -> &ToolDefinition {
        lazy_static::lazy_static! {
            static ref DEFINITION: ToolDefinition = ToolDefinition {
                name: "search_by_tags".to_string(),
                description: "Search notes by tags".to_string(),
                category: ToolCategory::Vault,
                input_schema: json!({
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
                output_schema: schemas::success_response(Some(json!({
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "name": {"type": "string"},
                            "folder": {"type": "string"},
                            "tags": {"type": "array", "items": {"type": "string"}}
                        }
                    }
                }))),
                deprecated: false,
                version: Some("1.0.0".to_string()),
            };
        }
        &DEFINITION
    }

    async fn execute(
        &self,
        params: Value,
        _context: &ToolExecutionContext,
    ) -> Result<ToolExecutionResult> {
        let tags = match params.get("tags").and_then(|t| t.as_array()) {
            Some(tags) => tags,
            None => {
                return Ok(ToolExecutionResult {
                    success: false,
                    result: None,
                    error: Some("Missing tags parameter".to_string()),
                    execution_time: std::time::Duration::from_millis(0),
                    tool_name: "search_by_tags".to_string(),
                    context_ref: Some(ContextRef::new()),
                });
            }
        };

        info!("Searching for files with tags: {:?}", tags);

        // Mock implementation
        let matching_files = vec![
            json!({
                "path": "knowledge/ai.md",
                "name": "AI Research",
                "folder": "knowledge",
                "tags": ["ai", "research", "technology"]
            }),
        ];

        Ok(ToolExecutionResult {
            success: true,
            result: Some(json!(matching_files)),
            error: None,
            execution_time: std::time::Duration::from_millis(0),
            tool_name: "search_by_tags".to_string(),
            context_ref: Some(ContextRef::new()),
        })
    }
}

/// Search notes in a specific folder
pub struct SearchByFolderTool;

impl SearchByFolderTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SearchByFolderTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SearchByFolderTool {
    fn definition(&self) -> &ToolDefinition {
        lazy_static::lazy_static! {
            static ref DEFINITION: ToolDefinition = ToolDefinition {
                name: "search_by_folder".to_string(),
                description: "Search notes in a specific folder".to_string(),
                category: ToolCategory::Vault,
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Folder path to search in"
                        },
                        "recursive": {
                            "type": "boolean",
                            "description": "Search recursively in subfolders",
                            "default": true
                        }
                    },
                    "required": ["path"]
                }),
                output_schema: schemas::success_response(Some(json!({
                    "type": "array",
                    "items": {"type": "string"}
                }))),
                deprecated: false,
                version: Some("1.0.0".to_string()),
            };
        }
        &DEFINITION
    }

    async fn execute(
        &self,
        params: Value,
        context: &ToolExecutionContext,
    ) -> Result<ToolExecutionResult> {
        let path = match params.get("path").and_then(|p| p.as_str()) {
            Some(path) => path,
            None => {
                return Ok(ToolExecutionResult {
                    success: false,
                    result: None,
                    error: Some("Missing path".to_string()),
                    execution_time: std::time::Duration::from_millis(0),
                    tool_name: "search_by_folder".to_string(),
                    context_ref: Some(ContextRef::new()),
                });
            }
        };

        let recursive = params
            .get("recursive")
            .and_then(|r| r.as_bool())
            .unwrap_or(true);

        info!("Searching in folder: {} (recursive: {})", path, recursive);

        let vault_path = context.vault_path.as_deref().unwrap_or(".");
        let search_path = Path::new(vault_path).join(path);

        // Use filesystem scanning as a fallback
        match self.scan_folder(&search_path, recursive).await {
            Ok(files) => Ok(ToolExecutionResult {
                success: true,
                result: Some(json!(files)),
                error: None,
                execution_time: std::time::Duration::from_millis(0),
                tool_name: "search_by_folder".to_string(),
                context_ref: Some(ContextRef::new()),
            }),
            Err(e) => Ok(ToolExecutionResult {
                success: false,
                result: None,
                error: Some(format!("Folder search failed: {}", e)),
                execution_time: std::time::Duration::from_millis(0),
                tool_name: "search_by_folder".to_string(),
                context_ref: Some(ContextRef::new()),
            }),
        }
    }
}

impl SearchByFolderTool {
    async fn scan_folder(&self, folder_path: &Path, recursive: bool) -> Result<Vec<String>> {
        use std::fs;

        if !folder_path.exists() {
            return Err(anyhow::anyhow!("Folder does not exist: {}", folder_path.display()));
        }

        let mut files = Vec::new();

        if recursive {
            let pattern = format!("{}/**/*.md", folder_path.display());
            for entry in glob::glob(&pattern).map_err(|e| anyhow::anyhow!("Invalid glob pattern: {}", e))? {
                match entry {
                    Ok(path) => {
                        if let Some(relative_path) = path.strip_prefix(folder_path).ok() {
                            files.push(relative_path.to_string_lossy().to_string());
                        }
                    }
                    Err(e) => {
                        warn!("Error during glob iteration: {}", e);
                    }
                }
            }
        } else {
            for entry in fs::read_dir(folder_path)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "md") {
                    if let Some(file_name) = path.file_name() {
                        files.push(file_name.to_string_lossy().to_string());
                    }
                }
            }
        }

        if files.is_empty() {
            warn!("No markdown files found in folder: {}", folder_path.display());
        }

        Ok(files)
    }
}

/// Index vault files
pub struct IndexVaultTool;

impl IndexVaultTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for IndexVaultTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for IndexVaultTool {
    fn definition(&self) -> &ToolDefinition {
        lazy_static::lazy_static! {
            static ref DEFINITION: ToolDefinition = ToolDefinition {
                name: "index_vault".to_string(),
                description: "Index all vault files for search and retrieval".to_string(),
                category: ToolCategory::Vault,
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Vault path to index",
                            "default": "."
                        },
                        "force": {
                            "type": "boolean",
                            "description": "Re-index existing files",
                            "default": false
                        }
                    }
                }),
                output_schema: schemas::success_response(Some(json!({
                    "type": "object",
                    "properties": {
                        "indexed": {"type": "number"},
                        "errors": {
                            "type": "array",
                            "items": {"type": "string"}
                        }
                    }
                }))),
                deprecated: false,
                version: Some("1.0.0".to_string()),
            };
        }
        &DEFINITION
    }

    async fn execute(
        &self,
        params: Value,
        context: &ToolExecutionContext,
    ) -> Result<ToolExecutionResult> {
        let vault_path = params
            .get("path")
            .and_then(|p| p.as_str())
            .unwrap_or(context.vault_path.as_deref().unwrap_or("."));

        let force = params.get("force").and_then(|f| f.as_bool()).unwrap_or(false);

        info!("Indexing vault at: {} (force: {})", vault_path, force);

        // Mock implementation - in real implementation this would:
        // 1. Scan the vault for markdown files
        // 2. Extract metadata from frontmatter
        // 3. Generate embeddings for content
        // 4. Store in the database

        let indexed_count = 42; // Mock count
        let errors: Vec<String> = vec![]; // Mock errors

        Ok(ToolExecutionResult {
            success: true,
            result: Some(json!({
                "indexed": indexed_count,
                "errors": errors
            })),
            error: None,
            execution_time: std::time::Duration::from_millis(0),
            tool_name: "index_vault".to_string(),
            context_ref: Some(ContextRef::new()),
        })
    }
}

/// Get metadata for a specific note
pub struct GetNoteMetadataTool;

impl GetNoteMetadataTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GetNoteMetadataTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for GetNoteMetadataTool {
    fn definition(&self) -> &ToolDefinition {
        lazy_static::lazy_static! {
            static ref DEFINITION: ToolDefinition = ToolDefinition {
                name: "get_note_metadata".to_string(),
                description: "Get metadata for a specific note".to_string(),
                category: ToolCategory::Vault,
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Note file path"
                        }
                    },
                    "required": ["path"]
                }),
                output_schema: schemas::success_response(Some(json!({
                    "type": "object",
                    "properties": {
                        "file_path": {"type": "string"},
                        "title": {"type": "string"},
                        "tags": {"type": "array", "items": {"type": "string"}},
                        "folder": {"type": "string"},
                        "properties": {"type": "object"},
                        "created_at": {"type": "string"},
                        "updated_at": {"type": "string"}
                    }
                }))),
                deprecated: false,
                version: Some("1.0.0".to_string()),
            };
        }
        &DEFINITION
    }

    async fn execute(
        &self,
        params: Value,
        _context: &ToolExecutionContext,
    ) -> Result<ToolExecutionResult> {
        let path = match params.get("path").and_then(|p| p.as_str()) {
            Some(path) => path,
            None => {
                return Ok(ToolExecutionResult {
                    success: false,
                    result: None,
                    error: Some("Missing path".to_string()),
                    execution_time: std::time::Duration::from_millis(0),
                    tool_name: "get_note_metadata".to_string(),
                    context_ref: Some(ContextRef::new()),
                });
            }
        };

        info!("Getting metadata for: {}", path);

        // Mock implementation
        let metadata = json!({
            "file_path": path,
            "title": "Sample Note",
            "tags": ["research", "important"],
            "folder": "projects",
            "properties": {
                "status": "active",
                "priority": "high"
            },
            "created_at": chrono::Utc::now().to_rfc3339(),
            "updated_at": chrono::Utc::now().to_rfc3339()
        });

        Ok(ToolExecutionResult {
            success: true,
            result: Some(metadata),
            error: None,
            execution_time: std::time::Duration::from_millis(0),
            tool_name: "get_note_metadata".to_string(),
            context_ref: Some(ContextRef::new()),
        })
    }
}

/// Create a vault tool by name
pub fn create_tool(name: &str) -> Box<dyn Tool> {
    match name {
        "search_by_properties" => Box::new(SearchByPropertiesTool::new()),
        "search_by_tags" => Box::new(SearchByTagsTool::new()),
        "search_by_folder" => Box::new(SearchByFolderTool::new()),
        "index_vault" => Box::new(IndexVaultTool::new()),
        "get_note_metadata" => Box::new(GetNoteMetadataTool::new()),
        _ => panic!("Unknown vault tool: {}", name),
    }
}

/// Register all vault tools with the tool manager
pub fn register_vault_tools(manager: &mut crate::system_tools::ToolManager) {
    manager.register_tool(SearchByPropertiesTool::new());
    manager.register_tool(SearchByTagsTool::new());
    manager.register_tool(SearchByFolderTool::new());
    manager.register_tool(IndexVaultTool::new());
    manager.register_tool(GetNoteMetadataTool::new());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system_tools::ToolManager;

    #[tokio::test]
    async fn test_search_by_properties_tool() {
        let tool = SearchByPropertiesTool::new();
        let context = ToolExecutionContext {
            user_id: None,
            session_id: None,
            working_directory: None,
            environment: std::collections::HashMap::new(),
            context: std::collections::HashMap::new(),
            vault_path: None,
        };

        let params = json!({
            "properties": {
                "status": "active"
            }
        });

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_search_by_tags_tool() {
        let tool = SearchByTagsTool::new();
        let context = ToolExecutionContext {
            user_id: None,
            session_id: None,
            working_directory: None,
            environment: std::collections::HashMap::new(),
            context: std::collections::HashMap::new(),
            vault_path: None,
        };

        let params = json!({
            "tags": ["research", "ai"]
        });

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
    }

    #[test]
    fn test_register_vault_tools() {
        let mut manager = ToolManager::new();
        register_vault_tools(&manager);

        let vault_tools = manager.list_tools_by_category(&ToolCategory::Vault);
        assert!(!vault_tools.is_empty());
        assert!(vault_tools.iter().any(|t| t.name == "search_by_properties"));
        assert!(vault_tools.iter().any(|t| t.name == "search_by_tags"));
    }
}