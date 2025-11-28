//! Factory Functions and Adapters for Trait Compatibility
//!
//! This module provides the public API for creating SurrealDB-backed implementations
//! of core traits. It enforces the Dependency Inversion Principle by hiding concrete
//! types and exposing only trait objects.
//!
//! ## Architecture (SOLID Phase 5)
//!
//! - Concrete types (SurrealClient, EAVGraphStore, etc.) are private to the crate
//! - This module provides factory functions that return trait objects
//! - CLI code depends on abstractions, not implementations
//!
//! ## Usage
//!
//! ```rust,no_run
//! use crucible_surrealdb::adapters;
//! use crucible_surrealdb::SurrealDbConfig;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = SurrealDbConfig {
//!         path: "./cache.db".to_string(),
//!         namespace: "crucible".to_string(),
//!         database: "kiln".to_string(),
//!         max_connections: Some(10),
//!         timeout_seconds: Some(30),
//!     };
//!
//!     // Create client (opaque handle)
//!     let client = adapters::create_surreal_client(config).await?;
//!
//!     // Create trait objects
//!     let enriched_store = adapters::create_enriched_note_store(client.clone());
//!     let merkle_store = adapters::create_merkle_store(client.clone());
//!
//!     Ok(())
//! }
//! ```

use crate::{EAVGraphStore, MerklePersistence, NoteIngestor, SurrealClient, SurrealDbConfig};
use anyhow::Result;
use async_trait::async_trait;
use crucible_core::enrichment::{EnrichedNote, EnrichedNoteStore};
use crucible_merkle::MerkleStore;
use std::sync::Arc;

// ============================================================================
// Opaque Handle for SurrealDB Client
// ============================================================================

/// Opaque handle to a SurrealDB client.
///
/// This type is intentionally opaque - you cannot access the underlying SurrealClient.
/// Use the factory functions in this module to create trait objects that depend on it.
///
/// # Cloning
///
/// This handle is cheap to clone (Arc-wrapped internally).
#[derive(Clone)]
pub struct SurrealClientHandle {
    client: Arc<SurrealClient>,
}

impl SurrealClientHandle {
    /// Create a handle from a SurrealClient (internal use only)
    pub(crate) fn new(client: SurrealClient) -> Self {
        Self {
            client: Arc::new(client),
        }
    }

    /// Get the inner client for special operations (kiln integration, etc.)
    ///
    /// This is exposed publicly but should be used sparingly. Most operations
    /// should go through the factory functions in this module.
    pub fn inner(&self) -> &SurrealClient {
        &self.client
    }

    /// Get an Arc to the inner client (internal use only)
    pub(crate) fn inner_arc(&self) -> Arc<SurrealClient> {
        Arc::clone(&self.client)
    }

    /// Get a trait object for KnowledgeRepository
    ///
    /// This is needed when you need to pass the client as a trait object
    /// to code that depends on the trait, not the implementation.
    pub fn as_knowledge_repository(&self) -> Arc<dyn crucible_core::traits::KnowledgeRepository> {
        Arc::clone(&self.client) as Arc<dyn crucible_core::traits::KnowledgeRepository>
    }
}

// ============================================================================
// Factory Functions - Public API
// ============================================================================

/// Create a SurrealDB client from configuration.
///
/// This is the main entry point for creating a database connection.
///
/// # Returns
///
/// An opaque handle that can be passed to other factory functions.
pub async fn create_surreal_client(config: SurrealDbConfig) -> Result<SurrealClientHandle> {
    let client = SurrealClient::new(config).await?;
    Ok(SurrealClientHandle::new(client))
}

/// Create an enriched note store backed by SurrealDB.
///
/// # Arguments
///
/// * `client` - Opaque handle to a SurrealDB client
///
/// # Returns
///
/// A trait object implementing `EnrichedNoteStore`
pub fn create_enriched_note_store(client: SurrealClientHandle) -> Arc<dyn EnrichedNoteStore> {
    Arc::new(EnrichedNoteStoreAdapter::new(client.inner_arc()))
}

/// Create a Merkle tree store backed by SurrealDB.
///
/// # Arguments
///
/// * `client` - Opaque handle to a SurrealDB client
///
/// # Returns
///
/// A trait object implementing `MerkleStore`
pub fn create_merkle_store(client: SurrealClientHandle) -> Arc<dyn MerkleStore> {
    Arc::new(MerklePersistence::new((*client.inner()).clone()))
}

/// Create a SurrealDB-backed change detection store
///
/// Returns a trait object implementing `ChangeDetectionStore` for file state tracking.
/// This is used by the pipeline's Phase 1 quick filter to avoid reprocessing unchanged files.
///
/// # Arguments
///
/// * `client` - SurrealDB client handle
///
/// # Returns
///
/// A trait object implementing `ChangeDetectionStore`
pub fn create_change_detection_store(
    client: SurrealClientHandle,
) -> Arc<dyn crucible_core::processing::ChangeDetectionStore> {
    Arc::new(
        crate::change_detection_store::SurrealChangeDetectionStore::new((*client.inner()).clone()),
    )
}

// ============================================================================
// Adapter Implementations
// ============================================================================

/// Adapter that owns EAVGraphStore and implements EnrichedNoteStore.
///
/// This solves the lifetime issue with NoteIngestor<'a> by owning the store
/// and creating NoteIngestor instances on-demand for each operation.
///
/// # Architecture
///
/// The adapter pattern is used here because `NoteIngestor<'a>` has a lifetime
/// parameter and cannot be directly wrapped in `Arc<dyn Trait>`. By owning
/// the `EAVGraphStore`, we can create short-lived `NoteIngestor` instances
/// that borrow from our owned store.
struct EnrichedNoteStoreAdapter {
    store: EAVGraphStore,
}

impl EnrichedNoteStoreAdapter {
    /// Create a new adapter with the given SurrealDB client.
    ///
    /// # Note
    ///
    /// SurrealClient is cheap to clone (it's Arc-wrapped internally), so we
    /// can safely clone it here.
    fn new(client: Arc<SurrealClient>) -> Self {
        Self {
            store: EAVGraphStore::new((*client).clone()),
        }
    }
}

#[async_trait]
impl EnrichedNoteStore for EnrichedNoteStoreAdapter {
    async fn store_enriched(&self, enriched: &EnrichedNote, relative_path: &str) -> Result<()> {
        // Create NoteIngestor on-demand (borrows from self.store)
        let ingestor = NoteIngestor::new(&self.store);
        ingestor.store_enriched(enriched, relative_path).await
    }

    async fn note_exists(&self, relative_path: &str) -> Result<bool> {
        // Create NoteIngestor on-demand (borrows from self.store)
        let ingestor = NoteIngestor::new(&self.store);
        ingestor.note_exists(relative_path).await
    }
}
