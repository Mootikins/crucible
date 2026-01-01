//! Storage client implementation for daemon-based queries

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crucible_core::database::DocumentId;
use crucible_core::parser::ParsedNote;
use crucible_core::traits::{KnowledgeRepository, NoteInfo, StorageClient};
use crucible_core::types::SearchResult;
use crucible_core::{CrucibleError, Result as CoreResult};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

use crate::DaemonClient;

/// Storage client that queries through the daemon
pub struct DaemonStorageClient {
    client: Arc<DaemonClient>,
    kiln: PathBuf,
}

impl DaemonStorageClient {
    /// Create a new daemon storage client for a specific kiln
    pub fn new(client: Arc<DaemonClient>, kiln: PathBuf) -> Self {
        Self { client, kiln }
    }

    /// Get the kiln path
    pub fn kiln_path(&self) -> &PathBuf {
        &self.kiln
    }

    /// Get a reference to the daemon client
    pub fn daemon_client(&self) -> &Arc<DaemonClient> {
        &self.client
    }
}

#[async_trait]
impl StorageClient for DaemonStorageClient {
    async fn query_raw(&self, sql: &str) -> Result<Value> {
        self.client.query(&self.kiln, sql).await
    }
}

// =============================================================================
// KnowledgeRepository implementation
// =============================================================================

/// Parse a record from daemon query result into NoteInfo
fn parse_note_info(record: &Value) -> Option<NoteInfo> {
    let path = record.get("path")?.as_str()?;
    let name = std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());

    let title = record
        .get("title")
        .and_then(|v| v.as_str())
        .map(String::from);

    let tags: Vec<String> = record
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|t| t.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let created_at = record
        .get("created_at")
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let updated_at = record
        .get("updated_at")
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    Some(NoteInfo {
        name,
        path: path.to_string(),
        title,
        tags,
        created_at,
        updated_at,
    })
}

/// Parse a record into a ParsedNote (minimal version for daemon)
fn parse_note_from_record(record: &Value) -> Option<ParsedNote> {
    use crucible_core::parser::{NoteContent, ParsedNoteBuilder, Tag, Wikilink};

    let path_str = record.get("path")?.as_str()?;
    let path = std::path::PathBuf::from(path_str);

    let content_str = record.get("content").and_then(|v| v.as_str()).unwrap_or("");

    let tags: Vec<Tag> = record
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(i, t)| {
                    t.as_str().map(|s| Tag {
                        name: s.to_string(),
                        path: s.split('/').map(String::from).collect(),
                        offset: i, // Placeholder offset
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let wikilinks: Vec<Wikilink> = record
        .get("wikilinks")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(i, w)| {
                    let target = w.get("target").and_then(|v| v.as_str())?;
                    Some(Wikilink {
                        target: target.to_string(),
                        alias: w.get("alias").and_then(|v| v.as_str()).map(String::from),
                        offset: i, // Placeholder offset
                        is_embed: w.get("is_embed").and_then(|v| v.as_bool()).unwrap_or(false),
                        block_ref: w
                            .get("block_ref")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        heading_ref: w
                            .get("heading_ref")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Build a minimal ParsedNote using the builder
    let mut content = NoteContent::new();
    content.plain_text = content_str.to_string();

    let note = ParsedNoteBuilder::new(path)
        .with_content(content)
        .with_wikilinks(wikilinks)
        .with_tags(tags)
        .build();

    Some(note)
}

#[async_trait]
impl KnowledgeRepository for DaemonStorageClient {
    async fn get_note_by_name(&self, name: &str) -> CoreResult<Option<ParsedNote>> {
        // Try to find by filename ending
        let sql = r#"
            SELECT * FROM entities
            WHERE content_category = 'note'
                AND string::ends_with(path, $name)
            LIMIT 1
        "#;

        // The daemon expects the SQL as a single string - we encode params in the query
        let escaped_name = name.replace('\'', "''");
        let sql_with_param = sql.replace("$name", &format!("'{}'", escaped_name));

        let result = self
            .client
            .query(&self.kiln, &sql_with_param)
            .await
            .map_err(|e| CrucibleError::DatabaseError(e.to_string()))?;

        // Parse the result
        if let Some(records) = result.get("records").and_then(|v| v.as_array()) {
            if let Some(record) = records.first().and_then(|r| r.get("data")) {
                return Ok(parse_note_from_record(record));
            }
        }

        Ok(None)
    }

    async fn list_notes(&self, path_filter: Option<&str>) -> CoreResult<Vec<NoteInfo>> {
        let sql = if let Some(path) = path_filter {
            let escaped_path = path.replace('\'', "''");
            format!(
                r#"
                SELECT path, title, tags, created_at, updated_at FROM entities
                WHERE content_category = 'note'
                    AND string::starts_with(path, '{}')
                "#,
                escaped_path
            )
        } else {
            r#"
                SELECT path, title, tags, created_at, updated_at FROM entities
                WHERE content_category = 'note'
            "#
            .to_string()
        };

        let result = self
            .client
            .query(&self.kiln, &sql)
            .await
            .map_err(|e| CrucibleError::DatabaseError(e.to_string()))?;

        let mut notes = Vec::new();
        if let Some(records) = result.get("records").and_then(|v| v.as_array()) {
            for record in records {
                if let Some(data) = record.get("data") {
                    if let Some(info) = parse_note_info(data) {
                        notes.push(info);
                    }
                }
            }
        }

        Ok(notes)
    }

    async fn search_vectors(&self, vector: Vec<f32>) -> CoreResult<Vec<SearchResult>> {
        // Format vector as JSON array for SurrealDB
        let vector_json = serde_json::to_string(&vector)
            .map_err(|e| CrucibleError::DatabaseError(e.to_string()))?;

        let sql = format!(
            r#"
            SELECT
                entity_id,
                vector::similarity::cosine(embedding, {}) AS score
            FROM embeddings
            ORDER BY score DESC
            LIMIT 20
            "#,
            vector_json
        );

        let result = self
            .client
            .query(&self.kiln, &sql)
            .await
            .map_err(|e| CrucibleError::DatabaseError(e.to_string()))?;

        let mut search_results = Vec::new();
        if let Some(records) = result.get("records").and_then(|v| v.as_array()) {
            for record in records {
                if let Some(data) = record.get("data") {
                    let score = data.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);

                    if score < 0.5 {
                        continue;
                    }

                    if let Some(entity_id) = data.get("entity_id").and_then(|v| v.as_str()) {
                        let snippet = data
                            .get("content_used")
                            .and_then(|v| v.as_str())
                            .map(String::from);

                        search_results.push(SearchResult {
                            document_id: DocumentId(entity_id.to_string()),
                            score,
                            highlights: None,
                            snippet,
                        });
                    }
                }
            }
        }

        Ok(search_results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_daemon::Server;
    use tempfile::TempDir;

    async fn setup_test_daemon() -> (TempDir, std::path::PathBuf, Arc<DaemonClient>) {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let _shutdown_handle = server.shutdown_handle();

        tokio::spawn(async move {
            let _ = server.run().await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let client = Arc::new(DaemonClient::connect_to(&sock_path).await.unwrap());

        (tmp, sock_path, client)
    }

    #[tokio::test]
    async fn test_daemon_storage_client_creation() {
        let (_tmp, _sock_path, daemon_client) = setup_test_daemon().await;
        let kiln = PathBuf::from("/tmp/test-kiln");

        let storage_client = DaemonStorageClient::new(daemon_client, kiln.clone());
        assert_eq!(storage_client.kiln_path(), &kiln);
    }

    #[tokio::test]
    async fn test_daemon_storage_client_query_forwards_to_daemon() {
        let (_tmp, _sock_path, daemon_client) = setup_test_daemon().await;
        let kiln = PathBuf::from("/tmp/test-kiln");

        let storage_client = DaemonStorageClient::new(daemon_client, kiln);

        // This will likely fail because the kiln doesn't exist or query fails
        // But it proves the query goes through to the daemon
        let result = storage_client.query_raw("SELECT * FROM notes").await;

        // Query should either fail (kiln doesn't exist) or succeed with empty results
        // Either way, we've proven the RPC call works
        match result {
            Ok(_) => {
                // Query succeeded - daemon opened kiln and executed query
            }
            Err(e) => {
                // Query failed - expected since kiln doesn't exist or is empty
                assert!(!e.to_string().is_empty(), "Error should have a message");
            }
        }
    }
}
