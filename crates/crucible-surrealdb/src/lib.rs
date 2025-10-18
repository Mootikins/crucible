//! # Crucible SurrealDB Backend
//!
//! This crate provides a SurrealDB backend implementation for the Crucible knowledge
//! management system. It implements the same interface as the DuckDB backend but
//! uses SurrealDB with RocksDB storage for enhanced features and performance.
//!
//! ## Features
//!
//! - **Native Vector Storage**: Efficient storage of embeddings as arrays
//! - **Graph Relations**: Support for document relationships
//! - **ACID Transactions**: Full transaction support
//! - **Live Queries**: Real-time cache updates
//! - **Schema Flexibility**: Dynamic fields without migrations
//!
//! ## Usage
//!
//! ```rust,no_run
//! use crucible_surrealdb::SurrealEmbeddingDatabase;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let db = SurrealEmbeddingDatabase::new("./cache.db").await?;
//!     db.initialize().await?;
//!
//!     // Use the database...
//!
//!     Ok(())
//! }
//! ```

pub mod database;
pub mod multi_client;
pub mod types;

pub use database::SurrealEmbeddingDatabase;
pub use multi_client::SurrealClient;
pub use types::*;

