//! Storage client implementation for daemon-based queries

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crucible_core::database::DocumentId;
use crucible_core::events::SessionEvent;
use crucible_core::parser::{BlockHash, ParsedNote};
use crucible_core::storage::{
    NoteRecord, NoteStore, SearchResult as StorageSearchResult, StorageError, StorageResult,
};
use crucible_core::traits::{KnowledgeRepository, NoteInfo, StorageClient};
use crucible_core::types::SearchResult as KnowledgeSearchResult;
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
    async fn query_raw(&self, _sql: &str) -> Result<Value> {
        anyhow::bail!(
            "Raw SQL queries are not supported through the daemon. \
             Use typed methods: search_vectors, list_notes, get_note_by_name"
        )
    }
}

// =============================================================================
// KnowledgeRepository implementation
// =============================================================================

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
        // Use the backend-agnostic get_note_by_name RPC method
        let result = self
            .client
            .get_note_by_name(&self.kiln, name)
            .await
            .map_err(|e| CrucibleError::DatabaseError(e.to_string()))?;

        match result {
            Some(data) => Ok(parse_note_from_record(&data)),
            None => Ok(None),
        }
    }

    async fn list_notes(&self, path_filter: Option<&str>) -> CoreResult<Vec<NoteInfo>> {
        // Use the backend-agnostic list_notes RPC method
        let results = self
            .client
            .list_notes(&self.kiln, path_filter)
            .await
            .map_err(|e| CrucibleError::DatabaseError(e.to_string()))?;

        Ok(results
            .into_iter()
            .map(|(name, path, title, tags, updated_at)| NoteInfo {
                name,
                path,
                title,
                tags,
                created_at: None,
                updated_at: updated_at.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc))
                }),
            })
            .collect())
    }

    async fn search_vectors(&self, vector: Vec<f32>) -> CoreResult<Vec<KnowledgeSearchResult>> {
        // Use the backend-agnostic search_vectors RPC method
        let results = self
            .client
            .search_vectors(&self.kiln, &vector, 20)
            .await
            .map_err(|e| CrucibleError::DatabaseError(e.to_string()))?;

        Ok(results
            .into_iter()
            .filter(|(_, score)| *score >= 0.5)
            .map(|(doc_id, score)| KnowledgeSearchResult {
                document_id: DocumentId(doc_id),
                score,
                highlights: None,
                snippet: None,
            })
            .collect())
    }
}

// =============================================================================
// DaemonNoteStore - NoteStore trait implementation via RPC
// =============================================================================

/// NoteStore implementation that delegates to daemon via RPC
///
/// This allows the CLI to use the NoteStore trait uniformly across
/// embedded and daemon modes.
pub struct DaemonNoteStore {
    client: Arc<DaemonStorageClient>,
}

impl DaemonNoteStore {
    /// Create a new DaemonNoteStore wrapping a DaemonStorageClient
    pub fn new(client: Arc<DaemonStorageClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl NoteStore for DaemonNoteStore {
    async fn upsert(&self, note: NoteRecord) -> StorageResult<Vec<SessionEvent>> {
        let note_path = PathBuf::from(&note.path);
        let note_title = Some(note.title.clone());

        self.client
            .client
            .note_upsert(self.client.kiln_path(), &note)
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        // Return a single event indicating the note was created/updated
        Ok(vec![SessionEvent::NoteCreated {
            path: note_path,
            title: note_title,
        }])
    }

    async fn get(&self, path: &str) -> StorageResult<Option<NoteRecord>> {
        self.client
            .client
            .note_get(self.client.kiln_path(), path)
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))
    }

    async fn delete(&self, path: &str) -> StorageResult<SessionEvent> {
        // Check if note exists before deleting
        let existed = self.get(path).await?.is_some();

        self.client
            .client
            .note_delete(self.client.kiln_path(), path)
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        Ok(SessionEvent::NoteDeleted {
            path: PathBuf::from(path),
            existed,
        })
    }

    async fn list(&self) -> StorageResult<Vec<NoteRecord>> {
        self.client
            .client
            .note_list(self.client.kiln_path())
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))
    }

    async fn get_by_hash(&self, hash: &BlockHash) -> StorageResult<Option<NoteRecord>> {
        // Not yet implemented via RPC - would need new endpoint
        // For now, do a linear scan (inefficient but correct)
        let notes = self.list().await?;
        Ok(notes.into_iter().find(|n| &n.content_hash == hash))
    }

    async fn search(
        &self,
        query_embedding: &[f32],
        limit: usize,
        _filter: Option<crucible_core::storage::Filter>,
    ) -> StorageResult<Vec<StorageSearchResult>> {
        // Use existing search_vectors RPC
        let results = self
            .client
            .client
            .search_vectors(self.client.kiln_path(), query_embedding, limit)
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        // Convert to StorageSearchResult - we need to fetch the full NoteRecord for each hit
        let mut hits = Vec::with_capacity(results.len());
        for (doc_id, score) in results {
            if let Ok(Some(note)) = self.get(&doc_id).await {
                hits.push(StorageSearchResult {
                    note,
                    score: score as f32,
                });
            }
        }

        Ok(hits)
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

        let server = Server::bind(&sock_path, None).await.unwrap();
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
    async fn test_daemon_storage_client_query_raw_returns_error() {
        let (_tmp, _sock_path, daemon_client) = setup_test_daemon().await;
        let kiln = PathBuf::from("/tmp/test-kiln");

        let storage_client = DaemonStorageClient::new(daemon_client, kiln);

        // Raw queries are not supported through the daemon
        let result = storage_client.query_raw("SELECT * FROM notes").await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not supported through the daemon"));
    }
}
