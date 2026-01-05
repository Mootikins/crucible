//! Storage Module
//!
//! This module provides storage abstractions and implementations for the Crucible system.
//!
//! ## Key Components
//!
//! - **NoteStore**: Unified note metadata and vector search storage
//! - **GraphView**: In-memory graph from denormalized links
//! - **Precognition**: Pure computation (hash + embed)
//!
//! ## Architecture
//!
//! The system follows a dependency inversion pattern where business logic depends on
//! trait abstractions rather than concrete implementations. This enables:
//! - Comprehensive unit testing with mock implementations
//! - Multiple storage backends (SurrealDB, SQLite, in-memory)
//! - Clean separation of concerns

pub mod error;
pub mod graph;
pub mod note_store;
pub mod traits;

// Re-export main types for convenience
pub use error::{StorageError, StorageResult};
pub use graph::InMemoryGraph;
pub use note_store::{Filter, GraphView, NoteRecord, NoteStore, Op, Precognition, SearchResult};
pub use traits::{ContentHasher, QuotaUsage, StorageBackend, StorageStats};
