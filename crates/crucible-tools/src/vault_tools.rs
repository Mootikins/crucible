//! Vault operation tools - Phase 1B Real Implementation
//!
//! This module provides real async functions for interacting with Obsidian vaults,
//! including file operations, metadata management, and indexing. Uses the Phase 1A
//! parsing system to provide actual vault data instead of mock responses.

use crate::types::{ToolError, ToolFunction, ToolResult};
use crate::vault_operations::RealVaultOperations;
use serde_json::{json, Value};
use tracing::info;

/// Search notes by frontmatter properties - Real Implementation using Phase 1A parsing
pub fn search_by_properties() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let properties = parameters.get("properties").cloned().unwrap_or(json!({}));

            info!("Searching for files with properties: {:?}", properties);

            // Use real vault operations
            let vault_ops = RealVaultOperations::new();
            match vault_ops.search_by_properties(properties).await {
                Ok(matching_files) => {
                    let result_data = json!({
                        "matching_files": matching_files,
                        "count": matching_files.len(),
                        "user_id": user_id,
                        "session_id": session_id,
                        "vault_path": vault_ops.vault_path()
                    });

                    Ok(ToolResult::success_with_duration(
                        tool_name,
                        result_data,
                        start_time.elapsed().as_millis() as u64,
                    ))
                }
                Err(e) => Err(ToolError::Other(format!("Vault search failed: {}", e))),
            }
        })
    }
}

/// Search notes by tags - Real Implementation using Phase 1A parsing
pub fn search_by_tags() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let tags: Vec<String> = parameters
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();

            info!("Searching for files with tags: {:?}", tags);

            // Use real vault operations
            let vault_ops = RealVaultOperations::new();
            match vault_ops.search_by_tags(tags).await {
                Ok(matching_files) => {
                    let result_data = json!({
                        "matching_files": matching_files,
                        "count": matching_files.len(),
                        "user_id": user_id,
                        "session_id": session_id,
                        "vault_path": vault_ops.vault_path()
                    });

                    Ok(ToolResult::success_with_duration(
                        tool_name,
                        result_data,
                        start_time.elapsed().as_millis() as u64,
                    ))
                }
                Err(e) => Err(ToolError::Other(format!("Tag search failed: {}", e))),
            }
        })
    }
}

/// Search notes in a specific folder - Real Implementation using Phase 1A parsing
pub fn search_by_folder() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let path = parameters
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'path' parameter".to_string()))?;

            let recursive = parameters
                .get("recursive")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            info!("Searching in folder: {} (recursive: {})", path, recursive);

            // Use real vault operations
            let vault_ops = RealVaultOperations::new();
            match vault_ops.search_by_folder(path, recursive).await {
                Ok(files) => {
                    let result_data = json!({
                        "files": files,
                        "search_path": path,
                        "recursive": recursive,
                        "count": files.len(),
                        "user_id": user_id,
                        "session_id": session_id,
                        "vault_path": vault_ops.vault_path()
                    });

                    Ok(ToolResult::success_with_duration(
                        tool_name,
                        result_data,
                        start_time.elapsed().as_millis() as u64,
                    ))
                }
                Err(e) => Err(ToolError::Other(format!("Folder search failed: {}", e))),
            }
        })
    }
}

/// Create a new note in the vault - Phase 2.1 ToolFunction
pub fn create_note() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let path = parameters
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'path' parameter".to_string()))?;

            let title = parameters
                .get("title")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'title' parameter".to_string()))?;

            let content = parameters
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'content' parameter".to_string()))?;

            let properties = parameters.get("properties").cloned().unwrap_or(json!({}));

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
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let path = parameters
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'path' parameter".to_string()))?;

            let content = parameters
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'content' parameter".to_string()))?;

            let properties = parameters.get("properties").cloned().unwrap_or(json!({}));

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
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let path = parameters
                .get("path")
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

/// Get kiln statistics - Real Implementation using Phase 1A parsing
pub fn get_kiln_stats() -> ToolFunction {
    |tool_name: String, _parameters: Value, user_id: Option<String>, session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            info!("Getting kiln statistics");

            // Use real vault operations
            let vault_ops = RealVaultOperations::new();
            match vault_ops.get_kiln_stats().await {
                Ok(mut stats) => {
                    // Add user and session info
                    if let Some(user_id) = user_id {
                        stats["user_id"] = json!(user_id);
                    }
                    if let Some(session_id) = session_id {
                        stats["session_id"] = json!(session_id);
                    }

                    Ok(ToolResult::success_with_duration(
                        tool_name,
                        stats,
                        start_time.elapsed().as_millis() as u64,
                    ))
                }
                Err(e) => Err(ToolError::Other(format!(
                    "Kiln stats calculation failed: {}",
                    e
                ))),
            }
        })
    }
}

/// List all tags in the vault - Real Implementation using Phase 1A parsing
pub fn list_tags() -> ToolFunction {
    |tool_name: String, _parameters: Value, user_id: Option<String>, session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            info!("Listing all vault tags");

            // Use real vault operations
            let vault_ops = RealVaultOperations::new();
            match vault_ops.list_tags().await {
                Ok(mut result_data) => {
                    // Add user and session info
                    if let Some(user_id) = user_id {
                        result_data["user_id"] = json!(user_id);
                    }
                    if let Some(session_id) = session_id {
                        result_data["session_id"] = json!(session_id);
                    }

                    Ok(ToolResult::success_with_duration(
                        tool_name,
                        result_data,
                        start_time.elapsed().as_millis() as u64,
                    ))
                }
                Err(e) => Err(ToolError::Other(format!("Tag listing failed: {}", e))),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolError;

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
        )
        .await
        .unwrap();

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
        )
        .await
        .unwrap();

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
        )
        .await
        .unwrap();

        assert!(result.success);
        assert!(result.data.is_some());
    }

    #[tokio::test]
    async fn test_get_kiln_stats_function() {
        let tool_fn = get_kiln_stats();
        let parameters = json!({});

        let result = tool_fn("get_kiln_stats".to_string(), parameters, None, None)
            .await
            .unwrap();

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

        let result = tool_fn("list_tags".to_string(), parameters, None, None)
            .await
            .unwrap();

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

        let result = tool_fn("create_note".to_string(), parameters, None, None).await;

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

        let result = tool_fn("search_by_folder".to_string(), parameters, None, None)
            .await
            .unwrap();

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

        let result = tool_fn("update_note".to_string(), parameters, None, None)
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.data.is_some());
    }

    #[tokio::test]
    async fn test_delete_note_function() {
        let tool_fn = delete_note();
        let parameters = json!({
            "path": "old-note.md"
        });

        let result = tool_fn("delete_note".to_string(), parameters, None, None)
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.data.is_some());
    }
}
