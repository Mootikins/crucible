//! Real SurrealDB Client Implementation
//!
//! This module provides a wrapper around the real SurrealDB Rust SDK,
//! replacing the previous mock implementation with actual database connectivity.
//!
//! ## Supported Backends
//!
//! - **Memory (Mem)**: In-memory storage for development and testing
//! - **File (RocksDB)**: Persistent file-based storage for production
//!
//! ## Usage
//!
//! ```no_run
//! use crucible_surrealdb::SurrealClient;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // In-memory database
//!     let client = SurrealClient::new_memory().await?;
//!
//!     // File-based database
//!     let client = SurrealClient::new_file("./data/vault.db").await?;
//!
//!     // Execute queries
//!     let result = client.query("SELECT * FROM notes", &[]).await?;
//!
//!     Ok(())
//! }
//! ```

use crate::types::SurrealDbConfig;
use crucible_core::{DbError, DbResult, QueryResult, Record, RecordId, SelectQuery, TableSchema};
use serde_json::Value;
use std::collections::HashMap;
use surrealdb::engine::local::Db;
use surrealdb::Surreal;

/// Real SurrealDB client wrapping the official Rust SDK
///
/// This client provides a thin wrapper around `surrealdb::Surreal<Db>`,
/// converting between our internal types and SurrealDB's types while
/// exposing the full power of real SurrealDB queries without custom parsing.
#[derive(Clone)]
pub struct SurrealClient {
    /// The underlying SurrealDB connection
    db: Surreal<Db>,

    /// Configuration for this client
    config: SurrealDbConfig,
}

impl std::fmt::Debug for SurrealClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurrealClient")
            .field("config", &self.config)
            .finish()
    }
}

impl SurrealClient {
    /// Create a new SurrealDB client with the given configuration
    ///
    /// This will create either an in-memory or file-based database
    /// depending on the configuration path.
    pub async fn new(config: SurrealDbConfig) -> DbResult<Self> {
        use surrealdb::engine::local::{Mem, RocksDb};

        let db = if config.path.is_empty() || config.path == ":memory:" {
            // In-memory database
            Surreal::new::<Mem>(()).await.map_err(|e| {
                DbError::Connection(format!("Failed to create in-memory database: {}", e))
            })?
        } else {
            // File-based RocksDB database
            Surreal::new::<RocksDb>(&config.path).await.map_err(|e| {
                DbError::Connection(format!("Failed to create file database at {}: {}", config.path, e))
            })?
        };

        // Use the configured namespace and database
        db.use_ns(&config.namespace)
            .use_db(&config.database)
            .await
            .map_err(|e| {
                DbError::Connection(format!(
                    "Failed to use namespace '{}' and database '{}': {}",
                    config.namespace, config.database, e
                ))
            })?;

        Ok(Self { db, config })
    }

    /// Create an in-memory SurrealDB client for testing
    ///
    /// This is the recommended way to create a client for development
    /// and testing, as it requires no external dependencies and is fast.
    pub async fn new_memory() -> DbResult<Self> {
        let config = SurrealDbConfig {
            namespace: "crucible".to_string(),
            database: "test".to_string(),
            path: ":memory:".to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(30),
        };
        Self::new(config).await
    }

    /// Create a file-based SurrealDB client using RocksDB
    ///
    /// Data will be persisted to the specified path. The path should
    /// be a directory where RocksDB can store its data files.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path for the database storage
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be created or opened.
    pub async fn new_file(path: &str) -> DbResult<Self> {
        let config = SurrealDbConfig {
            namespace: "crucible".to_string(),
            database: "vault".to_string(),
            path: path.to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(30),
        };
        Self::new(config).await
    }

    /// Execute a raw SurrealQL query with optional parameters
    ///
    /// This method provides direct access to SurrealDB's query engine,
    /// supporting the full SurrealQL syntax including:
    /// - SELECT with graph traversal (e.g., `SELECT ->has_embedding->* FROM notes:id`)
    /// - RELATE statements for creating edges
    /// - Complex WHERE clauses
    /// - Aggregations, grouping, etc.
    ///
    /// # Arguments
    ///
    /// * `sql` - The SurrealQL query string
    /// * `params` - Optional parameters for the query (currently unused, for API compatibility)
    ///
    /// # Returns
    ///
    /// A `QueryResult` containing the records returned by the query.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails to execute or parse.
    pub async fn query(&self, sql: &str, _params: &[Value]) -> DbResult<QueryResult> {
        // Execute the query
        let response = self.db.query(sql).await.map_err(|e| {
            DbError::Query(format!("Query execution failed: {}", e))
        })?;

        // Check the response for errors
        let mut response = response.check().map_err(|e| {
            DbError::Query(format!("Query returned error: {}", e))
        })?;

        // Extract the first result set as SurrealDB's Value type
        let surreal_value: surrealdb::Value = response.take(0).map_err(|e| {
            DbError::Query(format!("Failed to extract query results: {}", e))
        })?;

        // Convert SurrealDB Value to JSON by serializing and deserializing
        // This handles all the enum variants properly
        let json_string = serde_json::to_string(&surreal_value).map_err(|e| {
            DbError::Query(format!("Failed to serialize SurrealDB value: {}", e))
        })?;

        let json_value: Value = serde_json::from_str(&json_string).map_err(|e| {
            DbError::Query(format!("Failed to parse JSON from SurrealDB value: {}", e))
        })?;

        // Handle the result - it should be an array of records
        let records_array = match json_value {
            Value::Object(mut obj) if obj.contains_key("Array") => {
                // SurrealDB wraps results in {"Array": [...]}
                obj.remove("Array")
                    .and_then(|v| {
                        if let Value::Array(arr) = v {
                            Some(arr)
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default()
            }
            Value::Array(arr) => arr,
            other => vec![other],
        };

        // Extract the actual record objects from the wrapped structure
        let converted_records = records_array
            .into_iter()
            .filter_map(|item| {
                // Each item might be wrapped in {"Object": {...}}
                if let Value::Object(mut outer) = item {
                    if let Some(Value::Object(inner)) = outer.remove("Object") {
                        Some(self.convert_wrapped_object_to_record(inner))
                    } else {
                        Some(self.convert_value_to_record(Value::Object(outer)))
                    }
                } else {
                    Some(self.convert_value_to_record(item))
                }
            })
            .collect();

        Ok(QueryResult {
            records: converted_records,
            total_count: None,
            execution_time_ms: None,
            has_more: false,
        })
    }

    /// Insert a record into a table
    ///
    /// This method creates a new record in the specified table. If the record
    /// has an ID, it will be used; otherwise, SurrealDB will generate one.
    ///
    /// # Arguments
    ///
    /// * `table` - The table name
    /// * `record` - The record to insert
    ///
    /// # Returns
    ///
    /// A `QueryResult` containing the created record with its ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the insertion fails.
    pub async fn insert(&self, table: &str, record: Record) -> DbResult<QueryResult> {
        // Build a SurrealQL CREATE query instead of using the SDK's .create() method
        // This avoids serialization issues with serde_json::Value
        let id_str = record.id.as_ref().map(|id| id.0.as_str());

        // Convert data to JSON for the query
        let data_json = serde_json::to_string(&record.data).map_err(|e| {
            DbError::Query(format!("Failed to serialize record data: {}", e))
        })?;

        let sql = if let Some(id) = id_str {
            // Extract just the ID part (after the colon)
            let id_part = if let Some(pos) = id.find(':') {
                &id[pos + 1..]
            } else {
                id
            };
            // Wrap string IDs in backticks to ensure they're treated as identifiers
            format!("CREATE {}:`{}` CONTENT {}", table, id_part, data_json)
        } else {
            format!("CREATE {} CONTENT {}", table, data_json)
        };

        // Execute the query
        self.query(&sql, &[]).await
    }

    /// Convert a wrapped SurrealDB object to our Record format
    ///
    /// This handles the nested structure where each field is wrapped in its type variant,
    /// e.g., {"age": {"Number": {"Int": 30}}, "name": {"Strand": "Alice"}}
    fn convert_wrapped_object_to_record(&self, wrapped: serde_json::Map<String, Value>) -> Record {
        let mut unwrapped_data = serde_json::Map::new();
        let mut record_id = None;

        for (key, value) in wrapped {
            // Unwrap the typed values
            let unwrapped_value = self.unwrap_surreal_value(value);

            if key == "id" {
                // Special handling for ID field
                if let Some(id_str) = unwrapped_value.as_str() {
                    record_id = Some(RecordId(id_str.to_string()));
                }
            } else {
                unwrapped_data.insert(key, unwrapped_value);
            }
        }

        Record {
            id: record_id,
            data: unwrapped_data.into_iter().collect(),
        }
    }

    /// Unwrap a SurrealDB typed value to its simple JSON representation
    ///
    /// Converts {"Number": {"Int": 30}} -> 30, {"Strand": "Alice"} -> "Alice", etc.
    fn unwrap_surreal_value(&self, value: Value) -> Value {
        match value {
            Value::Object(mut obj) => {
                // Check for known SurrealDB type wrappers
                if let Some(inner) = obj.remove("Number") {
                    // Handle Number variants: {"Int": 30} or {"Float": 3.14}
                    match inner {
                        Value::Object(mut num_obj) => {
                            if let Some(int_val) = num_obj.remove("Int") {
                                return int_val;
                            } else if let Some(float_val) = num_obj.remove("Float") {
                                return float_val;
                            }
                            return Value::Object(num_obj);
                        }
                        other => return other,
                    }
                } else if let Some(Value::String(s)) = obj.remove("Strand") {
                    // Handle Strand (string) variant
                    return Value::String(s);
                } else if let Some(Value::String(s)) = obj.remove("String") {
                    // Handle plain String wrapper
                    return Value::String(s);
                } else if let Some(thing) = obj.remove("Thing") {
                    // Handle Thing (record ID) variant: {"tb": "test", "id": {"Number": 1}} or {"tb": "test", "id": {"String": "abc"}}
                    if let Value::Object(mut thing_obj) = thing {
                        let tb = thing_obj.remove("tb").and_then(|v| v.as_str().map(String::from));
                        let id_wrapped = thing_obj.remove("id");

                        if let (Some(table), Some(id_val_wrapped)) = (tb, id_wrapped) {
                            // Unwrap the ID value (it might be wrapped in Number or String)
                            let id_val = self.unwrap_surreal_value(id_val_wrapped);

                            // Format as "table:id"
                            let id_str = match id_val {
                                Value::Number(n) => n.to_string(),
                                Value::String(s) => s,
                                other => {
                                    // Should not happen after unwrapping, but handle gracefully
                                    eprintln!("Unexpected ID type after unwrapping: {:?}", other);
                                    format!("{:?}", other)
                                }
                            };
                            return Value::String(format!("{}:{}", table, id_str));
                        }
                    }
                } else if let Some(arr) = obj.remove("Array") {
                    // Handle Array variant
                    if let Value::Array(items) = arr {
                        return Value::Array(
                            items
                                .into_iter()
                                .map(|item| self.unwrap_surreal_value(item))
                                .collect(),
                        );
                    }
                    return arr;
                }

                // If it's not a known wrapper, return as-is
                Value::Object(obj)
            }
            other => other,
        }
    }

    /// Convert a SurrealDB Value to our Record format
    ///
    /// This helper extracts the `id` field (if present) and converts the rest
    /// of the fields into a HashMap.
    fn convert_value_to_record(&self, value: Value) -> Record {
        let mut data = match value {
            Value::Object(map) => map,
            _ => {
                // If it's not an object, wrap it in a map
                let mut map = serde_json::Map::new();
                map.insert("value".to_string(), value);
                map
            }
        };

        // Extract the id field if present
        let id = data.remove("id").and_then(|id_val| {
            if let Some(id_str) = id_val.as_str() {
                Some(RecordId(id_str.to_string()))
            } else {
                None
            }
        });

        Record {
            id,
            data: data.into_iter().collect(),
        }
    }

    /// Get a reference to the underlying SurrealDB connection
    ///
    /// This allows direct access to the SurrealDB SDK for advanced operations
    /// not covered by the wrapper API.
    pub fn db(&self) -> &Surreal<Db> {
        &self.db
    }

    /// Get the client configuration
    pub fn config(&self) -> &SurrealDbConfig {
        &self.config
    }

    /// Create a table with the given schema
    ///
    /// This is a compatibility method for code migrating from the mock client.
    /// SurrealDB tables are SCHEMALESS by default, so this just creates the table.
    pub async fn create_table(&self, _table: &str, _schema: TableSchema) -> DbResult<()> {
        // For now, we don't need to create explicit schemas for SCHEMALESS tables
        // The table will be created automatically when we insert the first record
        Ok(())
    }

    /// Execute a SELECT query
    ///
    /// This converts a `SelectQuery` structure to SurrealQL and executes it.
    pub async fn select(&self, select_query: SelectQuery) -> DbResult<QueryResult> {
        // Convert SelectQuery to SurrealQL
        let mut sql = String::from("SELECT ");

        // Columns
        if let Some(columns) = &select_query.columns {
            sql.push_str(&columns.join(", "));
        } else {
            sql.push('*');
        }

        sql.push_str(" FROM ");
        sql.push_str(&select_query.table);

        // TODO: Add WHERE, ORDER BY, LIMIT, OFFSET support
        // For now, just basic SELECT * FROM table

        self.query(&sql, &[]).await
    }

    /// List all tables in the database
    ///
    /// This queries the SurrealDB information schema to get table names.
    pub async fn list_tables(&self) -> DbResult<Vec<String>> {
        let result = self.query("INFO FOR DB", &[]).await?;

        // The INFO FOR DB command returns metadata about the database
        // We need to parse it to extract table names
        // For now, return an empty list as a stub
        Ok(vec![])
    }

    /// Initialize the database (compatibility method)
    ///
    /// This is a no-op for SurrealDB as initialization happens on connection.
    pub async fn initialize(&self) -> DbResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_memory_client() {
        let client = SurrealClient::new_memory().await.unwrap();
        assert_eq!(client.config().namespace, "crucible");
        assert_eq!(client.config().database, "test");
    }

    #[tokio::test]
    async fn test_simple_query() {
        let client = SurrealClient::new_memory().await.unwrap();

        // Create a record
        client
            .query("CREATE test:1 SET name = 'Alice', age = 30", &[])
            .await
            .unwrap();

        // Query it back
        let result = client.query("SELECT * FROM test:1", &[]).await.unwrap();

        assert_eq!(result.records.len(), 1);
        let record = &result.records[0];
        assert_eq!(record.data.get("name").and_then(|v| v.as_str()), Some("Alice"));
        assert_eq!(record.data.get("age").and_then(|v| v.as_i64()), Some(30));
    }

    #[tokio::test]
    async fn test_insert_with_id() {
        let client = SurrealClient::new_memory().await.unwrap();

        // Create a record using insert
        let mut data = HashMap::new();
        data.insert("title".to_string(), Value::String("Test Note".to_string()));
        data.insert("content".to_string(), Value::String("Hello World".to_string()));

        let record = Record {
            id: Some(RecordId("notes:test123".to_string())),
            data,
        };

        let result = client.insert("notes", record).await.unwrap();

        assert_eq!(result.records.len(), 1);
        assert_eq!(
            result.records[0].id.as_ref().map(|id| id.0.as_str()),
            Some("notes:test123")
        );
    }

    #[tokio::test]
    async fn test_graph_traversal() {
        let client = SurrealClient::new_memory().await.unwrap();

        // Create note and embedding
        client
            .query("CREATE notes:doc1 SET title = 'Document 1'", &[])
            .await
            .unwrap();

        client
            .query("CREATE embeddings:emb1 SET vector = [0.1, 0.2, 0.3]", &[])
            .await
            .unwrap();

        // Create relationship
        client
            .query("RELATE notes:doc1->has_embedding->embeddings:emb1", &[])
            .await
            .unwrap();

        // Query via graph traversal
        let result = client
            .query("SELECT ->has_embedding FROM notes:doc1", &[])
            .await
            .unwrap();

        assert!(!result.records.is_empty(), "Should find related data");
    }
}
