// Tool modules removed - all functionality moved to mod.rs

use crate::database::EmbeddingDatabase;
use crate::types::{ToolCallArgs, ToolCallResult};
use anyhow::Result;
use std::collections::HashMap;

/// Search notes by frontmatter properties
pub async fn search_by_properties(
    db: &EmbeddingDatabase,
    args: &ToolCallArgs,
) -> Result<ToolCallResult> {
    let properties = match args.properties.as_ref() {
        Some(props) => props,
        None => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some("Missing properties".to_string()),
            });
        }
    };

    match db.search_by_properties(properties).await {
        Ok(files) => Ok(ToolCallResult {
            success: true,
            data: Some(serde_json::to_value(files)?),
            error: None,
        }),
        Err(e) => Ok(ToolCallResult {
            success: false,
            data: None,
            error: Some(format!("Database error: {}", e)),
        }),
    }
}

/// Search notes by tags
pub async fn search_by_tags(db: &EmbeddingDatabase, args: &ToolCallArgs) -> Result<ToolCallResult> {
    let tags = match args.tags.as_ref() {
        Some(tags) => tags,
        None => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some("Missing tags".to_string()),
            });
        }
    };

    match db.search_by_tags(tags).await {
        Ok(files) => Ok(ToolCallResult {
            success: true,
            data: Some(serde_json::to_value(files)?),
            error: None,
        }),
        Err(e) => Ok(ToolCallResult {
            success: false,
            data: None,
            error: Some(format!("Database error: {}", e)),
        }),
    }
}

/// Search notes in a specific folder
pub async fn search_by_folder(
    db: &EmbeddingDatabase,
    args: &ToolCallArgs,
) -> Result<ToolCallResult> {
    let path = match args.path.as_ref() {
        Some(path) => path,
        None => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some("Missing path".to_string()),
            });
        }
    };
    let recursive = args.recursive.unwrap_or(true);

    match db.list_files().await {
        Ok(all_files) => {
            let filtered_files: Vec<String> = all_files
                .into_iter()
                .filter(|file_path| {
                    if recursive {
                        file_path.starts_with(path)
                    } else {
                        file_path.starts_with(path) && !file_path[path.len()..].contains('/')
                    }
                })
                .collect();

            Ok(ToolCallResult {
                success: true,
                data: Some(serde_json::to_value(filtered_files)?),
                error: None,
            })
        }
        Err(e) => Ok(ToolCallResult {
            success: false,
            data: None,
            error: Some(format!("Database error: {}", e)),
        }),
    }
}

/// Search notes by filename pattern
pub async fn search_by_filename(
    db: &EmbeddingDatabase,
    args: &ToolCallArgs,
) -> Result<ToolCallResult> {
    let pattern = match args.pattern.as_ref() {
        Some(pattern) => pattern,
        None => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some("Missing pattern".to_string()),
            });
        }
    };

    match db.list_files().await {
        Ok(all_files) => {
            let filtered_files: Vec<String> = all_files
                .into_iter()
                .filter(|file_path| {
                    if pattern.contains('*') {
                        // Simple wildcard matching
                        let regex_pattern = pattern.replace('*', ".*");
                        let regex = regex::Regex::new(&format!("^{}$", regex_pattern));
                        match regex {
                            Ok(re) => re.is_match(file_path),
                            Err(_) => file_path.contains(pattern),
                        }
                    } else {
                        file_path.contains(pattern)
                    }
                })
                .collect();

            Ok(ToolCallResult {
                success: true,
                data: Some(serde_json::to_value(filtered_files)?),
                error: None,
            })
        }
        Err(e) => Ok(ToolCallResult {
            success: false,
            data: None,
            error: Some(format!("Database error: {}", e)),
        }),
    }
}

/// Full-text search in note contents
pub async fn search_by_content(
    db: &EmbeddingDatabase,
    args: &ToolCallArgs,
) -> Result<ToolCallResult> {
    let query = match args.query.as_ref() {
        Some(query) => query,
        None => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some("Missing query".to_string()),
            });
        }
    };

    match db.list_files().await {
        Ok(all_files) => {
            let mut results = Vec::new();

            for file_path in all_files {
                match db.get_embedding(&file_path).await {
                    Ok(Some(embedding_data)) => {
                        if embedding_data
                            .content
                            .to_lowercase()
                            .contains(&query.to_lowercase())
                        {
                            results.push(serde_json::json!({
                                "file_path": file_path,
                                "content": embedding_data.content,
                                "metadata": embedding_data.metadata
                            }));
                        }
                    }
                    Ok(None) => continue,
                    Err(e) => {
                        return Ok(ToolCallResult {
                            success: false,
                            data: None,
                            error: Some(format!("Error accessing file {}: {}", file_path, e)),
                        });
                    }
                }
            }

            Ok(ToolCallResult {
                success: true,
                data: Some(serde_json::to_value(results)?),
                error: None,
            })
        }
        Err(e) => Ok(ToolCallResult {
            success: false,
            data: None,
            error: Some(format!("Database error: {}", e)),
        }),
    }
}

/// Semantic search using embeddings
pub async fn semantic_search(
    db: &EmbeddingDatabase,
    args: &ToolCallArgs,
) -> Result<ToolCallResult> {
    let query = match args.query.as_ref() {
        Some(query) => query,
        None => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some("Missing query".to_string()),
            });
        }
    };
    let top_k = args.top_k.unwrap_or(10);

    // Generate a dummy embedding for the query
    // In production, this would use an actual embedding model
    let dummy_embedding = generate_dummy_embedding(query);

    match db.search_similar(&dummy_embedding, top_k).await {
        Ok(results) => Ok(ToolCallResult {
            success: true,
            data: Some(serde_json::to_value(results)?),
            error: None,
        }),
        Err(e) => Ok(ToolCallResult {
            success: false,
            data: None,
            error: Some(format!("Semantic search error: {}", e)),
        }),
    }
}

/// Generate embeddings for all vault notes
pub async fn index_vault(db: &EmbeddingDatabase, args: &ToolCallArgs) -> Result<ToolCallResult> {
    let force = args.force.unwrap_or(false);

    // Create some dummy files for testing
    let dummy_files = vec!["file0.md", "file1.md", "file2.md", "file3.md", "file4.md"];

    let mut indexed_count = 0;
    let mut errors = Vec::new();

    for file_path in dummy_files {
        match db.file_exists(file_path).await {
            Ok(exists) => {
                if !force && exists {
                    continue;
                }

                // For demo purposes, create dummy content and embedding
                // In production, this would read actual file content and generate real embeddings
                let dummy_content = format!("Content for file: {}", file_path);
                let dummy_embedding = generate_dummy_embedding(&dummy_content);

                // Create metadata
                let metadata = crate::types::EmbeddingMetadata {
                    file_path: file_path.to_string(),
                    title: Some(file_path.to_string()),
                    tags: vec!["indexed".to_string()],
                    folder: "vault".to_string(),
                    properties: HashMap::new(),
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                };

                // Store embedding
                match db
                    .store_embedding(file_path, &dummy_content, &dummy_embedding, &metadata)
                    .await
                {
                    Ok(_) => indexed_count += 1,
                    Err(e) => errors.push(format!("Failed to index {}: {}", file_path, e)),
                }
            }
            Err(e) => errors.push(format!("Failed to check existence of {}: {}", file_path, e)),
        }
    }

    if errors.is_empty() {
        Ok(ToolCallResult {
            success: true,
            data: Some(serde_json::json!({ "indexed": indexed_count })),
            error: None,
        })
    } else {
        Ok(ToolCallResult {
            success: false,
            data: Some(serde_json::json!({
                "indexed": indexed_count,
                "errors": errors
            })),
            error: Some(format!("Indexing completed with {} errors", errors.len())),
        })
    }
}

/// Get metadata for a specific note
pub async fn get_note_metadata(
    db: &EmbeddingDatabase,
    args: &ToolCallArgs,
) -> Result<ToolCallResult> {
    let path = match args.path.as_ref() {
        Some(path) => path,
        None => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some("Missing path".to_string()),
            });
        }
    };

    match db.get_embedding(path).await {
        Ok(Some(embedding_data)) => Ok(ToolCallResult {
            success: true,
            data: Some(serde_json::to_value(embedding_data.metadata)?),
            error: None,
        }),
        Ok(None) => Ok(ToolCallResult {
            success: false,
            data: None,
            error: Some(format!("File not found: {}", path)),
        }),
        Err(e) => Ok(ToolCallResult {
            success: false,
            data: None,
            error: Some(format!("Database error: {}", e)),
        }),
    }
}

/// Update frontmatter properties of a note
pub async fn update_note_properties(
    db: &EmbeddingDatabase,
    args: &ToolCallArgs,
) -> Result<ToolCallResult> {
    let path = match args.path.as_ref() {
        Some(path) => path,
        None => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some("Missing path".to_string()),
            });
        }
    };

    let properties = match args.properties.as_ref() {
        Some(properties) => properties,
        None => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some("Missing properties".to_string()),
            });
        }
    };

    match db.get_embedding(path).await {
        Ok(Some(mut embedding_data)) => {
            // Update properties in metadata
            for (key, value) in properties {
                embedding_data
                    .metadata
                    .properties
                    .insert(key.clone(), value.clone());
            }
            embedding_data.metadata.updated_at = chrono::Utc::now();

            // Store updated embedding
            match db
                .store_embedding(
                    path,
                    &embedding_data.content,
                    &embedding_data.embedding,
                    &embedding_data.metadata,
                )
                .await
            {
                Ok(_) => Ok(ToolCallResult {
                    success: true,
                    data: Some(serde_json::json!({ "success": true })),
                    error: None,
                }),
                Err(e) => Ok(ToolCallResult {
                    success: false,
                    data: None,
                    error: Some(format!("Failed to update file: {}", e)),
                }),
            }
        }
        Ok(None) => Ok(ToolCallResult {
            success: false,
            data: None,
            error: Some(format!("File not found: {}", path)),
        }),
        Err(e) => Ok(ToolCallResult {
            success: false,
            data: None,
            error: Some(format!("Database error: {}", e)),
        }),
    }
}

/// Index a Crucible document
pub async fn index_document(db: &EmbeddingDatabase, args: &ToolCallArgs) -> Result<ToolCallResult> {
    // Parse document from arguments
    let document_value = match args.properties.as_ref().and_then(|p| p.get("document")) {
        Some(doc) => doc,
        None => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some("Missing document".to_string()),
            });
        }
    };

    // In a real implementation, you would deserialize this to a DocumentNode
    // For now, we'll work with the JSON directly
    let title = document_value
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("Untitled");
    let content = document_value
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("");
    let id = document_value
        .get("id")
        .and_then(|i| i.as_str())
        .unwrap_or("unknown");

    let file_path = format!("{}.md", title);
    let full_content = format!("{}: {}", title, content);
    let embedding = generate_dummy_embedding(&full_content);

    let metadata = crate::types::EmbeddingMetadata {
        file_path: file_path.clone(),
        title: Some(title.to_string()),
        tags: vec!["document".to_string()],
        folder: "documents".to_string(),
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "document_id".to_string(),
                serde_json::Value::String(id.to_string()),
            );
            props.insert(
                "type".to_string(),
                serde_json::Value::String("crucible_document".to_string()),
            );
            props
        },
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    match db
        .store_embedding(&file_path, &full_content, &embedding, &metadata)
        .await
    {
        Ok(_) => Ok(ToolCallResult {
            success: true,
            data: Some(serde_json::json!({
                "indexed": true,
                "file_path": file_path,
                "document_id": id
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

/// Search Crucible documents
pub async fn search_documents(
    db: &EmbeddingDatabase,
    args: &ToolCallArgs,
) -> Result<ToolCallResult> {
    let query = match args.query.as_ref() {
        Some(query) => query,
        None => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some("Missing query".to_string()),
            });
        }
    };

    let top_k = args.top_k.unwrap_or(10);
    let query_embedding = generate_dummy_embedding(query);

    match db.search_similar(&query_embedding, top_k).await {
        Ok(results) => {
            let documents: Vec<serde_json::Value> = results
                .into_iter()
                .map(|result| {
                    serde_json::json!({
                        "file_path": result.id,
                        "title": result.title,
                        "content": result.content,
                        "score": result.score,
                        "type": "document"
                    })
                })
                .collect();

            Ok(ToolCallResult {
                success: true,
                data: Some(serde_json::json!({
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

/// Get document statistics
pub async fn get_document_stats(
    db: &EmbeddingDatabase,
    _args: &ToolCallArgs,
) -> Result<ToolCallResult> {
    match db.get_stats().await {
        Ok(stats) => Ok(ToolCallResult {
            success: true,
            data: Some(serde_json::json!({
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

/// Update document properties
pub async fn update_document_properties(
    db: &EmbeddingDatabase,
    args: &ToolCallArgs,
) -> Result<ToolCallResult> {
    let document_id = match args
        .properties
        .as_ref()
        .and_then(|p| p.get("document_id"))
        .and_then(|id| id.as_str())
    {
        Some(id) => id,
        None => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some("Missing document_id".to_string()),
            });
        }
    };

    let properties = match args
        .properties
        .as_ref()
        .and_then(|p| p.get("properties"))
        .and_then(|props| props.as_object())
    {
        Some(props) => props,
        None => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some("Missing properties".to_string()),
            });
        }
    };

    let file_path = format!("{}.md", document_id);

    match db.get_embedding(&file_path).await {
        Ok(Some(mut embedding_data)) => {
            // Update properties in metadata
            for (key, value) in properties {
                embedding_data
                    .metadata
                    .properties
                    .insert(key.clone(), value.clone());
            }
            embedding_data.metadata.updated_at = chrono::Utc::now();

            match db
                .store_embedding(
                    &file_path,
                    &embedding_data.content,
                    &embedding_data.embedding,
                    &embedding_data.metadata,
                )
                .await
            {
                Ok(_) => Ok(ToolCallResult {
                    success: true,
                    data: Some(serde_json::json!({
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

/// Generate a dummy embedding for testing purposes
/// In production, this would call an actual embedding API
fn generate_dummy_embedding(content: &str) -> Vec<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    let hash = hasher.finish();

    // Generate a deterministic "embedding" based on content hash
    let mut embedding = vec![0.0; 1536];
    for i in 0..1536 {
        let seed = hash.wrapping_add(i as u64);
        let mut hasher = DefaultHasher::new();
        seed.hash(&mut hasher);
        let value = hasher.finish() as f32 / u64::MAX as f32;
        embedding[i] = (value - 0.5) * 2.0; // Normalize to [-1, 1]
    }

    embedding
}
