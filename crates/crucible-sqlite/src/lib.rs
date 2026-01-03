//! SQLite storage backend for Crucible
//!
//! This crate provides a SQLite-based implementation of Crucible's storage traits,
//! offering a lightweight alternative to the SurrealDB backend.
//!
//! ## Features
//!
//! - **EAV+Graph Storage**: Full implementation of entity, property, relation, tag, and block storage
//! - **FTS5 Full-Text Search**: Built-in full-text search using SQLite's FTS5 extension
//! - **WAL Mode**: Optimized for concurrent read access with write-ahead logging
//! - **Thread Safety**: Arc<Mutex<Connection>> pattern for concurrent access
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_sqlite::{SqliteConfig, SqlitePool};
//! use crucible_sqlite::eav::EavGraphStore;
//! use crucible_core::storage::EavGraphStorage;
//!
//! let pool = SqlitePool::new(SqliteConfig::new("./crucible.db"))?;
//! let storage = EavGraphStore::new(pool);
//!
//! // Use via the unified EavGraphStorage trait
//! let entity = storage.get_entity("note:example").await?;
//! ```

pub mod config;
pub mod connection;
pub mod eav;
pub mod error;
pub mod schema;

// Re-exports
pub use config::SqliteConfig;
pub use connection::SqlitePool;
pub use eav::EavGraphStore;
pub use error::{SqliteError, SqliteResult};
