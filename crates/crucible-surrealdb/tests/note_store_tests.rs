//! NoteStore Integration Tests
//!
//! This module tests the SurrealNoteStore implementation against a real SurrealDB
//! instance. Tests cover all NoteStore trait methods: CRUD operations, hash lookups,
//! and semantic search with various filter types.
//!
//! Run with: `cargo test -p crucible-surrealdb --test note_store_tests`

use std::collections::HashMap;

use crucible_core::parser::BlockHash;
use crucible_core::storage::{Filter, NoteRecord, NoteStore, Op};
use crucible_surrealdb::test_utils::{SurrealClient, SurrealNoteStore};
use serde_json::json;

// ============================================================================
// Test Setup Helpers
// ============================================================================

/// Create an in-memory SurrealNoteStore with schema applied
async fn setup_store() -> SurrealNoteStore {
    let client = SurrealClient::new_memory()
        .await
        .expect("Failed to create client");
    let store = SurrealNoteStore::new(client);
    store.apply_schema().await.expect("Failed to apply schema");
    store
}

/// Create a basic NoteRecord for testing
fn make_note(path: &str, title: &str) -> NoteRecord {
    NoteRecord::new(path.to_string(), BlockHash::zero()).with_title(title.to_string())
}

/// Create a NoteRecord with a specific hash
fn make_note_with_hash(path: &str, title: &str, hash_seed: u8) -> NoteRecord {
    // Create a simple hash by filling with the seed byte
    let hash_bytes = [hash_seed; 32];
    let hash = BlockHash::new(hash_bytes);
    NoteRecord::new(path.to_string(), hash).with_title(title.to_string())
}

/// Create a simple normalized embedding vector for testing
fn make_embedding(dimensions: usize, seed: f32) -> Vec<f32> {
    let mut embedding: Vec<f32> = (0..dimensions).map(|i| seed + (i as f32) * 0.01).collect();
    // Normalize the vector for cosine similarity
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut embedding {
            *x /= norm;
        }
    }
    embedding
}

// ============================================================================
// Test 1: Upsert + Get Roundtrip
// ============================================================================

/// Test that a note can be stored and retrieved with all fields intact
#[tokio::test]
async fn upsert_get_roundtrip() {
    // Arrange
    let store = setup_store().await;
    let mut properties = HashMap::new();
    properties.insert("status".to_string(), json!("published"));
    properties.insert("priority".to_string(), json!(5));

    let note = NoteRecord::new("notes/test.md", BlockHash::zero())
        .with_title("Test Note")
        .with_tags(vec!["rust".to_string(), "testing".to_string()])
        .with_links(vec!["notes/other.md".to_string()])
        .with_properties(properties);

    // Act
    store.upsert(note.clone()).await.expect("Failed to upsert");
    let retrieved = store
        .get("notes/test.md")
        .await
        .expect("Failed to get")
        .expect("Note should exist");

    // Assert
    assert_eq!(retrieved.path, "notes/test.md");
    assert_eq!(retrieved.title, "Test Note");
    assert_eq!(retrieved.tags, vec!["rust", "testing"]);
    assert_eq!(retrieved.links_to, vec!["notes/other.md"]);
    assert_eq!(retrieved.content_hash, BlockHash::zero());
    assert_eq!(
        retrieved.properties.get("status"),
        Some(&json!("published"))
    );
    assert_eq!(retrieved.properties.get("priority"), Some(&json!(5)));
}

/// Test that upsert updates an existing note
#[tokio::test]
async fn upsert_updates_existing_note() {
    // Arrange
    let store = setup_store().await;
    let note_v1 = make_note("notes/update.md", "Version 1");

    // Act - First upsert
    store.upsert(note_v1).await.expect("Failed to upsert v1");

    // Update with new title and tags
    let note_v2 = NoteRecord::new("notes/update.md", BlockHash::zero())
        .with_title("Version 2")
        .with_tags(vec!["updated".to_string()]);

    store.upsert(note_v2).await.expect("Failed to upsert v2");

    // Assert
    let retrieved = store
        .get("notes/update.md")
        .await
        .expect("Failed to get")
        .expect("Note should exist");

    assert_eq!(retrieved.title, "Version 2");
    assert_eq!(retrieved.tags, vec!["updated"]);
}

// ============================================================================
// Test 2: Delete
// ============================================================================

/// Test that delete removes a note and it cannot be retrieved
#[tokio::test]
async fn delete_removes_note() {
    // Arrange
    let store = setup_store().await;
    let note = make_note("notes/to-delete.md", "To Delete");
    store.upsert(note).await.expect("Failed to upsert");

    // Verify note exists
    let exists = store
        .get("notes/to-delete.md")
        .await
        .expect("Failed to get")
        .is_some();
    assert!(exists, "Note should exist before delete");

    // Act
    store
        .delete("notes/to-delete.md")
        .await
        .expect("Failed to delete");

    // Assert
    let after_delete = store
        .get("notes/to-delete.md")
        .await
        .expect("Failed to get");
    assert!(after_delete.is_none(), "Note should not exist after delete");
}

/// Test that delete is idempotent (deleting non-existent note succeeds)
#[tokio::test]
async fn delete_is_idempotent() {
    // Arrange
    let store = setup_store().await;

    // Act & Assert - Should not error on non-existent path
    store
        .delete("notes/does-not-exist.md")
        .await
        .expect("Delete should succeed for non-existent note");
}

// ============================================================================
// Test 3: Get By Hash
// ============================================================================

/// Test that a note can be retrieved by its content hash
#[tokio::test]
async fn get_by_hash_finds_note() {
    // Arrange
    let store = setup_store().await;
    let note = make_note_with_hash("notes/hashed.md", "Hashed Note", 42);
    let target_hash = note.content_hash;
    store.upsert(note).await.expect("Failed to upsert");

    // Act
    let retrieved = store
        .get_by_hash(&target_hash)
        .await
        .expect("Failed to get by hash")
        .expect("Note should exist");

    // Assert
    assert_eq!(retrieved.path, "notes/hashed.md");
    assert_eq!(retrieved.title, "Hashed Note");
    assert_eq!(retrieved.content_hash, target_hash);
}

/// Test that get_by_hash returns None for non-existent hash
#[tokio::test]
async fn get_by_hash_returns_none_for_missing() {
    // Arrange
    let store = setup_store().await;
    let nonexistent_hash = BlockHash::new([99; 32]);

    // Act
    let result = store
        .get_by_hash(&nonexistent_hash)
        .await
        .expect("Failed to get by hash");

    // Assert
    assert!(result.is_none(), "Should return None for non-existent hash");
}

// ============================================================================
// Test 4: List
// ============================================================================

/// Test that list returns all stored notes
#[tokio::test]
async fn list_returns_all_notes() {
    // Arrange
    let store = setup_store().await;
    let notes = vec![
        make_note("notes/first.md", "First"),
        make_note("notes/second.md", "Second"),
        make_note("notes/third.md", "Third"),
    ];

    for note in notes {
        store.upsert(note).await.expect("Failed to upsert");
    }

    // Act
    let listed = store.list().await.expect("Failed to list");

    // Assert
    assert_eq!(listed.len(), 3, "Should have 3 notes");

    let paths: Vec<&str> = listed.iter().map(|n| n.path.as_str()).collect();
    assert!(paths.contains(&"notes/first.md"));
    assert!(paths.contains(&"notes/second.md"));
    assert!(paths.contains(&"notes/third.md"));
}

/// Test that list returns empty vec when no notes exist
#[tokio::test]
async fn list_returns_empty_when_no_notes() {
    // Arrange
    let store = setup_store().await;

    // Act
    let listed = store.list().await.expect("Failed to list");

    // Assert
    assert!(listed.is_empty(), "Should return empty vec");
}

// ============================================================================
// Test 5: Search With No Filter
// ============================================================================

/// Test semantic search without any filter
#[tokio::test]
async fn search_without_filter() {
    // Arrange
    let store = setup_store().await;

    // Create notes with embeddings - using 384 dimensions (default)
    let embedding1 = make_embedding(384, 1.0);
    let embedding2 = make_embedding(384, 2.0);
    let embedding3 = make_embedding(384, 3.0);

    let note1 = make_note("notes/first.md", "First Note").with_embedding(embedding1.clone());
    let note2 = make_note("notes/second.md", "Second Note").with_embedding(embedding2);
    let note3 = make_note("notes/third.md", "Third Note").with_embedding(embedding3);

    store.upsert(note1).await.expect("Failed to upsert");
    store.upsert(note2).await.expect("Failed to upsert");
    store.upsert(note3).await.expect("Failed to upsert");

    // Act - Search with query embedding similar to first note
    let results = store
        .search(&embedding1, 10, None)
        .await
        .expect("Failed to search");

    // Assert
    assert!(!results.is_empty(), "Should find at least one result");
    assert!(results.len() <= 3, "Should not exceed number of notes");

    // First result should be most similar (highest score)
    // Since we're using embedding1 as query, note1 should be closest
    assert_eq!(
        results[0].note.path, "notes/first.md",
        "First result should be the note with matching embedding"
    );

    // Scores should be in descending order
    for window in results.windows(2) {
        assert!(
            window[0].score >= window[1].score,
            "Results should be sorted by descending score"
        );
    }
}

/// Test that search respects the k limit
#[tokio::test]
async fn search_respects_limit() {
    // Arrange
    let store = setup_store().await;

    // Create 5 notes with embeddings
    for i in 0..5 {
        let embedding = make_embedding(384, i as f32);
        let note = make_note(&format!("notes/note{}.md", i), &format!("Note {}", i))
            .with_embedding(embedding);
        store.upsert(note).await.expect("Failed to upsert");
    }

    // Act - Search with limit of 2
    let query = make_embedding(384, 0.0);
    let results = store.search(&query, 2, None).await.expect("Failed to search");

    // Assert
    assert_eq!(results.len(), 2, "Should return exactly 2 results");
}

// ============================================================================
// Test 6: Search With Tag Filter
// ============================================================================

/// Test semantic search filtered by tag
#[tokio::test]
async fn search_with_tag_filter() {
    // Arrange
    let store = setup_store().await;

    let embedding = make_embedding(384, 1.0);

    let note1 = make_note("notes/rust.md", "Rust Guide")
        .with_tags(vec!["rust".to_string()])
        .with_embedding(embedding.clone());

    let note2 = make_note("notes/python.md", "Python Guide")
        .with_tags(vec!["python".to_string()])
        .with_embedding(embedding.clone());

    let note3 = make_note("notes/rust-advanced.md", "Advanced Rust")
        .with_tags(vec!["rust".to_string(), "advanced".to_string()])
        .with_embedding(embedding.clone());

    store.upsert(note1).await.expect("Failed to upsert");
    store.upsert(note2).await.expect("Failed to upsert");
    store.upsert(note3).await.expect("Failed to upsert");

    // Act - Search filtered by "rust" tag
    let filter = Filter::Tag("rust".to_string());
    let results = store
        .search(&embedding, 10, Some(filter))
        .await
        .expect("Failed to search");

    // Assert
    assert_eq!(results.len(), 2, "Should find 2 notes with 'rust' tag");

    let paths: Vec<&str> = results.iter().map(|r| r.note.path.as_str()).collect();
    assert!(paths.contains(&"notes/rust.md"));
    assert!(paths.contains(&"notes/rust-advanced.md"));
    assert!(!paths.contains(&"notes/python.md"));
}

// ============================================================================
// Test 7: Search With Property Filter
// ============================================================================

/// Test semantic search filtered by property
#[tokio::test]
async fn search_with_property_filter() {
    // Arrange
    let store = setup_store().await;

    let embedding = make_embedding(384, 1.0);

    let mut props1 = HashMap::new();
    props1.insert("status".to_string(), json!("published"));

    let mut props2 = HashMap::new();
    props2.insert("status".to_string(), json!("draft"));

    let mut props3 = HashMap::new();
    props3.insert("status".to_string(), json!("published"));

    let note1 = make_note("notes/published1.md", "Published 1")
        .with_properties(props1)
        .with_embedding(embedding.clone());

    let note2 = make_note("notes/draft.md", "Draft")
        .with_properties(props2)
        .with_embedding(embedding.clone());

    let note3 = make_note("notes/published2.md", "Published 2")
        .with_properties(props3)
        .with_embedding(embedding.clone());

    store.upsert(note1).await.expect("Failed to upsert");
    store.upsert(note2).await.expect("Failed to upsert");
    store.upsert(note3).await.expect("Failed to upsert");

    // Act - Search filtered by status = "published"
    let filter = Filter::Property("status".to_string(), Op::Eq, json!("published"));
    let results = store
        .search(&embedding, 10, Some(filter))
        .await
        .expect("Failed to search");

    // Assert
    assert_eq!(
        results.len(),
        2,
        "Should find 2 notes with status=published"
    );

    let paths: Vec<&str> = results.iter().map(|r| r.note.path.as_str()).collect();
    assert!(paths.contains(&"notes/published1.md"));
    assert!(paths.contains(&"notes/published2.md"));
    assert!(!paths.contains(&"notes/draft.md"));
}

// ============================================================================
// Test 8: Search With Compound Filter
// ============================================================================

/// Test semantic search with AND compound filter
#[tokio::test]
async fn search_with_and_filter() {
    // Arrange
    let store = setup_store().await;

    let embedding = make_embedding(384, 1.0);

    let mut props = HashMap::new();
    props.insert("status".to_string(), json!("published"));

    // Note with both rust tag and published status
    let note1 = make_note("notes/rust-published.md", "Rust Published")
        .with_tags(vec!["rust".to_string()])
        .with_properties(props.clone())
        .with_embedding(embedding.clone());

    // Note with rust tag but draft status
    let mut draft_props = HashMap::new();
    draft_props.insert("status".to_string(), json!("draft"));
    let note2 = make_note("notes/rust-draft.md", "Rust Draft")
        .with_tags(vec!["rust".to_string()])
        .with_properties(draft_props)
        .with_embedding(embedding.clone());

    // Note with python tag and published status
    let note3 = make_note("notes/python-published.md", "Python Published")
        .with_tags(vec!["python".to_string()])
        .with_properties(props)
        .with_embedding(embedding.clone());

    store.upsert(note1).await.expect("Failed to upsert");
    store.upsert(note2).await.expect("Failed to upsert");
    store.upsert(note3).await.expect("Failed to upsert");

    // Act - Search with AND filter: rust tag AND published status
    let filter = Filter::And(vec![
        Filter::Tag("rust".to_string()),
        Filter::Property("status".to_string(), Op::Eq, json!("published")),
    ]);
    let results = store
        .search(&embedding, 10, Some(filter))
        .await
        .expect("Failed to search");

    // Assert
    assert_eq!(results.len(), 1, "Should find 1 note matching both criteria");
    assert_eq!(results[0].note.path, "notes/rust-published.md");
}

/// Test semantic search with OR compound filter
///
/// Note: The current SurrealNoteStore implementation has a limitation with compound
/// OR filters using the same parameter name (e.g., two Tag filters both use `$tag`).
/// This test uses different filter types to exercise OR logic correctly.
#[tokio::test]
async fn search_with_or_filter() {
    // Arrange
    let store = setup_store().await;

    let embedding = make_embedding(384, 1.0);

    // Note in projects/ path with rust tag
    let note1 = make_note("projects/rust.md", "Rust Project")
        .with_tags(vec!["rust".to_string()])
        .with_embedding(embedding.clone());

    // Note in notes/ path with python tag
    let note2 = make_note("notes/python.md", "Python Notes")
        .with_tags(vec!["python".to_string()])
        .with_embedding(embedding.clone());

    // Note in docs/ path with go tag (should not match either filter)
    let note3 = make_note("docs/go.md", "Go Docs")
        .with_tags(vec!["go".to_string()])
        .with_embedding(embedding.clone());

    store.upsert(note1).await.expect("Failed to upsert");
    store.upsert(note2).await.expect("Failed to upsert");
    store.upsert(note3).await.expect("Failed to upsert");

    // Act - Search with OR filter: rust tag OR projects/ path prefix
    // This combines different filter types to avoid parameter collision
    let filter = Filter::Or(vec![
        Filter::Tag("rust".to_string()),
        Filter::Path("notes/".to_string()),
    ]);
    let results = store
        .search(&embedding, 10, Some(filter))
        .await
        .expect("Failed to search");

    // Assert
    assert_eq!(
        results.len(),
        2,
        "Should find 2 notes matching either criteria"
    );

    let paths: Vec<&str> = results.iter().map(|r| r.note.path.as_str()).collect();
    // projects/rust.md matches the Tag("rust") filter
    assert!(paths.contains(&"projects/rust.md"));
    // notes/python.md matches the Path("notes/") filter
    assert!(paths.contains(&"notes/python.md"));
    // docs/go.md matches neither
    assert!(!paths.contains(&"docs/go.md"));
}

/// Test semantic search with path prefix filter
#[tokio::test]
async fn search_with_path_filter() {
    // Arrange
    let store = setup_store().await;

    let embedding = make_embedding(384, 1.0);

    let note1 = make_note("projects/alpha/readme.md", "Alpha")
        .with_embedding(embedding.clone());

    let note2 = make_note("projects/beta/readme.md", "Beta")
        .with_embedding(embedding.clone());

    let note3 = make_note("notes/general.md", "General")
        .with_embedding(embedding.clone());

    store.upsert(note1).await.expect("Failed to upsert");
    store.upsert(note2).await.expect("Failed to upsert");
    store.upsert(note3).await.expect("Failed to upsert");

    // Act - Search filtered by path prefix "projects/"
    let filter = Filter::Path("projects/".to_string());
    let results = store
        .search(&embedding, 10, Some(filter))
        .await
        .expect("Failed to search");

    // Assert
    assert_eq!(results.len(), 2, "Should find 2 notes in projects/");

    let paths: Vec<&str> = results.iter().map(|r| r.note.path.as_str()).collect();
    assert!(paths.contains(&"projects/alpha/readme.md"));
    assert!(paths.contains(&"projects/beta/readme.md"));
    assert!(!paths.contains(&"notes/general.md"));
}

// ============================================================================
// Edge Cases
// ============================================================================

/// Test that get returns None for non-existent path
#[tokio::test]
async fn get_returns_none_for_missing() {
    // Arrange
    let store = setup_store().await;

    // Act
    let result = store
        .get("notes/does-not-exist.md")
        .await
        .expect("Failed to get");

    // Assert
    assert!(result.is_none(), "Should return None for non-existent path");
}

/// Test search when no notes have embeddings
#[tokio::test]
async fn search_with_no_embeddings() {
    // Arrange
    let store = setup_store().await;

    // Create notes WITHOUT embeddings
    let note = make_note("notes/no-embedding.md", "No Embedding");
    store.upsert(note).await.expect("Failed to upsert");

    // Act
    let query = make_embedding(384, 1.0);
    let results = store.search(&query, 10, None).await.expect("Failed to search");

    // Assert
    assert!(
        results.is_empty(),
        "Should return empty results when no notes have embeddings"
    );
}
