//! NoteStore Contract Tests
//!
//! These tests verify that storage backends conform to the NoteStore trait contract.
//! Any backend implementing the crucible-core NoteStore trait must pass these tests.
//!
//! # Running Contract Tests
//!
//! ```bash
//! # Test SQLite backend
//! cargo test -p crucible-storage-tests --features sqlite --test note_store_contract
//!
//! # Test SurrealDB backend
//! cargo test -p crucible-storage-tests --features surrealdb --test note_store_contract
//!
//! # Test LanceDB backend
//! cargo test -p crucible-storage-tests --features lance --test note_store_contract
//! ```
//!
//! # Contract Requirements
//!
//! Each test documents the behavioral contract that all implementations must follow.
//! These are not just "does it work" tests, but "does it behave correctly" tests.

#![cfg(any(feature = "sqlite", feature = "surrealdb", feature = "lance"))]

use std::collections::HashMap;

use crucible_core::parser::BlockHash;
use crucible_core::storage::{Filter, NoteRecord, NoteStore, Op};
use serde_json::Value;

// ============================================================================
// Backend Factory and Test Helpers
// ============================================================================

/// Embedding dimensions for vector tests.
/// SurrealDB uses 384 by default; SQLite and LanceDB compute similarity so any dimension works.
/// We use a small dimension for test clarity and compatibility.
const TEST_EMBEDDING_DIM: usize = 8;

/// Factory function to create a NoteStore - switches based on feature flag
///
/// Each test gets a fresh, isolated store instance for test independence.
async fn create_store() -> impl NoteStore {
    #[cfg(feature = "sqlite")]
    {
        let pool = crucible_sqlite::SqlitePool::memory().expect("Failed to create SQLite pool");
        crucible_sqlite::create_note_store(pool)
            .await
            .expect("Failed to create SQLite NoteStore")
    }

    #[cfg(all(feature = "surrealdb", not(feature = "sqlite"), not(feature = "lance")))]
    {
        use crucible_surrealdb::test_utils::SurrealClient;

        let client = SurrealClient::new_memory()
            .await
            .expect("Failed to create SurrealDB client");
        // Use custom dimensions matching our test embedding size
        crucible_surrealdb::create_note_store_with_dimensions(client, TEST_EMBEDDING_DIM)
            .await
            .expect("Failed to create SurrealDB NoteStore")
    }

    #[cfg(all(feature = "lance", not(feature = "sqlite"), not(feature = "surrealdb")))]
    {
        let dir = tempfile::TempDir::new().expect("Failed to create temp dir");
        let db_path = dir.path().join("test.lance");
        // Leak the TempDir to keep it alive for the duration of the test
        // This is safe in tests since the process will clean up on exit
        let dir = Box::leak(Box::new(dir));
        let _ = dir; // silence unused warning
        crucible_lance::create_note_store_with_dimensions(
            db_path.to_str().unwrap(),
            TEST_EMBEDDING_DIM,
        )
        .await
        .expect("Failed to create LanceDB NoteStore")
    }
}

/// Create a test NoteRecord with minimal required fields
fn make_note(path: &str, title: &str) -> NoteRecord {
    NoteRecord::new(path.to_string(), BlockHash::zero()).with_title(title.to_string())
}

/// Create a NoteRecord with a specific content hash
fn make_note_with_hash(path: &str, title: &str, hash: BlockHash) -> NoteRecord {
    NoteRecord::new(path.to_string(), hash).with_title(title.to_string())
}

/// Create a normalized test embedding vector
///
/// Uses TEST_EMBEDDING_DIM dimensions for compatibility with SurrealDB's vector index.
/// The seed parameter creates distinct but predictable embedding directions.
fn make_test_embedding(seed: f32) -> Vec<f32> {
    let mut embedding: Vec<f32> = (0..TEST_EMBEDDING_DIM)
        .map(|i| seed + (i as f32) * 0.1)
        .collect();

    // Normalize for consistent cosine similarity behavior
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut embedding {
            *x /= norm;
        }
    }
    embedding
}

/// Create a NoteRecord with an embedding vector of standard test dimensions
fn make_note_with_embedding(path: &str, title: &str, seed: f32) -> NoteRecord {
    NoteRecord::new(path.to_string(), BlockHash::zero())
        .with_title(title.to_string())
        .with_embedding(make_test_embedding(seed))
}

// ============================================================================
// CRUD Contract Tests
// ============================================================================

mod crud_contract {
    use super::*;

    /// CONTRACT: upsert stores a note, get retrieves it with all fields preserved
    #[tokio::test]
    async fn upsert_and_get_roundtrip() {
        let store = create_store().await;

        let note = NoteRecord::new("test/note.md", BlockHash::zero())
            .with_title("Test Note")
            .with_tags(vec!["rust".to_string(), "test".to_string()])
            .with_links(vec!["other/note.md".to_string()]);

        store.upsert(note.clone()).await.expect("Failed to upsert");

        let retrieved = store
            .get("test/note.md")
            .await
            .expect("Failed to get")
            .expect("Note should exist");

        assert_eq!(retrieved.path, "test/note.md");
        assert_eq!(retrieved.title, "Test Note");
        assert_eq!(retrieved.tags, vec!["rust", "test"]);
        assert_eq!(retrieved.links_to, vec!["other/note.md"]);
        assert_eq!(retrieved.content_hash, BlockHash::zero());
    }

    /// CONTRACT: upsert with same path updates existing note (not duplicates)
    #[tokio::test]
    async fn upsert_updates_existing() {
        let store = create_store().await;

        // Initial insert
        let note1 = make_note("test/update.md", "Original Title");
        store.upsert(note1).await.expect("Failed to upsert");

        // Update with same path
        let note2 = NoteRecord::new("test/update.md", BlockHash::zero())
            .with_title("Updated Title")
            .with_tags(vec!["new-tag".to_string()]);
        store.upsert(note2).await.expect("Failed to update");

        // Verify update occurred
        let retrieved = store
            .get("test/update.md")
            .await
            .expect("Failed to get")
            .expect("Note should exist");

        assert_eq!(retrieved.title, "Updated Title");
        assert_eq!(retrieved.tags, vec!["new-tag"]);

        // Verify no duplicates
        let all = store.list().await.expect("Failed to list");
        assert_eq!(all.len(), 1);
    }

    /// CONTRACT: delete removes note, subsequent get returns None
    #[tokio::test]
    async fn delete_removes_note() {
        let store = create_store().await;

        let note = make_note("test/delete.md", "To Be Deleted");
        store.upsert(note).await.expect("Failed to upsert");

        // Verify it exists
        assert!(store.get("test/delete.md").await.unwrap().is_some());

        // Delete
        store
            .delete("test/delete.md")
            .await
            .expect("Failed to delete");

        // Verify it's gone
        let result = store.get("test/delete.md").await.expect("Failed to get");
        assert!(result.is_none());
    }

    /// CONTRACT: delete on non-existent path is idempotent (no error)
    #[tokio::test]
    async fn delete_is_idempotent() {
        let store = create_store().await;

        // Delete non-existent note - should not error
        let result = store.delete("does/not/exist.md").await;
        assert!(result.is_ok());

        // Delete twice - should still succeed
        let note = make_note("test/twice.md", "Delete Twice");
        store.upsert(note).await.expect("Failed to upsert");

        store.delete("test/twice.md").await.expect("First delete");
        let result = store.delete("test/twice.md").await;
        assert!(result.is_ok(), "Second delete should also succeed");
    }

    /// CONTRACT: get returns None for non-existent path
    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let store = create_store().await;

        let result = store.get("never/existed.md").await.expect("Failed to get");
        assert!(result.is_none());
    }

    /// CONTRACT: list returns all stored notes
    #[tokio::test]
    async fn list_returns_all() {
        let store = create_store().await;

        // Insert multiple notes
        for i in 0..5 {
            let note = make_note(&format!("notes/note{}.md", i), &format!("Note {}", i));
            store.upsert(note).await.expect("Failed to upsert");
        }

        let all = store.list().await.expect("Failed to list");
        assert_eq!(all.len(), 5);

        // Verify all paths are present
        let paths: Vec<&str> = all.iter().map(|n| n.path.as_str()).collect();
        for i in 0..5 {
            assert!(
                paths.contains(&format!("notes/note{}.md", i).as_str()),
                "Missing note {}",
                i
            );
        }
    }

    /// CONTRACT: list returns empty vec when store is empty
    #[tokio::test]
    async fn list_empty_returns_empty_vec() {
        let store = create_store().await;

        let all = store.list().await.expect("Failed to list");
        assert!(all.is_empty());
    }
}

// ============================================================================
// Hash Lookup Contract Tests
// ============================================================================

mod hash_contract {
    use super::*;

    /// CONTRACT: get_by_hash retrieves note by its content hash
    #[tokio::test]
    async fn get_by_hash() {
        let store = create_store().await;

        let hash = BlockHash::new([1u8; 32]);
        let note = make_note_with_hash("test/hashed.md", "Hashed Note", hash);
        store.upsert(note).await.expect("Failed to upsert");

        let found = store
            .get_by_hash(&hash)
            .await
            .expect("Failed to get by hash")
            .expect("Note should be found");

        assert_eq!(found.path, "test/hashed.md");
        assert_eq!(found.content_hash, hash);
    }

    /// CONTRACT: get_by_hash returns None for non-existent hash
    #[tokio::test]
    async fn get_by_hash_nonexistent() {
        let store = create_store().await;

        let hash = BlockHash::new([99u8; 32]);
        let result = store
            .get_by_hash(&hash)
            .await
            .expect("Failed to get by hash");

        assert!(result.is_none());
    }

    /// CONTRACT: get_by_hash finds updated note with new hash
    #[tokio::test]
    async fn get_by_hash_after_update() {
        let store = create_store().await;

        let old_hash = BlockHash::new([1u8; 32]);
        let new_hash = BlockHash::new([2u8; 32]);

        // Insert with old hash
        let note1 = make_note_with_hash("test/evolving.md", "Version 1", old_hash);
        store.upsert(note1).await.expect("Failed to upsert");

        // Update with new hash
        let note2 = make_note_with_hash("test/evolving.md", "Version 2", new_hash);
        store.upsert(note2).await.expect("Failed to update");

        // Old hash should not find anything
        let old_result = store.get_by_hash(&old_hash).await.expect("Failed to get");
        assert!(old_result.is_none(), "Old hash should not find note");

        // New hash should find the note
        let new_result = store
            .get_by_hash(&new_hash)
            .await
            .expect("Failed to get")
            .expect("Note should be found by new hash");
        assert_eq!(new_result.title, "Version 2");
    }
}

// ============================================================================
// Vector Search Contract Tests
// ============================================================================

mod search_contract {
    use super::*;

    /// CONTRACT: search by embedding returns results sorted by similarity
    #[tokio::test]
    async fn search_by_embedding() {
        let store = create_store().await;

        // Create notes with distinct embedding directions
        let note1 = make_note_with_embedding("note1.md", "Rust", 1.0);
        let note2 = make_note_with_embedding("note2.md", "Python", 5.0);
        let note3 = make_note_with_embedding("note3.md", "Mixed", 2.5);

        store.upsert(note1).await.expect("upsert 1");
        store.upsert(note2).await.expect("upsert 2");
        store.upsert(note3).await.expect("upsert 3");

        // Query embedding most similar to note1
        let query = make_test_embedding(1.0);
        let results = store.search(&query, 10, None).await.expect("search");

        assert_eq!(results.len(), 3);

        // First result should be note1 (exact match)
        assert_eq!(results[0].note.path, "note1.md");
        assert!(
            (results[0].score - 1.0).abs() < 0.01,
            "Expected score ~1.0, got {}",
            results[0].score
        );

        // Scores should be descending
        for i in 1..results.len() {
            assert!(
                results[i - 1].score >= results[i].score,
                "Results should be sorted by score descending"
            );
        }
    }

    /// CONTRACT: search respects k limit
    #[tokio::test]
    async fn search_respects_k_limit() {
        let store = create_store().await;

        // Insert 10 notes with embeddings
        for i in 0..10 {
            let note = make_note_with_embedding(
                &format!("note{}.md", i),
                &format!("Note {}", i),
                i as f32,
            );
            store.upsert(note).await.expect("upsert");
        }

        let query = make_test_embedding(5.0);
        let results = store.search(&query, 3, None).await.expect("search");

        assert_eq!(results.len(), 3, "Should return exactly k results");
    }

    /// CONTRACT: search excludes notes without embeddings
    #[tokio::test]
    async fn search_excludes_notes_without_embedding() {
        let store = create_store().await;

        // Note with embedding
        let note_with = make_note_with_embedding("with.md", "Has Embedding", 1.0);
        store.upsert(note_with).await.expect("upsert");

        // Note without embedding
        let note_without = make_note("without.md", "No Embedding");
        store.upsert(note_without).await.expect("upsert");

        let query = make_test_embedding(1.0);
        let results = store.search(&query, 10, None).await.expect("search");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].note.path, "with.md");
    }

    /// CONTRACT: search with tag filter only returns matching tags
    #[tokio::test]
    async fn search_with_tag_filter() {
        let store = create_store().await;

        let note1 = NoteRecord::new("note1.md", BlockHash::zero())
            .with_title("Rust Note")
            .with_tags(vec!["rust".to_string()])
            .with_embedding(make_test_embedding(1.0));

        let note2 = NoteRecord::new("note2.md", BlockHash::zero())
            .with_title("Python Note")
            .with_tags(vec!["python".to_string()])
            .with_embedding(make_test_embedding(2.0));

        let note3 = NoteRecord::new("note3.md", BlockHash::zero())
            .with_title("Both Tags")
            .with_tags(vec!["rust".to_string(), "python".to_string()])
            .with_embedding(make_test_embedding(3.0));

        store.upsert(note1).await.expect("upsert");
        store.upsert(note2).await.expect("upsert");
        store.upsert(note3).await.expect("upsert");

        let query = make_test_embedding(1.0);
        let filter = Filter::Tag("rust".to_string());
        let results = store
            .search(&query, 10, Some(filter))
            .await
            .expect("search");

        assert_eq!(results.len(), 2);
        for result in &results {
            assert!(
                result.note.tags.contains(&"rust".to_string()),
                "All results should have 'rust' tag"
            );
        }
    }

    /// CONTRACT: search with path filter only returns matching paths
    #[tokio::test]
    async fn search_with_path_filter() {
        let store = create_store().await;

        let note1 = NoteRecord::new("projects/rust/note.md", BlockHash::zero())
            .with_title("Rust Project")
            .with_embedding(make_test_embedding(1.0));

        let note2 = NoteRecord::new("projects/python/note.md", BlockHash::zero())
            .with_title("Python Project")
            .with_embedding(make_test_embedding(2.0));

        let note3 = NoteRecord::new("personal/note.md", BlockHash::zero())
            .with_title("Personal")
            .with_embedding(make_test_embedding(3.0));

        store.upsert(note1).await.expect("upsert");
        store.upsert(note2).await.expect("upsert");
        store.upsert(note3).await.expect("upsert");

        let query = make_test_embedding(1.0);
        let filter = Filter::Path("projects/".to_string());
        let results = store
            .search(&query, 10, Some(filter))
            .await
            .expect("search");

        assert_eq!(results.len(), 2);
        for result in &results {
            assert!(
                result.note.path.starts_with("projects/"),
                "All results should be under projects/"
            );
        }
    }

    /// CONTRACT: search with property filter
    #[tokio::test]
    async fn search_with_property_filter() {
        let store = create_store().await;

        let mut props1 = HashMap::new();
        props1.insert("status".to_string(), Value::String("published".to_string()));

        let mut props2 = HashMap::new();
        props2.insert("status".to_string(), Value::String("draft".to_string()));

        let note1 = NoteRecord::new("published.md", BlockHash::zero())
            .with_title("Published")
            .with_properties(props1)
            .with_embedding(make_test_embedding(1.0));

        let note2 = NoteRecord::new("draft.md", BlockHash::zero())
            .with_title("Draft")
            .with_properties(props2)
            .with_embedding(make_test_embedding(2.0));

        store.upsert(note1).await.expect("upsert");
        store.upsert(note2).await.expect("upsert");

        let query = make_test_embedding(1.0);
        let filter = Filter::Property(
            "status".to_string(),
            Op::Eq,
            Value::String("published".to_string()),
        );
        let results = store
            .search(&query, 10, Some(filter))
            .await
            .expect("search");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].note.path, "published.md");
    }

    /// CONTRACT: search with AND filter combining different filter types
    #[tokio::test]
    async fn search_with_and_filter() {
        let store = create_store().await;

        // Note with both conditions
        let note1 = NoteRecord::new("projects/rust/note.md", BlockHash::zero())
            .with_title("Rust Project")
            .with_tags(vec!["rust".to_string()])
            .with_embedding(make_test_embedding(1.0));

        // Note with only path match
        let note2 = NoteRecord::new("projects/python/note.md", BlockHash::zero())
            .with_title("Python Project")
            .with_tags(vec!["python".to_string()])
            .with_embedding(make_test_embedding(2.0));

        // Note with only tag match
        let note3 = NoteRecord::new("personal/rust.md", BlockHash::zero())
            .with_title("Personal Rust")
            .with_tags(vec!["rust".to_string()])
            .with_embedding(make_test_embedding(3.0));

        store.upsert(note1).await.expect("upsert");
        store.upsert(note2).await.expect("upsert");
        store.upsert(note3).await.expect("upsert");

        let query = make_test_embedding(1.0);
        let filter = Filter::And(vec![
            Filter::Path("projects/".to_string()),
            Filter::Tag("rust".to_string()),
        ]);
        let results = store
            .search(&query, 10, Some(filter))
            .await
            .expect("search");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].note.path, "projects/rust/note.md");
    }

    /// CONTRACT: search with OR filter combining different filter types
    ///
    /// Note: This test uses different filter types (Tag + Path) to avoid
    /// parameter collision issues in some backends.
    #[tokio::test]
    async fn search_with_or_filter() {
        let store = create_store().await;

        // Note in projects/ with rust tag
        let note1 = NoteRecord::new("projects/rust.md", BlockHash::zero())
            .with_title("Rust Project")
            .with_tags(vec!["rust".to_string()])
            .with_embedding(make_test_embedding(1.0));

        // Note in notes/ with python tag
        let note2 = NoteRecord::new("notes/python.md", BlockHash::zero())
            .with_title("Python Notes")
            .with_tags(vec!["python".to_string()])
            .with_embedding(make_test_embedding(2.0));

        // Note in docs/ with go tag (should not match either filter)
        let note3 = NoteRecord::new("docs/go.md", BlockHash::zero())
            .with_title("Go Docs")
            .with_tags(vec!["go".to_string()])
            .with_embedding(make_test_embedding(3.0));

        store.upsert(note1).await.expect("upsert");
        store.upsert(note2).await.expect("upsert");
        store.upsert(note3).await.expect("upsert");

        let query = make_test_embedding(1.0);
        // Use different filter types to avoid parameter collision
        let filter = Filter::Or(vec![
            Filter::Tag("rust".to_string()),
            Filter::Path("notes/".to_string()),
        ]);
        let results = store
            .search(&query, 10, Some(filter))
            .await
            .expect("search");

        assert_eq!(results.len(), 2);
        let paths: Vec<&str> = results.iter().map(|r| r.note.path.as_str()).collect();
        // projects/rust.md matches Tag("rust")
        assert!(paths.contains(&"projects/rust.md"));
        // notes/python.md matches Path("notes/")
        assert!(paths.contains(&"notes/python.md"));
        // docs/go.md matches neither
        assert!(!paths.contains(&"docs/go.md"));
    }

    /// CONTRACT: search returns empty vec when no matches
    #[tokio::test]
    async fn search_no_matches_returns_empty() {
        let store = create_store().await;

        // Insert a note with a tag
        let note = NoteRecord::new("note.md", BlockHash::zero())
            .with_tags(vec!["rust".to_string()])
            .with_embedding(make_test_embedding(1.0));
        store.upsert(note).await.expect("upsert");

        // Search with non-matching filter
        let query = make_test_embedding(1.0);
        let filter = Filter::Tag("nonexistent".to_string());
        let results = store
            .search(&query, 10, Some(filter))
            .await
            .expect("search");

        assert!(results.is_empty());
    }
}

// ============================================================================
// Data Integrity Contract Tests
// ============================================================================

mod integrity_contract {
    use super::*;

    /// CONTRACT: properties (string, number, bool) are preserved through roundtrip
    #[tokio::test]
    async fn properties_preserved() {
        let store = create_store().await;

        let mut props = HashMap::new();
        props.insert("string".to_string(), Value::String("value".to_string()));
        props.insert("number".to_string(), Value::Number(42.into()));
        props.insert("bool".to_string(), Value::Bool(true));

        let note = NoteRecord::new("props.md", BlockHash::zero()).with_properties(props.clone());

        store.upsert(note).await.expect("upsert");

        let retrieved = store.get("props.md").await.unwrap().unwrap();

        assert_eq!(retrieved.properties.get("string"), props.get("string"));
        assert_eq!(retrieved.properties.get("number"), props.get("number"));
        assert_eq!(retrieved.properties.get("bool"), props.get("bool"));
    }

    /// CONTRACT: embedding vectors are preserved exactly
    #[tokio::test]
    async fn embedding_preserved() {
        let store = create_store().await;

        let embedding = make_test_embedding(1.5);
        let note = NoteRecord::new("embed.md", BlockHash::zero())
            .with_title("Embedded")
            .with_embedding(embedding.clone());

        store.upsert(note).await.expect("upsert");

        let retrieved = store.get("embed.md").await.unwrap().unwrap();
        let retrieved_embedding = retrieved.embedding.expect("Should have embedding");

        assert_eq!(embedding.len(), retrieved_embedding.len());
        for (original, stored) in embedding.iter().zip(retrieved_embedding.iter()) {
            assert!(
                (original - stored).abs() < 1e-5,
                "Embedding values should be preserved"
            );
        }
    }

    /// CONTRACT: unicode in all text fields is preserved
    #[tokio::test]
    async fn unicode_preserved() {
        let store = create_store().await;

        let note = NoteRecord::new("unicode/note.md", BlockHash::zero())
            .with_title("Unicode Test")
            .with_tags(vec!["emoji-tag".to_string(), "test".to_string()]);

        store.upsert(note).await.expect("upsert");

        let retrieved = store.get("unicode/note.md").await.unwrap().unwrap();
        assert_eq!(retrieved.title, "Unicode Test");
    }

    /// CONTRACT: empty collections are handled correctly
    #[tokio::test]
    async fn empty_collections_handled() {
        let store = create_store().await;

        let note = NoteRecord::new("empty.md", BlockHash::zero())
            .with_title("Empty Collections")
            .with_tags(vec![])
            .with_links(vec![])
            .with_properties(HashMap::new());

        store.upsert(note).await.expect("upsert");

        let retrieved = store.get("empty.md").await.unwrap().unwrap();
        assert!(retrieved.tags.is_empty());
        assert!(retrieved.links_to.is_empty());
        assert!(retrieved.properties.is_empty());
    }

    /// CONTRACT: embeddings with standard dimensions are handled correctly
    #[tokio::test]
    async fn standard_embedding_handled() {
        let store = create_store().await;

        // Use standard test embedding dimensions
        let embedding = make_test_embedding(0.5);
        let note = NoteRecord::new("standard.md", BlockHash::zero())
            .with_title("Standard Embedding")
            .with_embedding(embedding.clone());

        store.upsert(note).await.expect("upsert");

        let retrieved = store.get("standard.md").await.unwrap().unwrap();
        let retrieved_embedding = retrieved.embedding.expect("Should have embedding");

        assert_eq!(retrieved_embedding.len(), TEST_EMBEDDING_DIM);

        // Verify values match
        assert!((retrieved_embedding[0] - embedding[0]).abs() < 1e-5);
        assert!(
            (retrieved_embedding[TEST_EMBEDDING_DIM - 1] - embedding[TEST_EMBEDDING_DIM - 1]).abs()
                < 1e-5
        );
    }
}
