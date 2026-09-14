//! Knowledge Repository trait for semantic note operations
//!
//! This trait provides high-level operations for working with knowledge stored in the kiln.
//!
//! # Purpose
//!
//! `KnowledgeRepository` decouples agents and tools from storage implementation details,
//! enabling:
//! - Testing without a full database backend
//! - Future storage backend changes (SQLite → something else)
//! - Consistent API across different storage mechanisms
//!
//! # Relationship to Other Storage Traits
//!
//! Crucible has multiple storage-related traits organized by abstraction level:
//!
//! ## High-Level: Knowledge Operations (This Module)
//!
//! - **`KnowledgeRepository`** - Semantic note operations
//!   - `get_note_by_name()` - Retrieve parsed notes by name/wikilink
//!   - `get_note_by_path()` - Read one index row by exact path
//!   - `list_notes()` - Browse notes with filtering
//!   - `list_note_records()` - Every index row the authority can read
//!   - `links_for_note()` - The resolved links of one note, both directions
//!   - `search_vectors()` - Semantic search with embeddings
//!
//! ## Mid-Level: Database Operations
//!
//!
//! ## Low-Level: Content-Addressed Storage
//!
//! - **`crate::storage::ContentAddressedStorage`** - Blocks and trees
//!   - Content-addressed block storage
//!   - Merkle tree operations
//!   - Change detection
//!
//! # Usage Guidance
//!
//! **When to use `KnowledgeRepository`:**
//! - **Agents and tools** - High-level note operations without database details
//! - **Semantic search** - Finding relevant notes using embeddings
//! - **Tests** - Mock implementations for deterministic behavior
//! - **Cross-cutting concerns** - Code that works with notes but doesn't care about storage
//!
//! **When to use lower-level traits:**
//! - **`Storage`** - Need raw database queries or schema management
//! - - **`ContentAddressedStorage`** - Need Merkle trees or change detection
//!
//! # Implementation Notes
//!
//! Storage backends implement `KnowledgeRepository`, providing the primary
//! interface used throughout Crucible.

use crate::parser::ParsedNote;
use crate::storage::BlockRecord;
use crate::types::SearchResult;
use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// File-level information about a note
///
/// Contains basic file metadata like name, path, and timestamps.
/// For computed enrichment metadata (reading time, complexity), see
/// `crate::enrichment::types::EnrichmentMetadata`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteInfo {
    pub name: String,
    pub path: String,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The note's own frontmatter, filtered through
    /// [`crate::storage::note_store::public_properties`] — what the author
    /// wrote, never what the daemon stamped. It is what lets a client filter,
    /// sort or group notes without asking a plugin to do it.
    #[serde(default)]
    pub properties: std::collections::BTreeMap<String, serde_json::Value>,
}

/// The listing view of a stored note. `name` is the file stem, or the whole
/// path when the stem is not UTF-8.
impl From<crate::storage::note_store::NoteRecord> for NoteInfo {
    fn from(record: crate::storage::note_store::NoteRecord) -> Self {
        let name = std::path::Path::new(&record.path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&record.path)
            .to_string();
        // THE boundary: a stored record becomes the listing view here and
        // nowhere else, so the daemon's own stamps are dropped here and no
        // route can forget to.
        let properties = crate::storage::note_store::public_properties(&record.properties);
        Self {
            name,
            path: record.path,
            title: Some(record.title),
            tags: record.tags,
            created_at: None,
            updated_at: Some(record.updated_at),
            properties,
        }
    }
}

/// The resolved links of one note, both directions, as note paths.
///
/// Dangling targets are not here: they name no note, so nothing can follow
/// them. Both lists are sorted and deduplicated.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoteLinks {
    /// The notes this note links to.
    pub outlinks: Vec<String>,
    /// The notes that link to this note.
    pub backlinks: Vec<String>,
}

/// Abstract interface for accessing knowledge in the kiln
///
/// This trait decouples the tool system from the specific storage backend (SQLite),
/// allowing tools to be tested in isolation and supporting future backend changes.
#[async_trait]
pub trait KnowledgeRepository: Send + Sync {
    /// Retrieve a note by its name or wikilink target
    async fn get_note_by_name(&self, name: &str) -> Result<Option<ParsedNote>>;

    /// The index row for one kiln-relative path, exactly as the indexer
    /// stored it. `None` means the index has no row, so the caller reads the
    /// file instead. Unlike [`Self::get_note_by_name`] this is an exact match.
    async fn get_note_by_path(
        &self,
        path: &str,
    ) -> Result<Option<crate::storage::note_store::NoteRecord>>;

    /// List notes, optionally filtered by a directory path
    async fn list_notes(&self, path: Option<&str>) -> Result<Vec<NoteInfo>>;

    /// Search for notes using vector embeddings, returning at most `limit`
    /// hits ranked by similarity descending.
    async fn search_vectors(&self, vector: Vec<f32>, limit: usize) -> Result<Vec<SearchResult>>;

    /// Search at block granularity: each hit names a passage inside a note,
    /// with its byte span and its own text as the snippet.
    ///
    /// Required rather than defaulted, so a repository cannot silently answer
    /// nothing here while answering `search_vectors`. An implementation with
    /// no block store returns an empty vector, and the caller falls back.
    async fn search_blocks(&self, vector: Vec<f32>, limit: usize) -> Result<Vec<SearchResult>>;

    /// Every stored block of one note, in span order, vectors included.
    ///
    /// A retrieval strategy reads a hit's neighbours here. Required for the
    /// same reason as [`Self::search_blocks`]: a repository with no block
    /// store answers an empty vector, and says so in its own body.
    async fn blocks_for_note(&self, path: &str) -> Result<Vec<BlockRecord>>;

    /// Every index row the repository's authority can read, as the indexer
    /// stored it. The graph view of a kiln starts here: a caller pairs it
    /// with [`Self::links_for_note`] to build an adjacency.
    async fn list_note_records(&self) -> Result<Vec<crate::storage::note_store::NoteRecord>>;

    /// The resolved links of one note, filtered to what the authority can
    /// read. A path the authority cannot read answers with empty lists, the
    /// same as a path with no note.
    async fn links_for_note(&self, path: &str) -> Result<NoteLinks>;
}
