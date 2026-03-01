//! Storage Module
//!
//! This module provides storage abstractions and implementations for the Crucible system.
//!
//! ## Key Components
//!
//! - **NoteStore**: Unified note metadata and vector search storage
//! - **GraphView**: In-memory graph from denormalized links
//! - **Pure computation**: Hash and embedding generation
//!
//! ## Architecture
//!
//! The system follows a dependency inversion pattern where business logic depends on
//! trait abstractions rather than concrete implementations. This enables:
//! - Comprehensive unit testing with mock implementations
//! - Multiple storage backends (SQLite, in-memory)
//! - Clean separation of concerns

pub mod error;
pub mod error_ext;
pub mod graph;
pub mod note_store;
pub mod traits;

// Re-export main types for convenience
pub use error::{StorageError, StorageResult};
pub use error_ext::StorageResultExt;
pub use graph::InMemoryGraph;
pub use note_store::{Filter, GraphView, NoteRecord, NoteStore, Op, SearchResult};
pub use traits::{ContentHasher, QuotaUsage, StorageBackend, StorageStats};
