//! NoteStore Storage Abstraction
//!
//! This module provides a unified storage abstraction for note metadata and search.
//! It replaces the previous scattered storage traits with three clean abstractions:
//!
//! - [`NoteStore`] - Storage CRUD + vector search
//! - [`GraphView`] - In-memory graph from denormalized links
//! - [`Precognition`] - Pure computation (hash + embed)
//!
//! # Design Philosophy
//!
//! NoteStore is a storage **index** over plaintext files, not a document database.
//! The source of truth remains the markdown files in the user's kiln. This module
//! provides efficient querying, semantic search, and graph traversal over that data.
//!
//! # Example
//!
//! ```ignore
//! use crucible_core::storage::{NoteStore, NoteRecord, Filter, Op};
//!
//! async fn example(store: &dyn NoteStore) -> Result<(), StorageError> {
//!     // Upsert a note record
//!     let record = NoteRecord {
//!         path: "notes/example.md".to_string(),
//!         content_hash: BlockHash::zero(),
//!         embedding: Some(vec![0.1; 768]),
//!         title: "Example Note".to_string(),
//!         tags: vec!["rust".to_string()],
//!         links_to: vec!["notes/other.md".to_string()],
//!         properties: Default::default(),
//!         updated_at: chrono::Utc::now(),
//!     };
//!     store.upsert(record).await?;
//!
//!     // Search by embedding
//!     let query_embedding = vec![0.1; 768];
//!     let filter = Filter::Tag("rust".to_string());
//!     let results = store.search(&query_embedding, 10, Some(filter)).await?;
//!     Ok(())
//! }
//! ```

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::parser::BlockHash;
use crate::storage::StorageResult;

// ============================================================================
// Core Types
// ============================================================================

/// A note record stored in the NoteStore index
///
/// This represents the indexed metadata for a single note. The actual content
/// lives in the plaintext markdown file at `path`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteRecord {
    /// Primary key: path to the plaintext file (relative to kiln root)
    pub path: String,

    /// BLAKE3 content hash (32 bytes) for change detection
    pub content_hash: BlockHash,

    /// Optional embedding vector (typically 768+ dimensions)
    ///
    /// `None` if the note hasn't been embedded yet or if embedding failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,

    /// Note title (from frontmatter or first heading)
    pub title: String,

    /// Tags from both inline tags and frontmatter
    #[serde(default)]
    pub tags: Vec<String>,

    /// Denormalized outlinks (paths this note links to)
    ///
    /// Used by [`GraphView`] to build the in-memory graph without
    /// querying the database.
    #[serde(default)]
    pub links_to: Vec<String>,

    /// Frontmatter properties (arbitrary key-value pairs)
    #[serde(default)]
    pub properties: HashMap<String, Value>,

    /// When this record was last updated
    pub updated_at: DateTime<Utc>,
}

impl NoteRecord {
    /// Create a new NoteRecord with minimal required fields
    pub fn new(path: impl Into<String>, content_hash: BlockHash) -> Self {
        Self {
            path: path.into(),
            content_hash,
            embedding: None,
            title: String::new(),
            tags: Vec::new(),
            links_to: Vec::new(),
            properties: HashMap::new(),
            updated_at: Utc::now(),
        }
    }

    /// Builder-style: set the title
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Builder-style: set tags
    #[must_use]
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Builder-style: set outlinks
    #[must_use]
    pub fn with_links(mut self, links: Vec<String>) -> Self {
        self.links_to = links;
        self
    }

    /// Builder-style: set embedding
    #[must_use]
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Builder-style: set properties
    #[must_use]
    pub fn with_properties(mut self, properties: HashMap<String, Value>) -> Self {
        self.properties = properties;
        self
    }

    /// Check if this note has an embedding
    pub fn has_embedding(&self) -> bool {
        self.embedding.is_some()
    }

    /// Get embedding dimensions if present
    pub fn embedding_dimensions(&self) -> Option<usize> {
        self.embedding.as_ref().map(Vec::len)
    }
}

impl Default for NoteRecord {
    fn default() -> Self {
        Self {
            path: String::new(),
            content_hash: BlockHash::zero(),
            embedding: None,
            title: String::new(),
            tags: Vec::new(),
            links_to: Vec::new(),
            properties: HashMap::new(),
            updated_at: Utc::now(),
        }
    }
}

/// A search result with score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// The matching note record
    pub note: NoteRecord,

    /// Similarity score (higher is more similar)
    ///
    /// Typically cosine similarity in range [0, 1] for normalized embeddings,
    /// but backends may use different scoring metrics.
    pub score: f32,
}

impl SearchResult {
    /// Create a new search result
    pub fn new(note: NoteRecord, score: f32) -> Self {
        Self { note, score }
    }
}

// ============================================================================
// Filter Types
// ============================================================================

/// Comparison operators for property filters
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    /// Equal
    Eq,
    /// Not equal
    Ne,
    /// Greater than
    Gt,
    /// Less than
    Lt,
    /// Greater than or equal
    Gte,
    /// Less than or equal
    Lte,
    /// Contains (for strings/arrays)
    Contains,
    /// Regex match (for strings)
    Matches,
}

impl Op {
    /// Check if this operator is a comparison operator
    pub fn is_comparison(&self) -> bool {
        matches!(self, Op::Gt | Op::Lt | Op::Gte | Op::Lte)
    }

    /// Check if this operator is a string operator
    pub fn is_string_op(&self) -> bool {
        matches!(self, Op::Contains | Op::Matches)
    }
}

/// Filter expressions for search queries
///
/// Filters can be combined with `And` and `Or` for complex queries.
///
/// # Examples
///
/// ```ignore
/// // Filter by tag
/// let filter = Filter::Tag("rust".to_string());
///
/// // Filter by path prefix
/// let filter = Filter::Path("projects/".to_string());
///
/// // Complex filter: tag AND property
/// let filter = Filter::And(vec![
///     Filter::Tag("rust".to_string()),
///     Filter::Property("status".to_string(), Op::Eq, json!("published")),
/// ]);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Filter {
    /// Filter by tag (exact match)
    Tag(String),

    /// Filter by path prefix
    Path(String),

    /// Filter by frontmatter property
    Property(String, Op, Value),

    /// Logical AND of multiple filters
    And(Vec<Filter>),

    /// Logical OR of multiple filters
    Or(Vec<Filter>),
}

impl Filter {
    /// Create a tag filter
    pub fn tag(tag: impl Into<String>) -> Self {
        Filter::Tag(tag.into())
    }

    /// Create a path prefix filter
    pub fn path(prefix: impl Into<String>) -> Self {
        Filter::Path(prefix.into())
    }

    /// Create a property equality filter
    pub fn property_eq(key: impl Into<String>, value: Value) -> Self {
        Filter::Property(key.into(), Op::Eq, value)
    }

    /// Create an AND filter
    pub fn and(filters: Vec<Filter>) -> Self {
        Filter::And(filters)
    }

    /// Create an OR filter
    pub fn or(filters: Vec<Filter>) -> Self {
        Filter::Or(filters)
    }

    /// Check if this is a compound filter
    pub fn is_compound(&self) -> bool {
        matches!(self, Filter::And(_) | Filter::Or(_))
    }
}

// ============================================================================
// NoteStore Trait
// ============================================================================

/// Storage index over plaintext files
///
/// This trait defines the interface for storing and querying note metadata.
/// Implementations may use SurrealDB, SQLite, or any other backing store.
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` to allow concurrent access.
///
/// # Error Handling
///
/// All operations return `StorageResult` which wraps `StorageError` for
/// consistent error handling across the crate.
#[async_trait]
pub trait NoteStore: Send + Sync {
    /// Insert or update a note record
    ///
    /// If a record with the same `path` exists, it will be replaced.
    async fn upsert(&self, note: NoteRecord) -> StorageResult<()>;

    /// Get a note record by path
    ///
    /// Returns `None` if the note doesn't exist.
    async fn get(&self, path: &str) -> StorageResult<Option<NoteRecord>>;

    /// Delete a note record by path
    ///
    /// This is idempotent: deleting a non-existent note succeeds.
    async fn delete(&self, path: &str) -> StorageResult<()>;

    /// List all note records
    ///
    /// For large kilns, consider using pagination or streaming.
    async fn list(&self) -> StorageResult<Vec<NoteRecord>>;

    /// Find a note by its content hash
    ///
    /// Useful for deduplication and detecting moved files.
    async fn get_by_hash(&self, hash: &BlockHash) -> StorageResult<Option<NoteRecord>>;

    /// Search notes by embedding similarity
    ///
    /// # Arguments
    ///
    /// * `embedding` - Query embedding vector
    /// * `k` - Maximum number of results to return
    /// * `filter` - Optional filter to narrow results
    ///
    /// # Returns
    ///
    /// Results sorted by descending similarity score.
    async fn search(
        &self,
        embedding: &[f32],
        k: usize,
        filter: Option<Filter>,
    ) -> StorageResult<Vec<SearchResult>>;
}

// ============================================================================
// GraphView Trait
// ============================================================================

/// In-memory graph built from denormalized links
///
/// This trait provides efficient graph traversal over the link structure
/// in a kiln. The graph is built from the `links_to` field of [`NoteRecord`]s
/// and must be rebuilt when notes change.
///
/// # Performance
///
/// All methods are synchronous and should be fast (O(1) or O(k) where k is
/// the number of neighbors). The `rebuild` method may be O(n) where n is
/// the number of notes.
pub trait GraphView: Send + Sync {
    /// Get all notes this note links to
    ///
    /// Returns paths of notes that this note directly links to.
    fn outlinks(&self, path: &str) -> Vec<String>;

    /// Get all notes that link to this note
    ///
    /// Returns paths of notes that contain links to this note.
    fn backlinks(&self, path: &str) -> Vec<String>;

    /// Get all neighbors within a given depth
    ///
    /// # Arguments
    ///
    /// * `path` - Starting note path
    /// * `depth` - Maximum link distance (1 = direct links only)
    ///
    /// # Returns
    ///
    /// Paths of all notes reachable within the given depth, not including
    /// the starting note.
    fn neighbors(&self, path: &str, depth: usize) -> Vec<String>;

    /// Rebuild the graph from note records
    ///
    /// This should be called after bulk updates to the NoteStore.
    fn rebuild(&mut self, notes: &[NoteRecord]);
}

// ============================================================================
// Precognition Trait
// ============================================================================

/// Pure computation for hashing and embedding
///
/// This trait encapsulates the computational operations needed before
/// storing a note: computing content hashes and generating embeddings.
///
/// # Design Rationale
///
/// Separating these operations into their own trait allows:
/// - Easy mocking for tests
/// - Swapping embedding backends without changing storage code
/// - Batch processing optimizations
#[async_trait]
pub trait Precognition: Send + Sync {
    /// Compute a BLAKE3 hash of content
    ///
    /// This is a pure, synchronous operation.
    fn hash(&self, content: &[u8]) -> BlockHash;

    /// Generate an embedding for text content
    ///
    /// This is typically an async operation that may call an external
    /// service (Ollama, OpenAI, etc.) or run a local model.
    ///
    /// # Errors
    ///
    /// Returns an error if embedding generation fails (network error,
    /// model not loaded, etc.).
    async fn embed(&self, content: &str) -> StorageResult<Vec<f32>>;
}

// ============================================================================
// Blanket Implementations
// ============================================================================

/// Blanket implementation of NoteStore for Arc<T>
#[async_trait]
impl<T: NoteStore + ?Sized> NoteStore for std::sync::Arc<T> {
    async fn upsert(&self, note: NoteRecord) -> StorageResult<()> {
        (**self).upsert(note).await
    }

    async fn get(&self, path: &str) -> StorageResult<Option<NoteRecord>> {
        (**self).get(path).await
    }

    async fn delete(&self, path: &str) -> StorageResult<()> {
        (**self).delete(path).await
    }

    async fn list(&self) -> StorageResult<Vec<NoteRecord>> {
        (**self).list().await
    }

    async fn get_by_hash(&self, hash: &BlockHash) -> StorageResult<Option<NoteRecord>> {
        (**self).get_by_hash(hash).await
    }

    async fn search(
        &self,
        embedding: &[f32],
        k: usize,
        filter: Option<Filter>,
    ) -> StorageResult<Vec<SearchResult>> {
        (**self).search(embedding, k, filter).await
    }
}

/// Blanket implementation of Precognition for Arc<T>
#[async_trait]
impl<T: Precognition + ?Sized> Precognition for std::sync::Arc<T> {
    fn hash(&self, content: &[u8]) -> BlockHash {
        (**self).hash(content)
    }

    async fn embed(&self, content: &str) -> StorageResult<Vec<f32>> {
        (**self).embed(content).await
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_note_record_builder() {
        let record = NoteRecord::new("test/note.md", BlockHash::zero())
            .with_title("Test Note")
            .with_tags(vec!["rust".to_string(), "test".to_string()])
            .with_links(vec!["other/note.md".to_string()]);

        assert_eq!(record.path, "test/note.md");
        assert_eq!(record.title, "Test Note");
        assert_eq!(record.tags.len(), 2);
        assert_eq!(record.links_to.len(), 1);
        assert!(!record.has_embedding());
    }

    #[test]
    fn test_note_record_with_embedding() {
        let embedding = vec![0.1, 0.2, 0.3];
        let record =
            NoteRecord::new("test/note.md", BlockHash::zero()).with_embedding(embedding.clone());

        assert!(record.has_embedding());
        assert_eq!(record.embedding_dimensions(), Some(3));
        assert_eq!(record.embedding, Some(embedding));
    }

    #[test]
    fn test_search_result() {
        let note = NoteRecord::new("test.md", BlockHash::zero());
        let result = SearchResult::new(note, 0.95);

        assert_eq!(result.score, 0.95);
        assert_eq!(result.note.path, "test.md");
    }

    #[test]
    fn test_filter_constructors() {
        let tag_filter = Filter::tag("rust");
        assert!(matches!(tag_filter, Filter::Tag(t) if t == "rust"));

        let path_filter = Filter::path("projects/");
        assert!(matches!(path_filter, Filter::Path(p) if p == "projects/"));

        let prop_filter = Filter::property_eq("status", Value::String("published".to_string()));
        assert!(matches!(prop_filter, Filter::Property(k, Op::Eq, _) if k == "status"));

        let compound = Filter::and(vec![Filter::tag("a"), Filter::tag("b")]);
        assert!(compound.is_compound());
    }

    #[test]
    fn test_op_classification() {
        assert!(Op::Gt.is_comparison());
        assert!(Op::Lt.is_comparison());
        assert!(Op::Gte.is_comparison());
        assert!(Op::Lte.is_comparison());
        assert!(!Op::Eq.is_comparison());

        assert!(Op::Contains.is_string_op());
        assert!(Op::Matches.is_string_op());
        assert!(!Op::Eq.is_string_op());
    }

    #[test]
    fn test_note_record_serialization() {
        let record = NoteRecord::new("test.md", BlockHash::zero())
            .with_title("Test")
            .with_tags(vec!["a".to_string()]);

        let json = serde_json::to_string(&record).expect("serialize");
        let deserialized: NoteRecord = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.path, record.path);
        assert_eq!(deserialized.title, record.title);
        assert_eq!(deserialized.tags, record.tags);
    }

    #[test]
    fn test_filter_serialization() {
        let filter = Filter::And(vec![
            Filter::Tag("rust".to_string()),
            Filter::Property("status".to_string(), Op::Eq, Value::String("draft".to_string())),
        ]);

        let json = serde_json::to_string(&filter).expect("serialize");
        let deserialized: Filter = serde_json::from_str(&json).expect("deserialize");

        assert!(matches!(deserialized, Filter::And(filters) if filters.len() == 2));
    }
}
