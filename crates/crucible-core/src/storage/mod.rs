//! Storage Module
//!
//! This module provides storage abstractions and implementations for the Crucible system.
//!
//! ## Key Components
//!
//! - **NoteStore**: Unified note metadata and vector search storage
//! - **BlockStore**: Block-granularity vectors, so retrieval can name a passage
//! - **PropertyStore**: Note property storage
//!
//! ## Architecture
//!
//! The system follows a dependency inversion pattern where business logic depends on
//! trait abstractions rather than concrete implementations. This enables:
//! - Comprehensive unit testing with mock implementations
//! - Multiple storage backends (SQLite, in-memory)
//! - Clean separation of concerns

pub mod block_store;
pub mod error;
pub mod error_ext;
pub mod note_store;
pub mod property_store;
pub mod scope;
pub mod scoped_links;

// Re-export main types for convenience
pub use block_store::{BlockHit, BlockRecord, BlockStore, CachedVector};
pub use error::{StorageError, StorageResult};
pub use error_ext::StorageResultExt;
pub use note_store::{
    Filter, GraphLink, InboundLink, LinkOccurrence, NoteRecord, NoteStore, Op, SearchResult,
};
pub use property_store::PropertyStore;
pub use scope::{Scope, ScopeError};
pub use scoped_links::{scoped_backlinks, scoped_outlinks, sorted_unique, visible_paths};
