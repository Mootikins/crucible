use super::*;
use crucible_core::config::EmbeddingProviderConfig;
use tempfile::TempDir;

/// Helper to get a path that doesn't exist and works cross-platform
fn nonexistent_path() -> PathBuf {
    let tmp = TempDir::new().unwrap();
    let base = tmp.path().to_path_buf();
    drop(tmp); // Remove the temp dir
    base.join("nonexistent").join("path")
}

#[test]
fn test_excluded_dirs_constant() {
    // Verify the constant contains exactly the 5 expected directories
    assert_eq!(EXCLUDED_DIRS.len(), 5);
    assert!(EXCLUDED_DIRS.contains(&".crucible"));
    assert!(EXCLUDED_DIRS.contains(&".git"));
    assert!(EXCLUDED_DIRS.contains(&".obsidian"));
    assert!(EXCLUDED_DIRS.contains(&"node_modules"));
    assert!(EXCLUDED_DIRS.contains(&".trash"));
}

#[test]
fn pipeline_config_enables_enrichment_when_provider_configured() {
    let config = pipeline_config(Some(&EmbeddingProviderConfig::mock(Some(384))));
    assert!(!config.skip_enrichment);
}

#[test]
fn pipeline_config_skips_enrichment_when_provider_missing() {
    let config = pipeline_config(None);
    assert!(config.skip_enrichment);
}

#[tokio::test]
async fn enrichment_config_wiring_no_config_skips_enrichment() {
    let km = KilnManager::new();
    assert!(km.enrichment_config().is_none());
}

#[tokio::test]
async fn enrichment_config_wiring_with_config_enables_enrichment() {
    let (tx, _rx) = broadcast::channel(1);
    let km = KilnManager::with_event_tx(
        tx,
        Some(EmbeddingProviderConfig::mock(Some(384))),
        crucible_core::config::default_max_precognition_chars(),
    );
    assert!(km.enrichment_config().is_some());
}

#[tokio::test]
async fn test_kiln_manager_new() {
    let km = KilnManager::new();
    let list = km.list().await;
    assert!(list.is_empty());
}

#[tokio::test]
async fn test_open_creates_kiln_if_needed() {
    let km = KilnManager::new();
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().join("test_kiln");

    // Open should succeed (creates new kiln)
    let result = km.open(&kiln_path).await;
    assert!(result.is_ok());

    // Should now be in the list
    let list = km.list().await;
    assert_eq!(list.len(), 1);
}

#[tokio::test]
async fn test_open_reads_kiln_name_from_kiln_toml() {
    let km = KilnManager::new();
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().join("named_kiln");
    std::fs::create_dir_all(kiln_path.join(".crucible")).unwrap();
    std::fs::write(
        kiln_path.join(".crucible").join("kiln.toml"),
        "[kiln]\nname = \"crucible-docs\"\n",
    )
    .unwrap();

    km.open(&kiln_path).await.unwrap();

    let list = km.list().await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].1.as_deref(), Some("crucible-docs"));
}

#[tokio::test]
async fn test_open_without_workspace_toml_has_null_name() {
    let km = KilnManager::new();
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().join("unnamed_kiln");

    km.open(&kiln_path).await.unwrap();

    let list = km.list().await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].1, None);
}

#[tokio::test]
async fn test_close_unopened_kiln_succeeds() {
    let km = KilnManager::new();
    let path = nonexistent_path();
    // Closing a kiln that was never opened should succeed (no-op)
    let result = km.close(&path).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_list_empty_initially() {
    let km = KilnManager::new();
    let list = km.list().await;
    assert_eq!(list.len(), 0);
}

#[tokio::test]
async fn test_close_removes_from_list() {
    let km = KilnManager::new();
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().join("test_kiln");

    // Open the kiln
    km.open(&kiln_path).await.unwrap();

    // Verify it's in the list
    let list = km.list().await;
    assert_eq!(list.len(), 1);

    // Close it
    km.close(&kiln_path).await.unwrap();

    // Verify it's no longer in the list
    let list = km.list().await;
    assert_eq!(list.len(), 0);
}

#[tokio::test]
async fn test_default_trait() {
    let km = KilnManager::default();
    let list = km.list().await;
    assert!(list.is_empty());
}

#[tokio::test]
async fn test_get_or_open_creates_kiln() {
    let km = KilnManager::new();
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().join("test_kiln");

    let result = km.get_or_open(&kiln_path).await;
    assert!(result.is_ok());

    // Should now be in the list
    let list = km.list().await;
    assert_eq!(list.len(), 1);
}

#[tokio::test]
async fn test_get_or_open_reuses_existing() {
    let km = KilnManager::new();
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().join("test_kiln");

    // First call creates the kiln
    let _handle1 = km.get_or_open(&kiln_path).await.unwrap();

    // Second call should reuse the same connection
    let _handle2 = km.get_or_open(&kiln_path).await.unwrap();

    // Should only have one entry in the list
    let list = km.list().await;
    assert_eq!(list.len(), 1);
}

#[tokio::test]
async fn test_get_returns_none_if_not_open() {
    let km = KilnManager::new();
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().join("test_kiln");

    // get() should return None if kiln is not open
    let result = km.get(&kiln_path).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_returns_handle_if_open() {
    let km = KilnManager::new();
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().join("test_kiln");

    // Open the kiln first
    km.open(&kiln_path).await.unwrap();

    // get() should now return Some(handle)
    let result = km.get(&kiln_path).await;
    assert!(result.is_some());
}

#[tokio::test]
async fn test_find_kiln_for_path_returns_matching_kiln() {
    let km = KilnManager::new();
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().join("my_kiln");

    km.open(&kiln_path).await.unwrap();

    let file_in_kiln = kiln_path.join("notes").join("test.md");
    let result = km.find_kiln_for_path(&file_in_kiln).await;
    assert!(result.is_some());
    assert_eq!(
        result.unwrap(),
        kiln_path.canonicalize().unwrap_or(kiln_path)
    );
}

#[tokio::test]
async fn test_find_kiln_for_path_returns_none_for_unrelated_path() {
    let km = KilnManager::new();
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().join("my_kiln");

    km.open(&kiln_path).await.unwrap();

    let unrelated = PathBuf::from("/some/other/path/note.md");
    let result = km.find_kiln_for_path(&unrelated).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_find_kiln_for_path_returns_none_when_no_kilns_open() {
    let km = KilnManager::new();
    let path = PathBuf::from("/any/path/note.md");
    let result = km.find_kiln_for_path(&path).await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_find_kiln_for_path_with_multiple_kilns() {
    let km = KilnManager::new();
    let tmp = TempDir::new().unwrap();
    let kiln_a = tmp.path().join("kiln_a");
    let kiln_b = tmp.path().join("kiln_b");

    km.open(&kiln_a).await.unwrap();
    km.open(&kiln_b).await.unwrap();

    let file_in_b = kiln_b.join("sub").join("test.md");
    let result = km.find_kiln_for_path(&file_in_b).await;
    assert!(result.is_some());
    assert_eq!(result.unwrap(), kiln_b.canonicalize().unwrap_or(kiln_b));
}

#[tokio::test]
async fn test_get_updates_last_access() {
    let km = KilnManager::new();
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().join("test_kiln");

    // Open and get initial access time
    km.open(&kiln_path).await.unwrap();
    let initial_list = km.list().await;
    let initial_time = initial_list[0].2;

    // Wait a bit
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Call get()
    let _ = km.get(&kiln_path).await;

    // Last access should be updated
    let updated_list = km.list().await;
    let updated_time = updated_list[0].2;

    assert!(updated_time > initial_time);
}

#[tokio::test]
async fn test_file_deleted_removes_note_after_processing() {
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::NoteRecord;

    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().join("test_kiln");
    std::fs::create_dir_all(&kiln_path).unwrap();

    // Create 3 markdown files on disk
    std::fs::write(
        kiln_path.join("alpha.md"),
        "---\ntitle: Alpha\n---\n\nAlpha content.\n",
    )
    .unwrap();
    std::fs::write(
        kiln_path.join("beta.md"),
        "---\ntitle: Beta\n---\n\nBeta content.\n",
    )
    .unwrap();
    std::fs::write(
        kiln_path.join("gamma.md"),
        "---\ntitle: Gamma\n---\n\nGamma content.\n",
    )
    .unwrap();

    let km = KilnManager::new();

    // Open the kiln and populate the DB with relative-path records.
    // (The pipeline currently stores absolute paths, which is a known
    // mismatch with handle_file_deleted's relative-path convention.
    // We use upsert() with relative paths to test the deletion logic
    // end-to-end.)
    let handle = km.get_or_open(&kiln_path).await.unwrap();
    let note_store = handle.as_note_store();

    note_store
        .upsert(NoteRecord::new("alpha.md", BlockHash::zero()).with_title("Alpha"))
        .await
        .unwrap();
    note_store
        .upsert(NoteRecord::new("beta.md", BlockHash::zero()).with_title("Beta"))
        .await
        .unwrap();
    note_store
        .upsert(NoteRecord::new("gamma.md", BlockHash::zero()).with_title("Gamma"))
        .await
        .unwrap();

    // Verify all 3 notes exist in the store
    let auth = crucible_core::storage::Scope::workspace_unchecked(&kiln_path);
    let notes = note_store.list(&auth).await.unwrap();
    assert_eq!(notes.len(), 3, "DB should contain 3 notes");

    // Delete beta.md from disk
    let beta_abs = kiln_path.join("beta.md");
    std::fs::remove_file(&beta_abs).unwrap();

    // Handle the deletion through KilnManager
    let existed = km.handle_file_deleted(&kiln_path, &beta_abs).await.unwrap();
    assert!(
        existed,
        "handle_file_deleted should report the note existed"
    );

    // Verify DB now has exactly 2 notes
    let notes = note_store.list(&auth).await.unwrap();
    assert_eq!(notes.len(), 2, "DB should contain 2 notes after deletion");

    // Verify the deleted note is gone
    assert!(
        note_store.get("beta.md", &auth).await.unwrap().is_none(),
        "deleted note should not be in the store",
    );

    // Verify the remaining 2 notes are intact
    let alpha = note_store.get("alpha.md", &auth).await.unwrap();
    assert!(alpha.is_some(), "alpha.md should still exist");
    assert_eq!(alpha.unwrap().title, "Alpha");

    let gamma = note_store.get("gamma.md", &auth).await.unwrap();
    assert!(gamma.is_some(), "gamma.md should still exist");
    assert_eq!(gamma.unwrap().title, "Gamma");
}

#[tokio::test]
async fn open_named_kilns_opens_matching_kilns() {
    use crucible_core::config::KilnEntry;

    let tmp1 = TempDir::new().unwrap();
    let tmp2 = TempDir::new().unwrap();

    // Create minimal .crucible dirs so open() succeeds
    std::fs::create_dir_all(tmp1.path().join(".crucible")).unwrap();
    std::fs::create_dir_all(tmp2.path().join(".crucible")).unwrap();

    let mut kilns = HashMap::new();
    kilns.insert(
        "vault".to_string(),
        KilnEntry::Path(tmp1.path().to_path_buf()),
    );
    kilns.insert(
        "docs".to_string(),
        KilnEntry::Path(tmp2.path().to_path_buf()),
    );

    let project_kilns = vec!["vault".to_string(), "docs".to_string()];

    let manager = KilnManager::new();
    let opened = manager.open_named_kilns(&kilns, &project_kilns).await;

    assert_eq!(opened.len(), 2);
    let listed = manager.list().await;
    assert_eq!(listed.len(), 2);
}

#[tokio::test]
async fn open_named_kilns_skips_lazy_kilns() {
    use crucible_core::config::KilnEntry;

    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join(".crucible")).unwrap();

    let mut kilns = HashMap::new();
    kilns.insert(
        "active".to_string(),
        KilnEntry::Path(tmp.path().to_path_buf()),
    );
    kilns.insert(
        "lazy_one".to_string(),
        KilnEntry::Config {
            path: PathBuf::from("/should/not/be/opened"),
            lazy: true,
        },
    );

    let names = vec!["active".to_string(), "lazy_one".to_string()];
    let manager = KilnManager::new();
    let opened = manager.open_named_kilns(&kilns, &names).await;

    assert_eq!(opened, vec!["active"]);
    assert_eq!(manager.list().await.len(), 1);
}

#[tokio::test]
async fn open_named_kilns_warns_on_missing_name() {
    let kilns = HashMap::new();
    let names = vec!["nonexistent".to_string()];

    let manager = KilnManager::new();
    let opened = manager.open_named_kilns(&kilns, &names).await;

    assert!(opened.is_empty());
    assert!(manager.list().await.is_empty());
}

// =========================================================================
// Memory Scoping — Lance post-filter
// =========================================================================
//
// These tests verify the LanceDB → SQLite post-filter pipeline. Lance
// is the similarity oracle; SQLite is the scope oracle. A hit must
// pass BOTH to reach the caller.

/// Default LanceDB dim (matches `LanceVectorIndex::open`).
const LANCE_DIM: usize = 768;

fn unit_embedding() -> Vec<f32> {
    let mut v = vec![0.0_f32; LANCE_DIM];
    v[0] = 1.0;
    v
}

/// Seed a note into a kiln by upserting a NoteRecord with both a
/// known embedding (so Lance picks it up) and an explicit scope.
async fn seed_note_with_scope(
    km: &KilnManager,
    kiln: &Path,
    path: &str,
    scope: crucible_core::storage::Scope,
) {
    let handle = km.get_or_open(kiln).await.unwrap();
    let emb = unit_embedding();
    let record =
        crucible_core::storage::NoteRecord::new(path, crucible_core::parser::BlockHash::zero())
            .with_title(path)
            .with_embedding(emb.clone())
            .with_scope(scope);

    // Upsert into SQLite (scope-stamped) and Lance (vector key).
    let store = handle.as_note_store();
    store.upsert(record.clone()).await.unwrap();
    // Lance keys by note path; if upsert fails the test will surface
    // it as an empty hit list (the assertion below).
    handle
        .vectors
        .upsert(path, emb)
        .await
        .expect("Lance upsert");
}

#[tokio::test]
async fn storage_handle_uses_configured_embedding_dimension() {
    use crucible_core::config::FastEmbedConfig;

    // Configured 384-dim model (the default fastembed) must open a 384-dim
    // index — the old code hardcoded 768, so every non-768 upsert silently
    // failed the length check and semantic search returned nothing.
    let tmp = TempDir::new().unwrap();
    let db = tmp.path().join(".crucible").join("crucible-sqlite.db");
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let cfg = EmbeddingProviderConfig::FastEmbed(FastEmbedConfig {
        dimensions: 384,
        ..Default::default()
    });
    let handle = create_storage_handle(&db, tmp.path(), Some(&cfg))
        .await
        .unwrap();
    assert_eq!(handle.vectors.dimension(), 384);
    handle
        .vectors
        .upsert("notes/x.md", vec![0.1_f32; 384])
        .await
        .expect("384-dim upsert lands");

    // No enrichment config → default dimension (fresh dir, index dim is fixed at open).
    let tmp2 = TempDir::new().unwrap();
    let db2 = tmp2.path().join(".crucible").join("crucible-sqlite.db");
    std::fs::create_dir_all(db2.parent().unwrap()).unwrap();
    let handle2 = create_storage_handle(&db2, tmp2.path(), None)
        .await
        .unwrap();
    assert_eq!(handle2.vectors.dimension(), DEFAULT_EMBEDDING_DIM);
}

#[tokio::test]
async fn lance_search_vectors_post_filters_by_scope() {
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().to_path_buf();
    std::fs::create_dir_all(kiln_path.join(".crucible")).unwrap();
    let km = KilnManager::new();
    km.open(&kiln_path).await.unwrap();

    seed_note_with_scope(
        &km,
        &kiln_path,
        "own.md",
        crucible_core::storage::Scope::workspace_unchecked(&kiln_path),
    )
    .await;
    seed_note_with_scope(
        &km,
        &kiln_path,
        "stranger.md",
        crucible_core::storage::Scope::Workspace {
            path: std::path::PathBuf::from("/some/other/kiln"),
        },
    )
    .await;
    let handle = km.get(&kiln_path).await.unwrap();
    let query = unit_embedding();

    // Authority = this kiln's workspace. own.md must appear,
    // stranger.md must NOT.
    let auth = crucible_core::storage::Scope::workspace_unchecked(&kiln_path);
    let hits = handle.search_vectors(query, 10, &auth).await.unwrap();
    let ids: Vec<_> = hits.iter().map(|(id, _)| id.as_str()).collect();
    assert!(ids.contains(&"own.md"), "got: {:?}", ids);
    assert!(
        !ids.contains(&"stranger.md"),
        "Lance post-filter leaked cross-scope: {:?}",
        ids
    );
}

#[tokio::test]
async fn lance_results_excluded_when_scope_mismatch() {
    // A pure-cross-scope kiln (every note belongs to a stranger workspace)
    // returns an empty hit list under workspace authority.
    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().to_path_buf();
    std::fs::create_dir_all(kiln_path.join(".crucible")).unwrap();
    let km = KilnManager::new();
    km.open(&kiln_path).await.unwrap();

    seed_note_with_scope(
        &km,
        &kiln_path,
        "alien_a.md",
        crucible_core::storage::Scope::Workspace {
            path: std::path::PathBuf::from("/strangers/A"),
        },
    )
    .await;
    seed_note_with_scope(
        &km,
        &kiln_path,
        "alien_b.md",
        crucible_core::storage::Scope::Workspace {
            path: std::path::PathBuf::from("/strangers/B"),
        },
    )
    .await;

    let handle = km.get(&kiln_path).await.unwrap();
    let auth = crucible_core::storage::Scope::workspace_unchecked(&kiln_path);
    let hits = handle
        .search_vectors(unit_embedding(), 10, &auth)
        .await
        .unwrap();
    assert!(
        hits.is_empty(),
        "every note is cross-scope, must return no hits — got {:?}",
        hits
    );
}

#[test]
fn normalize_note_path_strips_absolute_prefix() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path();
    std::fs::create_dir_all(kiln.join("notes")).unwrap();
    std::fs::write(kiln.join("notes/hello.md"), "").unwrap();

    let file = kiln.join("notes/hello.md");
    let result = normalize_note_path(&file, kiln);
    assert_eq!(result, Some("notes/hello.md".to_string()));
}

#[test]
fn normalize_note_path_returns_none_outside_kiln() {
    let kiln = Path::new("/home/user/docs");
    let file = Path::new("/other/path/hello.md");
    let result = normalize_note_path(file, kiln);
    assert_eq!(result, None);
}

#[test]
fn normalize_note_path_handles_same_directory() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path();
    std::fs::write(kiln.join("hello.md"), "").unwrap();

    let file = kiln.join("hello.md");
    let result = normalize_note_path(&file, kiln);
    assert_eq!(result, Some("hello.md".to_string()));
}

// ===========================================================================
// Index reconciliation
//
// Index deletion is otherwise only observed live (watcher / fs.trash /
// note.rename), so anything deleted while the daemon was down never leaves
// the index — discovery is purely additive and never catches up.
// ===========================================================================

/// Relative paths currently in a kiln's note index.
async fn indexed_paths(km: &KilnManager, kiln_path: &Path) -> Vec<String> {
    let handle = km.get(kiln_path).await.expect("kiln should be open");
    let scope = crucible_core::storage::Scope::workspace(kiln_path)
        .unwrap_or_else(|_| crucible_core::storage::Scope::workspace_unchecked(kiln_path));
    let mut paths: Vec<String> = handle
        .as_note_store()
        .list(&scope)
        .await
        .expect("list notes")
        .into_iter()
        .map(|n| n.path)
        .collect();
    paths.sort();
    paths
}

#[tokio::test]
async fn reprocess_drops_notes_deleted_while_the_daemon_was_down() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path();
    std::fs::write(kiln.join("kept.md"), "# Kept\n").unwrap();
    std::fs::create_dir_all(kiln.join("sub")).unwrap();
    std::fs::write(kiln.join("sub/removed.md"), "# Removed\n").unwrap();

    let km = KilnManager::new();
    km.open_and_process(kiln, false).await.unwrap();
    assert_eq!(
        indexed_paths(&km, kiln).await,
        vec!["kept.md".to_string(), "sub/removed.md".to_string()],
        "both notes should index on the first pass"
    );

    // The bug: an external delete (git rm, branch checkout, daemon down) is
    // never routed through handle_file_deleted.
    std::fs::remove_file(kiln.join("sub/removed.md")).unwrap();

    km.open_and_process(kiln, false).await.unwrap();

    assert_eq!(
        indexed_paths(&km, kiln).await,
        vec!["kept.md".to_string()],
        "the deleted note must not survive as a ghost in the index"
    );
}

#[tokio::test]
async fn reprocess_keeps_every_note_that_still_exists() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path();
    std::fs::create_dir_all(kiln.join("nested/deep")).unwrap();
    std::fs::write(kiln.join("a.md"), "# A\n").unwrap();
    std::fs::write(kiln.join("nested/b.md"), "# B\n").unwrap();
    std::fs::write(kiln.join("nested/deep/c.md"), "# C\n").unwrap();

    let km = KilnManager::new();
    km.open_and_process(kiln, false).await.unwrap();
    let before = indexed_paths(&km, kiln).await;
    assert_eq!(before.len(), 3);

    // Reconciling against an unchanged tree must be a no-op, at every depth.
    km.open_and_process(kiln, false).await.unwrap();

    assert_eq!(
        indexed_paths(&km, kiln).await,
        before,
        "reconciliation must not delete notes that are still on disk"
    );
}

#[tokio::test]
async fn reprocess_does_not_empty_the_index_when_the_kiln_root_is_gone() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().join("kiln");
    std::fs::create_dir_all(&kiln).unwrap();
    std::fs::write(kiln.join("note.md"), "# Note\n").unwrap();

    let km = KilnManager::new();
    km.open_and_process(&kiln, false).await.unwrap();
    assert_eq!(indexed_paths(&km, &kiln).await, vec!["note.md".to_string()]);

    // An unmounted share or a renamed directory reads as "zero files on disk".
    // Treating that as "the user deleted everything" would wipe the index on a
    // transient fault, so a missing root must never drive deletions.
    std::fs::remove_dir_all(&kiln).unwrap();

    let _ = km.open_and_process(&kiln, false).await;

    assert_eq!(
        indexed_paths(&km, &kiln).await,
        vec!["note.md".to_string()],
        "a vanished kiln root must not be read as an instruction to delete"
    );
}

// ===========================================================================
// Canvas indexing
// ===========================================================================

/// The headline of canvas graph citizenship: a note referenced by a canvas
/// shows that canvas in its backlinks. Obsidian does not do this, so it is a
/// deliberate behaviour that needs a test which would notice it regressing.
#[tokio::test]
async fn a_note_referenced_by_a_canvas_gains_the_canvas_as_a_backlink() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path();
    std::fs::create_dir_all(kiln.join("Help")).unwrap();
    std::fs::write(kiln.join("Help/Target.md"), "# Target\n").unwrap();
    std::fs::write(kiln.join("Other.md"), "# Other\n").unwrap();
    std::fs::write(
        kiln.join("Board.canvas"),
        serde_json::json!({
            "nodes": [
                { "id": "f", "type": "file", "x": 0, "y": 0, "width": 1, "height": 1,
                  "file": "Help/Target.md" },
                { "id": "t", "type": "text", "x": 0, "y": 0, "width": 1, "height": 1,
                  "text": "context: [[Other]]" }
            ],
            "edges": []
        })
        .to_string(),
    )
    .unwrap();

    let km = KilnManager::new();
    km.open_and_process(kiln, false).await.unwrap();

    let handle = km.get(kiln).await.unwrap();
    let store = handle.as_note_store();

    assert_eq!(
        store.backlinks("Help/Target.md").await.unwrap(),
        vec!["Board.canvas".to_string()],
        "a canvas file node must register as a backlink on its target"
    );
    assert_eq!(
        store.backlinks("Other.md").await.unwrap(),
        vec!["Board.canvas".to_string()],
        "a wikilink inside a canvas text card must register too"
    );
}

#[tokio::test]
async fn a_canvas_is_indexed_alongside_notes() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path();
    std::fs::write(kiln.join("note.md"), "# N\n").unwrap();
    std::fs::write(
        kiln.join("board.canvas"),
        serde_json::json!({ "nodes": [] }).to_string(),
    )
    .unwrap();
    // An asset is present in the kiln but must never enter the index.
    std::fs::write(kiln.join("diagram.png"), [0x89, 0x50, 0x4e, 0x47]).unwrap();

    let km = KilnManager::new();
    let (discovered, ..) = km.open_and_process(kiln, false).await.unwrap();
    assert_eq!(discovered, 2, "discovery covers notes and canvases only");

    assert_eq!(
        indexed_paths(&km, kiln).await,
        vec!["board.canvas".to_string(), "note.md".to_string()]
    );
}

/// Reconciliation must treat canvases exactly like notes — a canvas deleted
/// while the daemon was down is as much a ghost as a note.
#[tokio::test]
async fn a_deleted_canvas_is_reconciled_out_of_the_index() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path();
    std::fs::write(kiln.join("keep.md"), "# K\n").unwrap();
    std::fs::write(
        kiln.join("gone.canvas"),
        serde_json::json!({ "nodes": [] }).to_string(),
    )
    .unwrap();

    let km = KilnManager::new();
    km.open_and_process(kiln, false).await.unwrap();
    assert_eq!(indexed_paths(&km, kiln).await.len(), 2);

    std::fs::remove_file(kiln.join("gone.canvas")).unwrap();
    km.open_and_process(kiln, false).await.unwrap();

    assert_eq!(indexed_paths(&km, kiln).await, vec!["keep.md".to_string()]);
}

/// A kiln processed before the text index existed has notes in SQLite and an
/// empty `notes_fts`. Without the backfill, `cru search` would return nothing
/// on every such kiln until each note happened to change — a search that is
/// silently blank on all existing data, which is the exact failure shape this
/// index was added to fix.
#[tokio::test]
async fn opening_a_kiln_backfills_a_text_index_that_was_never_written() {
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::note_store::NoteRecord;
    use crucible_core::storage::Scope;

    let kiln = TempDir::new().unwrap();
    let root = kiln.path().to_path_buf();
    std::fs::create_dir_all(root.join(".crucible")).unwrap();
    std::fs::write(
        root.join("legacy.md"),
        "# Legacy\n\nzqxjvbn only appears in the body.\n",
    )
    .unwrap();

    let handle = create_storage_handle(
        &root.join(".crucible").join("crucible-sqlite.db"),
        &root,
        None,
    )
    .await
    .unwrap();

    // Stand in for a note indexed by an older build: metadata present, no
    // text-index row.
    handle
        .as_note_store()
        .upsert(NoteRecord::new("legacy.md", BlockHash::zero()).with_title("Legacy"))
        .await
        .unwrap();
    assert!(
        handle.text.is_empty().await.unwrap(),
        "precondition: the text index starts empty"
    );

    backfill_text_index(&root, handle.as_note_store().as_ref(), &handle.text).await;

    let hits = handle.text.search("\"zqxjvbn\"", 10).await.unwrap();
    assert_eq!(
        hits.len(),
        1,
        "the backfill must index the body of an already-known note, got {hits:?}"
    );
    assert_eq!(hits[0].path, "legacy.md");

    // And the scope authority used by the backfill must actually see the note.
    let listed = handle
        .as_note_store()
        .list(&Scope::workspace_unchecked(&root))
        .await
        .unwrap();
    assert_eq!(listed.len(), 1, "sanity: the note is visible to the walk");
}
