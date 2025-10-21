//! Database integration for Rune tools
//!
//! This module provides database integration capabilities for Rune tools,
//! using SurrealDB for data persistence and querying.

use crate::errors::{RuneError, RuneResult};
use crucible_surrealdb::{SurrealClient, SurrealDbConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Database configuration
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Connection string or file path
    pub connection_string: String,
    /// Connection pool size
    pub pool_size: u32,
    /// Connection timeout in seconds
    pub timeout_secs: u64,
    /// Namespace for SurrealDB
    pub namespace: Option<String>,
    /// Database name for SurrealDB
    pub database: Option<String>,
    /// Username for authentication
    pub username: Option<String>,
    /// Password for authentication
    pub password: Option<String>,
    /// Additional options
    pub options: HashMap<String, String>,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            connection_string: "memory".to_string(),
            pool_size: 10,
            timeout_secs: 30,
            namespace: Some("crucible".to_string()),
            database: Some("tools".to_string()),
            username: None,
            password: None,
            options: HashMap::new(),
        }
    }
}

/// Database connection manager
pub struct DatabaseManager {
    /// SurrealDB client
    client: Arc<SurrealClient>,
    /// Default configuration
    config: DatabaseConfig,
}

/// Database value types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DatabaseValue {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Binary(Vec<u8>),
    Array(Vec<DatabaseValue>),
    Object(HashMap<String, DatabaseValue>),
    DateTime(String),
    Uuid(String),
}

impl From<serde_json::Value> for DatabaseValue {
    fn from(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => DatabaseValue::Null,
            serde_json::Value::Bool(b) => DatabaseValue::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    DatabaseValue::Integer(i)
                } else if let Some(f) = n.as_f64() {
                    DatabaseValue::Float(f)
                } else {
                    DatabaseValue::String(n.to_string())
                }
            }
            serde_json::Value::String(s) => DatabaseValue::String(s),
            serde_json::Value::Array(arr) => {
                DatabaseValue::Array(arr.into_iter().map(DatabaseValue::from).collect())
            }
            serde_json::Value::Object(obj) => {
                DatabaseValue::Object(
                    obj.into_iter()
                        .map(|(k, v)| (k, DatabaseValue::from(v)))
                        .collect(),
                )
            }
        }
    }
}

impl From<DatabaseValue> for serde_json::Value {
    fn from(value: DatabaseValue) -> Self {
        match value {
            DatabaseValue::Null => serde_json::Value::Null,
            DatabaseValue::Bool(b) => serde_json::Value::Bool(b),
            DatabaseValue::Integer(i) => serde_json::Value::Number(serde_json::Number::from(i)),
            DatabaseValue::Float(f) => serde_json::Value::Number(serde_json::Number::from_f64(f).unwrap_or_else(|| serde_json::Number::from(0))),
            DatabaseValue::String(s) => serde_json::Value::String(s),
            DatabaseValue::Binary(b) => serde_json::Value::Array(b.into_iter().map(|byte| serde_json::Value::Number(serde_json::Number::from(byte))).collect()),
            DatabaseValue::Array(arr) => serde_json::Value::Array(arr.into_iter().map(serde_json::Value::from).collect()),
            DatabaseValue::Object(obj) => serde_json::Value::Object(
                obj.into_iter()
                    .map(|(k, v)| (k, serde_json::Value::from(v)))
                    .collect(),
            ),
            DatabaseValue::DateTime(s) => serde_json::Value::String(s),
            DatabaseValue::Uuid(s) => serde_json::Value::String(s),
        }
    }
}

// Re-export QueryResult from crucible_core for consistency
pub use crucible_core::database::QueryResult;

/// Statement result (for non-query operations)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatementResult {
    /// Number of rows affected
    pub rows_affected: u64,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Last inserted ID (if applicable)
    pub last_insert_id: Option<String>,
}

/// Database schema information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSchema {
    /// Table names
    pub tables: Vec<String>,
    /// Column information per table
    pub columns: HashMap<String, Vec<ColumnInfo>>,
}

/// Column information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    /// Column name
    pub name: String,
    /// Column type
    pub data_type: String,
    /// Whether column is nullable
    pub nullable: bool,
    /// Default value
    pub default_value: Option<String>,
}

/// Connection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// Connection type
    pub connection_type: String,
    /// Database name
    pub database_name: String,
    /// Connection status
    pub status: String,
    /// Connection time
    pub connected_at: chrono::DateTime<chrono::Utc>,
}

impl DatabaseManager {
    /// Create a new database manager
    pub async fn new(config: DatabaseConfig) -> RuneResult<Self> {
        info!("Creating database manager with config: {:?}", config);

        // Create SurrealDB configuration
        let surreal_config = SurrealDbConfig {
            path: config.connection_string.clone(),
            namespace: config.namespace.clone().unwrap_or_else(|| "crucible".to_string()),
            database: config.database.clone().unwrap_or_else(|| "tools".to_string()),
            ..Default::default()
        };

        // Create SurrealDB client
        let client = Arc::new(
            SurrealClient::new(surreal_config)
                .await
                .map_err(|e| RuneError::DatabaseError {
                    message: e.to_string(),
                    operation: Some("create_client".to_string()),
                    source: anyhow::anyhow!(e)
                })?,
        );

        Ok(Self { client, config })
    }

    /// Create a database manager with default configuration
    pub async fn default() -> RuneResult<Self> {
        Self::new(DatabaseConfig::default()).await
    }

    /// Execute a query and return results
    pub async fn execute_query(&self, query: &str, params: Vec<DatabaseValue>) -> RuneResult<QueryResult> {
        debug!("Executing query: {} with {} parameters", query, params.len());

        let start_time = std::time::Instant::now();

        // Convert parameters to SurrealDB format
        let surreal_params: Vec<serde_json::Value> = params
            .into_iter()
            .map(serde_json::Value::from)
            .collect();

        // Execute query using SurrealDB
        let result = self.client.query(query, &surreal_params).await
            .map_err(|e| RuneError::DatabaseError {
                message: e.to_string(),
                operation: Some("execute_query".to_string()),
                source: anyhow::anyhow!(e)
            })?;

        let execution_time = start_time.elapsed().as_millis() as u64;

        // Return the result from crucible_core QueryResult directly
        // The SurrealClient query method already returns the correct structure
        Ok(result)
    }

    /// Execute a statement that doesn't return results
    pub async fn execute_statement(&self, statement: &str, params: Vec<DatabaseValue>) -> RuneResult<StatementResult> {
        debug!("Executing statement: {} with {} parameters", statement, params.len());

        let start_time = std::time::Instant::now();

        // Convert parameters to SurrealDB format
        let surreal_params: Vec<serde_json::Value> = params
            .into_iter()
            .map(serde_json::Value::from)
            .collect();

        // Execute statement using SurrealDB
        let result = self.client.execute(statement, &surreal_params).await
            .map_err(|e| RuneError::DatabaseError {
                message: e.to_string(),
                operation: Some("execute_statement".to_string()),
                source: anyhow::anyhow!(e)
            })?;

        let execution_time = start_time.elapsed().as_millis() as u64;

        let statement_result = StatementResult {
            rows_affected: result.total_count.unwrap_or(0),
            execution_time_ms: execution_time,
            last_insert_id: None, // Not available in crucible_core QueryResult
        };

        Ok(statement_result)
    }

    /// Get database schema information
    pub async fn get_schema(&self) -> RuneResult<DatabaseSchema> {
        debug!("Getting database schema");

        // Query for table information
        let _query_result = self.execute_query("INFO FOR TABLE;", vec![]).await?;

        // For now, return a basic schema
        // TODO: Implement proper schema extraction from SurrealDB
        Ok(DatabaseSchema {
            tables: vec![],
            columns: HashMap::new(),
        })
    }

    /// Check if connection is healthy
    pub async fn health_check(&self) -> RuneResult<bool> {
        debug!("Performing health check");

        // Simple query to check connection
        match self.execute_query("SELECT 1 as health_check;", vec![]).await {
            Ok(_) => Ok(true),
            Err(e) => {
                warn!("Health check failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Get connection information
    pub fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo {
            connection_type: "SurrealDB".to_string(),
            database_name: self.config.database.clone().unwrap_or_else(|| "unknown".to_string()),
            status: "connected".to_string(),
            connected_at: chrono::Utc::now(),
        }
    }

    /// Close the database connection
    pub async fn close(&self) -> RuneResult<()> {
        info!("Closing database connection");
        // SurrealDB client will be automatically dropped when Arc goes out of scope
        Ok(())
    }
}

/// Create a database module for Rune
pub fn create_database_module(manager: Arc<DatabaseManager>) -> crate::errors::RuneResult<rune::Module> {
    let mut module = rune::Module::with_crate("database")?;

    // Add database manager instance
    module.set_value("manager", manager)?;

    Ok(module)
}

/// Result of a database operation for Rune tools
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseOperationResult {
    /// Success flag
    pub success: bool,
    /// Result data (if applicable)
    pub data: Option<serde_json::Value>,
    /// Error message (if failed)
    pub error: Option<String>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
}

impl DatabaseOperationResult {
    /// Create a successful result
    pub fn success(data: Option<serde_json::Value>, execution_time_ms: u64) -> Self {
        Self {
            success: true,
            data,
            error: None,
            execution_time_ms,
        }
    }

    /// Create an error result
    pub fn error(error: String, execution_time_ms: u64) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
            execution_time_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_config_default() {
        let config = DatabaseConfig::default();
        assert_eq!(config.connection_string, "memory");
        assert_eq!(config.pool_size, 10);
        assert_eq!(config.timeout_secs, 30);
    }

    #[test]
    fn test_database_value_conversion() {
        let json_val = serde_json::json!({"key": "value", "number": 42});
        let db_val = DatabaseValue::from(json_val.clone());
        let back_to_json = serde_json::Value::from(db_val);

        assert_eq!(json_val, back_to_json);
    }
}