//! Graph traversal module for Steel scripts
//!
//! Provides database-backed graph queries via Steel functions.
//!
//! ## Steel Usage
//!
//! ```scheme
//! ;; Find a note by title (database query)
//! (db-find "Index")  ; => note hash or #f
//!
//! ;; Get outlinks from database
//! (db-outlinks "Index")  ; => list of notes
//!
//! ;; Get inlinks from database
//! (db-inlinks "Index")   ; => list of notes
//!
//! ;; Execute arbitrary graph query
//! (db-query "find(\"Index\") | ->wikilink[]")
//!
//! ;; Get a note by path (NoteStore)
//! (note-get "path/to/note.md")  ; => note record or #f
//!
//! ;; List notes with optional limit
//! (note-list 10)  ; => list of note records
//! ```

use crate::error::SteelError;
use crucible_core::storage::NoteStore;
use crucible_core::traits::GraphQueryExecutor;
use std::sync::Arc;

/// Graph module that provides database-backed queries
///
/// This is designed to be registered with a Steel executor to provide
/// db-find, db-outlinks, db-inlinks functions.
pub struct GraphModule {
    executor: Arc<dyn GraphQueryExecutor>,
}

impl GraphModule {
    /// Create a new graph module with a database executor
    pub fn new(executor: Arc<dyn GraphQueryExecutor>) -> Self {
        Self { executor }
    }

    /// Find a note by title
    pub async fn find(&self, title: &str) -> Result<Option<serde_json::Value>, SteelError> {
        let query = format!(r#"find("{}")"#, escape_quotes(title));
        let results = self
            .executor
            .execute(&query)
            .await
            .map_err(|e| SteelError::Execution(format!("Graph query error: {}", e)))?;

        Ok(results.into_iter().next())
    }

    /// Get outlinks from a note
    pub async fn outlinks(&self, title: &str) -> Result<Vec<serde_json::Value>, SteelError> {
        let query = format!(r#"outlinks("{}")"#, escape_quotes(title));
        self.executor
            .execute(&query)
            .await
            .map_err(|e| SteelError::Execution(format!("Graph query error: {}", e)))
    }

    /// Get inlinks to a note
    pub async fn inlinks(&self, title: &str) -> Result<Vec<serde_json::Value>, SteelError> {
        let query = format!(r#"inlinks("{}")"#, escape_quotes(title));
        self.executor
            .execute(&query)
            .await
            .map_err(|e| SteelError::Execution(format!("Graph query error: {}", e)))
    }

    /// Get all neighbors (outlinks + inlinks)
    pub async fn neighbors(&self, title: &str) -> Result<Vec<serde_json::Value>, SteelError> {
        let query = format!(r#"neighbors("{}")"#, escape_quotes(title));
        self.executor
            .execute(&query)
            .await
            .map_err(|e| SteelError::Execution(format!("Graph query error: {}", e)))
    }

    /// Execute an arbitrary graph query
    pub async fn query(&self, query: &str) -> Result<Vec<serde_json::Value>, SteelError> {
        self.executor
            .execute(query)
            .await
            .map_err(|e| SteelError::Execution(format!("Graph query error: {}", e)))
    }

    /// Generate Steel code that defines the db-* functions
    ///
    /// These functions are stubs that will be replaced by Rust implementations
    /// when registered with an executor that has database access.
    pub fn steel_stubs() -> &'static str {
        r#"
;; Database-backed graph functions (stubs - replaced by Rust)
;; These provide the same interface as the pure Steel graph functions
;; but query the actual database.

(define (db-find title)
  (error "db-find not available: no database connection"))

(define (db-outlinks title)
  (error "db-outlinks not available: no database connection"))

(define (db-inlinks title)
  (error "db-inlinks not available: no database connection"))

(define (db-neighbors title)
  (error "db-neighbors not available: no database connection"))

(define (db-query q)
  (error "db-query not available: no database connection"))
"#
    }
}

// =============================================================================
// NoteStore Module
// =============================================================================

/// NoteStore module that provides note access via Steel functions
///
/// This module allows Steel scripts to access notes from the NoteStore.
/// Functions are designed to return JSON-compatible values that can be
/// easily used in Steel code.
///
/// ## Steel Usage
///
/// ```scheme
/// ;; Get a note by path
/// (define note (note-get "path/to/note.md"))
/// (if note
///     (hash-ref note 'title)
///     #f)
///
/// ;; List notes (with optional limit)
/// (define all-notes (note-list 100))
/// (map (lambda (n) (hash-ref n 'title)) all-notes)
/// ```
pub struct NoteStoreModule {
    store: Arc<dyn NoteStore>,
}

impl NoteStoreModule {
    /// Create a new note store module
    pub fn new(store: Arc<dyn NoteStore>) -> Self {
        Self { store }
    }

    /// Get a note by path
    ///
    /// Returns the note record as a JSON object, or None if not found.
    pub async fn get(&self, path: &str) -> Result<Option<serde_json::Value>, SteelError> {
        match self.store.get(path).await {
            Ok(Some(record)) => {
                serde_json::to_value(&record)
                    .map(Some)
                    .map_err(|e| SteelError::Execution(format!("Failed to serialize note: {}", e)))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(SteelError::Execution(format!("NoteStore error: {}", e))),
        }
    }

    /// List notes with optional limit
    ///
    /// Returns a JSON array of note records. If limit is 0 or negative,
    /// returns all notes.
    pub async fn list(&self, limit: i64) -> Result<serde_json::Value, SteelError> {
        match self.store.list().await {
            Ok(records) => {
                let limited: Vec<_> = if limit > 0 {
                    records.into_iter().take(limit as usize).collect()
                } else {
                    records
                };
                serde_json::to_value(&limited)
                    .map_err(|e| SteelError::Execution(format!("Failed to serialize notes: {}", e)))
            }
            Err(e) => Err(SteelError::Execution(format!("NoteStore error: {}", e))),
        }
    }

    /// Generate Steel code that defines the note-* functions
    ///
    /// These functions are stubs that will be replaced by Rust implementations
    /// when registered with an executor that has NoteStore access.
    pub fn steel_stubs() -> &'static str {
        r#"
;; NoteStore functions (stubs - replaced by Rust)
;; These provide access to notes stored in the NoteStore.

(define (note-get path)
  (error "note-get not available: no NoteStore connection"))

(define (note-list limit)
  (error "note-list not available: no NoteStore connection"))
"#
    }
}

/// Escape quotes in a string for safe embedding in queries
fn escape_quotes(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use crate::SteelExecutor;
    use serde_json::json;

    // Include the graph library source
    const GRAPH_LIB: &str = include_str!("../lib/graph.scm");

    #[tokio::test]
    async fn test_graph_find_existing() {
        let executor = SteelExecutor::new().unwrap();

        // Load the graph library
        executor.execute_source(GRAPH_LIB).await.unwrap();

        // Create test data and find a note
        let result = executor
            .execute_source(
                r#"
                (define notes
                  (list
                    (hash 'title "Index" 'path "Index.md" 'links '("Project A"))
                    (hash 'title "Project A" 'path "a.md" 'links '())))
                (note-title (graph-find notes "Index"))
                "#,
            )
            .await
            .unwrap();

        assert_eq!(result, json!("Index"));
    }

    #[tokio::test]
    async fn test_graph_find_missing() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(GRAPH_LIB).await.unwrap();

        let result = executor
            .execute_source(
                r#"
                (define notes
                  (list (hash 'title "Index" 'path "Index.md" 'links '())))
                (graph-find notes "Missing")
                "#,
            )
            .await
            .unwrap();

        assert_eq!(result, json!(false));
    }

    #[tokio::test]
    async fn test_graph_outlinks() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(GRAPH_LIB).await.unwrap();

        let result = executor
            .execute_source(
                r#"
                (define notes
                  (list
                    (hash 'title "Index" 'path "Index.md" 'links '("Project A" "Project B"))
                    (hash 'title "Project A" 'path "a.md" 'links '())
                    (hash 'title "Project B" 'path "b.md" 'links '())))
                (map note-title (graph-outlinks notes "Index"))
                "#,
            )
            .await
            .unwrap();

        // Should return titles of linked notes
        let titles: Vec<String> = serde_json::from_value(result).unwrap();
        assert!(titles.contains(&"Project A".to_string()));
        assert!(titles.contains(&"Project B".to_string()));
    }

    #[tokio::test]
    async fn test_graph_inlinks() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(GRAPH_LIB).await.unwrap();

        let result = executor
            .execute_source(
                r#"
                (define notes
                  (list
                    (hash 'title "Index" 'path "Index.md" 'links '("Project A"))
                    (hash 'title "Project A" 'path "a.md" 'links '("Index"))
                    (hash 'title "Project B" 'path "b.md" 'links '())))
                (map note-title (graph-inlinks notes "Index"))
                "#,
            )
            .await
            .unwrap();

        // Only Project A links to Index
        assert_eq!(result, json!(["Project A"]));
    }

    #[tokio::test]
    async fn test_graph_neighbors() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(GRAPH_LIB).await.unwrap();

        let result = executor
            .execute_source(
                r#"
                (define notes
                  (list
                    (hash 'title "Hub" 'path "hub.md" 'links '("A"))
                    (hash 'title "A" 'path "a.md" 'links '())
                    (hash 'title "B" 'path "b.md" 'links '("Hub"))))
                (length (graph-neighbors notes "Hub"))
                "#,
            )
            .await
            .unwrap();

        // Hub links to A, B links to Hub => 2 neighbors
        assert_eq!(result, json!(2));
    }

    #[tokio::test]
    async fn test_graph_filter_by_tag() {
        let executor = SteelExecutor::new().unwrap();
        executor.execute_source(GRAPH_LIB).await.unwrap();

        let result = executor
            .execute_source(
                r#"
                (define notes
                  (list
                    (hash 'title "Index" 'path "Index.md" 'links '() 'tags '("important"))
                    (hash 'title "Project A" 'path "a.md" 'links '() 'tags '("project" "important"))
                    (hash 'title "Draft" 'path "draft.md" 'links '() 'tags '("draft"))))
                (map note-title (graph-filter-by-tag notes "important"))
                "#,
            )
            .await
            .unwrap();

        let titles: Vec<String> = serde_json::from_value(result).unwrap();
        assert_eq!(titles.len(), 2);
        assert!(titles.contains(&"Index".to_string()));
        assert!(titles.contains(&"Project A".to_string()));
    }
}

// =============================================================================
// NoteStoreModule Tests
// =============================================================================

#[cfg(test)]
mod note_store_tests {
    use super::*;
    use async_trait::async_trait;
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::{Filter, NoteRecord, SearchResult, StorageError, StorageResult};

    /// Mock NoteStore that returns predetermined results
    struct MockNoteStore {
        notes: Vec<NoteRecord>,
    }

    impl MockNoteStore {
        fn new(notes: Vec<NoteRecord>) -> Self {
            Self { notes }
        }

        fn with_sample_notes() -> Self {
            Self::new(vec![
                NoteRecord::new("notes/index.md", BlockHash::zero())
                    .with_title("Index".to_string())
                    .with_tags(vec!["home".to_string()]),
                NoteRecord::new("notes/project-a.md", BlockHash::zero())
                    .with_title("Project A".to_string())
                    .with_links(vec!["notes/index.md".to_string()]),
                NoteRecord::new("notes/project-b.md", BlockHash::zero())
                    .with_title("Project B".to_string()),
            ])
        }
    }

    #[async_trait]
    impl NoteStore for MockNoteStore {
        async fn upsert(&self, _note: NoteRecord) -> StorageResult<()> {
            Ok(())
        }

        async fn get(&self, path: &str) -> StorageResult<Option<NoteRecord>> {
            Ok(self.notes.iter().find(|n| n.path == path).cloned())
        }

        async fn delete(&self, _path: &str) -> StorageResult<()> {
            Ok(())
        }

        async fn list(&self) -> StorageResult<Vec<NoteRecord>> {
            Ok(self.notes.clone())
        }

        async fn get_by_hash(&self, _hash: &BlockHash) -> StorageResult<Option<NoteRecord>> {
            Ok(None)
        }

        async fn search(
            &self,
            _embedding: &[f32],
            _limit: usize,
            _filter: Option<Filter>,
        ) -> StorageResult<Vec<SearchResult>> {
            Ok(vec![])
        }
    }

    /// Mock NoteStore that always fails
    struct FailingNoteStore {
        message: String,
    }

    #[async_trait]
    impl NoteStore for FailingNoteStore {
        async fn upsert(&self, _note: NoteRecord) -> StorageResult<()> {
            Err(StorageError::backend(&self.message))
        }

        async fn get(&self, _path: &str) -> StorageResult<Option<NoteRecord>> {
            Err(StorageError::backend(&self.message))
        }

        async fn delete(&self, _path: &str) -> StorageResult<()> {
            Err(StorageError::backend(&self.message))
        }

        async fn list(&self) -> StorageResult<Vec<NoteRecord>> {
            Err(StorageError::backend(&self.message))
        }

        async fn get_by_hash(&self, _hash: &BlockHash) -> StorageResult<Option<NoteRecord>> {
            Err(StorageError::backend(&self.message))
        }

        async fn search(
            &self,
            _embedding: &[f32],
            _limit: usize,
            _filter: Option<Filter>,
        ) -> StorageResult<Vec<SearchResult>> {
            Err(StorageError::backend(&self.message))
        }
    }

    // =========================================================================
    // note_get tests
    // =========================================================================

    #[tokio::test]
    async fn test_note_get_returns_record() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_sample_notes());
        let module = NoteStoreModule::new(store);

        let result = module.get("notes/index.md").await.unwrap();
        assert!(result.is_some());

        let note = result.unwrap();
        assert_eq!(note["path"], "notes/index.md");
        assert_eq!(note["title"], "Index");
    }

    #[tokio::test]
    async fn test_note_get_returns_none_for_missing() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_sample_notes());
        let module = NoteStoreModule::new(store);

        let result = module.get("nonexistent.md").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_note_get_error_handling() {
        let store: Arc<dyn NoteStore> = Arc::new(FailingNoteStore {
            message: "Database connection lost".to_string(),
        });
        let module = NoteStoreModule::new(store);

        let result = module.get("notes/index.md").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NoteStore error"));
    }

    // =========================================================================
    // note_list tests
    // =========================================================================

    #[tokio::test]
    async fn test_note_list_returns_all() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_sample_notes());
        let module = NoteStoreModule::new(store);

        let result = module.list(0).await.unwrap();
        let notes: Vec<serde_json::Value> = serde_json::from_value(result).unwrap();
        assert_eq!(notes.len(), 3);
    }

    #[tokio::test]
    async fn test_note_list_respects_limit() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_sample_notes());
        let module = NoteStoreModule::new(store);

        let result = module.list(2).await.unwrap();
        let notes: Vec<serde_json::Value> = serde_json::from_value(result).unwrap();
        assert_eq!(notes.len(), 2);
    }

    #[tokio::test]
    async fn test_note_list_limit_exceeds_count() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_sample_notes());
        let module = NoteStoreModule::new(store);

        let result = module.list(100).await.unwrap();
        let notes: Vec<serde_json::Value> = serde_json::from_value(result).unwrap();
        assert_eq!(notes.len(), 3);
    }

    #[tokio::test]
    async fn test_note_list_empty_store() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::new(vec![]));
        let module = NoteStoreModule::new(store);

        let result = module.list(10).await.unwrap();
        let notes: Vec<serde_json::Value> = serde_json::from_value(result).unwrap();
        assert!(notes.is_empty());
    }

    #[tokio::test]
    async fn test_note_list_error_handling() {
        let store: Arc<dyn NoteStore> = Arc::new(FailingNoteStore {
            message: "Storage unavailable".to_string(),
        });
        let module = NoteStoreModule::new(store);

        let result = module.list(10).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NoteStore error"));
    }

    // =========================================================================
    // steel_stubs tests
    // =========================================================================

    #[test]
    fn test_steel_stubs_contains_note_get() {
        let stubs = NoteStoreModule::steel_stubs();
        assert!(stubs.contains("note-get"));
        assert!(stubs.contains("note-list"));
    }

    // =========================================================================
    // NoteRecord serialization tests
    // =========================================================================

    #[tokio::test]
    async fn test_note_record_includes_all_fields() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_sample_notes());
        let module = NoteStoreModule::new(store);

        let result = module.get("notes/index.md").await.unwrap().unwrap();

        // Verify essential fields are present
        assert!(result.get("path").is_some());
        assert!(result.get("title").is_some());
        assert!(result.get("tags").is_some());
        assert!(result.get("content_hash").is_some());

        // Check tags array
        let tags = result["tags"].as_array().unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0], "home");
    }

    #[tokio::test]
    async fn test_note_record_with_links() {
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::with_sample_notes());
        let module = NoteStoreModule::new(store);

        let result = module.get("notes/project-a.md").await.unwrap().unwrap();

        let links = result["links_to"].as_array().unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0], "notes/index.md");
    }
}
