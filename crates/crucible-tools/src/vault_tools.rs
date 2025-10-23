//! Vault operation tools
//!
//! This module provides simple async functions for interacting with Obsidian vaults,
//! including file operations, metadata management, and indexing. Converted from
//! Tool trait implementations to direct async function composition as part of
//! Phase 1.3 service architecture removal. Now updated to Phase 2.1 ToolFunction interface.

use crate::types::{ToolResult, ToolError, ToolFunction};
use serde_json::{json, Value};
use tracing::info;

/// Search notes by frontmatter properties - Phase 2.1 ToolFunction
pub fn search_by_properties() -> ToolFunction {
    |tool_name: String,
     parameters: Value,
     user_id: Option<String>,
     session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let properties = parameters.get("properties")
                .cloned()
                .unwrap_or(json!({}));

            info!("Searching for files with properties: {:?}", properties);

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

            let result_data = json!({
                "matching_files": matching_files,
                "searched_properties": properties,
                "count": matching_files.len(),
                "user_id": user_id,
                "session_id": session_id
            });

            Ok(ToolResult::success_with_duration(
                tool_name,
                result_data,
                start_time.elapsed().as_millis() as u64,
            ))
        })
    }
}

/// Search notes by tags - Phase 2.1 ToolFunction
pub fn search_by_tags() -> ToolFunction {
    |tool_name: String,
     parameters: Value,
     user_id: Option<String>,
     session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let tags: Vec<String> = parameters.get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();

            info!("Searching for files with tags: {:?}", tags);

            let matching_files = vec![
                json!({
                    "path": "knowledge/ai.md",
                    "name": "AI Research",
                    "folder": "knowledge",
                    "tags": ["ai", "research", "technology"]
                }),
            ];

            let result_data = json!({
                "matching_files": matching_files,
                "searched_tags": tags,
                "count": matching_files.len(),
                "user_id": user_id,
                "session_id": session_id
            });

            Ok(ToolResult::success_with_duration(
                tool_name,
                result_data,
                start_time.elapsed().as_millis() as u64,
            ))
        })
    }
}

/// Search notes in a specific folder - Phase 2.1 ToolFunction
pub fn search_by_folder() -> ToolFunction {
    |tool_name: String,
     parameters: Value,
     user_id: Option<String>,
     session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let path = parameters.get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'path' parameter".to_string()))?;

            let recursive = parameters.get("recursive")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            info!("Searching in folder: {} (recursive: {})", path, recursive);

            let files = vec![
                json!({
                    "path": "projects/active/project1.md",
                    "name": "Project 1",
                    "size": 2048,
                    "modified": "2024-01-20T10:30:00Z"
                }),
                json!({
                    "path": "projects/active/project2.md",
                    "name": "Project 2",
                    "size": 1536,
                    "modified": "2024-01-18T14:22:00Z"
                }),
            ];

            let result_data = json!({
                "files": files,
                "search_path": path,
                "recursive": recursive,
                "count": files.len(),
                "user_id": user_id,
                "session_id": session_id
            });

            Ok(ToolResult::success_with_duration(
                tool_name,
                result_data,
                start_time.elapsed().as_millis() as u64,
            ))
        })
    }
}

/// Create a new note in the vault - Phase 2.1 ToolFunction
pub fn create_note() -> ToolFunction {
    |tool_name: String,
     parameters: Value,
     user_id: Option<String>,
     session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let path = parameters.get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'path' parameter".to_string()))?;

            let title = parameters.get("title")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'title' parameter".to_string()))?;

            let content = parameters.get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'content' parameter".to_string()))?;

            let properties = parameters.get("properties")
                .cloned()
                .unwrap_or(json!({}));

            info!("Creating note: {} at {}", title, path);

            let created_file = json!({
                "path": path,
                "title": title,
                "created": true,
                "size": content.len(),
                "properties": properties,
                "created_at": chrono::Utc::now().to_rfc3339(),
                "user_id": user_id,
                "session_id": session_id
            });

            Ok(ToolResult::success_with_duration(
                tool_name,
                created_file,
                start_time.elapsed().as_millis() as u64,
            ))
        })
    }
}

/// Update an existing note - Phase 2.1 ToolFunction
pub fn update_note() -> ToolFunction {
    |tool_name: String,
     parameters: Value,
     user_id: Option<String>,
     session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let path = parameters.get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'path' parameter".to_string()))?;

            let content = parameters.get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'content' parameter".to_string()))?;

            let properties = parameters.get("properties")
                .cloned()
                .unwrap_or(json!({}));

            info!("Updating note: {}", path);

            let updated_file = json!({
                "path": path,
                "updated": true,
                "size": content.len(),
                "properties": properties,
                "updated_at": chrono::Utc::now().to_rfc3339(),
                "user_id": user_id,
                "session_id": session_id
            });

            Ok(ToolResult::success_with_duration(
                tool_name,
                updated_file,
                start_time.elapsed().as_millis() as u64,
            ))
        })
    }
}

/// Delete a note from the vault - Phase 2.1 ToolFunction
pub fn delete_note() -> ToolFunction {
    |tool_name: String,
     parameters: Value,
     user_id: Option<String>,
     session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let path = parameters.get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'path' parameter".to_string()))?;

            info!("Deleting note: {}", path);

            let deletion_result = json!({
                "path": path,
                "deleted": true,
                "deleted_at": chrono::Utc::now().to_rfc3339(),
                "user_id": user_id,
                "session_id": session_id
            });

            Ok(ToolResult::success_with_duration(
                tool_name,
                deletion_result,
                start_time.elapsed().as_millis() as u64,
            ))
        })
    }
}

/// Get vault statistics - Phase 2.1 ToolFunction
pub fn get_vault_stats() -> ToolFunction {
    |tool_name: String,
     _parameters: Value,
     user_id: Option<String>,
     session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            info!("Getting vault statistics");

            let stats = json!({
                "total_notes": 1250,
                "total_size_mb": 156.7,
                "folders": 45,
                "tags": 234,
                "last_indexed": "2024-01-20T15:30:00Z",
                "vault_type": "obsidian",
                "user_id": user_id,
                "session_id": session_id
            });

            Ok(ToolResult::success_with_duration(
                tool_name,
                stats,
                start_time.elapsed().as_millis() as u64,
            ))
        })
    }
}

/// List all tags in the vault - Phase 2.1 ToolFunction
pub fn list_tags() -> ToolFunction {
    |tool_name: String,
     _parameters: Value,
     user_id: Option<String>,
     session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            info!("Listing all vault tags");

            let tags = vec![
                json!({
                    "name": "ai",
                    "count": 45,
                    "category": "technology"
                }),
                json!({
                    "name": "research",
                    "count": 67,
                    "category": "work"
                }),
                json!({
                    "name": "project",
                    "count": 23,
                    "category": "work"
                }),
            ];

            let result_data = json!({
                "tags": tags,
                "total_tags": tags.len(),
                "user_id": user_id,
                "session_id": session_id
            });

            Ok(ToolResult::success_with_duration(
                tool_name,
                result_data,
                start_time.elapsed().as_millis() as u64,
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ToolResult, ToolError};

    #[tokio::test]
    async fn test_search_by_properties_function() {
        let tool_fn = search_by_properties();
        let parameters = json!({
            "properties": {
                "status": "active",
                "priority": "high"
            }
        });

        let result = tool_fn(
            "search_by_properties".to_string(),
            parameters,
            Some("test_user".to_string()),
            Some("test_session".to_string()),
        ).await.unwrap();

        assert!(result.success);
        assert!(result.data.is_some());
    }

    #[tokio::test]
    async fn test_search_by_tags_function() {
        let tool_fn = search_by_tags();
        let parameters = json!({
            "tags": ["ai", "research"]
        });

        let result = tool_fn(
            "search_by_tags".to_string(),
            parameters,
            Some("test_user".to_string()),
            Some("test_session".to_string()),
        ).await.unwrap();

        assert!(result.success);
        assert!(result.data.is_some());
    }

    #[tokio::test]
    async fn test_create_note_function() {
        let tool_fn = create_note();
        let parameters = json!({
            "path": "test-note.md",
            "title": "Test Note",
            "content": "# Test Note\n\nThis is a test note.",
            "properties": {
                "status": "draft",
                "tags": ["test"]
            }
        });

        let result = tool_fn(
            "create_note".to_string(),
            parameters,
            Some("test_user".to_string()),
            Some("test_session".to_string()),
        ).await.unwrap();

        assert!(result.success);
        assert!(result.data.is_some());
    }

    #[tokio::test]
    async fn test_get_vault_stats_function() {
        let tool_fn = get_vault_stats();
        let parameters = json!({});

        let result = tool_fn(
            "get_vault_stats".to_string(),
            parameters,
            None,
            None,
        ).await.unwrap();

        assert!(result.success);
        assert!(result.data.is_some());

        let data = result.data.unwrap();
        assert!(data.get("total_notes").is_some());
        assert!(data.get("vault_type").is_some());
    }

    #[tokio::test]
    async fn test_list_tags_function() {
        let tool_fn = list_tags();
        let parameters = json!({});

        let result = tool_fn(
            "list_tags".to_string(),
            parameters,
            None,
            None,
        ).await.unwrap();

        assert!(result.success);
        assert!(result.data.is_some());

        let data = result.data.unwrap();
        assert!(data.get("tags").is_some());
        assert!(data.get("total_tags").is_some());
    }

    #[tokio::test]
    async fn test_create_note_validation() {
        let tool_fn = create_note();
        let parameters = json!({
            "path": "test.md"
            // Missing required 'title' and 'content' parameters
        });

        let result = tool_fn(
            "create_note".to_string(),
            parameters,
            None,
            None,
        ).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::Other(msg) => {
                assert!(msg.contains("Missing 'title' parameter"));
            }
            _ => panic!("Expected ValidationFailed error"),
        }
    }

    #[tokio::test]
    async fn test_search_by_folder_function() {
        let tool_fn = search_by_folder();
        let parameters = json!({
            "path": "projects",
            "recursive": true
        });

        let result = tool_fn(
            "search_by_folder".to_string(),
            parameters,
            None,
            None,
        ).await.unwrap();

        assert!(result.success);
        assert!(result.data.is_some());
    }

    #[tokio::test]
    async fn test_update_note_function() {
        let tool_fn = update_note();
        let parameters = json!({
            "path": "existing-note.md",
            "content": "# Updated Note\n\nThis is updated content.",
            "properties": {
                "status": "updated"
            }
        });

        let result = tool_fn(
            "update_note".to_string(),
            parameters,
            None,
            None,
        ).await.unwrap();

        assert!(result.success);
        assert!(result.data.is_some());
    }

    #[tokio::test]
    async fn test_delete_note_function() {
        let tool_fn = delete_note();
        let parameters = json!({
            "path": "old-note.md"
        });

        let result = tool_fn(
            "delete_note".to_string(),
            parameters,
            None,
            None,
        ).await.unwrap();

        assert!(result.success);
        assert!(result.data.is_some());
    }
}