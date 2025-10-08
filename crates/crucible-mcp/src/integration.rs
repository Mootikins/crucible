// crates/crucible-mcp/src/integration.rs
//! Integration helpers for Crucible ecosystem

use crate::database::EmbeddingDatabase;
use crate::types::{EmbeddingMetadata, ToolCallResult};
use anyhow::Result;
use crucible_core::DocumentNode;
use serde_json::json;
use std::collections::HashMap;

/// Convert a DocumentNode to EmbeddingMetadata
pub fn document_to_metadata(doc: &DocumentNode) -> EmbeddingMetadata {
    EmbeddingMetadata {
        file_path: format!("{}.md", doc.title),
        title: Some(doc.title.clone()),
        tags: vec![], // Could extract from properties
        folder: "documents".to_string(),
        properties: {
            let mut props = HashMap::new();
            props.insert("id".to_string(), json!(doc.id.to_string()));
            props.insert("created_at".to_string(), json!(doc.created_at.to_rfc3339()));
            props.insert("updated_at".to_string(), json!(doc.updated_at.to_rfc3339()));
            props.insert("position".to_string(), json!(doc.position));
            props.insert("collapsed".to_string(), json!(doc.collapsed));
            if let Some(parent_id) = doc.parent_id {
                props.insert("parent_id".to_string(), json!(parent_id.to_string()));
            }
            if !doc.children.is_empty() {
                props.insert("children".to_string(), json!(doc.children.iter().map(|id| id.to_string()).collect::<Vec<_>>()));
            }
            props
        },
        created_at: doc.created_at,
        updated_at: doc.updated_at,
    }
}

/// Index a DocumentNode in the embedding database
pub async fn index_document(
    db: &EmbeddingDatabase,
    doc: &DocumentNode,
) -> Result<ToolCallResult> {
    let metadata = document_to_metadata(doc);
    let file_path = &metadata.file_path;
    
    // Generate a dummy embedding - in production this would use a real embedding model
    let content = format!("{}: {}", doc.title, doc.content);
    let embedding = generate_document_embedding(&content);
    
    match db.store_embedding(file_path, &content, &embedding, &metadata).await {
        Ok(_) => Ok(ToolCallResult {
            success: true,
            data: Some(json!({
                "indexed": true,
                "file_path": file_path,
                "document_id": doc.id.to_string()
            })),
            error: None,
        }),
        Err(e) => Ok(ToolCallResult {
            success: false,
            data: None,
            error: Some(format!("Failed to index document: {}", e)),
        }),
    }
}

/// Search for documents by content and return DocumentNode-compatible data
pub async fn search_documents(
    db: &EmbeddingDatabase,
    query: &str,
    top_k: u32,
) -> Result<ToolCallResult> {
    let query_embedding = generate_document_embedding(query);
    
    match db.search_similar(&query_embedding, top_k).await {
        Ok(results) => {
            let documents: Vec<serde_json::Value> = results
                .into_iter()
                .map(|result| json!({
                    "file_path": result.id,
                    "title": result.title,
                    "content": result.content,
                    "score": result.score,
                    "type": "document"
                }))
                .collect();
            
            Ok(ToolCallResult {
                success: true,
                data: Some(json!({
                    "documents": documents,
                    "query": query,
                    "total_results": documents.len()
                })),
                error: None,
            })
        }
        Err(e) => Ok(ToolCallResult {
            success: false,
            data: None,
            error: Some(format!("Document search failed: {}", e)),
        }),
    }
}

/// Update document properties in the embedding database
pub async fn update_document_properties(
    db: &EmbeddingDatabase,
    document_id: &str,
    properties: &HashMap<String, serde_json::Value>,
) -> Result<ToolCallResult> {
    let file_path = format!("{}.md", document_id);
    
    match db.get_embedding(&file_path).await {
        Ok(Some(mut embedding_data)) => {
            // Update properties in metadata
            for (key, value) in properties {
                embedding_data.metadata.properties.insert(key.clone(), value.clone());
            }
            embedding_data.metadata.updated_at = chrono::Utc::now();
            
            match db.store_embedding(&file_path, &embedding_data.content, &embedding_data.embedding, &embedding_data.metadata).await {
                Ok(_) => Ok(ToolCallResult {
                    success: true,
                    data: Some(json!({
                        "updated": true,
                        "document_id": document_id,
                        "properties_updated": properties.len()
                    })),
                    error: None,
                }),
                Err(e) => Ok(ToolCallResult {
                    success: false,
                    data: None,
                    error: Some(format!("Failed to update document: {}", e)),
                }),
            }
        }
        Ok(None) => Ok(ToolCallResult {
            success: false,
            data: None,
            error: Some(format!("Document not found: {}", document_id)),
        }),
        Err(e) => Ok(ToolCallResult {
            success: false,
            data: None,
            error: Some(format!("Database error: {}", e)),
        }),
    }
}

/// Generate a dummy embedding for document content
/// In production, this would call an actual embedding service
fn generate_document_embedding(content: &str) -> Vec<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    let hash = hasher.finish();
    
    // Generate a deterministic "embedding" based on content hash
    let mut embedding = vec![0.0; 1536]; // Standard embedding size
    for i in 0..1536 {
        let seed = hash.wrapping_add(i as u64);
        let mut hasher = DefaultHasher::new();
        seed.hash(&mut hasher);
        let value = hasher.finish() as f32 / u64::MAX as f32;
        embedding[i] = (value - 0.5) * 2.0; // Normalize to [-1, 1]
    }
    
    embedding
}

/// Get document statistics from the database
pub async fn get_document_stats(db: &EmbeddingDatabase) -> Result<ToolCallResult> {
    match db.get_stats().await {
        Ok(stats) => Ok(ToolCallResult {
            success: true,
            data: Some(json!({
                "total_documents": stats.get("total_files").unwrap_or(&0),
                "database_type": "duckdb",
                "embedding_dimension": 1536,
                "index_type": "cosine_similarity"
            })),
            error: None,
        }),
        Err(e) => Ok(ToolCallResult {
            success: false,
            data: None,
            error: Some(format!("Failed to get statistics: {}", e)),
        }),
    }
}
