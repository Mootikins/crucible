//! SQLite storage backend for Crucible
//!
//! This crate provides a SQLite-based implementation of Crucible's storage traits,
//! offering a lightweight alternative to the SurrealDB backend.
//!
//! ## Features
//!
//! - **NoteStore**: Unified note metadata and vector search storage
//! - **Graph Queries**: Full pipeline support for jaq, SQL sugar, and PGQ MATCH syntax
//! - **FTS5 Full-Text Search**: Built-in full-text search using SQLite's FTS5 extension
//! - **WAL Mode**: Optimized for concurrent read access with write-ahead logging
//! - **Thread Safety**: Arc<Mutex<Connection>> pattern for concurrent access
//!
//! ## Graph Query Syntaxes
//!
//! ```ignore
//! // jaq-style (like jq)
//! executor.execute(r#"outlinks("Index")"#).await?;
//! executor.execute(r#"inlinks("Index")"#).await?;
//!
//! // SQL sugar
//! executor.execute("SELECT outlinks FROM 'Index'").await?;
//! executor.execute("SELECT inlinks FROM 'Index'").await?;
//!
//! // PGQ MATCH (SQL:2023 graph pattern matching)
//! executor.execute("MATCH (a {title: 'Index'})-[:wikilink]->(b)").await?;
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_sqlite::{SqliteConfig, SqlitePool};
//! use crucible_sqlite::create_note_store;
//! use crucible_core::storage::NoteStore;
//!
//! let pool = SqlitePool::new(SqliteConfig::new("./crucible.db"))?;
//! let store = create_note_store(pool)?;
//!
//! // Use via the unified NoteStore trait
//! let note = store.get("notes/example.md").await?;
//! ```

pub mod config;
pub mod connection;
pub mod error;
pub mod fts;
pub mod graph_query;
pub mod graph_view;
pub mod note_store;
pub mod repository;
pub mod schema;

// Re-exports
pub use config::SqliteConfig;
pub use connection::SqlitePool;
pub use error::{SqliteError, SqliteResult};
pub use fts::{FtsIndex, FtsResult};
pub use graph_query::SqliteGraphQueryExecutor;
pub use graph_view::SqliteGraphView;
pub use note_store::{create_note_store, SqliteNoteStore};
pub use repository::{create_knowledge_repository, SqliteKnowledgeRepository};
