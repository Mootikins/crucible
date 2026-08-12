//! SQLite storage backend for Crucible
//!
//! This crate provides a SQLite-based implementation of Crucible's storage traits,
//! offering a lightweight default storage backend.
//!
//! ## Features
//!
//! - **NoteStore**: Unified note metadata and vector search storage
//! - **FTS5 Full-Text Search**: Built-in full-text search using SQLite's FTS5 extension
//! - **WAL Mode**: Optimized for concurrent read access with write-ahead logging
//! - **Thread Safety**: Arc<Mutex<Connection>> pattern for concurrent access
//!
//! There is no graph query language here. This module doc used to advertise
//! "Full pipeline support for jaq, SQL sugar, and PGQ MATCH syntax" over a
//! `query/` subtree that no caller ever reached and whose renderer targeted
//! tables Crucible has never had (an `edges` table; `entities.path`), so its
//! own golden SQL could not be prepared. Both the subtree and the claim were
//! removed together — see [[2026-08-11-dead-code-and-schema-migrations]] and
//! `docs/Help/Query/Index.md`. Graph reads go through
//! `NoteStore::graph_links`/`backlinks` over `note_links` instead.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_daemon::storage::sqlite::{SqliteConfig, SqlitePool};
//! use crucible_daemon::storage::sqlite::SqliteNoteStore;
//! use crucible_core::storage::NoteStore;
//!
//! let pool = SqlitePool::new(SqliteConfig::new("./crucible.db"))?;
//! let store = SqliteNoteStore::new(pool);
//!
//! // Use via the unified NoteStore trait
//! let note = store.get("notes/example.md").await?;
//! ```

pub mod adapters;
pub mod config;
pub mod connection;
mod error_ext;
pub mod fts;
pub(crate) mod link_index;
pub mod note_store;
pub mod property_store;
pub mod repository;
pub mod schema;

// Re-exports
pub use adapters::{create_sqlite_client, SqliteClientHandle};
pub use config::SqliteConfig;
pub use connection::SqlitePool;
pub use crucible_core::storage::StorageResult as SqliteResult;
pub use fts::{FtsIndex, FtsResult};
pub use note_store::SqliteNoteStore;
pub use property_store::SqlitePropertyStore;
pub use repository::{
    create_knowledge_repository, create_knowledge_repository_with_kiln, SqliteKnowledgeRepository,
};
