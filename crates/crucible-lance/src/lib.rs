//! LanceDB integration for Crucible lightweight storage mode
//!
//! Provides embedding cache, vector search, and NoteStore implementation
//! backed by LanceDB - a vector-native database optimized for embedding search.
//!
//! # Features
//!
//! - **NoteStore implementation**: Full CRUD operations for note metadata
//! - **Native vector search**: Uses LanceDB's optimized vector similarity search
//! - **Embedding cache**: Incremental embedding by caching previously generated vectors
//!
//! # Example
//!
//! ```rust,ignore
//! use crucible_lance::note_store::{LanceNoteStore, create_note_store};
//! use crucible_core::storage::NoteStore;
//!
//! // Create a new store
//! let store = create_note_store("/path/to/lance.db").await?;
//!
//! // Use via the NoteStore trait
//! let note = store.get("notes/example.md").await?;
//! let results = store.search(&query_embedding, 10, None).await?;
//! ```

pub mod embedding_cache;
pub mod error;
pub mod note_store;
pub mod store;
pub mod vector_search;

// Re-export main types for convenience
pub use error::{LanceError, LanceResult};
pub use note_store::{create_note_store, create_note_store_with_dimensions, LanceNoteStore};
pub use store::LanceStore;
pub use vector_search::VectorSearchResult;
