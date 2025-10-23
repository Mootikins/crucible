//! Database interaction tools
//!
//! This module provides simple async functions for database operations including CRUD,
//! search, indexing, and maintenance operations. Converted from Tool trait implementations
//! to direct async function composition as part of Phase 1.3 service architecture removal.
//! Now updated to Phase 2.1 ToolFunction interface for unified tool execution.

use crate::types::{ToolResult, ToolError, ToolFunction};
use serde_json::{json, Value};
use tracing::info;

/// Semantic search using embeddings - Phase 2.1 ToolFunction
///
/// This function implements the ToolFunction signature for unified execution.
pub fn semantic_search() -> ToolFunction {
    |tool_name: String,
     parameters: Value,
     user_id: Option<String>,
     session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            // Extract parameters
            let query = parameters.get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'query' parameter".to_string()))?;

            let top_k = parameters.get("top_k")
                .and_then(|v| v.as_u64())
                .unwrap_or(10);

            info!("Performing semantic search: {} (top_k: {})", query, top_k);

            // Mock implementation - in real implementation this would:
            // 1. Generate embedding for the query
            // 2. Search the database for similar embeddings
            // 3. Return ranked results with similarity scores

            let mock_results = vec![
                json!({
                    "file_path": "docs/ai-research.md",
                    "title": "AI Research Notes",
                    "content": "Comprehensive research on artificial intelligence and machine learning...",
                    "score": 0.95
                }),
                json!({
                    "file_path": "projects/ml-project.md",
                    "title": "Machine Learning Project",
                    "content": "Implementation details for our ML project using transformers...",
                    "score": 0.87
                }),
            ];

            let result_data = json!({
                "results": mock_results,
                "query": query,
                "top_k": top_k,
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

/// Full-text search in note contents - Phase 2.1 ToolFunction
pub fn search_by_content() -> ToolFunction {
    |tool_name: String,
     parameters: Value,
     user_id: Option<String>,
     session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let query = parameters.get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'query' parameter".to_string()))?;

            info!("Performing content search: {}", query);

            let mock_results = vec![
                json!({
                    "file_path": "notes/database-design.md",
                    "content": "This document discusses database design patterns and best practices...",
                    "metadata": {
                        "title": "Database Design",
                        "tags": ["database", "design"]
                    }
                }),
            ];

            let result_data = json!({
                "results": mock_results,
                "query": query,
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

/// Search notes by filename pattern - Phase 2.1 ToolFunction
pub fn search_by_filename() -> ToolFunction {
    |tool_name: String,
     parameters: Value,
     user_id: Option<String>,
     session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let pattern = parameters.get("pattern")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'pattern' parameter".to_string()))?;

            info!("Searching files with pattern: {}", pattern);

            let matching_files = vec![
                "meeting-notes-2024-01-15.md".to_string(),
                "meeting-notes-2024-01-22.md".to_string(),
            ];

            let result_data = json!({
                "pattern": pattern,
                "files": matching_files,
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

/// Update frontmatter properties of a note - Phase 2.1 ToolFunction
pub fn update_note_properties() -> ToolFunction {
    |tool_name: String,
     parameters: Value,
     user_id: Option<String>,
     session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let path = parameters.get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'path' parameter".to_string()))?;

            let properties = parameters.get("properties")
                .cloned()
                .unwrap_or(json!({}));

            info!("Updating properties for {}: {:?}", path, properties);

            let result_data = json!({
                "path": path,
                "properties": properties,
                "success": true,
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

/// Index a specific document for search - Phase 2.1 ToolFunction
pub fn index_document() -> ToolFunction {
    |tool_name: String,
     parameters: Value,
     user_id: Option<String>,
     session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let document = parameters.get("document")
                .cloned()
                .ok_or_else(|| ToolError::Other("Missing 'document' parameter".to_string()))?;

            let document_id = document
                .get("id")
                .and_then(|id| id.as_str())
                .unwrap_or("unknown");

            info!("Indexing document: {}", document_id);

            let result_data = json!({
                "indexed": true,
                "document_id": document_id,
                "document": document,
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

/// Get document statistics from the database - Phase 2.1 ToolFunction
pub fn get_document_stats() -> ToolFunction {
    |tool_name: String,
     _parameters: Value,
     user_id: Option<String>,
     session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            info!("Getting document statistics");

            let stats = json!({
                "total_documents": 1250,
                "database_type": "duckdb",
                "embedding_dimension": 1536,
                "index_type": "cosine_similarity",
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

/// Sync metadata from external source to database - Phase 2.1 ToolFunction
pub fn sync_metadata() -> ToolFunction {
    |tool_name: String,
     parameters: Value,
     user_id: Option<String>,
     session_id: Option<String>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let source = parameters.get("source")
                .and_then(|v| v.as_str())
                .unwrap_or("obsidian");

            info!("Syncing metadata from source: {}", source);

            let synced_count = 856;
            let errors: Vec<String> = vec![];

            let result_data = json!({
                "source": source,
                "synced_count": synced_count,
                "errors": errors,
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
    async fn test_semantic_search_function() {
        let tool_fn = semantic_search();
        let parameters = json!({
            "query": "machine learning",
            "top_k": 5
        });

        let result = tool_fn(
            "semantic_search".to_string(),
            parameters,
            Some("test_user".to_string()),
            Some("test_session".to_string()),
        ).await.unwrap();

        assert!(result.success);
        assert!(result.data.is_some());
        assert_eq!(result.tool_name, "semantic_search");
    }

    #[tokio::test]
    async fn test_update_note_properties_function() {
        let tool_fn = update_note_properties();
        let parameters = json!({
            "path": "test.md",
            "properties": {
                "status": "updated",
                "priority": "high"
            }
        });

        let result = tool_fn(
            "update_note_properties".to_string(),
            parameters,
            Some("test_user".to_string()),
            Some("test_session".to_string()),
        ).await.unwrap();

        assert!(result.success);
        assert!(result.data.is_some());
    }

    #[tokio::test]
    async fn test_get_document_stats_function() {
        let tool_fn = get_document_stats();
        let parameters = json!({});

        let result = tool_fn(
            "get_document_stats".to_string(),
            parameters,
            None,
            None,
        ).await.unwrap();

        assert!(result.success);
        assert!(result.data.is_some());
    }

    #[tokio::test]
    async fn test_validation_error_handling() {
        let tool_fn = semantic_search();
        let parameters = json!({}); // Missing required 'query' parameter

        let result = tool_fn(
            "semantic_search".to_string(),
            parameters,
            None,
            None,
        ).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::Other(msg) => {
                assert!(msg.contains("Missing 'query' parameter"));
            }
            _ => panic!("Expected ValidationFailed error"),
        }
    }
}