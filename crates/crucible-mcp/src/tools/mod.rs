// Tool modules removed - all functionality moved to mod.rs

use crate::database::EmbeddingDatabase;
use crate::embeddings::EmbeddingProvider;
use crate::types::{ToolCallArgs, ToolCallResult};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

/// Sync metadata from Obsidian plugin to database for all existing files
///
/// This updates tags, properties, and folder information without re-generating embeddings.
/// Only updates files that already exist in the database.
pub async fn sync_metadata_from_obsidian(db: &EmbeddingDatabase) -> Result<(usize, Vec<String>)> {
    use crate::obsidian_client::ObsidianClient;

    let client = match ObsidianClient::new() {
        Ok(client) => client,
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Failed to connect to Obsidian plugin: {}. Make sure the Crucible plugin is running.",
                e
            ));
        }
    };

    let mut synced_count = 0;
    let mut errors = Vec::new();

    // Get list of files that exist in the database
    let db_files = db.list_files().await?;

    tracing::info!("Syncing metadata for {} files from Obsidian", db_files.len());

    for file_path in db_files {
        // Get fresh metadata from Obsidian
        match client.get_metadata(&file_path).await {
            Ok(obs_metadata) => {
                // Create updated metadata struct
                let metadata = crate::types::EmbeddingMetadata {
                    file_path: file_path.clone(),
                    title: obs_metadata.properties.get("title")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| Some(file_path.clone())),
                    tags: obs_metadata.tags.clone(),
                    folder: obs_metadata.folder.clone(),
                    properties: obs_metadata.properties.clone(),
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                };

                // Update metadata in database
                match db.update_metadata(&file_path, &metadata).await {
                    Ok(_) => {
                        synced_count += 1;
                        tracing::debug!("Synced metadata for {}: {:?}", file_path, obs_metadata.tags);
                    }
                    Err(e) => {
                        errors.push(format!("Failed to update metadata for {}: {}", file_path, e));
                    }
                }
            }
            Err(e) => {
                errors.push(format!("Failed to get metadata for {}: {}", file_path, e));
            }
        }
    }

    tracing::info!("Metadata sync complete: {} files synced, {} errors", synced_count, errors.len());

    Ok((synced_count, errors))
}

/// Search notes by frontmatter properties
pub async fn search_by_properties(
    db: &EmbeddingDatabase,
    args: &ToolCallArgs,
) -> Result<ToolCallResult> {
    // Sync metadata from Obsidian before searching
    if let Err(e) = sync_metadata_from_obsidian(db).await {
        tracing::warn!("Failed to sync metadata before search: {}", e);
    }

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
    // Sync metadata from Obsidian before searching
    if let Err(e) = sync_metadata_from_obsidian(db).await {
        tracing::warn!("Failed to sync metadata before search: {}", e);
    }

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
    provider: &Arc<dyn EmbeddingProvider>,
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

    // Generate embedding for the query using the provider
    let embedding_result = match provider.embed(query).await {
        Ok(result) => result,
        Err(e) => {
            tracing::error!("Failed to generate embedding for query: {}", e);
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some(format!("Failed to generate embedding for query: {}", e)),
            });
        }
    };
    let query_embedding = embedding_result.embedding;

    match db.search_similar(&query_embedding, top_k).await {
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
pub async fn index_vault(
    db: &EmbeddingDatabase,
    provider: &Arc<dyn EmbeddingProvider>,
    args: &ToolCallArgs,
) -> Result<ToolCallResult> {
    use crate::obsidian_client::ObsidianClient;

    let force = args.force.unwrap_or(false);

    // Create ObsidianClient to fetch data from the Obsidian plugin API
    let client = match ObsidianClient::new() {
        Ok(client) => client,
        Err(e) => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some(format!("Failed to connect to Obsidian plugin: {}. Make sure the Crucible plugin is running.", e)),
            });
        }
    };

    let mut indexed_count = 0;
    let mut errors = Vec::new();

    // Get list of all files from Obsidian
    let files = match client.list_files().await {
        Ok(files) => files,
        Err(e) => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some(format!("Failed to list files from Obsidian: {}", e)),
            });
        }
    };

    // Filter to only markdown files if needed
    let md_files: Vec<_> = files
        .into_iter()
        .filter(|f| f.path.ends_with(".md"))
        .collect();

    tracing::info!("Found {} markdown files to index", md_files.len());

    // Prepare content for batch embedding
    let mut files_to_index = Vec::new();
    let mut file_contents = Vec::new();
    let mut file_metadata = Vec::new();

    for file_info in md_files {
        let file_path = &file_info.path;

        // Check if already indexed (unless force flag is set)
        match db.file_exists(file_path).await {
            Ok(exists) => {
                if !force && exists {
                    continue;
                }

                // Get file content from Obsidian
                let content = match client.get_file(file_path).await {
                    Ok(content) => content,
                    Err(e) => {
                        errors.push(format!("Failed to get content for {}: {}", file_path, e));
                        continue;
                    }
                };

                // Get file metadata (tags, properties, etc.) from Obsidian
                let obs_metadata = match client.get_metadata(file_path).await {
                    Ok(metadata) => metadata,
                    Err(e) => {
                        errors.push(format!("Failed to get metadata for {}: {}", file_path, e));
                        continue;
                    }
                };

                // Limit content length to avoid Ollama batch size issues
                const MAX_CONTENT_LENGTH: usize = 8000;
                let original_length = content.len();
                let truncated_content = if content.len() > MAX_CONTENT_LENGTH {
                    let mut truncated = content.chars().take(MAX_CONTENT_LENGTH).collect::<String>();
                    truncated.push_str("...");
                    truncated
                } else {
                    content
                };

                tracing::debug!(
                    "Processing file {}: original length={}, processed length={}, tags={:?}",
                    file_path,
                    original_length,
                    truncated_content.len(),
                    obs_metadata.tags
                );

                // Check if we have any content after processing
                if truncated_content.trim().is_empty() {
                    tracing::warn!("File {} appears to be empty or only whitespace after processing", file_path);
                }

                files_to_index.push(file_path.clone());
                file_contents.push(truncated_content);
                file_metadata.push(obs_metadata);
            }
            Err(e) => errors.push(format!("Failed to check existence of {}: {}", file_path, e)),
        }
    }

    // Generate embeddings in batch if there are files to index
    if !files_to_index.is_empty() {
        // Clone content strings for the provider (embed_batch takes ownership)
        let content_strings: Vec<String> = file_contents.clone();

        match provider.embed_batch(content_strings).await {
            Ok(embedding_results) => {
                // Store each embedding with metadata from Obsidian
                for (idx, (file_path, content)) in files_to_index.iter().zip(file_contents.iter()).enumerate() {
                    if let Some(embedding_result) = embedding_results.get(idx) {
                        if let Some(obs_metadata) = file_metadata.get(idx) {
                            // Create metadata using data from Obsidian
                            let metadata = crate::types::EmbeddingMetadata {
                                file_path: file_path.clone(),
                                title: obs_metadata.properties.get("title")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                                    .or_else(|| Some(file_path.clone())),
                                tags: obs_metadata.tags.clone(),
                                folder: obs_metadata.folder.clone(),
                                properties: obs_metadata.properties.clone(),
                                created_at: chrono::Utc::now(),
                                updated_at: chrono::Utc::now(),
                            };

                            // Store embedding
                            match db
                                .store_embedding(file_path, content, &embedding_result.embedding, &metadata)
                                .await
                            {
                                Ok(_) => indexed_count += 1,
                                Err(e) => errors.push(format!("Failed to index {}: {}", file_path, e)),
                            }
                        }
                    }
                }
            }
            Err(e) => {
                errors.push(format!("Failed to generate embeddings: {}", e));
            }
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
pub async fn index_document(
    db: &EmbeddingDatabase,
    provider: &Arc<dyn EmbeddingProvider>,
    args: &ToolCallArgs,
) -> Result<ToolCallResult> {
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

    // Generate embedding using the provider
    let embedding_result = match provider.embed(&full_content).await {
        Ok(result) => result,
        Err(e) => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some(format!("Failed to generate embedding for document: {}", e)),
            });
        }
    };
    let embedding = embedding_result.embedding;

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
    provider: &Arc<dyn EmbeddingProvider>,
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

    // Generate embedding using the provider
    let embedding_result = match provider.embed(query).await {
        Ok(result) => result,
        Err(e) => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some(format!("Failed to generate embedding for query: {}", e)),
            });
        }
    };
    let query_embedding = embedding_result.embedding;

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
