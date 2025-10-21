//! Database integration for Rune tools
//!
//! This module provides database integration capabilities for Rune tools,
//! supporting DuckDB and SurrealDB for data persistence and querying.

use crate::errors::{RuneError, RuneResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use duckdb::types::{Value, ToSql};

/// Database configuration
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Database type
    pub db_type: DatabaseType,
    /// Connection string or file path
    pub connection_string: String,
    /// Whether to enable WAL mode (for DuckDB)
    pub enable_wal: bool,
    /// Maximum memory usage in MB
    pub max_memory_mb: Option<u64>,
    /// Thread count for parallel queries
    pub thread_count: Option<u32>,
    /// Connection pool size
    pub pool_size: u32,
    /// Connection timeout in seconds
    pub timeout_secs: u64,
    /// Additional options
    pub options: HashMap<String, String>,
}

/// Database types supported
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatabaseType {
    /// DuckDB - embedded analytical database
    DuckDB,
    /// SurrealDB - distributed document database
    SurrealDB,
}

impl DatabaseType {
    /// Get default port for this database type
    pub fn default_port(&self) -> u16 {
        match self {
            DatabaseType::DuckDB => 0, // Embedded, no port
            DatabaseType::SurrealDB => 8000,
        }
    }

    /// Get description for this database type
    pub fn description(&self) -> &'static str {
        match self {
            DatabaseType::DuckDB => "DuckDB - Embedded analytical database",
            DatabaseType::SurrealDB => "SurrealDB - Distributed document database",
        }
    }
}

/// Database connection manager
pub struct DatabaseManager {
    /// Database connections
    connections: Arc<RwLock<HashMap<String, Arc<dyn DatabaseConnection>>>>,
    /// Default configuration
    default_config: DatabaseConfig,
}

/// Trait for database connections
#[async_trait::async_trait]
pub trait DatabaseConnection: Send + Sync {
    /// Execute a query and return results
    async fn execute_query(&self, query: &str, params: Vec<DatabaseValue>) -> RuneResult<QueryResult>;

    /// Execute a statement that doesn't return results
    async fn execute_statement(&self, statement: &str, params: Vec<DatabaseValue>) -> RuneResult<StatementResult>;

    /// Get database schema information
    async fn get_schema(&self) -> RuneResult<DatabaseSchema>;

    /// Check if connection is healthy
    async fn health_check(&self) -> RuneResult<bool>;

    /// Get connection information
    fn connection_info(&self) -> ConnectionInfo;

    /// Close the connection
    async fn close(&self) -> RuneResult<()>;
}

/// Query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Column names
    pub columns: Vec<String>,
    /// Rows of data
    pub rows: Vec<Vec<DatabaseValue>>,
    /// Number of rows affected (for INSERT/UPDATE/DELETE)
    pub rows_affected: u64,
    /// Query execution time in milliseconds
    pub execution_time_ms: u64,
}

/// Statement result (for non-query operations)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatementResult {
    /// Number of rows affected
    pub rows_affected: u64,
    /// Last inserted ID (if applicable)
    pub last_insert_id: Option<String>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
}

/// Database value
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
}

/// Database schema information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSchema {
    /// Table information
    pub tables: Vec<TableInfo>,
    /// View information
    pub views: Vec<ViewInfo>,
    /// Function information
    pub functions: Vec<FunctionInfo>,
}

/// Table information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableInfo {
    /// Table name
    pub name: String,
    /// Column information
    pub columns: Vec<ColumnInfo>,
    /// Table type (table, view, etc.)
    pub table_type: String,
    /// Estimated row count
    pub estimated_rows: Option<u64>,
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
    pub default_value: Option<DatabaseValue>,
    /// Whether column is primary key
    pub primary_key: bool,
}

/// View information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewInfo {
    /// View name
    pub name: String,
    /// View definition/query
    pub definition: String,
    /// Columns in the view
    pub columns: Vec<ColumnInfo>,
}

/// Function information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInfo {
    /// Function name
    pub name: String,
    /// Function arguments
    pub arguments: Vec<ColumnInfo>,
    /// Return type
    pub return_type: String,
    /// Function type (scalar, aggregate, etc.)
    pub function_type: String,
}

/// Connection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// Database type
    pub db_type: DatabaseType,
    /// Connection string (sanitized)
    pub connection_string: String,
    /// Connection status
    pub status: ConnectionStatus,
    /// Connection timestamp
    pub connected_at: chrono::DateTime<chrono::Utc>,
    /// Number of queries executed
    pub queries_executed: u64,
    /// Total query time
    pub total_query_time_ms: u64,
}

/// Connection status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Error(String),
}

/// DuckDB connection implementation
pub struct DuckDBConnection {
    connection: Arc<tokio::sync::Mutex<duckdb::Connection>>,
    connection_info: ConnectionInfo,
}

impl DuckDBConnection {
    /// Create a new DuckDB connection
    pub async fn new(config: DatabaseConfig) -> RuneResult<Self> {
        let start_time = std::time::Instant::now();

        let connection = duckdb::Connection::open(&config.connection_string)
            .map_err(|e| RuneError::DatabaseError {
                message: format!("Failed to connect to DuckDB: {}", e),
                operation: Some("connect".to_string()),
                source: anyhow::anyhow!(e),
            })?;

        // Configure connection
        if config.enable_wal {
            connection.execute("PRAGMA journal_mode=WAL")
                .map_err(|e| RuneError::DatabaseError {
                    message: format!("Failed to enable WAL mode: {}", e),
                    operation: Some("configure".to_string()),
                    source: anyhow::anyhow!(e),
                })?;
        }

        if let Some(max_memory) = config.max_memory_mb {
            connection.execute(&format!("PRAGMA memory_limit='{}MB'", max_memory))
                .map_err(|e| RuneError::DatabaseError {
                    message: format!("Failed to set memory limit: {}", e),
                    operation: Some("configure".to_string()),
                    source: anyhow::anyhow!(e),
                })?;
        }

        if let Some(threads) = config.thread_count {
            connection.execute(&format!("PRAGMA threads={}", threads))
                .map_err(|e| RuneError::DatabaseError {
                    message: format!("Failed to set thread count: {}", e),
                    operation: Some("configure".to_string()),
                    source: anyhow::anyhow!(e),
                })?;
        }

        let connection_info = ConnectionInfo {
            db_type: DatabaseType::DuckDB,
            connection_string: sanitize_connection_string(&config.connection_string),
            status: ConnectionStatus::Connected,
            connected_at: chrono::Utc::now(),
            queries_executed: 0,
            total_query_time_ms: start_time.elapsed().as_millis() as u64,
        };

        Ok(Self {
            connection: Arc::new(tokio::sync::Mutex::new(connection)),
            connection_info,
        })
    }
}

#[async_trait::async_trait]
impl DatabaseConnection for DuckDBConnection {
    async fn execute_query(&self, query: &str, params: Vec<DatabaseValue>) -> RuneResult<QueryResult> {
        let start_time = std::time::Instant::now();
        let mut connection = self.connection.lock().await;

        let mut stmt = connection.prepare(query)?;

        // Convert parameters
        let duckdb_params: Vec<_> = params.into_iter()
            .map(|v| convert_to_duckdb_value(v))
            .collect();

        let mut rows = Vec::new();
        let mut columns = Vec::new();

        // Execute query
        let mut results = stmt.query(&duckdb_params)?;

        // Get column names
        for i in 0..results.column_count() {
            columns.push(results.column_name(i).unwrap_or("unknown").to_string());
        }

        // Collect rows
        while let Some(row) = results.next()? {
            let mut row_data = Vec::new();
            for i in 0..results.column_count() {
                let value = match row.get::<_, duckdb::types::Value>(i)? {
                    duckdb::types::Value::Null => DatabaseValue::Null,
                    duckdb::types::Value::Boolean(b) => DatabaseValue::Bool(b),
                    duckdb::types::Value::TinyInt(i) => DatabaseValue::Integer(i as i64),
                    duckdb::types::Value::SmallInt(i) => DatabaseValue::Integer(i as i64),
                    duckdb::types::Value::Int(i) => DatabaseValue::Integer(i),
                    duckdb::types::Value::BigInt(i) => DatabaseValue::Integer(i),
                    duckdb::types::Value::UTinyInt(i) => DatabaseValue::Integer(i as i64),
                    duckdb::types::Value::USmallInt(i) => DatabaseValue::Integer(i as i64),
                    duckdb::types::Value::UInt(i) => DatabaseValue::Integer(i as i64),
                    duckdb::types::Value::UBigInt(i) => DatabaseValue::Integer(i as i64),
                    duckdb::types::Value::Float(f) => DatabaseValue::Float(f),
                    duckdb::types::Value::Double(f) => DatabaseValue::Float(f),
                    duckdb::types::Value::Timestamp(t) => DatabaseValue::String(t.to_rfc3339()),
                    duckdb::types::Value::Text(s) => DatabaseValue::String(s),
                    duckdb::types::Value::Blob(b) => DatabaseValue::Binary(b),
                    duckdb::types::Value::Date32(d) => DatabaseValue::String(d.to_string()),
                    duckdb::types::Value::Time64(t) => DatabaseValue::String(t.to_string()),
                };
                row_data.push(value);
            }
            rows.push(row_data);
        }

        let execution_time_ms = start_time.elapsed().as_millis() as u64;

        Ok(QueryResult {
            columns,
            rows,
            rows_affected: 0,
            execution_time_ms,
        })
    }

    async fn execute_statement(&self, statement: &str, params: Vec<DatabaseValue>) -> RuneResult<StatementResult> {
        let start_time = std::time::Instant::now();
        let mut connection = self.connection.lock().await;

        let mut stmt = connection.prepare(statement)?;

        // Convert parameters
        let duckdb_params: Vec<_> = params.into_iter()
            .map(|v| convert_to_duckdb_value(v))
            .collect();

        let rows_affected = stmt.execute(&duckdb_params)?;
        let execution_time_ms = start_time.elapsed().as_millis() as u64;

        Ok(StatementResult {
            rows_affected,
            last_insert_id: None, // DuckDB doesn't have last_insert_id in the same way
            execution_time_ms,
        })
    }

    async fn get_schema(&self) -> RuneResult<DatabaseSchema> {
        let mut connection = self.connection.lock().await;

        // Get tables
        let mut tables = Vec::new();
        let mut stmt = connection.prepare(
            "SELECT table_name, table_type FROM information_schema.tables WHERE table_schema = 'main'"
        )?;

        let mut results = stmt.query([])?;
        while let Some(row) = results.next()? {
            let table_name: String = row.get(0)?;
            let table_type: String = row.get(1)?;

            // Get columns for this table
            let mut columns = Vec::new();
            let mut col_stmt = connection.prepare(&format!(
                "SELECT column_name, data_type, is_nullable, column_default FROM information_schema.columns WHERE table_name = '{}'",
                table_name
            ))?;

            let mut col_results = col_stmt.query([])?;
            while let Some(col_row) = col_results.next()? {
                let column_name: String = col_row.get(0)?;
                let data_type: String = col_row.get(1)?;
                let is_nullable: String = col_row.get(2)?;
                let default_value: Option<String> = col_row.get(3)?;

                columns.push(ColumnInfo {
                    name: column_name,
                    data_type,
                    nullable: is_nullable == "YES",
                    default_value: default_value.map(DatabaseValue::String),
                    primary_key: false, // Would need separate query for primary key info
                });
            }

            tables.push(TableInfo {
                name: table_name,
                columns,
                table_type,
                estimated_rows: None,
            });
        }

        Ok(DatabaseSchema {
            tables,
            views: Vec::new(),
            functions: Vec::new(),
        })
    }

    async fn health_check(&self) -> RuneResult<bool> {
        match self.execute_query("SELECT 1", Vec::new()).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    fn connection_info(&self) -> ConnectionInfo {
        self.connection_info.clone()
    }

    async fn close(&self) -> RuneResult<()> {
        // DuckDB connection is closed when dropped
        Ok(())
    }
}

/// SurrealDB connection implementation
pub struct SurrealDBConnection {
    client: Arc<crucible_surrealdb::SurrealClient>,
    connection_info: ConnectionInfo,
}

impl SurrealDBConnection {
    /// Create a new SurrealDB connection
    pub async fn new(config: DatabaseConfig) -> RuneResult<Self> {
        let start_time = std::time::Instant::now();

        // Create SurrealDB config from connection string
        let surreal_config = crucible_surrealdb::SurrealDbConfig {
            namespace: "crucible".to_string(),
            database: "cache".to_string(),
            path: config.connection_string.clone(),
            max_connections: Some(config.pool_size),
            timeout_seconds: Some(30),
        };

        let client = crucible_surrealdb::SurrealClient::new(surreal_config)
            .await
            .map_err(|e| RuneError::DatabaseError {
                message: format!("Failed to connect to SurrealDB: {}", e),
                operation: Some("connect".to_string()),
                source: anyhow::anyhow!(e),
            })?;

        // Note: The mock SurrealClient doesn't require authentication or namespace/database setup
        // In a real SurrealDB implementation, you would do:
        // client.signin(...).await?;
        // client.use_ns("namespace").use_db("database").await?;

        let connection_info = ConnectionInfo {
            db_type: DatabaseType::SurrealDB,
            connection_string: sanitize_connection_string(&config.connection_string),
            status: ConnectionStatus::Connected,
            connected_at: chrono::Utc::now(),
            queries_executed: 0,
            total_query_time_ms: start_time.elapsed().as_millis() as u64,
        };

        Ok(Self {
            client: Arc::new(client),
            connection_info,
        })
    }
}

#[async_trait::async_trait]
impl DatabaseConnection for SurrealDBConnection {
    async fn execute_query(&self, query: &str, _params: Vec<DatabaseValue>) -> RuneResult<QueryResult> {
        // This is a simplified implementation
        // In a real implementation, you'd need to handle SurrealDB's specific query format
        warn!("SurrealDB query execution not fully implemented");

        Ok(QueryResult {
            columns: vec!["result".to_string()],
            rows: vec![vec![DatabaseValue::String("Not implemented".to_string())]],
            rows_affected: 0,
            execution_time_ms: 0,
        })
    }

    async fn execute_statement(&self, _statement: &str, _params: Vec<DatabaseValue>) -> RuneResult<StatementResult> {
        warn!("SurrealDB statement execution not fully implemented");

        Ok(StatementResult {
            rows_affected: 0,
            last_insert_id: None,
            execution_time_ms: 0,
        })
    }

    async fn get_schema(&self) -> RuneResult<DatabaseSchema> {
        warn!("SurrealDB schema retrieval not fully implemented");

        Ok(DatabaseSchema {
            tables: Vec::new(),
            views: Vec::new(),
            functions: Vec::new(),
        })
    }

    async fn health_check(&self) -> RuneResult<bool> {
        // Simple health check - try to get version info
        match self.client.version().await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    fn connection_info(&self) -> ConnectionInfo {
        self.connection_info.clone()
    }

    async fn close(&self) -> RuneResult<()> {
        // SurrealDB connection cleanup
        Ok(())
    }
}

/// Convert DatabaseValue to DuckDB value
fn convert_to_duckdb_value(value: DatabaseValue) -> duckdb::types::Value {
    match value {
        DatabaseValue::Null => duckdb::types::Value::Null,
        DatabaseValue::Bool(b) => duckdb::types::Value::Boolean(b),
        DatabaseValue::Integer(i) => duckdb::types::Value::BigInt(i),
        DatabaseValue::Float(f) => duckdb::types::Value::Double(f),
        DatabaseValue::String(s) => duckdb::types::Value::Text(s),
        DatabaseValue::Binary(b) => duckdb::types::Value::Blob(b),
        DatabaseValue::Array(_) => duckdb::types::Value::Text("array".to_string()), // Simplified
        DatabaseValue::Object(_) => duckdb::types::Value::Text("object".to_string()), // Simplified
    }
}

/// Sanitize connection string for logging
fn sanitize_connection_string(conn_str: &str) -> String {
    // Remove potential sensitive information
    if conn_str.contains("password=") {
        "[REDACTED]".to_string()
    } else {
        conn_str.to_string()
    }
}

impl DatabaseManager {
    /// Create a new database manager
    pub fn new(default_config: DatabaseConfig) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            default_config,
        }
    }

    /// Create a database connection
    pub async fn create_connection(
        &self,
        name: String,
        config: Option<DatabaseConfig>,
    ) -> RuneResult<String> {
        let config = config.unwrap_or_else(|| self.default_config.clone());
        let connection: Arc<dyn DatabaseConnection> = match config.db_type {
            DatabaseType::DuckDB => {
                Arc::new(DuckDBConnection::new(config).await
                    .map_err(|e| RuneError::DatabaseError {
                        message: format!("Failed to create DuckDB connection: {}", e),
                        operation: Some("connect".to_string()),
                        source: anyhow::anyhow!(e),
                    })?)
            }
            DatabaseType::SurrealDB => {
                Arc::new(SurrealDBConnection::new(config).await
                    .map_err(|e| RuneError::DatabaseError {
                        message: format!("Failed to create SurrealDB connection: {}", e),
                        operation: Some("connect".to_string()),
                        source: anyhow::anyhow!(e),
                    })?)
            }
        };

        let mut connections = self.connections.write().await;
        connections.insert(name.clone(), connection);

        info!("Created database connection: {}", name);
        Ok(name)
    }

    /// Get a database connection
    pub async fn get_connection(&self, name: &str) -> Option<Arc<dyn DatabaseConnection>> {
        let connections = self.connections.read().await;
        connections.get(name).cloned()
    }

    /// Remove a database connection
    pub async fn remove_connection(&self, name: &str) -> RuneResult<bool> {
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.remove(name) {
            // Close the connection
            let _ = connection.close().await;
            info!("Removed database connection: {}", name);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// List all connections
    pub async fn list_connections(&self) -> Vec<String> {
        let connections = self.connections.read().await;
        connections.keys().cloned().collect()
    }

    /// Health check all connections
    pub async fn health_check_all(&self) -> HashMap<String, bool> {
        let mut results = HashMap::new();
        let connections = self.connections.read().await;

        for (name, connection) in connections.iter() {
            match connection.health_check().await {
                Ok(healthy) => {
                    results.insert(name.clone(), healthy);
                }
                Err(_) => {
                    results.insert(name.clone(), false);
                }
            }
        }

        results
    }

    /// Close all connections
    pub async fn close_all(&self) -> RuneResult<()> {
        let mut connections = self.connections.write().await;
        for (name, connection) in connections.drain() {
            if let Err(e) = connection.close().await {
                warn!("Failed to close connection '{}': {}", name, e);
            }
        }
        Ok(())
    }
}

/// Create a default database configuration for DuckDB
pub fn default_duckdb_config(file_path: &str) -> DatabaseConfig {
    DatabaseConfig {
        db_type: DatabaseType::DuckDB,
        connection_string: file_path.to_string(),
        enable_wal: true,
        max_memory_mb: Some(512),
        thread_count: Some(4),
        pool_size: 1,
        timeout_secs: 30,
        options: HashMap::new(),
    }
}

/// Create a default database configuration for SurrealDB
pub fn default_surrealdb_config(address: &str) -> DatabaseConfig {
    DatabaseConfig {
        db_type: DatabaseType::SurrealDB,
        connection_string: address.to_string(),
        enable_wal: false,
        max_memory_mb: None,
        thread_count: None,
        pool_size: 5,
        timeout_secs: 30,
        options: HashMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_duckdb_connection() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.duckdb");

        let config = default_duckdb_config(db_path.to_str().unwrap());
        let connection = DuckDBConnection::new(config).await?;

        // Test health check
        let healthy = connection.health_check().await?;
        assert!(healthy);

        // Test simple query
        let result = connection.execute_query("SELECT 1 as test_col", Vec::new()).await?;
        assert_eq!(result.columns, vec!["test_col"]);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][0], DatabaseValue::Integer(1));

        // Test statement
        let statement_result = connection.execute_statement("CREATE TABLE test (id INTEGER)", Vec::new()).await?;
        assert_eq!(statement_result.rows_affected, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_database_manager() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("manager_test.duckdb");

        let config = default_duckdb_config(db_path.to_str().unwrap());
        let manager = DatabaseManager::new(config);

        // Create connection
        let conn_name = manager.create_connection("test".to_string(), None).await?;
        assert_eq!(conn_name, "test");

        // List connections
        let connections = manager.list_connections().await;
        assert_eq!(connections.len(), 1);
        assert!(connections.contains(&"test".to_string()));

        // Get connection
        let connection = manager.get_connection("test").await;
        assert!(connection.is_some());

        // Health check
        let health_results = manager.health_check_all().await;
        assert_eq!(health_results.len(), 1);
        assert!(health_results.get("test").unwrap_or(&false));

        // Remove connection
        let removed = manager.remove_connection("test").await?;
        assert!(removed);

        let connections = manager.list_connections().await;
        assert_eq!(connections.len(), 0);

        Ok(())
    }

    #[test]
    fn test_database_config() {
        let config = default_duckdb_config("test.duckdb");
        assert!(matches!(config.db_type, DatabaseType::DuckDB));
        assert_eq!(config.connection_string, "test.duckdb");
        assert!(config.enable_wal);

        let config = default_surrealdb_config("ws://localhost:8000");
        assert!(matches!(config.db_type, DatabaseType::SurrealDB));
        assert_eq!(config.connection_string, "ws://localhost:8000");
        assert_eq!(config.pool_size, 5);
    }

    #[test]
    fn test_database_value_conversion() {
        let value = DatabaseValue::Integer(42);
        let duckdb_value = convert_to_duckdb_value(value);
        assert!(matches!(duckdb_value, duckdb::Value::BigInt(42)));

        let value = DatabaseValue::String("test".to_string());
        let duckdb_value = convert_to_duckdb_value(value);
        assert!(matches!(duckdb_value, duckdb::Value::Text(s) if s == "test"));
    }
}