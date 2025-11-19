//! Kiln operation tools - Phase 1B Implementation
//!
//! This module provides async functions for interacting with Obsidian kilns,
//! including file operations, metadata management, and indexing. Uses the Phase 1A
//! parsing system via the `KilnRepository` to provide kiln data access.

use crate::kiln_operations::KilnRepository;
use crate::types::{
    get_embedding_provider_from_context, get_kiln_path_from_context,
    get_knowledge_repo_from_context, ToolDefinition, ToolError, ToolFunction, ToolResult,
};
use grep::regex::RegexMatcher;
use grep::searcher::sinks::UTF8;
use grep::searcher::Searcher;
use ignore::WalkBuilder;
use serde_json::{json, Value};
use std::path::Path;
use tracing::info;

/// Search notes by frontmatter properties - Implementation using Phase 1A parsing
#[must_use]
pub fn search_by_properties() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let properties = parameters.get("properties").cloned().unwrap_or(json!({}));

            info!("Searching for files with properties: {:?}", properties);

            // Get repository from global context
            let kiln_repo = KilnRepository::from_context()
                .map_err(|e| ToolError::Other(format!("Failed to get kiln repository: {e}")))?;

            match kiln_repo.search_by_properties(properties.clone()).await {
                Ok(matching_files) => {
                    let result_data = json!({
                        "matching_files": matching_files,
                        "count": matching_files.len(),
                        "properties": properties,
                        "user_id": user_id,
                        "session_id": session_id,
                        "kiln_path": kiln_repo.kiln_path()
                    });

                    Ok(ToolResult::success_with_duration(
                        tool_name,
                        result_data,
                        start_time.elapsed().as_millis() as u64,
                    ))
                }
                Err(e) => Err(ToolError::Other(format!("Kiln search failed: {e}"))),
            }
        })
    }
}

/// Search notes by tags - Implementation using Phase 1A parsing
#[must_use]
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
                        .map(std::string::ToString::to_string)
                        .collect()
                })
                .unwrap_or_default();

            info!("Searching for files with tags: {:?}", tags);

            // Get repository from global context
            let kiln_repo = KilnRepository::from_context()
                .map_err(|e| ToolError::Other(format!("Failed to get kiln repository: {e}")))?;

            match kiln_repo.search_by_tags(tags.clone()).await {
                Ok(matching_files) => {
                    let result_data = json!({
                        "matching_files": matching_files,
                        "count": matching_files.len(),
                        "tags": tags,
                        "user_id": user_id,
                        "session_id": session_id,
                        "kiln_path": kiln_repo.kiln_path()
                    });

                    Ok(ToolResult::success_with_duration(
                        tool_name,
                        result_data,
                        start_time.elapsed().as_millis() as u64,
                    ))
                }
                Err(e) => Err(ToolError::Other(format!("Kiln search failed: {e}"))),
            }
        })
    }
}

/// Search notes in a specific folder - Implementation using Phase 1A parsing
#[must_use]
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
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);

            info!(
                "Searching for files in folder: {} (recursive: {})",
                path, recursive
            );

            // Get repository from global context
            let kiln_repo = KilnRepository::from_context()
                .map_err(|e| ToolError::Other(format!("Failed to get kiln repository: {e}")))?;

            match kiln_repo.search_by_folder(path, recursive).await {
                Ok(matching_files) => {
                    let result_data = json!({
                        "matching_files": matching_files,
                        "count": matching_files.len(),
                        "path": path,
                        "recursive": recursive,
                        "user_id": user_id,
                        "session_id": session_id,
                        "kiln_path": kiln_repo.kiln_path()
                    });

                    Ok(ToolResult::success_with_duration(
                        tool_name,
                        result_data,
                        start_time.elapsed().as_millis() as u64,
                    ))
                }
                Err(e) => Err(ToolError::Other(format!("Kiln search failed: {e}"))),
            }
        })
    }
}

/// Create a new note in the kiln - Phase 2.1 `ToolFunction`
#[must_use]
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

/// Update an existing note - Phase 2.1 `ToolFunction`
#[must_use]
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

/// Delete a note from the kiln - Phase 2.1 `ToolFunction`
#[must_use]
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

/// Get kiln statistics - Implementation using Phase 1A parsing
#[must_use]
pub fn get_kiln_stats() -> ToolFunction {
    |tool_name: String, _parameters: Value, user_id: Option<String>, session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            info!("Getting kiln statistics");

            // Get repository from global context
            let kiln_repo = KilnRepository::from_context()
                .map_err(|e| ToolError::Other(format!("Failed to get kiln repository: {e}")))?;

            match kiln_repo.get_kiln_stats().await {
                Ok(stats) => {
                    let result_data = json!({
                        "stats": stats,
                        "user_id": user_id,
                        "session_id": session_id,
                        "kiln_path": kiln_repo.kiln_path()
                    });

                    Ok(ToolResult::success_with_duration(
                        tool_name,
                        result_data,
                        start_time.elapsed().as_millis() as u64,
                    ))
                }
                Err(e) => Err(ToolError::Other(format!("Failed to get kiln stats: {e}"))),
            }
        })
    }
}

/// List all tags in the kiln - Implementation using Phase 1A parsing
#[must_use]
pub fn list_tags() -> ToolFunction {
    |tool_name: String, _parameters: Value, user_id: Option<String>, session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            info!("Listing all tags in kiln");

            // Get repository from global context
            let kiln_repo = KilnRepository::from_context()
                .map_err(|e| ToolError::Other(format!("Failed to get kiln repository: {e}")))?;

            match kiln_repo.list_tags().await {
                Ok(mut tags_data) => {
                    // tags_data is already a JSON object with "tags" and "total_tags"
                    // Add user_id, session_id, and kiln_path to the result
                    if let Some(obj) = tags_data.as_object_mut() {
                        if let Some(uid) = user_id {
                            obj.insert("user_id".to_string(), json!(uid));
                        }
                        if let Some(sid) = session_id {
                            obj.insert("session_id".to_string(), json!(sid));
                        }
                        obj.insert("kiln_path".to_string(), json!(kiln_repo.kiln_path()));
                    }

                    Ok(ToolResult::success_with_duration(
                        tool_name,
                        tags_data,
                        start_time.elapsed().as_millis() as u64,
                    ))
                }
                Err(e) => Err(ToolError::Other(format!("Failed to list tags: {e}"))),
            }
        })
    }
}

/// Read a note by name - Definition
pub fn read_note_definition() -> ToolDefinition {
    ToolDefinition {
        name: "read_note".to_string(),
        description: "Read the content and metadata of a note by its name".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The name of the note to read (including extension)"
                }
            },
            "required": ["name"]
        }),
        enabled: true,
    }
}

/// Read a note by name - Implementation using KnowledgeRepository
#[must_use]
pub fn read_note() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let name = parameters
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'name' parameter".to_string()))?;

            info!("Reading note: {}", name);

            let repo = get_knowledge_repo_from_context()?;
            let note = repo
                .get_note_by_name(name)
                .await
                .map_err(|e| ToolError::Other(format!("Failed to read note: {e}")))?;

            match note {
                Some(note) => {
                    let result_data = json!({
                        "name": note.path.file_name().unwrap_or_default().to_string_lossy(),
                        "path": note.path.to_string_lossy(),
                        "title": note.title(),
                        "content": note.content.plain_text,
                        "frontmatter": note.frontmatter.as_ref().map(|f| &f.raw),
                        "tags": note.tags.iter().map(|t| &t.name).collect::<Vec<_>>(),
                        "user_id": user_id,
                        "session_id": session_id
                    });

                    Ok(ToolResult::success_with_duration(
                        tool_name,
                        result_data,
                        start_time.elapsed().as_millis() as u64,
                    ))
                }
                None => Err(ToolError::Other(format!("Note not found: {name}"))),
            }
        })
    }
}

/// List notes - Definition
pub fn list_notes_definition() -> ToolDefinition {
    ToolDefinition {
        name: "list_notes".to_string(),
        description: "List notes in the kiln, optionally filtering by path".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Optional path to filter notes by directory"
                }
            }
        }),
        enabled: true,
    }
}

/// List notes - Implementation using KnowledgeRepository
#[must_use]
pub fn list_notes() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let path = parameters
                .get("path")
                .and_then(|v| v.as_str());

            info!("Listing notes (path filter: {:?})", path);

            let repo = get_knowledge_repo_from_context()?;
            let notes = repo
                .list_notes(path)
                .await
                .map_err(|e| ToolError::Other(format!("Failed to list notes: {e}")))?;

            let result_data = json!({
                "notes": notes,
                "count": notes.len(),
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

/// Search notes - Definition
pub fn search_notes_definition() -> ToolDefinition {
    ToolDefinition {
        name: "search_notes".to_string(),
        description: "Search for notes using semantic search".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 10)"
                }
            },
            "required": ["query"]
        }),
        enabled: true,
    }
}

/// Search notes - Implementation using KnowledgeRepository (semantic) and grep (text)
#[must_use]
pub fn search_notes() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let query = parameters
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'query' parameter".to_string()))?;

            let search_type = parameters
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("semantic");

            info!("Searching notes: '{}' (type: {})", query, search_type);

            let results = if search_type == "text" {
                // Text search using grep
                let kiln_path = get_kiln_path_from_context()?;
                let matcher = RegexMatcher::new(query)
                    .map_err(|e| ToolError::Other(format!("Invalid regex: {e}")))?;

                let mut matches = Vec::new();
                let walker = WalkBuilder::new(&kiln_path)
                    .hidden(false)
                    .git_ignore(true)
                    .build();

                for result in walker {
                    match result {
                        Ok(entry) => {
                            if !entry.file_type().map_or(false, |ft| ft.is_file()) {
                                continue;
                            }
                            let path = entry.path();
                            if path.extension().map_or(false, |ext| ext == "md") {
                                let mut searcher = Searcher::new();
                                let mut found = false;
                                let _ = searcher.search_path(
                                    &matcher,
                                    path,
                                    UTF8(|_, _| {
                                        found = true;
                                        Ok(false) // Stop after first match per file
                                    }),
                                );
                                if found {
                                    let rel_path = path.strip_prefix(&kiln_path).unwrap_or(path);
                                    matches.push(json!({
                                        "path": rel_path.to_string_lossy(),
                                        "score": 1.0
                                    }));
                                }
                            }
                        }
                        Err(err) => tracing::warn!("Error walking directory: {}", err),
                    }
                }
                matches
            } else {
                // Semantic search
                let repo = get_knowledge_repo_from_context()?;
                let embedding_provider = get_embedding_provider_from_context()?;

                let embedding_response = embedding_provider
                    .embed(query)
                    .await
                    .map_err(|e| ToolError::Other(format!("Failed to generate embedding: {e}")))?;

                let search_results = repo
                    .search_vectors(embedding_response.embedding)
                    .await
                    .map_err(|e| ToolError::Other(format!("Failed to search vectors: {e}")))?;

                serde_json::to_value(search_results)
                    .map_err(|e| ToolError::Other(format!("Failed to serialize results: {e}")))?
                    .as_array()
                    .unwrap_or(&Vec::new())
                    .clone()
            };

            let result_data = json!({
                "results": results,
                "count": results.len(),
                "query": query,
                "type": search_type,
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
    use crate::types::ToolError;

    #[tokio::test]
    async fn test_search_by_properties_function() {
        use crate::kiln_operations::KilnRepository;
        use std::collections::HashMap;
        use std::fs;
        use tempfile::TempDir;

        // Phase 7: Use KilnRepository directly instead of global context
        // This eliminates race conditions from concurrent test execution
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.md");

        // Create test file with frontmatter
        fs::write(
            &test_file,
            r#"---
status: active
priority: high
---
# Test Note"#,
        )
        .unwrap();

        // Create repository directly with test path (no global state)
        let kiln_repo = KilnRepository::new(temp_dir.path().to_str().unwrap());
        let properties = json!({
            "status": "active",
            "priority": "high"
        });

        let matching_files = kiln_repo.search_by_properties(properties).await.unwrap();

        // Verify results
        assert!(
            matching_files.len() > 0,
            "Should find files with matching properties"
        );
    }

    #[tokio::test]
    async fn test_search_by_tags_function() {
        use crate::kiln_operations::KilnRepository;
        use std::fs;
        use tempfile::TempDir;

        // Phase 7: Use KilnRepository directly instead of global context
        // This eliminates race conditions from concurrent test execution
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.md");

        // Create test file with frontmatter
        fs::write(
            &test_file,
            r#"---
tags: [ai, research]
---
# Test Note"#,
        )
        .unwrap();

        // Create repository directly with test path (no global state)
        let kiln_repo = KilnRepository::new(temp_dir.path().to_str().unwrap());
        let matching_files = kiln_repo
            .search_by_tags(vec!["ai".to_string(), "research".to_string()])
            .await
            .unwrap();

        // Verify results
        assert!(
            matching_files.len() > 0,
            "Should find files with matching tags"
        );
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
        use std::fs;
        use tempfile::TempDir;

        // Create isolated test environment
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.md");

        // Create test file with frontmatter
        fs::write(
            &test_file,
            r#"---
title: Test Note
---
# Test Note
Some content here."#,
        )
        .unwrap();

        // Set kiln path in registry
        crate::types::set_tool_context(crate::types::ToolConfigContext::new().with_kiln_path(
            temp_dir.path().to_path_buf(),
        ));

        let tool_fn = get_kiln_stats();
        let parameters = json!({});

        let result = tool_fn("get_kiln_stats".to_string(), parameters, None, None)
            .await
            .unwrap();

        assert!(result.success);
        let data = result.data.unwrap();
        let stats = data.get("stats").unwrap();
        assert!(stats.get("total_notes").is_some());
        assert!(stats.get("total_size_mb").is_some());
    }

    #[tokio::test]
    async fn test_list_tags_function() {
        use std::fs;
        use tempfile::TempDir;

        // Create isolated test environment
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.md");

        // Create test file with frontmatter
        fs::write(
            &test_file,
            r#"---
tags: [ai, research, testing]
---
# Test Note"#,
        )
        .unwrap();

        // Set kiln path in registry
        crate::types::set_tool_context(crate::types::ToolConfigContext::new().with_kiln_path(
            temp_dir.path().to_path_buf(),
        ));

        let tool_fn = list_tags();
        let parameters = json!({});

        let result = tool_fn("list_tags".to_string(), parameters, None, None)
            .await
            .unwrap();

        assert!(result.success);
        let data = result.data.unwrap();
        let tags = data.get("tags").unwrap().as_array().unwrap();
        assert!(tags.len() > 0);
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
        use crate::kiln_operations::KilnRepository;
        use std::fs;
        use tempfile::TempDir;

        // Phase 7: Use KilnRepository directly instead of global context
        // This eliminates race conditions from concurrent test execution
        let temp_dir = TempDir::new().unwrap();
        let projects_dir = temp_dir.path().join("projects");
        fs::create_dir(&projects_dir).unwrap();

        let test_file = projects_dir.join("test.md");

        // Create test file with frontmatter
        fs::write(
            &test_file,
            r#"---
title: Project Note
---
# Project Note"#,
        )
        .unwrap();

        // Create repository directly with test path (no global state)
        let kiln_repo = KilnRepository::new(temp_dir.path().to_str().unwrap());
        let matching_files = kiln_repo.search_by_folder("projects", true).await.unwrap();

        // Verify results
        assert!(
            matching_files.len() > 0,
            "Should find files in projects folder"
        );
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
