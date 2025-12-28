//! Knowledge Repository trait for semantic note operations
//!
//! This trait provides high-level operations for working with knowledge stored in the kiln.
//!
//! # Purpose
//!
//! `KnowledgeRepository` decouples agents and tools from storage implementation details,
//! enabling:
//! - Testing without a full database backend
//! - Future storage backend changes (SurrealDB → something else)
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
//!   - `list_notes()` - Browse notes with filtering
//!   - `search_vectors()` - Semantic search with embeddings
//!
//! ## Mid-Level: Database Operations
//!
//! - **`crate::traits::storage::Storage`** - Database queries
//!   - Raw SurrealQL/SQL queries
//!   - Statistics and metadata
//!   - Schema management
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
//! `SurrealClient` implements `KnowledgeRepository`, providing the primary
//! implementation used throughout Crucible.

use crate::parser::ParsedNote;
use crate::types::SearchResult;
use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// File-level information about a note
///
/// Contains basic file metadata like name, path, and timestamps.
/// For computed enrichment metadata (reading time, complexity), see
/// `crucible_core::enrichment::types::EnrichmentMetadata`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteInfo {
    pub name: String,
    pub path: String,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Abstract interface for accessing knowledge in the kiln
///
/// This trait decouples the tool system from the specific storage backend (SurrealDB),
/// allowing tools to be tested in isolation and supporting future backend changes.
#[async_trait]
pub trait KnowledgeRepository: Send + Sync {
    /// Retrieve a note by its name or wikilink target
    async fn get_note_by_name(&self, name: &str) -> Result<Option<ParsedNote>>;

    /// List notes, optionally filtered by a directory path
    async fn list_notes(&self, path: Option<&str>) -> Result<Vec<NoteInfo>>;

    /// Search for notes using vector embeddings
    async fn search_vectors(&self, _vector: Vec<f32>) -> Result<Vec<SearchResult>> {
        // Default implementation returns empty if not supported
        Ok(Vec::new())
    }
}
