use crate::parser::ParsedNote;
use crate::types::SearchResult;
use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Metadata about a note
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteMetadata {
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
    async fn list_notes(&self, path: Option<&str>) -> Result<Vec<NoteMetadata>>;

    /// Search for notes using vector embeddings
    async fn search_vectors(&self, _vector: Vec<f32>) -> Result<Vec<SearchResult>> {
        // Default implementation returns empty if not supported
        Ok(Vec::new())
    }
}
