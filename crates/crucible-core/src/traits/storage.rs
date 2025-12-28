//! Storage abstraction trait for database operations
//!
//! This trait defines query and schema operations for storage backends.
//!
//! # Relationship to Other Storage Traits
//!
//! Crucible has multiple storage-related traits organized by purpose:
//!
//! ## Database Operations (This Module)
//!
//! - **`Storage`** - Database queries, statistics, and schema management
//!   - `query()` - Execute database queries (SurrealQL, SQL, etc.)
//!   - `get_stats()` - Database statistics and metadata
//!   - `list_tables()` - Introspection and autocomplete
//!   - `initialize_schema()` - Setup tables and indexes
//!
//! ## Content-Addressed Storage (`crate::storage::traits`)
//!
//! - **`ContentAddressedStorage`** - Block and Merkle tree operations
//! - **`BlockOperations`** - Individual content blocks
//! - **`TreeOperations`** - Merkle tree structures
//! - **`StorageManagement`** - Maintenance and lifecycle
//!
//! ## Semantic Knowledge Operations (`crate::traits::knowledge`)
//!
//! - **`KnowledgeRepository`** - High-level note operations
//!   - `get_note_by_name()` - Retrieve parsed notes
//!   - `list_notes()` - Browse and filter notes
//!   - `search_vectors()` - Semantic search with embeddings
//!
//! # Usage Guidance
//!
//! **When to use `Storage` (this trait):**
//! - Executing raw database queries
//! - Getting database statistics and metadata
//! - Initializing database schema
//! - Working directly with the database backend
//!
//! **When to use `ContentAddressedStorage`:**
//! - Storing and retrieving content blocks
//! - Computing and comparing Merkle trees
//! - Change detection and incremental updates
//!
//! **When to use `KnowledgeRepository`:**
//! - High-level note operations in agents/tools
//! - Semantic search and retrieval
//! - Working with parsed notes and metadata

use async_trait::async_trait;
use std::collections::HashMap;

// Re-export database types to avoid duplication
// These are the canonical definitions for database operations
pub use crate::database::{DbError, DbResult, QueryResult, Record, RecordId};

// Alias for backward compatibility - prefer DbError/DbResult in new code
pub type StorageError = DbError;
pub type StorageResult<T> = DbResult<T>;

/// Unified storage abstraction
///
/// This trait defines the essential operations that CrucibleCore needs from its storage layer.
/// It combines relational, graph, and note concerns into the methods that Core actually uses.
///
/// ## Design Rationale
///
/// Rather than having Core depend on three separate traits (RelationalDB, GraphDB, DocumentDB),
/// this unified trait captures the actual usage patterns from CrucibleCore:
/// - `query()` - Execute queries (currently used for raw SurrealDB queries)
/// - `get_stats()` - Get database statistics (used for dashboard/status)
/// - `list_tables()` - List available tables (used for autocomplete)
/// - `initialize_schema()` - Set up database schema (used at startup)
///
/// ## Thread Safety
///
/// Implementations must be Send + Sync to enable use across async boundaries.
#[async_trait]
pub trait Storage: Send + Sync {
    /// Execute a query and return results
    ///
    /// This is the primary query interface. The query format depends on the underlying
    /// storage implementation (e.g., SurrealQL for SurrealDB).
    ///
    /// # Arguments
    ///
    /// * `query` - The query string to execute
    /// * `params` - Optional query parameters for parameterized queries
    ///
    /// # Returns
    ///
    /// Returns a `QueryResult` containing records and metadata, or a `StorageError`.
    async fn query(
        &self,
        query: &str,
        params: &[(&str, serde_json::Value)],
    ) -> StorageResult<QueryResult>;

    /// Get database statistics
    ///
    /// Returns statistics about the database state, including:
    /// - Table counts
    /// - Database type/version
    /// - Connection information
    ///
    /// # Returns
    ///
    /// Returns a map of statistic names to values, or a `StorageError`.
    async fn get_stats(&self) -> StorageResult<HashMap<String, serde_json::Value>>;

    /// List all tables in the database
    ///
    /// Used for autocomplete, introspection, and validation.
    ///
    /// # Returns
    ///
    /// Returns a vector of table names, or a `StorageError`.
    async fn list_tables(&self) -> StorageResult<Vec<String>>;

    /// Initialize database schema
    ///
    /// Sets up the required tables, indexes, and constraints for Crucible.
    /// This should be idempotent - safe to call multiple times.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or a `StorageError`.
    async fn initialize_schema(&self) -> StorageResult<()>;
}

// Note: QueryResult, Record, and RecordId are now re-exported from database.rs
// Tests for those types are in database.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_error_is_db_error() {
        // Verify that StorageError is an alias for DbError
        let err: StorageError = DbError::NotFound("test".to_string());
        assert!(matches!(err, DbError::NotFound(_)));
    }
}
