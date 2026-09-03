//! Storage client implementation for daemon-based queries

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use crucible_core::events::{InternalSessionEvent, SessionEvent};
use crucible_core::parser::{BlockHash, ParsedNote};
use crucible_core::storage::{
    GraphLink, InboundLink, LinkOccurrence, NoteRecord, NoteStore,
    SearchResult as StorageSearchResult, StorageError, StorageResult, StorageResultExt,
};
use crucible_core::traits::{KnowledgeRepository, NoteInfo, StorageClient};
use crucible_core::types::SearchResult as KnowledgeSearchResult;
use crucible_core::DocumentId;
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

/// DTO for wikilink data returned by the daemon RPC
#[derive(serde::Deserialize)]
struct WikilinkDto {
    target: String,
    #[serde(default)]
    alias: Option<String>,
    #[serde(default)]
    is_embed: Option<bool>,
    #[serde(default)]
    block_ref: Option<String>,
    #[serde(default)]
    heading_ref: Option<String>,
}

/// DTO matching the JSON shape returned by get_note_by_name RPC
#[derive(serde::Deserialize)]
struct NoteRecordDto {
    path: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    wikilinks: Option<Vec<WikilinkDto>>,
}

impl NoteRecordDto {
    /// Convert this DTO into a ParsedNote, mapping fields to the canonical types.
    fn into_parsed_note(self) -> ParsedNote {
        use crucible_core::parser::{NoteContent, ParsedNoteBuilder, Tag, Wikilink};

        let path = std::path::PathBuf::from(self.path);

        let tags: Vec<Tag> = self
            .tags
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, s)| Tag {
                path: s.split('/').map(String::from).collect(),
                name: s,
                offset: i, // Placeholder offset
            })
            .collect();

        let wikilinks: Vec<Wikilink> = self
            .wikilinks
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, w)| Wikilink {
                target: w.target,
                alias: w.alias,
                offset: i,           // Placeholder offset (no span data on the wire)
                target_span: (0, 0), // Placeholder span
                is_embed: w.is_embed.unwrap_or(false),
                block_ref: w.block_ref,
                heading_ref: w.heading_ref,
            })
            .collect();

        let mut content = NoteContent::new();
        content.plain_text = self.content.unwrap_or_default();

        ParsedNoteBuilder::new(path)
            .with_content(content)
            .with_wikilinks(wikilinks)
            .with_tags(tags)
            .build()
    }
}

/// Parse a record into a ParsedNote (minimal version for daemon)
pub(crate) fn parse_note_from_record(record: &Value) -> Option<ParsedNote> {
    serde_json::from_value::<NoteRecordDto>(record.clone())
        .ok()
        .map(NoteRecordDto::into_parsed_note)
}

#[async_trait]
impl KnowledgeRepository for DaemonStorageClient {
    async fn search_blocks(
        &self,
        _vector: Vec<f32>,
        _limit: usize,
    ) -> crucible_core::Result<Vec<crucible_core::types::SearchResult>> {
        // No block store behind this repository; callers fall back to notes.
        Ok(Vec::new())
    }

    async fn blocks_for_note(
        &self,
        _path: &str,
    ) -> crucible_core::Result<Vec<crucible_core::storage::BlockRecord>> {
        // No block store behind this repository.
        Ok(Vec::new())
    }

    /// No RPC carries index rows or resolved links to a client, and no
    /// client-side caller reads them. Empty is the honest answer.
    async fn list_note_records(
        &self,
    ) -> CoreResult<Vec<crucible_core::storage::note_store::NoteRecord>> {
        Ok(Vec::new())
    }

    async fn links_for_note(&self, _path: &str) -> CoreResult<crucible_core::traits::NoteLinks> {
        Ok(crucible_core::traits::NoteLinks::default())
    }

    async fn get_note_by_name(&self, name: &str) -> CoreResult<Option<ParsedNote>> {
        // Use the backend-agnostic get_note_by_name RPC method
        let result = self
            .client
            .get_note_by_name(&self.kiln, name, None)
            .await
            .map_err(|e| CrucibleError::DatabaseError(e.to_string()))?;

        match result {
            Some(data) => Ok(parse_note_from_record(&data)),
            None => Ok(None),
        }
    }

    /// No RPC carries an index row with its properties, and no client-side
    /// caller reads one: the tools that do live in the daemon. `None` is the
    /// honest answer, not a stub.
    async fn get_note_by_path(
        &self,
        _path: &str,
    ) -> CoreResult<Option<crucible_core::storage::note_store::NoteRecord>> {
        Ok(None)
    }

    async fn list_notes(&self, path_filter: Option<&str>) -> CoreResult<Vec<NoteInfo>> {
        // Use the backend-agnostic list_notes RPC method
        let results = self
            .client
            .list_notes(&self.kiln, path_filter, None)
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

    async fn search_vectors(
        &self,
        vector: Vec<f32>,
        limit: usize,
    ) -> CoreResult<Vec<KnowledgeSearchResult>> {
        // Use the backend-agnostic search_vectors RPC method
        let results = self
            .client
            .search_vectors(&self.kiln, &vector, limit, None)
            .await
            .map_err(|e| CrucibleError::DatabaseError(e.to_string()))?;

        Ok(results
            .into_iter()
            .filter(|hit| hit.score >= 0.5)
            .map(|hit| KnowledgeSearchResult {
                document_id: DocumentId(hit.document_id),
                score: hit.score,
                highlights: None,
                snippet: hit.snippet,
                kiln: None,
                block: hit.block,
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
            .storage_backend()?;

        // Return a single event indicating the note was created/updated
        Ok(vec![SessionEvent::internal(
            InternalSessionEvent::NoteCreated {
                path: note_path,
                title: note_title,
            },
        )])
    }

    async fn get(
        &self,
        path: &str,
        authority: &crucible_core::storage::Scope,
    ) -> StorageResult<Option<NoteRecord>> {
        self.client
            .client
            .note_get_scoped(self.client.kiln_path(), path, Some(authority.clone()))
            .await
            .storage_backend()
    }

    async fn content_hash(
        &self,
        path: &str,
    ) -> StorageResult<Option<crucible_core::parser::BlockHash>> {
        // Best effort over the scoped read: the RPC surface has no unscoped
        // note fetch, and it does not need one. The indexer runs daemon-side
        // against the SQLite store; this client-side impl exists for CLI
        // callers, none of which drive the pipeline.
        let authority = crucible_core::storage::Scope::workspace_unchecked(self.client.kiln_path());
        Ok(self.get(path, &authority).await?.map(|n| n.content_hash))
    }

    async fn delete(&self, path: &str) -> StorageResult<SessionEvent> {
        // Internal existence check bound to this client's kiln so the
        // bookkeeping `get` doesn't need an admin authority.
        let authority = crucible_core::storage::Scope::workspace_unchecked(self.client.kiln_path());
        let existed = self.get(path, &authority).await?.is_some();

        self.client
            .client
            .note_delete(self.client.kiln_path(), path)
            .await
            .storage_backend()?;

        Ok(SessionEvent::internal(InternalSessionEvent::NoteDeleted {
            path: PathBuf::from(path),
            existed,
        }))
    }

    async fn list(
        &self,
        authority: &crucible_core::storage::Scope,
    ) -> StorageResult<Vec<NoteRecord>> {
        self.client
            .client
            .note_list_scoped(self.client.kiln_path(), Some(authority.clone()))
            .await
            .storage_backend()
    }

    async fn backlinks(&self, target_path: &str) -> StorageResult<Vec<String>> {
        // `kiln.graph` is the one RPC that carries the resolved-link index.
        // A resolved edge names its target by note path, so the backlinks of
        // `target_path` are the sources of the resolved edges that end there.
        let mut sources: Vec<String> = self
            .graph_links()
            .await?
            .into_iter()
            .filter(|e| e.resolved && e.target == target_path)
            .map(|e| e.source)
            .collect();
        sources.sort_unstable();
        sources.dedup();
        Ok(sources)
    }

    async fn inbound_links(&self, _target_path: &str) -> StorageResult<Vec<InboundLink>> {
        Err(no_link_index_over_rpc("inbound_links"))
    }

    async fn graph_links(&self) -> StorageResult<Vec<GraphLink>> {
        let graph = self
            .client
            .client
            .kiln_graph(self.client.kiln_path(), None)
            .await
            .storage_backend()?;
        let links = graph.get("links").cloned().unwrap_or(Value::Array(vec![]));
        serde_json::from_value(links).map_err(|e| StorageError::Deserialization(e.to_string()))
    }

    fn needs_link_reindex(&self) -> bool {
        // The relink pass runs in the daemon, against the SQLite store.
        false
    }

    async fn reindex_links(&self, _path: &str, _links: &[LinkOccurrence]) -> StorageResult<()> {
        Err(no_link_index_over_rpc("reindex_links"))
    }

    /// Find a note by content hash.
    ///
    /// The RPC surface has no hash lookup. This method lists every note in
    /// scope, then scans the list for the hash. The cost is one `list_notes`
    /// call plus a linear scan; it is best effort for CLI callers. Do not
    /// call it in a loop. `content_hash` above has the same shape: it
    /// fetches the whole note to read one field.
    async fn get_by_hash(
        &self,
        hash: &BlockHash,
        authority: &crucible_core::storage::Scope,
    ) -> StorageResult<Option<NoteRecord>> {
        let notes = self.list(authority).await?;
        Ok(notes.into_iter().find(|n| &n.content_hash == hash))
    }

    async fn search(
        &self,
        query_embedding: &[f32],
        limit: usize,
        filter: Option<crucible_core::storage::Filter>,
    ) -> StorageResult<Vec<StorageSearchResult>> {
        // Extract scope from the filter if the caller passed one. This is
        // the canonical entry point — daemon-side handlers also accept
        // scope explicitly via `search_vectors`. We pull scope out
        // of the filter so plugins that built a `Filter::Scope(...)` get
        // the same enforcement as direct RPC callers.
        let scope = filter.as_ref().and_then(extract_scope_from_filter);

        let results = self
            .client
            .client
            .search_vectors(
                self.client.kiln_path(),
                query_embedding,
                limit,
                scope.clone(),
            )
            .await
            .storage_backend()?;

        // Hydrate results — use the same authority for hydration as the
        // search itself so we never reveal a note that the search filter
        // would have hidden.
        let hydration_authority = scope.unwrap_or_else(|| {
            crucible_core::storage::Scope::workspace_unchecked(self.client.kiln_path())
        });
        // The reply is one row per block. This store answers in notes, so a
        // note with several blocks near the query is one hit, at its first
        // row's rank.
        let results = crate::rpc_client::first_per_note(results);
        let mut hits = Vec::with_capacity(results.len());
        for hit in results {
            if let Ok(Some(note)) = self.get(&hit.document_id, &hydration_authority).await {
                hits.push(StorageSearchResult {
                    note,
                    score: hit.score as f32,
                });
            }
        }

        Ok(hits)
    }
}

/// The rename splice and the relink pass read and write link rows directly.
/// They run in the daemon, and the RPC surface does not expose the rows. A
/// CLI-side store that answered "no rows" would make a rename splice nothing.
fn no_link_index_over_rpc(method: &str) -> StorageError {
    StorageError::Backend(format!(
        "NoteStore::{method} is not available through the daemon; the link index is daemon-side"
    ))
}

/// Walk a `Filter` tree and pull out a scope authority if one was specified.
/// Used by [`DaemonNoteStore::search`] so callers can express scope via
/// `Filter::Scope(...)` and the RPC carries it as a top-level param.
fn extract_scope_from_filter(
    f: &crucible_core::storage::Filter,
) -> Option<crucible_core::storage::Scope> {
    use crucible_core::storage::Filter;
    match f {
        Filter::Scope(s) => Some(s.clone()),
        Filter::And(filters) | Filter::Or(filters) => {
            filters.iter().find_map(extract_scope_from_filter)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Server;
    use tempfile::TempDir;

    async fn setup_test_daemon() -> (TempDir, std::path::PathBuf, Arc<DaemonClient>) {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind_with_data_home(&sock_path, tmp.path().to_path_buf())
            .await
            .unwrap();
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
    /// The link queries go through `kiln.graph`, so a CLI-side store answers
    /// them from the daemon's resolved-link index, not with an empty list.
    #[tokio::test]
    async fn daemon_note_store_answers_link_queries_from_the_daemon_index() {
        let (_tmp, _sock_path, daemon_client) = setup_test_daemon().await;
        let kiln_dir = TempDir::new().unwrap();
        let kiln = kiln_dir.path().canonicalize().unwrap();
        std::fs::write(
            kiln.join("Alpha.md"),
            "# Alpha\n\nlinks [[Beta]] and [[Ghost]]\n",
        )
        .unwrap();
        std::fs::write(kiln.join("Beta.md"), "# Beta\n\npoints [[Gamma]]\n").unwrap();
        std::fs::write(kiln.join("Gamma.md"), "# Gamma\n").unwrap();
        daemon_client
            .kiln_open_with_options(&kiln, true, false)
            .await
            .unwrap();

        let store = DaemonNoteStore::new(Arc::new(DaemonStorageClient::new(
            daemon_client,
            kiln.clone(),
        )));

        let mut edges: Vec<(String, String, bool)> = store
            .graph_links()
            .await
            .unwrap()
            .into_iter()
            .map(|e| (e.source, e.target, e.resolved))
            .collect();
        edges.sort();
        assert_eq!(
            edges,
            vec![
                ("Alpha.md".into(), "Beta.md".into(), true),
                ("Alpha.md".into(), "ghost".into(), false),
                ("Beta.md".into(), "Gamma.md".into(), true),
            ]
        );

        assert_eq!(store.backlinks("Beta.md").await.unwrap(), vec!["Alpha.md"]);
        assert!(store.backlinks("Alpha.md").await.unwrap().is_empty());
        assert!(!store.needs_link_reindex());

        // The daemon owns the index. A CLI-side store says so instead of
        // answering "no rows", which a rename would read as "nothing to splice".
        assert!(store.inbound_links("Beta.md").await.is_err());
        assert!(store.reindex_links("Beta.md", &[]).await.is_err());
    }

    /// A daemon whose kilns embed with the mock provider, under an isolated
    /// data root, so the search reply has block rows to reduce.
    async fn setup_embedding_daemon() -> (TempDir, Arc<DaemonClient>) {
        use crate::server::BindWithPluginConfigParams;
        use crucible_core::config::{EmbeddingProviderConfig, MockConfig};

        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");
        let server = Server::bind_with_plugin_config(BindWithPluginConfigParams {
            path: sock_path.clone(),
            data_home: Some(tmp.path().to_path_buf()),
            config_home: Some(tmp.path().join("config")),
            enrichment_config: Some(EmbeddingProviderConfig::Mock(MockConfig {
                model: "mock-model".to_string(),
                dimensions: 384,
            })),
            ..Default::default()
        })
        .await
        .unwrap();
        tokio::spawn(async move {
            let _ = server.run().await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let client = Arc::new(DaemonClient::connect_to(&sock_path).await.unwrap());
        (tmp, client)
    }

    /// The daemon answers `search_vectors` with one row per block. This
    /// store answers in notes, so a note with several blocks near the query
    /// is one hit, not one per block.
    #[tokio::test]
    async fn daemon_note_store_search_answers_each_note_once() {
        let (_tmp, daemon_client) = setup_embedding_daemon().await;
        let kiln_dir = TempDir::new().unwrap();
        let kiln = kiln_dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(kiln.join(".crucible")).unwrap();
        std::fs::write(kiln.join(".crucible").join("kiln.toml"), "").unwrap();
        std::fs::write(
            kiln.join("Many.md"),
            "# Many\n\n\
             The first paragraph says the kiln keeps notes in plain text.\n\n\
             The second paragraph says a session attaches a flat set of kilns.\n\n\
             The third paragraph says the daemon owns every byte of storage.\n",
        )
        .unwrap();
        std::fs::write(
            kiln.join("One.md"),
            "# One\n\nA single paragraph about the parser and its byte spans.\n",
        )
        .unwrap();
        daemon_client
            .kiln_open_with_options(&kiln, true, false)
            .await
            .unwrap();
        let query = daemon_client
            .embed_query(&kiln, "where do notes live")
            .await
            .unwrap();

        let store = DaemonNoteStore::new(Arc::new(DaemonStorageClient::new(
            daemon_client,
            kiln.clone(),
        )));
        let hits = store.search(&query, 10, None).await.unwrap();

        let paths: Vec<&str> = hits.iter().map(|h| h.note.path.as_str()).collect();
        let mut distinct = paths.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert!(!paths.is_empty(), "the processed kiln answers the query");
        assert_eq!(
            paths.len(),
            distinct.len(),
            "a note appears once, at its best block's rank: {paths:?}"
        );
    }
}
