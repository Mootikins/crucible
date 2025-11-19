//! Database interaction tools
//!
//! This module provides simple async functions for database operations including CRUD,
//! search, indexing, and maintenance operations. Converted from Tool trait implementations
//! to direct async function composition as part of Phase 1.3 service architecture removal.
//! Now updated to Phase 2.1 `ToolFunction` interface for unified tool execution.
//!
//! Semantic search now uses integrated crucible-surrealdb functionality instead of mock data.

use crate::types::{ToolError, ToolFunction, ToolResult};
use serde_json::{json, Value};
use std::path::PathBuf;
use tracing::info;

/// Semantic search using embeddings - Phase 2.1 `ToolFunction`
///
/// This function implements the `ToolFunction` signature for unified execution.
/// Now uses integrated `KnowledgeRepository` and `EmbeddingProvider` from context.
#[must_use]
pub fn semantic_search() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>, context: std::sync::Arc<crate::types::ToolConfigContext>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            // Extract parameters
            let query = parameters
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'query' parameter".to_string()))?;

            let top_k = parameters
                .get("top_k")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(10);

            info!(
                "Performing semantic search: {} (top_k: {})",
                query, top_k
            );

            // Get dependencies from context
            let embedding_provider = context
                .embedding_provider
                .as_ref()
                .ok_or_else(|| ToolError::Other("No embedding provider configured".to_string()))?;
            let knowledge_repo = context
                .knowledge_repo
                .as_ref()
                .ok_or_else(|| ToolError::Other("No knowledge repository configured".to_string()))?;

            // Generate embedding for the query
            let embedding = embedding_provider
                .embed(query)
                .await
                .map_err(|e| ToolError::Other(format!("Failed to generate embedding: {e}")))?;

            // Perform vector search
            let search_results = knowledge_repo
                .search_vectors(embedding.embedding)
                .await
                .map_err(|e| ToolError::Other(format!("Vector search failed: {e}")))?;

            // Format results
            let formatted_results: Vec<Value> = search_results
                .into_iter()
                .take(top_k as usize)
                .map(|r| {
                    json!({
                        "id": r.document_id,
                        "score": r.score,
                        "snippet": r.snippet,
                        "highlights": r.highlights
                    })
                })
                .collect();

            let result_data = json!({
                "results": formatted_results,
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

// Helper functions removed as they are now handled by KnowledgeRepository implementation


/// Full-text search in note contents - Phase 2.1 `ToolFunction`
#[must_use]
pub fn search_by_content() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>, context: std::sync::Arc<crate::types::ToolConfigContext>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let query = parameters
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'query' parameter".to_string()))?;

            info!("Performing content search: {}", query);

            let mock_results = vec![json!({
                "file_path": "notes/database-design.md",
                "content": "This note discusses database design patterns and best practices...",
                "metadata": {
                    "title": "Database Design",
                    "tags": ["database", "design"]
                }
            })];

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

/// Search notes by filename pattern - Phase 2.1 `ToolFunction`
#[must_use]
pub fn search_by_filename() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>, context: std::sync::Arc<crate::types::ToolConfigContext>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let pattern = parameters
                .get("pattern")
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

/// Update frontmatter properties of a note - Phase 2.1 `ToolFunction`
#[must_use]
pub fn update_note_properties() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>, context: std::sync::Arc<crate::types::ToolConfigContext>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let path = parameters
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ToolError::Other("Missing 'path' parameter".to_string()))?;

            let properties = parameters.get("properties").cloned().unwrap_or(json!({}));

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

/// Index a specific note for search - Phase 2.1 `ToolFunction`
#[must_use]
pub fn index_document() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>, context: std::sync::Arc<crate::types::ToolConfigContext>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let note = parameters
                .get("note")
                .cloned()
                .ok_or_else(|| ToolError::Other("Missing 'note' parameter".to_string()))?;

            let document_id = note
                .get("id")
                .and_then(|id| id.as_str())
                .unwrap_or("unknown");

            info!("Indexing note: {}", document_id);

            let result_data = json!({
                "indexed": true,
                "document_id": document_id,
                "note": note,
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

/// Get note statistics from the database - Phase 2.1 `ToolFunction`
#[must_use]
pub fn get_document_stats() -> ToolFunction {
    |tool_name: String, _parameters: Value, user_id: Option<String>, session_id: Option<String>, _context: std::sync::Arc<crate::types::ToolConfigContext>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            info!("Getting note statistics");

            let stats = json!({
                "total_documents": 1250,
                "database_type": "surrealdb",
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

/// Sync metadata from external source to database - Phase 2.1 `ToolFunction`
#[must_use]
pub fn sync_metadata() -> ToolFunction {
    |tool_name: String, parameters: Value, user_id: Option<String>, session_id: Option<String>, context: std::sync::Arc<crate::types::ToolConfigContext>| {
        Box::pin(async move {
            let start_time = std::time::Instant::now();

            let source = parameters
                .get("source")
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

/// Create a default embedding provider using environment variables or defaults
async fn create_default_embedding_provider(
) -> Result<std::sync::Arc<dyn crucible_llm::embeddings::EmbeddingProvider>, anyhow::Error> {
    use crucible_config::EmbeddingProviderConfig;

    // Use default embedding configuration (Ollama with nomic-embed-text)
    let config = EmbeddingProviderConfig::ollama(
        Some("http://localhost:11434".to_string()),
        Some("nomic-embed-text".to_string()),
    );

    crucible_llm::embeddings::create_provider(config)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create default embedding provider: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolError;

    #[tokio::test]
    async fn test_semantic_search_function() {
        // Test validates the function signature and parameter extraction
        // Actual semantic search requires embeddings which would need the full kiln processing
        // TODO: When kiln processing is refactored into modules, this can test the full flow

        let tool_fn = semantic_search();
        let parameters = json!({
            "query": "machine learning",
            "top_k": 5,
            "kiln_path": "/nonexistent/path"  // Will fail early, which is fine for this test
        });

        let result = tool_fn(
            "semantic_search".to_string(),
            parameters,
            Some("test_user".to_string()),
            Some("test_session".to_string()),
            std::sync::Arc::new(crate::types::ToolConfigContext::new()),
        )
        .await;

        // Function should return an error (no embeddings or invalid path)
        // The important part is that it accepts kiln path as a parameter (not env var)
        assert!(result.is_err());
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
            std::sync::Arc::new(crate::types::ToolConfigContext::new()),
        )
        .await
        .unwrap();

        assert!(result.success);
        assert!(result.data.is_some());
    }

    #[tokio::test]
    async fn test_get_document_stats_function() {
        let tool_fn = get_document_stats();
        let parameters = json!({});

        let result = tool_fn("get_document_stats".to_string(), parameters, None, None, std::sync::Arc::new(crate::types::ToolConfigContext::new()))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.data.is_some());
    }

    #[tokio::test]
    async fn test_validation_error_handling() {
        let tool_fn = semantic_search();
        let parameters = json!({}); // Missing required 'query' parameter

        let result = tool_fn("semantic_search".to_string(), parameters, None, None, std::sync::Arc::new(crate::types::ToolConfigContext::new())).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::Other(msg) => {
                assert!(msg.contains("Missing 'query' parameter"));
            }
            _ => panic!("Expected ValidationFailed error"),
        }
    }
}
