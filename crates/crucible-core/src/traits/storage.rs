//! Storage abstraction trait
//!
//! This trait combines the concerns of RelationalDB, GraphDB, and DocumentDB
//! into a unified Storage interface that matches what CrucibleCore actually needs.
//!
//! The original three traits (RelationalDB, GraphDB, DocumentDB) remain in
//! `database.rs` as implementation references, but this trait is what Core uses.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Common result type for storage operations
pub type StorageResult<T> = Result<T, StorageError>;

/// Storage operation errors
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum StorageError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Query error: {0}")]
    Query(String),

    #[error("Schema error: {0}")]
    Schema(String),

    #[error("Transaction error: {0}")]
    Transaction(String),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Unified storage abstraction
///
/// This trait defines the essential operations that CrucibleCore needs from its storage layer.
/// It combines relational, graph, and document concerns into the methods that Core actually uses.
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

/// Query result containing records and metadata
///
/// This matches the structure returned by CrucibleCore.query() and used
/// by SurrealDB implementations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Result records (each record is a map of field names to values)
    pub records: Vec<Record>,

    /// Total count of matching records (if available)
    pub total_count: Option<u64>,

    /// Query execution time in milliseconds
    pub execution_time_ms: Option<u64>,

    /// Whether there are more results available (pagination)
    pub has_more: bool,
}

impl QueryResult {
    /// Create a new empty query result
    pub fn empty() -> Self {
        Self {
            records: Vec::new(),
            total_count: Some(0),
            execution_time_ms: None,
            has_more: false,
        }
    }

    /// Create a query result with records
    pub fn with_records(records: Vec<Record>) -> Self {
        let total_count = records.len() as u64;
        Self {
            records,
            total_count: Some(total_count),
            execution_time_ms: None,
            has_more: false,
        }
    }
}

/// Database record (row/document)
///
/// Represents a single result record with an optional ID and field data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    /// Optional record identifier
    pub id: Option<RecordId>,

    /// Record field data (column values or document fields)
    pub data: HashMap<String, serde_json::Value>,
}

impl Record {
    /// Create a new record without an ID
    pub fn new(data: HashMap<String, serde_json::Value>) -> Self {
        Self { id: None, data }
    }

    /// Create a new record with an ID
    pub fn with_id(id: RecordId, data: HashMap<String, serde_json::Value>) -> Self {
        Self { id: Some(id), data }
    }
}

/// Record identifier
///
/// Wraps a string identifier for type safety.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RecordId(pub String);

impl std::fmt::Display for RecordId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for RecordId {
    fn from(s: String) -> Self {
        RecordId(s)
    }
}

impl From<&str> for RecordId {
    fn from(s: &str) -> Self {
        RecordId(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_result_empty() {
        let result = QueryResult::empty();
        assert_eq!(result.records.len(), 0);
        assert_eq!(result.total_count, Some(0));
        assert!(!result.has_more);
    }

    #[test]
    fn test_query_result_with_records() {
        let mut data = HashMap::new();
        data.insert("name".to_string(), serde_json::json!("test"));
        let record = Record::new(data);

        let result = QueryResult::with_records(vec![record]);
        assert_eq!(result.records.len(), 1);
        assert_eq!(result.total_count, Some(1));
    }

    #[test]
    fn test_record_id_display() {
        let id = RecordId("users:123".to_string());
        assert_eq!(format!("{}", id), "users:123");
    }

    #[test]
    fn test_record_id_from() {
        let id1: RecordId = "test:1".into();
        let id2: RecordId = "test:1".to_string().into();
        assert_eq!(id1, id2);
    }
}
