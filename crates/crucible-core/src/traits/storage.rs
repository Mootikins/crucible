//! Storage abstraction trait
//!
//! This trait combines the concerns of RelationalDB, GraphDB, and DocumentDB
//! into a unified Storage interface that matches what CrucibleCore actually needs.
//!
//! The original three traits (RelationalDB, GraphDB, DocumentDB) remain in
//! `database.rs` as implementation references, but this trait is what Core uses.
//!
//! ## Type Ownership
//!
//! This module re-exports types from `database.rs` to avoid duplication:
//! - `DbError` is the canonical error type for database operations
//! - `QueryResult`, `Record`, `RecordId` are re-exported from database.rs

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
