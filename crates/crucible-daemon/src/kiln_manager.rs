//! Multi-kiln connection manager
//!
//! Manages connections to multiple kilns on-demand with idle timeout.
//! Supports SQLite backend via feature flags.

use anyhow::Result;
use crucible_core::config::read_kiln_config;
use crucible_core::events::InternalSessionEvent;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};

use crate::pipeline::{NotePipeline, NotePipelineConfig};
use crate::watch::{EventFilter, WatchManager, WatchManagerConfig};
use crucible_core::processing::InMemoryChangeDetectionStore;
use crucible_core::storage::note_store::NoteRecord;
use crucible_core::traits::{KnowledgeRepository, NoteInfo};
use crucible_core::EXCLUDED_DIRS;

use crate::embedding::get_or_create_embedding_provider;
use crate::file_watch_bridge::create_event_bridge;
use crate::protocol::SessionEventMessage;

use crucible_core::config::EmbeddingProviderConfig;

/// Canonicalize a path, falling back to the path as-given if it cannot be
/// resolved (e.g. the file was deleted, or lives on a filesystem that does not
/// support canonicalization).
fn canonical_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Normalize a file path to be relative to the kiln root.
///
/// Strips the kiln prefix (canonical or as-given) and normalizes
/// separators to forward slashes. Returns `None` if the path is not
/// inside the kiln.
pub fn normalize_note_path(file_path: &Path, kiln_path: &Path) -> Option<String> {
    let canonical = canonical_or_self(kiln_path);
    let relative = file_path
        .strip_prefix(&canonical)
        .or_else(|_| file_path.strip_prefix(kiln_path))
        .ok()?;
    Some(relative.to_string_lossy().replace('\\', "/"))
}

// Backend-specific imports
use crate::storage::sqlite::{adapters as sqlite_adapters, SqliteClientHandle, SqliteConfig};

// ===========================================================================
// Constants
// ===========================================================================

// ===========================================================================
// Backend Abstraction
// ===========================================================================

/// Per-kiln storage. SQLite owns everything: note metadata, properties, the
/// `KnowledgeRepository` surface, the FTS5 text index, and the
/// `notes.embedding` column that backs semantic search. (The former LanceDB
/// vector index was deleted after measurement showed a tuned SQLite scan
/// returning identical results 3.2x faster, with usable recall — the IVF_PQ
/// index had 0.132 recall@10k at kiln sizes.)
#[derive(Clone)]
pub struct StorageHandle {
    pub sqlite: SqliteClientHandle,
    /// FTS5 full-text index over note titles and bodies. Backs
    /// `search_text` — the thing `cru search` needs to see inside a note.
    pub text: Arc<crate::storage::sqlite::FtsIndex>,
}

impl StorageHandle {
    /// Stable label for diagnostic logs.
    pub fn backend_name(&self) -> &'static str {
        "sqlite"
    }

    /// Note metadata store (SQLite).
    pub fn as_note_store(&self) -> Arc<dyn crucible_core::storage::NoteStore> {
        self.sqlite.as_note_store()
    }

    /// Property/EAV store (SQLite).
    pub fn as_property_store(&self) -> Arc<dyn crucible_core::storage::PropertyStore> {
        self.sqlite.as_property_store()
    }

    /// Scope-aware vector similarity search (exact cosine over
    /// `notes.embedding`).
    ///
    /// Returns (document_id, score) pairs sorted by similarity descending,
    /// tie-broken by path ascending. The scope filter is applied at the SQL
    /// layer (`Filter::Scope`), so out-of-scope rows never occupy result
    /// slots — the old Lance over-fetch + post-filter could return fewer
    /// than `limit` hits when strangers dominated the similarity ranking.
    pub async fn search_vectors(
        &self,
        vector: Vec<f32>,
        limit: usize,
        authority: &crucible_core::storage::Scope,
    ) -> Result<Vec<(String, f64)>> {
        let results = self
            .sqlite
            .as_note_store()
            .search(
                &vector,
                limit,
                Some(crucible_core::storage::Filter::Scope(authority.clone())),
            )
            .await?;
        Ok(results
            .into_iter()
            .map(|r| (r.note.path, r.score as f64))
            .collect())
    }

    /// List notes by metadata filter. Always reads from SQLite.
    ///
    /// `authority` is the request authority — see [`crucible_core::storage::Scope`].
    /// Records whose stored scope is outside the caller's authority are
    /// filtered out at the SQL layer.
    pub async fn list_notes(
        &self,
        path_filter: Option<&str>,
        authority: &crucible_core::storage::Scope,
    ) -> Result<Vec<NoteInfo>> {
        let records = self.sqlite.as_note_store().list(authority).await?;
        Ok(records
            .into_iter()
            .filter(|r| path_filter.is_none_or(|p| r.path.contains(p)))
            .map(|r| NoteInfo {
                name: std::path::Path::new(&r.path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&r.path)
                    .to_string(),
                path: r.path,
                title: Some(r.title),
                tags: r.tags,
                created_at: None,
                updated_at: Some(r.updated_at),
            })
            .collect())
    }

    /// Case-insensitive fuzzy lookup by path or title.
    ///
    /// `authority` is the request authority — see [`crucible_core::storage::Scope`].
    pub async fn get_note_by_name(
        &self,
        name: &str,
        authority: &crucible_core::storage::Scope,
    ) -> Result<Option<NoteRecord>> {
        let records = self.sqlite.as_note_store().list(authority).await?;
        let name_lower = name.to_lowercase();
        Ok(records.into_iter().find(|r| {
            r.path.to_lowercase().contains(&name_lower)
                || r.title.to_lowercase().contains(&name_lower)
        }))
    }

    /// Resolve a note by name and collect the notes that wikilink to it.
    ///
    /// Returns `None` if `name` resolves to no note. The second element is
    /// the backlink sources as [`NoteInfo`], sorted by path for stable
    /// output; the third maps source path → the first link occurrence's
    /// byte span in that source (from the resolved-link index), so callers
    /// can jump straight to the referencing block. Sources whose only rows
    /// are span-less legacy entries are absent from the map.
    ///
    /// `authority` is the request authority — see [`crucible_core::storage::Scope`].
    pub async fn get_backlinks(
        &self,
        name: &str,
        authority: &crucible_core::storage::Scope,
    ) -> Result<
        Option<(
            NoteRecord,
            Vec<NoteInfo>,
            std::collections::HashMap<String, (i64, i64)>,
        )>,
    > {
        let records = self.sqlite.as_note_store().list(authority).await?;
        let name_lower = name.to_lowercase();
        let Some(target) = records
            .iter()
            .find(|r| {
                r.path.to_lowercase().contains(&name_lower)
                    || r.title.to_lowercase().contains(&name_lower)
            })
            .cloned()
        else {
            return Ok(None);
        };

        // Deterministic backlinks from the resolved-link index — the same
        // resolver the rename rewrite uses, so what backlinks show is exactly
        // what a rename would rewrite. (The old fuzzy candidate matching made
        // [[async]] structurally match EVERY note whose stem is `async`.)
        let sources = self.sqlite.as_note_store().backlinks(&target.path).await?;
        let source_set: std::collections::HashSet<&str> =
            sources.iter().map(String::as_str).collect();
        let mut backlinks: Vec<NoteInfo> = records
            .into_iter()
            .filter(|r| r.path != target.path)
            .filter(|r| source_set.contains(r.path.as_str()))
            .map(|r| NoteInfo {
                name: Path::new(&r.path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&r.path)
                    .to_string(),
                path: r.path,
                title: Some(r.title),
                tags: r.tags,
                created_at: None,
                updated_at: Some(r.updated_at),
            })
            .collect();
        backlinks.sort_by(|a, b| a.path.cmp(&b.path));

        // First in-file occurrence per source, from the same link index —
        // negative spans are span-less legacy rows, skipped.
        let mut spans = std::collections::HashMap::new();
        for link in self
            .sqlite
            .as_note_store()
            .inbound_links(&target.path)
            .await?
        {
            if link.span_start < 0 {
                continue;
            }
            spans
                .entry(link.source_path)
                .and_modify(|s: &mut (i64, i64)| {
                    if link.span_start < s.0 {
                        *s = (link.span_start, link.span_end);
                    }
                })
                .or_insert((link.span_start, link.span_end));
        }

        Ok(Some((target, backlinks, spans)))
    }

    /// Knowledge repository trait surface (SQLite-backed).
    pub fn as_knowledge_repository(&self) -> Arc<dyn KnowledgeRepository> {
        self.sqlite.as_knowledge_repository()
    }
}

/// Rebuild the resolved-link index for every note in a kiln by re-parsing
/// the files on disk. Runs once after the note_links v1→v2 migration (the
/// old rows carried raw text without spans and were unrecoverable in place).
/// Best-effort: unreadable/unparseable files are skipped with a warning.
/// Fill the FTS5 index from the notes already in SQLite, reading each body
/// from disk. Best-effort: a note that cannot be read is skipped rather than
/// failing the kiln open.
/// Whether the text index is missing any note the store knows about.
///
/// A count comparison, not an emptiness check — see the call site. Counting
/// rather than diffing keys because the two are equal in the healthy case and
/// a mismatch only needs to trigger a re-walk, which is idempotent.
async fn backfill_needed(
    handle: &StorageHandle,
    root: &Path,
) -> crucible_core::storage::StorageResult<bool> {
    use crucible_core::storage::Scope;

    let indexed = handle.text.count().await?;
    let known = handle
        .as_note_store()
        .list(&Scope::workspace_unchecked(root))
        .await?
        .len() as i64;

    Ok(indexed < known)
}

async fn backfill_text_index(
    root: &Path,
    store: &dyn crucible_core::storage::NoteStore,
    text: &crate::storage::sqlite::FtsIndex,
) {
    use crucible_core::storage::Scope;

    let authority = Scope::workspace_unchecked(root);
    let records = match store.list(&authority).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "text index backfill: listing notes failed");
            return;
        }
    };
    if records.is_empty() {
        return;
    }

    let mut indexed = 0usize;
    for rec in &records {
        let file = root.join(&rec.path);
        let Ok(body) = tokio::fs::read_to_string(&file).await else {
            continue;
        };
        if let Err(e) = text.index(&rec.path, &rec.title, &body).await {
            tracing::warn!(path = %rec.path, error = %e, "text index backfill: write failed");
            continue;
        }
        indexed += 1;
    }
    if indexed > 0 {
        // Merge the freshly written segments once, instead of leaving the
        // first queries after a backfill to pay for the fragmentation.
        if let Err(e) = text.optimize().await {
            tracing::warn!(error = %e, "FTS optimize after backfill failed; search still works, just slower");
        }
    }
    info!(notes = indexed, "Text index backfilled");
}

async fn relink_kiln(root: &Path, store: &dyn crucible_core::storage::NoteStore) {
    use crucible_core::parser::{traits::MarkdownParser, CrucibleParser};
    use crucible_core::storage::{LinkOccurrence, Scope};

    let authority = Scope::workspace_unchecked(root);
    let records = match store.list(&authority).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "link relink: listing notes failed; index stays empty until notes are re-processed");
            return;
        }
    };
    let parser = CrucibleParser::new();
    let mut relinked = 0usize;
    for rec in &records {
        let file = root.join(&rec.path);
        let parsed = match parser.parse_file(&file).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(path = %rec.path, error = %e, "link relink: parse failed, skipping");
                continue;
            }
        };
        let links: Vec<LinkOccurrence> = parsed
            .wikilinks
            .iter()
            .map(|w| LinkOccurrence {
                raw_target: w.target.clone(),
                span_start: parsed.body_offset + w.target_span.0,
                span_end: parsed.body_offset + w.target_span.1,
                is_embed: w.is_embed,
            })
            .collect();
        if let Err(e) = store.reindex_links(&rec.path, &links).await {
            tracing::warn!(path = %rec.path, error = %e, "link relink: write failed");
            continue;
        }
        relinked += 1;
    }
    info!(
        notes = relinked,
        "Resolved-link index rebuilt (note_links v2 migration)"
    );
}

// ===========================================================================
// KilnConnection and KilnManager
// ===========================================================================

/// Connection to a single kiln
pub struct KilnConnection {
    pub handle: StorageHandle,
    pub pipeline: NotePipeline,
    pub name: Option<String>,
    pub last_access: Instant,
    watch_manager: Option<WatchManager>,
}

/// Manages connections to multiple kilns
pub struct KilnManager {
    connections: RwLock<HashMap<PathBuf, KilnConnection>>,
    event_tx: Option<broadcast::Sender<SessionEventMessage>>,
    enrichment_config: Option<EmbeddingProviderConfig>,
    max_precognition_chars: usize,
}

impl KilnManager {
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            event_tx: None,
            enrichment_config: None,
            max_precognition_chars: crucible_core::config::default_max_precognition_chars(),
        }
    }

    pub fn with_event_tx(
        event_tx: broadcast::Sender<SessionEventMessage>,
        enrichment_config: Option<EmbeddingProviderConfig>,
        max_precognition_chars: usize,
    ) -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            event_tx: Some(event_tx),
            enrichment_config,
            max_precognition_chars,
        }
    }

    pub fn enrichment_config(&self) -> Option<&EmbeddingProviderConfig> {
        self.enrichment_config.as_ref()
    }

    pub fn max_precognition_chars(&self) -> usize {
        self.max_precognition_chars
    }

    /// Open a connection to a kiln (or return existing)
    pub async fn open(&self, kiln_path: &Path) -> Result<()> {
        let canonical = canonical_or_self(kiln_path);

        {
            let conns = self.connections.read().await;
            if conns.contains_key(&canonical) {
                return Ok(()); // Already open
            }
        }

        let db_path = canonical.join(".crucible").join("crucible-sqlite.db");

        info!("Opening kiln at {:?}", db_path);

        let handle = create_storage_handle(&db_path, &canonical).await?;
        info!(
            "Kiln opened with {} backend at {:?}",
            handle.backend_name(),
            db_path
        );

        // Phase-3 one-time migration: note_links v1 stored raw text and was
        // dropped on upgrade; rebuild the resolved-link index by re-parsing
        // links from disk (parse only — embeddings are untouched).
        {
            let store = handle.as_note_store();
            if store.needs_link_reindex() {
                relink_kiln(&canonical, store.as_ref()).await;
            }
        }

        // Backfill on open: a kiln processed before the text index existed
        // has notes in SQLite and nothing in `notes_fts`, so `cru search`
        // would find nothing in it until every note happened to change.
        // Shipping a search that is silently blank on existing kilns is the
        // failure this whole change is correcting, so pay the walk.
        //
        // Gated on a count comparison rather than on emptiness. Emptiness
        // makes the backfill strictly one-shot: a daemon killed part-way
        // through, or a single note that was unreadable that morning, leaves
        // a non-empty index that is never completed — the same silent gap in
        // a smaller size. `index()` deletes before inserting, so re-running
        // is idempotent.
        match backfill_needed(&handle, &canonical).await {
            Ok(true) => {
                backfill_text_index(&canonical, handle.as_note_store().as_ref(), &handle.text).await
            }
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(error = %e, "could not check the text index; skipping backfill")
            }
        }

        let mut pipeline = create_pipeline(&handle, self.enrichment_config.as_ref()).await?;
        pipeline.set_kiln_root(canonical.clone());
        info!("Pipeline created for kiln at {:?}", canonical);

        let name = read_kiln_name(&canonical);

        let watch_manager = self.start_watch_manager(&canonical).await;

        let mut conns = self.connections.write().await;
        conns.insert(
            canonical.clone(),
            KilnConnection {
                handle,
                pipeline,
                name,
                last_access: Instant::now(),
                watch_manager,
            },
        );
        // Drop the write lock before checking classification
        drop(conns);

        // Check if workspace has a data classification configured.
        // If not, emit ClassificationRequired so clients can prompt the user.
        let classification =
            crate::trust_resolution::resolve_kiln_classification(&canonical, &canonical);
        if classification.is_none() {
            if let Some(ref tx) = self.event_tx {
                let event = SessionEventMessage::new(
                    "system",
                    "classification_required",
                    serde_json::json!({ "kiln_path": canonical.to_string_lossy() }),
                );
                crate::event_emitter::emit_event(tx, event);
            }
        }

        Ok(())
    }

    /// Open a kiln and process all indexable files (notes and canvases)
    /// through the pipeline.
    ///
    /// Returns (discovered_count, processed_count, skipped_count, errors).
    /// If the kiln is already open, still runs processing.
    pub async fn open_and_process(
        &self,
        kiln_path: &Path,
        force: bool,
    ) -> Result<(usize, usize, usize, Vec<(PathBuf, String)>)> {
        // Ensure kiln is open
        self.open(kiln_path).await?;

        // A missing or unreadable root also discovers zero files. Reading that
        // as "the user deleted everything" would empty the index on a
        // transient fault (unmounted share, renamed directory), so the sweep
        // below only runs when the root is demonstrably there.
        let root_present = kiln_path.is_dir();

        // Discover files
        let files = discover_indexable_files(kiln_path);
        let discovered = files.len();

        if root_present {
            match self.reconcile_deleted(kiln_path, &files).await {
                Ok(0) => {}
                Ok(removed) => info!(
                    "Reconciliation dropped {} deleted note(s) from the index in {:?}",
                    removed, kiln_path
                ),
                // Reconciliation is cleanup, not the caller's request — a
                // failure here must not fail the processing run.
                Err(e) => warn!("Index reconciliation failed for {:?}: {}", kiln_path, e),
            }
        }

        if files.is_empty() {
            info!("No indexable files found in {:?}", kiln_path);
            return Ok((0, 0, 0, Vec::new()));
        }

        info!(
            "Discovered {} indexable files in {:?}",
            discovered, kiln_path
        );

        let (processed, skipped, errors) = self.process_batch(kiln_path, &files, force).await?;
        Ok((discovered, processed, skipped, errors))
    }

    /// Drop index entries whose files are no longer on disk, returning how
    /// many were removed.
    ///
    /// Index deletion is otherwise only ever observed live — the watcher, the
    /// `fs.trash` RPC, and `note.rename` all route through
    /// [`Self::handle_file_deleted`], and all three require the daemon to be
    /// running at the moment of deletion. A `git rm`, a branch checkout, or an
    /// external editor while the daemon is down is never seen, and discovery
    /// is purely additive, so nothing catches up afterwards: the entry becomes
    /// a permanent ghost that still lists, 404s on open, and contributes
    /// phantom backlinks.
    ///
    /// Deletions route back through `handle_file_deleted` rather than hitting
    /// the note store directly, so a reconciled delete is byte-for-byte the
    /// same operation as a live one — including dropping the FTS row, which
    /// would otherwise keep answering text searches with notes that no
    /// longer exist.
    ///
    /// Note that the candidate set comes from a scope-filtered `list`, whose
    /// SQL matches each note's recorded `properties.scope.path` against
    /// `kiln_path`. A kiln whose notes were indexed under a different spelling
    /// of its path (or that has since moved on disk) therefore reconciles
    /// nothing rather than everything. That is the safe direction to fail —
    /// under-deleting leaves ghosts, over-deleting destroys the index — but it
    /// does mean a moved kiln needs a reindex, not just a reprocess.
    async fn reconcile_deleted(&self, kiln_path: &Path, on_disk: &[PathBuf]) -> Result<usize> {
        let mut present: HashSet<String> = HashSet::with_capacity(on_disk.len());
        for file in on_disk {
            match normalize_note_path(file, kiln_path) {
                Some(rel) => {
                    present.insert(rel);
                }
                // Every discovered path is a descendant of `kiln_path` by
                // construction (the walk starts there), so a miss means the
                // prefix logic disagrees with the walk. The blast radius of
                // guessing is the whole index, so refuse to delete anything.
                None => {
                    warn!(
                        "Skipping reconciliation for {:?}: discovered file {:?} did not normalize",
                        kiln_path, file
                    );
                    return Ok(0);
                }
            }
        }

        let handle = self
            .get(kiln_path)
            .await
            .ok_or_else(|| anyhow::anyhow!("Kiln not open for reconciliation"))?;

        let scope = crucible_core::storage::Scope::workspace(kiln_path)
            .unwrap_or_else(|_| crucible_core::storage::Scope::workspace_unchecked(kiln_path));

        let ghosts: Vec<String> = handle
            .as_note_store()
            .list(&scope)
            .await?
            .into_iter()
            .map(|note| note.path)
            .filter(|path| !present.contains(path))
            .collect();

        let mut removed = 0;
        for ghost in ghosts {
            match self
                .handle_file_deleted(kiln_path, &kiln_path.join(&ghost))
                .await
            {
                Ok(true) => removed += 1,
                Ok(false) => {}
                Err(e) => warn!("Reconciliation could not drop {:?}: {}", ghost, e),
            }
        }

        Ok(removed)
    }

    /// Close a kiln connection
    pub async fn close(&self, kiln_path: &Path) -> Result<()> {
        let canonical = canonical_or_self(kiln_path);
        let mut conns = self.connections.write().await;
        if let Some(mut conn) = conns.remove(&canonical) {
            if let Some(ref mut wm) = conn.watch_manager {
                if let Err(e) = wm.shutdown().await {
                    warn!(
                        "Failed to shutdown watch manager for {:?}: {}",
                        canonical, e
                    );
                }
            }
            info!("Closed kiln at {:?}", canonical);
        }
        Ok(())
    }

    /// List all open kilns
    pub async fn list(&self) -> Vec<(PathBuf, Option<String>, Instant)> {
        let conns = self.connections.read().await;
        conns
            .iter()
            .map(|(path, conn)| (path.clone(), conn.name.clone(), conn.last_access))
            .collect()
    }

    /// Get handle for a kiln if it's already open (does not open if closed)
    #[allow(dead_code)] // peek-without-open API, exercised by tests
    pub async fn get(&self, kiln_path: &Path) -> Option<StorageHandle> {
        let canonical = canonical_or_self(kiln_path);

        let mut conns = self.connections.write().await;
        if let Some(conn) = conns.get_mut(&canonical) {
            conn.last_access = Instant::now();
            Some(conn.handle.clone())
        } else {
            None
        }
    }

    /// Process a file through the kiln's pipeline
    ///
    /// Opens the kiln if not already open, then processes the file.
    /// Returns Ok(true) if file was processed, Ok(false) if skipped (unchanged).
    pub async fn process_file(&self, kiln_path: &Path, file_path: &Path) -> Result<bool> {
        // Ensure kiln is open
        self.open(kiln_path).await?;

        let canonical = canonical_or_self(kiln_path);

        let mut conns = self.connections.write().await;
        let conn = conns
            .get_mut(&canonical)
            .ok_or_else(|| anyhow::anyhow!("Kiln not found after opening"))?;

        conn.last_access = Instant::now();

        // Process file through pipeline
        use crate::pipeline::ProcessingResult;
        match conn.pipeline.process(file_path).await {
            Ok(ProcessingResult::Success { .. }) => Ok(true),
            Ok(ProcessingResult::Skipped) => Ok(false),
            Ok(ProcessingResult::NoChanges) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Like [`Self::process_file`] but bypassing change detection for this
    /// one call. A renamed note can land on a path whose change-detection
    /// state is stale-but-matching (A→B→A round trip), which would skip the
    /// reindex the rename depends on. Toggling force is safe: the connections
    /// write-lock is held across the process call, so no other pipeline use
    /// can observe the flag.
    pub async fn process_file_forced(&self, kiln_path: &Path, file_path: &Path) -> Result<bool> {
        self.open(kiln_path).await?;
        let canonical = canonical_or_self(kiln_path);
        let mut conns = self.connections.write().await;
        let conn = conns
            .get_mut(&canonical)
            .ok_or_else(|| anyhow::anyhow!("Kiln not found after opening"))?;
        conn.last_access = Instant::now();

        use crate::pipeline::ProcessingResult;
        conn.pipeline.set_force_reprocess(true);
        let result = conn.pipeline.process(file_path).await;
        conn.pipeline.set_force_reprocess(false);
        match result {
            Ok(ProcessingResult::Success { .. }) => Ok(true),
            Ok(_) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub async fn handle_file_deleted(&self, kiln_path: &Path, file_path: &Path) -> Result<bool> {
        use crucible_core::events::SessionEvent;

        if !is_indexable_kiln_file(file_path) {
            return Ok(false);
        }

        self.open(kiln_path).await?;

        let canonical = canonical_or_self(kiln_path);

        let mut conns = self.connections.write().await;
        let conn = conns
            .get_mut(&canonical)
            .ok_or_else(|| anyhow::anyhow!("Kiln not found after opening"))?;

        conn.last_access = Instant::now();

        let relative_path = match normalize_note_path(file_path, kiln_path) {
            Some(p) => p,
            None => return Ok(false),
        };
        let event = conn.handle.as_note_store().delete(&relative_path).await?;

        // Drop the note from the text index too: a stale row there returns a
        // hit the user can click on and open nothing. (The embedding needs no
        // separate cleanup — it lives on the deleted `notes` row.)
        if let Err(e) = conn.handle.text.remove(&relative_path).await {
            tracing::warn!(path = %relative_path, ?e, "failed to remove deleted note from text index");
        }

        match event {
            SessionEvent::Internal(inner) => {
                if let InternalSessionEvent::NoteDeleted { existed, .. } = inner.as_ref() {
                    Ok(*existed)
                } else {
                    Ok(false)
                }
            }
            _ => Ok(false),
        }
    }

    /// Process multiple files through the kiln's pipeline
    ///
    /// Returns (processed_count, skipped_count, errors)
    ///
    /// Emits no per-file event, and never has. The batch progress events that
    /// were deleted lived on the RPC fast path (`server/kiln.rs`) instead, over
    /// an explicit path list; this is the loop where indexing is actually slow.
    /// A future full-kiln progress producer belongs here, addressed to
    /// `WILDCARD_SESSION` so both surfaces can receive it — the manager already
    /// holds an `event_tx`, so the missing piece is a consumer, not a sender.
    pub async fn process_batch(
        &self,
        kiln_path: &Path,
        file_paths: &[PathBuf],
        force: bool,
    ) -> Result<(usize, usize, Vec<(PathBuf, String)>)> {
        use crate::pipeline::ProcessingResult;

        // Ensure kiln is open
        self.open(kiln_path).await?;

        let canonical = canonical_or_self(kiln_path);

        let mut conns = self.connections.write().await;
        let conn = conns
            .get_mut(&canonical)
            .ok_or_else(|| anyhow::anyhow!("Kiln not found after opening"))?;

        conn.last_access = Instant::now();

        // Apply force flag to pipeline config for this batch
        conn.pipeline.set_force_reprocess(force);

        let mut processed = 0;
        let mut skipped = 0;
        let mut errors = Vec::new();

        for path in file_paths {
            match conn.pipeline.process(path).await {
                Ok(ProcessingResult::Success { warnings, .. }) => {
                    processed += 1;

                    if !warnings.is_empty() {
                        for warning in warnings {
                            warn!("Parse warning for {}: {}", path.display(), warning);
                        }
                    }
                }
                Ok(ProcessingResult::Skipped) | Ok(ProcessingResult::NoChanges) => {
                    skipped += 1;
                }
                Err(e) => {
                    errors.push((path.clone(), e.to_string()));
                }
            }
        }

        // A batch of delete+insert pairs leaves the FTS term index spread
        // across segments that every query then consults; merging them here
        // is subsecond (measured 0.25s at 12k notes) and took a churned
        // index's phrase queries from 24.7ms back to 16.1ms. On an
        // already-merged index it is near a no-op, so no threshold.
        if processed > 0 {
            if let Err(e) = conn.handle.text.optimize().await {
                warn!(error = %e, "FTS optimize after batch failed; search still works, just slower");
            }
        }

        Ok((processed, skipped, errors))
    }

    pub async fn get_or_open(&self, kiln_path: &Path) -> Result<StorageHandle> {
        let canonical = canonical_or_self(kiln_path);

        // Try to get existing and update last_access
        {
            let mut conns = self.connections.write().await;
            if let Some(conn) = conns.get_mut(&canonical) {
                conn.last_access = Instant::now();
                return Ok(conn.handle.clone());
            }
        }

        // Open new connection
        self.open(kiln_path).await?;

        let conns = self.connections.read().await;
        conns
            .get(&canonical)
            .map(|c| c.handle.clone())
            .ok_or_else(|| anyhow::anyhow!("Failed to get connection after opening"))
    }

    /// Find which open kiln contains the given file path.
    ///
    /// Both the incoming `file_path` and stored kiln keys are canonicalized
    /// (kiln keys are canonicalized at `open()` time). If `file_path` cannot
    /// be canonicalized (e.g., file was deleted between event and lookup),
    /// we fall back to the raw path which may still match if the kiln key
    /// also wasn't canonicalized (defensive).
    pub async fn find_kiln_for_path(&self, file_path: &Path) -> Option<PathBuf> {
        let canonical = canonical_or_self(file_path);
        let conns = self.connections.read().await;
        conns
            .keys()
            .filter(|kiln_path| canonical.starts_with(kiln_path))
            .max_by_key(|p| p.components().count())
            .cloned()
    }

    /// Open the kilns `names` resolve to, returning the ones actually opened.
    ///
    /// Resolution is the registry's, not this manager's: names, `~` expansion,
    /// relative anchoring and the `lazy` skip all live in one place, so there
    /// is no second answer to "where is kiln X" that can drift from the first.
    /// A [`KilnResolution::Lazy`] entry is skipped by construction — it is a
    /// distinct variant rather than a flag a caller has to remember — and an
    /// unresolvable name opens nothing at all.
    pub async fn open_registered(
        &self,
        registry: &crate::kiln_registry::KilnRegistry,
        names: &[crucible_core::config::KilnName],
    ) -> Vec<crucible_core::config::KilnName> {
        use crate::kiln_registry::KilnResolution;

        let mut opened = Vec::new();
        for name in names {
            match registry.resolve(name) {
                KilnResolution::Unknown => warn!(kiln = %name, "Kiln not found in the registry"),
                KilnResolution::Lazy(_) => tracing::debug!(kiln = %name, "Skipping lazy kiln"),
                KilnResolution::Ready(kiln) => match self.open(kiln.resolved_path()).await {
                    Ok(()) => {
                        info!(kiln = %name, path = %kiln.path().display(), "Opened project kiln");
                        opened.push(name.clone());
                    }
                    Err(e) => warn!(kiln = %name, error = %e, "Failed to open project kiln"),
                },
            }
        }
        opened
    }

    async fn start_watch_manager(&self, kiln_path: &Path) -> Option<WatchManager> {
        let event_tx = self.event_tx.as_ref()?;

        let bridge = create_event_bridge(event_tx.clone());
        let config = WatchManagerConfig {
            enable_default_handlers: true,
            queue_capacity: 1000,
            debounce_delay: std::time::Duration::from_millis(500),
            ..Default::default()
        };

        let mut wm = match WatchManager::with_emitter(config, bridge).await {
            Ok(wm) => wm,
            Err(e) => {
                warn!("Failed to create watch manager for {:?}: {}", kiln_path, e);
                return None;
            }
        };

        if let Err(e) = wm.start().await {
            warn!("Failed to start watch manager for {:?}: {}", kiln_path, e);
            return None;
        }

        let filter = EXCLUDED_DIRS.iter().fold(
            crucible_core::kiln::KilnFileKind::INDEXABLE_EXTENSIONS
                .iter()
                .fold(EventFilter::new(), |f, ext| f.with_extension(*ext)),
            |f, dir| f.exclude_dir(kiln_path.join(dir)),
        );

        let watch_config =
            crate::watch::traits::WatchConfig::new(format!("kiln-{}", kiln_path.display()))
                .with_filter(filter)
                .with_debounce(crate::watch::traits::DebounceConfig::new(500));

        if let Err(e) = wm.add_watch(kiln_path.to_path_buf(), watch_config).await {
            warn!("Failed to add watch for {:?}: {}", kiln_path, e);
            let _ = wm.shutdown().await;
            return None;
        }

        info!("File watcher started for kiln at {:?}", kiln_path);
        Some(wm)
    }
}

impl Default for KilnManager {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Backend Factory
// ===========================================================================

/// Create a NotePipeline for daemon-side file processing
///
/// Creates a pipeline with:
/// - In-memory change detection
/// - NoteStore from the storage handle
fn pipeline_config(enrichment_config: Option<&EmbeddingProviderConfig>) -> NotePipelineConfig {
    NotePipelineConfig {
        skip_enrichment: enrichment_config.is_none(),
        force_reprocess: false,
    }
}

async fn create_pipeline(
    handle: &StorageHandle,
    enrichment_config: Option<&EmbeddingProviderConfig>,
) -> Result<NotePipeline> {
    // Change detection (in-memory)
    let change_detector = Arc::new(InMemoryChangeDetectionStore::new());

    let embedding_provider = if let Some(config) = enrichment_config {
        match get_or_create_embedding_provider(config).await {
            Ok(provider) => {
                info!("Kiln enrichment active: embedding provider configured");
                Some(provider)
            }
            Err(e) => {
                tracing::warn!("Embedding provider unavailable, semantic search disabled: {e}");
                None
            }
        }
    } else {
        info!("Kiln enrichment skipped (no config)");
        None
    };
    let enricher = Arc::new(crate::enrichment::Enricher::from_optional_provider(
        embedding_provider,
    ));

    // Get NoteStore from handle
    let note_store = handle.as_note_store();

    let config = pipeline_config(enrichment_config);

    let pipeline = NotePipeline::with_config(change_detector, enricher, note_store, config)
        .with_text_index(handle.text.clone());

    Ok(pipeline)
}

/// Open a kiln's storage: SQLite for metadata, properties, text index and
/// embeddings, at `<kiln>/.crucible/crucible-sqlite.db`.
///
/// `kiln_path` is the kiln root (canonicalized by `open()`); the SQLite
/// handle is bound to it so `as_knowledge_repository()` enforces
/// `Scope::Workspace(kiln_path)` authority on reads.
async fn create_storage_handle(sqlite_db_path: &Path, kiln_path: &Path) -> Result<StorageHandle> {
    let sqlite_config = SqliteConfig::new(sqlite_db_path);
    let sqlite = sqlite_adapters::create_sqlite_client(sqlite_config)
        .await?
        .with_kiln_path(kiln_path.to_path_buf());

    // No setup call: `notes_fts` is created by the migration ladder, which ran
    // when `sqlite`'s pool opened the database.
    let text = Arc::new(crate::storage::sqlite::FtsIndex::new(sqlite.pool().clone()));

    Ok(StorageHandle { sqlite, text })
}

// ===========================================================================
// File Discovery
// ===========================================================================

/// Check if a path is a markdown file
/// Whether the kiln indexes this file. Notes and canvases both do — see
/// [`KilnFileKind`](crucible_core::kiln::KilnFileKind).
fn is_indexable_kiln_file(path: &Path) -> bool {
    crucible_core::kiln::is_indexable_file(path)
}

/// Check if a directory should be excluded from file discovery
fn is_excluded_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| EXCLUDED_DIRS.contains(&name))
        .unwrap_or(false)
}

/// Discover indexable files in a kiln directory — notes, canvases and plain text.
fn discover_indexable_files(kiln_path: &Path) -> Vec<PathBuf> {
    use walkdir::WalkDir;

    WalkDir::new(kiln_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_excluded_dir(e.path()))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file() && is_indexable_kiln_file(e.path()))
        .map(|e| e.path().to_path_buf())
        .collect()
}

/// Expand a leading `~/` to the user's home directory.
///
/// Delegates rather than re-implementing: three copies of this existed, they
/// disagreed on the bare `~` case, and one of them panicked on it. One
/// expander, one answer.
pub(crate) fn expand_tilde_path(path: &Path) -> PathBuf {
    // Only a `~` path needs the expander, and only a path that is valid UTF-8
    // can have one. Everything else is returned byte-for-byte: converting
    // lossily first would rewrite a non-UTF-8 path into a *different* path,
    // and the whole point of a resolver is that the path checked is the path
    // used.
    match path.to_str() {
        Some(s) if s.starts_with('~') => {
            crate::project_manager::resolve_registration_root(s, dirs::home_dir().as_deref())
        }
        _ => path.to_path_buf(),
    }
}

fn read_kiln_name(kiln_path: &Path) -> Option<String> {
    let config = read_kiln_config(kiln_path)?;
    let trimmed = config.kiln.name.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests;
