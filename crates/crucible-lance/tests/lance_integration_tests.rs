//! Integration tests for crucible-lance vector store operations
//!
//! These tests exercise the public API of LanceNoteStore through the NoteStore trait,
//! verifying end-to-end behavior including persistence, search, and lifecycle operations.

use std::collections::HashMap;

use crucible_core::parser::BlockHash;
use crucible_core::storage::{Filter, NoteRecord, NoteStore};
use crucible_lance::{create_note_store, create_note_store_with_dimensions, LanceNoteStore};
use tempfile::TempDir;

/// Small embedding dimension for fast tests
const TEST_DIM: usize = 8;

/// Create an isolated store with a fresh temp directory.
/// Returns `(TempDir, LanceNoteStore)` — hold the TempDir to keep the directory alive.
async fn setup() -> (TempDir, LanceNoteStore) {
    let dir = TempDir::new().expect("failed to create temp dir");
    let db_path = dir.path().join("integration.lance");
    let store = create_note_store_with_dimensions(db_path.to_str().unwrap(), TEST_DIM)
        .await
        .expect("failed to create store");
    (dir, store)
}

/// Generate a deterministic normalized embedding from a seed value.
fn embedding(seed: f32) -> Vec<f32> {
    let mut v: Vec<f32> = (0..TEST_DIM).map(|i| seed + (i as f32) * 0.1).collect();
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}

/// Helper to create a NoteRecord with embedding.
fn note(path: &str, title: &str, seed: f32) -> NoteRecord {
    NoteRecord::new(path.to_string(), BlockHash::zero())
        .with_title(title.to_string())
        .with_embedding(embedding(seed))
}

/// Helper to create a NoteRecord without embedding.
fn note_no_embed(path: &str, title: &str) -> NoteRecord {
    NoteRecord::new(path.to_string(), BlockHash::zero()).with_title(title.to_string())
}

// ============================================================================
// 1. Store initialization
// ============================================================================

#[tokio::test]
async fn init_store_with_default_dimensions() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("default.lance");
    let store = create_note_store(db_path.to_str().unwrap())
        .await
        .expect("create_note_store should succeed");

    // Default dimension is 768
    assert_eq!(store.embedding_dimensions(), 768);
}

#[tokio::test]
async fn init_store_with_custom_dimensions() {
    let (_dir, store) = setup().await;
    assert_eq!(store.embedding_dimensions(), TEST_DIM);
    assert!(!store.path().is_empty());
}

// ============================================================================
// 2. Note upsert — store and retrieve
// ============================================================================

#[tokio::test]
async fn upsert_and_retrieve_note() {
    let (_dir, store) = setup().await;

    let n = NoteRecord::new("docs/hello.md".to_string(), BlockHash::zero())
        .with_title("Hello".to_string())
        .with_tags(vec!["greeting".to_string()])
        .with_links(vec!["docs/world.md".to_string()])
        .with_embedding(embedding(1.0));

    let events = store.upsert(n).await.expect("upsert failed");
    assert!(!events.is_empty(), "upsert should emit events");

    let got = store
        .get("docs/hello.md")
        .await
        .expect("get failed")
        .expect("note should exist after upsert");

    assert_eq!(got.path, "docs/hello.md");
    assert_eq!(got.title, "Hello");
    assert_eq!(got.tags, vec!["greeting"]);
    assert_eq!(got.links_to, vec!["docs/world.md"]);
    assert!(got.embedding.is_some());
}

// ============================================================================
// 3. Vector search
// ============================================================================

#[tokio::test]
async fn search_returns_nearest_neighbors() {
    let (_dir, store) = setup().await;

    // Insert 3 notes with distinct embeddings
    store.upsert(note("a.md", "A", 1.0)).await.unwrap();
    store.upsert(note("b.md", "B", 5.0)).await.unwrap();
    store.upsert(note("c.md", "C", 1.1)).await.unwrap();

    // Query with embedding close to seed=1.0
    let results = store.search(&embedding(1.0), 10, None).await.unwrap();

    assert_eq!(results.len(), 3);
    // Exact match should be first
    assert_eq!(results[0].note.path, "a.md");
    // Scores descending
    for w in results.windows(2) {
        assert!(w[0].score >= w[1].score, "results should be sorted by score desc");
    }
}

// ============================================================================
// 4. Note deletion
// ============================================================================

#[tokio::test]
async fn delete_removes_note_from_store_and_search() {
    let (_dir, store) = setup().await;

    store.upsert(note("rm.md", "Remove Me", 2.0)).await.unwrap();
    assert!(store.get("rm.md").await.unwrap().is_some());

    store.delete("rm.md").await.expect("delete failed");
    assert!(store.get("rm.md").await.unwrap().is_none());

    // Also gone from search results
    let results = store.search(&embedding(2.0), 10, None).await.unwrap();
    assert!(
        results.iter().all(|r| r.note.path != "rm.md"),
        "deleted note should not appear in search"
    );
}

// ============================================================================
// 5. Search with k limit
// ============================================================================

#[tokio::test]
async fn search_respects_k_limit() {
    let (_dir, store) = setup().await;

    // Insert 10 notes
    for i in 0..10 {
        store
            .upsert(note(&format!("n{i}.md"), &format!("N{i}"), i as f32))
            .await
            .unwrap();
    }

    let k = 4;
    let results = store.search(&embedding(5.0), k, None).await.unwrap();
    assert_eq!(results.len(), k, "search must return at most k results");
}

// ============================================================================
// 6. Empty store
// ============================================================================

#[tokio::test]
async fn search_on_empty_store_returns_empty() {
    let (_dir, store) = setup().await;

    let results = store.search(&embedding(0.0), 10, None).await.unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn list_on_empty_store_returns_empty() {
    let (_dir, store) = setup().await;

    let all = store.list().await.unwrap();
    assert!(all.is_empty());
}

// ============================================================================
// 7. Multiple upserts (update / deduplication)
// ============================================================================

#[tokio::test]
async fn upsert_same_path_twice_updates_in_place() {
    let (_dir, store) = setup().await;

    // First insert
    let n1 = NoteRecord::new("dup.md".to_string(), BlockHash::zero())
        .with_title("Original".to_string())
        .with_tags(vec!["v1".to_string()])
        .with_embedding(embedding(1.0));
    store.upsert(n1).await.unwrap();

    // Second insert same path, different content
    let n2 = NoteRecord::new("dup.md".to_string(), BlockHash::zero())
        .with_title("Updated".to_string())
        .with_tags(vec!["v2".to_string()])
        .with_embedding(embedding(2.0));
    store.upsert(n2).await.unwrap();

    // Only one record should exist
    let all = store.list().await.unwrap();
    assert_eq!(all.len(), 1, "upsert should not create duplicates");

    let got = store.get("dup.md").await.unwrap().unwrap();
    assert_eq!(got.title, "Updated");
    assert_eq!(got.tags, vec!["v2"]);
}

// ============================================================================
// 8. Batch upsert (multiple notes)
// ============================================================================

#[tokio::test]
async fn batch_upsert_multiple_notes() {
    let (_dir, store) = setup().await;

    let notes: Vec<NoteRecord> = (0..5)
        .map(|i| note(&format!("batch/{i}.md"), &format!("Batch {i}"), i as f32))
        .collect();

    for n in notes {
        store.upsert(n).await.unwrap();
    }

    let all = store.list().await.unwrap();
    assert_eq!(all.len(), 5, "all batch-upserted notes should be listed");

    // Verify each is retrievable
    for i in 0..5 {
        let got = store
            .get(&format!("batch/{i}.md"))
            .await
            .unwrap()
            .expect("each batch note should be retrievable");
        assert_eq!(got.title, format!("Batch {i}"));
    }
}

// ============================================================================
// Bonus coverage: search with filter, get_by_hash, properties round-trip
// ============================================================================

#[tokio::test]
async fn search_with_tag_filter() {
    let (_dir, store) = setup().await;

    let rust_note = NoteRecord::new("rust.md".to_string(), BlockHash::zero())
        .with_title("Rust".to_string())
        .with_tags(vec!["rust".to_string()])
        .with_embedding(embedding(1.0));

    let python_note = NoteRecord::new("python.md".to_string(), BlockHash::zero())
        .with_title("Python".to_string())
        .with_tags(vec!["python".to_string()])
        .with_embedding(embedding(1.1));

    store.upsert(rust_note).await.unwrap();
    store.upsert(python_note).await.unwrap();

    let filter = Filter::Tag("rust".to_string());
    let results = store.search(&embedding(1.0), 10, Some(filter)).await.unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].note.path, "rust.md");
}

#[tokio::test]
async fn get_by_hash_round_trip() {
    let (_dir, store) = setup().await;

    let hash = BlockHash::new([42u8; 32]);
    let n = NoteRecord::new("hashed.md".to_string(), hash).with_title("Hashed".to_string());
    store.upsert(n).await.unwrap();

    let found = store
        .get_by_hash(&hash)
        .await
        .unwrap()
        .expect("note should be found by hash");
    assert_eq!(found.path, "hashed.md");

    // Non-existent hash returns None
    let missing = store.get_by_hash(&BlockHash::new([99u8; 32])).await.unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn properties_survive_round_trip() {
    let (_dir, store) = setup().await;

    let mut props = HashMap::new();
    props.insert(
        "status".to_string(),
        serde_json::Value::String("draft".to_string()),
    );
    props.insert("version".to_string(), serde_json::Value::Number(3.into()));

    let n = NoteRecord::new("props.md".to_string(), BlockHash::zero())
        .with_title("Props".to_string())
        .with_properties(props.clone());

    store.upsert(n).await.unwrap();

    let got = store.get("props.md").await.unwrap().unwrap();
    assert_eq!(got.properties.get("status"), props.get("status"));
    assert_eq!(got.properties.get("version"), props.get("version"));
}

#[tokio::test]
async fn delete_nonexistent_is_idempotent() {
    let (_dir, store) = setup().await;

    // Should not error
    let event = store.delete("ghost.md").await;
    assert!(event.is_ok(), "deleting a nonexistent note should succeed");
}

#[tokio::test]
async fn search_excludes_notes_without_embedding() {
    let (_dir, store) = setup().await;

    store.upsert(note("with.md", "With", 1.0)).await.unwrap();
    store
        .upsert(note_no_embed("without.md", "Without"))
        .await
        .unwrap();

    let results = store.search(&embedding(1.0), 10, None).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].note.path, "with.md");
}
