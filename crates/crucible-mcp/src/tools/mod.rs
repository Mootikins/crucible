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
pub async fn search_by_properties(db: &EmbeddingDatabase, args: &ToolCallArgs) -> Result<ToolCallResult> {
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

    tracing::info!("Searching for files with properties: {:?}", properties);

    // Use database for consistent, offline-capable search
    match db.list_files().await {
        Ok(all_files) => {
            let mut matching_files = Vec::new();

            for file_path in all_files {
                match db.get_embedding(&file_path).await {
                    Ok(Some(embedding_data)) => {
                        // Check if file has ALL the requested properties
                        let has_all_properties = properties.iter().all(|(required_key, required_value)| {
                            if let Some(file_value) = embedding_data.metadata.properties.get(required_key) {
                                // Convert both values to strings for comparison
                                let file_value_str = match file_value {
                                    serde_json::Value::String(s) => s.clone(),
                                    serde_json::Value::Number(n) => n.to_string(),
                                    serde_json::Value::Bool(b) => b.to_string(),
                                    _ => file_value.to_string(),
                                };
                                let required_value_str = match required_value {
                                    serde_json::Value::String(s) => s.clone(),
                                    serde_json::Value::Number(n) => n.to_string(),
                                    serde_json::Value::Bool(b) => b.to_string(),
                                    _ => required_value.to_string(),
                                };
                                file_value_str.to_lowercase() == required_value_str.to_lowercase()
                            } else {
                                false
                            }
                        });

                        if has_all_properties {
                            matching_files.push(serde_json::json!({
                                "path": file_path,
                                "name": embedding_data.metadata.title.as_ref().unwrap_or(&file_path),
                                "folder": embedding_data.metadata.folder,
                                "size": 0, // We don't store size in metadata
                                "properties": embedding_data.metadata.properties,
                            }));
                        }
                    }
                    Ok(None) => {
                        tracing::debug!("File {} not found in database", file_path);
                    }
                    Err(e) => {
                        tracing::warn!("Error accessing file {}: {}", file_path, e);
                    }
                }
            }

            tracing::info!("Found {} files matching properties {:?}", matching_files.len(), properties);

            Ok(ToolCallResult {
                success: true,
                data: Some(serde_json::to_value(matching_files)?),
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

/// Search notes by tags
pub async fn search_by_tags(db: &EmbeddingDatabase, args: &ToolCallArgs) -> Result<ToolCallResult> {
    let tags = match args.tags.as_ref() {
        Some(tags) => tags,
        None => {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some("Missing tags parameter".to_string()),
            });
        }
    };

    tracing::info!("Searching for files with tags: {:?}", tags);

    // Use database for consistent, offline-capable search
    match db.list_files().await {
        Ok(all_files) => {
            let mut matching_files = Vec::new();

            for file_path in all_files {
                match db.get_embedding(&file_path).await {
                    Ok(Some(embedding_data)) => {
                        // Check if file has ALL the requested tags
                        let has_all_tags = tags.iter().all(|required_tag| {
                            embedding_data.metadata.tags.iter().any(|file_tag| {
                                file_tag.to_lowercase() == required_tag.to_lowercase()
                            })
                        });

                        if has_all_tags {
                            matching_files.push(serde_json::json!({
                                "path": file_path,
                                "name": embedding_data.metadata.title.as_ref().unwrap_or(&file_path),
                                "folder": embedding_data.metadata.folder,
                                "size": 0, // We don't store size in metadata
                                "tags": embedding_data.metadata.tags,
                            }));
                        }
                    }
                    Ok(None) => {
                        tracing::debug!("File {} not found in database", file_path);
                    }
                    Err(e) => {
                        tracing::warn!("Error accessing file {}: {}", file_path, e);
                    }
                }
            }

            tracing::info!("Found {} files matching tags {:?}", matching_files.len(), tags);

            Ok(ToolCallResult {
                success: true,
                data: Some(serde_json::to_value(matching_files)?),
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
    use crate::utils::path_sanitizer::{sanitize_error_message, safe_path_for_logging};
    use std::path::Path;

    let force = args.force.unwrap_or(false);

    // Try to create ObsidianClient to fetch data from the Obsidian plugin API
    let client_result = ObsidianClient::new();

    let client = match client_result {
        Ok(client) => {
            tracing::info!("Successfully connected to Obsidian plugin for indexing");
            Some(client)
        }
        Err(e) => {
            tracing::warn!("Failed to connect to Obsidian plugin: {}. Using filesystem scanning only.", e);
            None
        }
    };

    let mut indexed_count = 0;
    let mut errors = Vec::new();

    // Try to get list of all files from Obsidian, with fallback to filesystem scanning
    let files = if let Some(client) = &client {
        match client.list_files().await {
            Ok(files) => {
                tracing::info!("Successfully retrieved {} files from Obsidian plugin", files.len());
                files
            }
            Err(e) => {
                tracing::warn!("Failed to list files from Obsidian: {}. Attempting filesystem fallback.", e);
                // Fallback: try to scan filesystem if a path is provided
                if let Some(vault_path) = args.path.as_ref() {
                    match scan_filesystem_for_markdown_files(vault_path).await {
                        Ok(files) => {
                            tracing::info!("Filesystem fallback found {} markdown files", files.len());
                            files
                        }
                        Err(scan_err) => {
                            let error_msg = sanitize_error_message(
                                &format!("Failed to list files from Obsidian: {} and filesystem fallback failed: {}", e, scan_err),
                                Some(Path::new(vault_path))
                            );
                            return Ok(ToolCallResult {
                                success: false,
                                data: None,
                                error: Some(error_msg),
                            });
                        }
                    }
                } else {
                    let error_msg = sanitize_error_message(
                        &format!("Failed to list files from Obsidian: {}. Provide a valid vault path for filesystem fallback.", e),
                        None
                    );
                    return Ok(ToolCallResult {
                        success: false,
                        data: None,
                        error: Some(error_msg),
                    });
                }
            }
        }
    } else {
        tracing::info!("No Obsidian plugin available, using filesystem scanning directly");
        // Fallback: try to scan filesystem if a path is provided
        if let Some(vault_path) = args.path.as_ref() {
            match scan_filesystem_for_markdown_files(vault_path).await {
                Ok(files) => {
                    tracing::info!("Filesystem scanning found {} markdown files", files.len());
                    files
                }
                Err(scan_err) => {
                    let error_msg = sanitize_error_message(
                        &format!("Filesystem scanning failed: {}", scan_err),
                        Some(Path::new(vault_path))
                    );
                    return Ok(ToolCallResult {
                        success: false,
                        data: None,
                        error: Some(error_msg),
                    });
                }
            }
        } else {
            return Ok(ToolCallResult {
                success: false,
                data: None,
                error: Some("No Obsidian plugin available and no vault path provided for filesystem scanning".to_string()),
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

                // Try to get file content from Obsidian if available, otherwise use filesystem
                let content = if let Some(client) = &client {
                    match client.get_file(file_path).await {
                        Ok(content) => {
                            tracing::debug!("Got content for {} from Obsidian plugin", safe_path_for_logging(Path::new(file_path), None));
                            content
                        }
                        Err(e) => {
                            tracing::warn!("Failed to get content for {} from Obsidian: {}. Trying filesystem fallback.",
                                safe_path_for_logging(Path::new(file_path), None), e);

                            // Fallback: try to read from filesystem
                            match read_file_content_fallback(file_path).await {
                                Ok(content) => {
                                    tracing::debug!("Got content for {} from filesystem fallback", safe_path_for_logging(Path::new(file_path), None));
                                    content
                                }
                                Err(read_err) => {
                                    let sanitized_error = sanitize_error_message(
                                        &format!("Failed to get content for {}: Obsidian error: {}, Filesystem error: {}",
                                            safe_path_for_logging(Path::new(file_path), None), e, read_err),
                                        Some(Path::new(file_path))
                                    );
                                    errors.push(sanitized_error);
                                    continue;
                                }
                            }
                        }
                    }
                } else {
                    // No Obsidian client, use filesystem directly
                    match read_file_content_fallback(file_path).await {
                        Ok(content) => {
                            tracing::debug!("Got content for {} from filesystem", safe_path_for_logging(Path::new(file_path), None));
                            content
                        }
                        Err(read_err) => {
                            let sanitized_error = sanitize_error_message(
                                &format!("Failed to get content for {} from filesystem: {}",
                                    safe_path_for_logging(Path::new(file_path), None), read_err),
                                Some(Path::new(file_path))
                            );
                            errors.push(sanitized_error);
                            continue;
                        }
                    }
                };

                // Try to get file metadata from Obsidian if available, otherwise use filesystem
                let obs_metadata = if let Some(client) = &client {
                    match client.get_metadata(file_path).await {
                        Ok(metadata) => {
                            tracing::debug!("Got metadata for {} from Obsidian plugin", safe_path_for_logging(Path::new(file_path), None));
                            metadata
                        }
                        Err(e) => {
                            tracing::warn!("Failed to get metadata for {} from Obsidian: {}. Using basic filesystem metadata.",
                                safe_path_for_logging(Path::new(file_path), None), e);

                            // Fallback: create basic metadata from filesystem
                            match create_basic_file_metadata(file_path).await {
                                Ok(metadata) => {
                                    tracing::debug!("Created basic metadata for {} from filesystem", safe_path_for_logging(Path::new(file_path), None));
                                    metadata
                                }
                                Err(metadata_err) => {
                                    tracing::warn!("Failed to create basic metadata for {}: {}", safe_path_for_logging(Path::new(file_path), None), metadata_err);
                                    // Continue with minimal metadata
                                    create_minimal_file_metadata(file_path)
                                }
                            }
                        }
                    }
                } else {
                    // No Obsidian client, use filesystem directly
                    match create_basic_file_metadata(file_path).await {
                        Ok(metadata) => {
                            tracing::debug!("Created basic metadata for {} from filesystem", safe_path_for_logging(Path::new(file_path), None));
                            metadata
                        }
                        Err(metadata_err) => {
                            tracing::warn!("Failed to create basic metadata for {}: {}", safe_path_for_logging(Path::new(file_path), None), metadata_err);
                            // Continue with minimal metadata
                            create_minimal_file_metadata(file_path)
                        }
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

/// Scan filesystem for markdown files as fallback when Obsidian plugin is unavailable
async fn scan_filesystem_for_markdown_files(vault_path: &str) -> Result<Vec<crate::obsidian_client::FileInfo>> {
    use std::fs;
    use std::path::Path;

    let vault_dir = Path::new(vault_path);
    if !vault_dir.exists() {
        return Err(anyhow::anyhow!("Vault path does not exist: {}", vault_path));
    }

    let mut files = Vec::new();

    // Use glob pattern to find all markdown files
    let pattern = format!("{}/**/*.md", vault_path);

    for entry in glob::glob(&pattern).map_err(|e| anyhow::anyhow!("Invalid glob pattern: {}", e))? {
        match entry {
            Ok(path) => {
                // Skip hidden directories and files
                if path.components().any(|c| {
                    c.as_os_str().to_string_lossy().starts_with('.')
                }) {
                    continue;
                }

                // Get file metadata
                match fs::metadata(&path) {
                    Ok(metadata) => {
                        // Get relative path from vault root
                        let relative_path = path.strip_prefix(vault_dir)
                            .unwrap_or(&path)
                            .to_string_lossy();

                        // Extract folder from path
                        let folder = path.parent()
                            .and_then(|p| p.strip_prefix(vault_dir).ok())
                            .and_then(|p| p.to_str())
                            .unwrap_or("")
                            .to_string();

                        // Get file name
                        let name = path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .to_string();

                        let modified_timestamp = metadata.modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .and_then(|d| i64::try_from(d.as_secs()).ok())
                            .unwrap_or(0);

                        let created_timestamp = metadata.created()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .and_then(|d| i64::try_from(d.as_secs()).ok())
                            .unwrap_or(modified_timestamp);

                        files.push(crate::obsidian_client::FileInfo {
                            path: relative_path.to_string(),
                            name,
                            folder,
                            extension: "md".to_string(),
                            size: metadata.len(),
                            created: created_timestamp,
                            modified: modified_timestamp,
                        });
                    }
                    Err(e) => {
                        tracing::warn!("Failed to get metadata for {}: {}", path.display(), e);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Error during glob iteration: {}", e);
            }
        }
    }

    if files.is_empty() {
        return Err(anyhow::anyhow!("No markdown files found in vault path: {}", vault_path));
    }

    Ok(files)
}

/// Read file content as fallback when Obsidian plugin is unavailable
async fn read_file_content_fallback(file_path: &str) -> Result<String> {
    use std::fs;

    // If file_path is already an absolute path, use it directly
    // Otherwise, assume it's relative to the current directory
    let full_path = if std::path::Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        // Try to resolve relative to current directory
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(file_path)
            .to_string_lossy()
            .to_string()
    };

    fs::read_to_string(&full_path)
        .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", full_path, e))
}

/// Create basic file metadata from filesystem as fallback when Obsidian plugin is unavailable
async fn create_basic_file_metadata(file_path: &str) -> Result<crate::obsidian_client::FileMetadata> {
    use std::fs;
    use std::path::Path;

    let path = Path::new(file_path);

    // Try to read the file to extract basic frontmatter
    let content = fs::read_to_string(path)?;

    // Extract basic metadata from content
    let mut tags = Vec::new();
    let mut properties = std::collections::HashMap::new();
    let mut title = None;

    // Simple frontmatter parsing
    if content.starts_with("---") {
        let lines: Vec<&str> = content.lines().collect();
        let mut in_frontmatter = false;

        for line in lines.iter().skip(1) {
            if *line == "---" {
                break;
            }

            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');

                match key {
                    "tags" => {
                        // Simple tag parsing (comma-separated or array-like)
                        if value.contains('[') {
                            // Array format: [tag1, tag2]
                            let cleaned = value.trim_matches(|c| c == '[' || c == ']' || c == ' ');
                            tags.extend(cleaned.split(',').map(|t| t.trim().to_string()));
                        } else {
                            // Comma-separated: tag1, tag2
                            tags.extend(value.split(',').map(|t| t.trim().to_string()));
                        }
                    }
                    "title" => {
                        title = Some(value.to_string());
                    }
                    _ => {
                        properties.insert(key.to_string(), serde_json::Value::String(value.to_string()));
                    }
                }
            }
        }
    }

    // Extract folder from path
    let folder = path.parent()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .to_string();

    let file_metadata = fs::metadata(path);
    let size = file_metadata.as_ref().map(|m| m.len()).unwrap_or(0);
    let created = file_metadata
        .as_ref()
        .ok()
        .and_then(|m| m.created().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0);
    let modified = file_metadata
        .as_ref()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(created);

    Ok(crate::obsidian_client::FileMetadata {
        path: file_path.to_string(),
        properties,
        tags,
        folder,
        links: Vec::new(), // Would need more complex parsing to extract links
        backlinks: Vec::new(),
        stats: crate::obsidian_client::FileStats {
            size,
            created,
            modified,
            word_count: 0, // Would need more complex parsing to count words
        },
    })
}

/// Create minimal file metadata when all else fails
fn create_minimal_file_metadata(file_path: &str) -> crate::obsidian_client::FileMetadata {
    use std::path::Path;

    let path = Path::new(file_path);

    let folder = path.parent()
        .and_then(|p| p.to_str())
        .unwrap_or("")
        .to_string();

    crate::obsidian_client::FileMetadata {
        path: file_path.to_string(),
        properties: std::collections::HashMap::new(),
        tags: Vec::new(),
        folder,
        links: Vec::new(),
        backlinks: Vec::new(),
        stats: crate::obsidian_client::FileStats {
            size: 0,
            created: 0,
            modified: 0,
            word_count: 0,
        },
    }
}
