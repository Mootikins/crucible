//! Enriched Note Storage Trait
//!
//! Defines the contract for storing enriched notes following the Dependency
//! Inversion Principle.

use super::types::EnrichedNote;
use anyhow::Result;

/// Trait for storing enriched notes
///
/// This trait defines the contract for persisting enriched notes to storage.
/// Implementations are provided in the infrastructure layer (crucible-surrealdb).
///
/// ## Dependency Inversion
///
/// By defining this trait in the core domain layer, we ensure that:
/// - High-level modules (pipeline) don't depend on low-level modules (surrealdb)
/// - Both depend on abstractions (this trait)
/// - Easy to swap implementations or add new storage backends
///
/// ## Example
///
/// ```rust,ignore
/// use crucible_core::enrichment::{EnrichedNoteStore, EnrichedNote};
/// use std::sync::Arc;
///
/// async fn store_note(
///     storage: Arc<dyn EnrichedNoteStore>,
///     enriched: &EnrichedNote,
///     path: &str,
/// ) -> Result<()> {
///     storage.store_enriched(enriched, path).await
/// }
/// ```
#[async_trait::async_trait]
pub trait EnrichedNoteStore: Send + Sync {
    /// Store an enriched note with all its associated data
    ///
    /// This method persists:
    /// - Parsed note content (AST, metadata, links, etc.)
    /// - Merkle tree (for change detection)
    /// - Vector embeddings (for semantic search)
    /// - Enrichment metadata (reading time, complexity, language)
    /// - Inferred relations (semantic similarity, etc.)
    ///
    /// # Arguments
    ///
    /// * `enriched` - The enriched note to store
    /// * `relative_path` - Relative path within the vault/kiln
    ///
    /// # Returns
    ///
    /// - `Ok(())` on successful storage
    /// - `Err(...)` if storage fails
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Database connection fails
    /// - Transaction fails
    /// - Validation fails
    ///
    /// The implementation should handle partial failures gracefully where possible
    /// and ensure transactional consistency.
    async fn store_enriched(
        &self,
        enriched: &EnrichedNote,
        relative_path: &str,
    ) -> Result<()>;

    /// Check if a note exists in storage
    ///
    /// This is useful for determining whether to create or update a note.
    ///
    /// # Arguments
    ///
    /// * `relative_path` - Relative path within the vault/kiln
    ///
    /// # Returns
    ///
    /// - `Ok(true)` if the note exists
    /// - `Ok(false)` if the note doesn't exist
    /// - `Err(...)` if the check fails
    async fn note_exists(&self, relative_path: &str) -> Result<bool> {
        // Default implementation returns false (conservative)
        // Implementations should override this for better performance
        let _ = relative_path;
        Ok(false)
    }
}
