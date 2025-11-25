//! SurrealDB Persistence Layer for Change Detection
//!
//! This module provides storage for file state tracking used by the pipeline's
//! Phase 1 quick filter to avoid reprocessing unchanged files.
//!
//! ## Features
//!
//! - **Cache-based design**: All data is disposable and can be rebuilt
//! - **Simple schema**: Only tracks hash, modified_time, and size
//! - **Path-based lookups**: Fast queries by relative file path
//! - **Hash indexing**: Support for hash-based queries

use crate::{DbError, SurrealClient};
use crucible_core::processing::{ChangeDetectionError, ChangeDetectionResult, ChangeDetectionStore, FileState};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::{debug, trace};

/// Database record for file state
///
/// Matches the file_state table schema in schema.surql
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct FileStateRecord {
    /// Relative path from kiln root
    relative_path: String,
    /// BLAKE3 hash of file content
    file_hash: String,
    /// File modification time (Unix timestamp in seconds)
    modified_time: i64,
    /// File size in bytes
    file_size: i64,
}

impl FileStateRecord {
    /// Convert from FileState to database record
    fn from_file_state(path: &Path, state: &FileState) -> Result<Self, String> {
        let relative_path = path
            .to_str()
            .ok_or_else(|| "Invalid UTF-8 in path".to_string())?
            .to_string();

        // Convert SystemTime to Unix timestamp
        let modified_time = state
            .modified_time
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| format!("Invalid system time: {}", e))?
            .as_secs() as i64;

        Ok(Self {
            relative_path,
            file_hash: state.file_hash.clone(),
            modified_time,
            file_size: state.file_size as i64,
        })
    }

    /// Convert from database record to FileState
    fn to_file_state(&self) -> FileState {
        FileState {
            file_hash: self.file_hash.clone(),
            modified_time: SystemTime::UNIX_EPOCH
                + std::time::Duration::from_secs(self.modified_time as u64),
            file_size: self.file_size as u64,
        }
    }
}

/// SurrealDB-backed change detection store
///
/// Provides persistent storage for file state tracking. This is a cache layer -
/// all data can be cleared and rebuilt from source files without data loss.
pub(crate) struct SurrealChangeDetectionStore {
    client: SurrealClient,
}

impl SurrealChangeDetectionStore {
    /// Create a new store with the given SurrealDB client
    pub fn new(client: SurrealClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl ChangeDetectionStore for SurrealChangeDetectionStore {
    async fn get_file_state(&self, path: &Path) -> ChangeDetectionResult<Option<FileState>> {
        let relative_path = path
            .to_str()
            .ok_or_else(|| ChangeDetectionError::InvalidPath("Invalid UTF-8 in path".to_string()))?
            .to_string();

        trace!("Getting file state for: {}", relative_path);

        // Query by the relative_path field (which is indexed and unique)
        // We use string interpolation for the value since SurrealDB doesn't
        // handle parameterized strings in WHERE clauses reliably
        let query = format!("SELECT * FROM file_state WHERE relative_path = '{}' LIMIT 1",
            relative_path.replace("'", "\\'"));  // Escape single quotes

        let params = vec![];

        let result = self
            .client
            .query(&query, &params)
            .await
            .map_err(|e| ChangeDetectionError::Storage(format!("Failed to query file state: {}", e)))?;

        // Parse the first result
        if result.records.is_empty() {
            trace!("No stored state found for: {}", relative_path);
            return Ok(None);
        }

        let data_value = serde_json::to_value(&result.records[0].data)
            .map_err(|e| ChangeDetectionError::Serialization(format!("Failed to convert record data: {}", e)))?;

        // Extract fields manually to handle datetime conversion
        let obj = data_value.as_object()
            .ok_or_else(|| ChangeDetectionError::Serialization("Expected object".to_string()))?;

        let file_hash = obj.get("file_hash")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ChangeDetectionError::Serialization("Missing file_hash".to_string()))?
            .to_string();

        let file_size = obj.get("file_size")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ChangeDetectionError::Serialization("Missing file_size".to_string()))?;

        // Parse datetime string to Unix timestamp
        let datetime_str = obj.get("modified_time")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ChangeDetectionError::Serialization("Missing modified_time".to_string()))?;

        use chrono::{DateTime, Utc};
        let dt = DateTime::parse_from_rfc3339(datetime_str)
            .map_err(|e| ChangeDetectionError::Serialization(format!("Invalid datetime: {}", e)))?;
        let modified_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(dt.timestamp() as u64);

        let file_state = FileState {
            file_hash: file_hash.clone(),
            modified_time,
            file_size: file_size as u64,
        };

        trace!("Found stored state for {}: hash={}", relative_path, &file_hash[..8]);
        Ok(Some(file_state))
    }

    async fn store_file_state(&self, path: &Path, state: FileState) -> ChangeDetectionResult<()> {
        let record = FileStateRecord::from_file_state(path, &state)
            .map_err(|e| ChangeDetectionError::InvalidPath(e))?;

        trace!("Storing file state for {}: hash={}", record.relative_path, &record.file_hash[..8]);

        // Use CREATE OR REPLACE (true UPSERT) to ensure record is created if it doesn't exist
        // Convert path to a safe record ID by URL-encoding
        let record_id = urlencoding::encode(&record.relative_path).to_string();

        // Delete existing record and create new one (true upsert)
        // SurrealDB doesn't have native UPSERT, so we DELETE then CREATE
        let delete_query = format!("DELETE file_state:`{}`", record_id);
        self.client.query(&delete_query, &[]).await.ok(); // Ignore errors if doesn't exist

        // Use CONTENT syntax with inline values (parameters don't work in SET/CONTENT)
        // Convert Unix timestamp to ISO 8601 datetime string for SurrealDB
        use chrono::{DateTime, Utc};
        let dt = DateTime::<Utc>::from_timestamp(record.modified_time, 0)
            .ok_or_else(|| ChangeDetectionError::InvalidPath("Invalid timestamp".to_string()))?;
        let datetime_str = dt.to_rfc3339();

        let query = format!(r#"
            CREATE file_state:`{}` CONTENT {{
                relative_path: "{}",
                file_hash: "{}",
                modified_time: "{}",
                file_size: {}
            }}
        "#,
            record_id,
            record.relative_path.replace("\"", "\\\""), // Escape quotes
            record.file_hash.replace("\"", "\\\""),
            datetime_str,
            record.file_size
        );

        let params = vec![];

        self.client
            .query(&query, &params)
            .await
            .map_err(|e| ChangeDetectionError::Storage(format!("Failed to store file state: {}", e)))?;

        debug!("Successfully stored file state for: {}", record.relative_path);
        Ok(())
    }

    async fn delete_file_state(&self, path: &Path) -> ChangeDetectionResult<()> {
        let relative_path = path
            .to_str()
            .ok_or_else(|| ChangeDetectionError::InvalidPath("Invalid UTF-8 in path".to_string()))?
            .to_string();

        let record_id = urlencoding::encode(&relative_path).to_string();

        let query = "DELETE type::thing('file_state', $record_id)";
        let params = vec![Value::String(record_id)];

        self.client
            .query(query, &params)
            .await
            .map_err(|e| ChangeDetectionError::Storage(format!("Failed to delete file state: {}", e)))?;

        Ok(())
    }

    async fn list_tracked_files(&self) -> ChangeDetectionResult<Vec<PathBuf>> {
        let query = "SELECT relative_path FROM file_state";

        let result = self
            .client
            .query(query, &[])
            .await
            .map_err(|e| ChangeDetectionError::Storage(format!("Failed to list tracked files: {}", e)))?;

        let mut paths = Vec::new();
        for record in result.records {
            let data_value = serde_json::to_value(&record.data)
                .map_err(|e| ChangeDetectionError::Serialization(format!("Failed to convert record data: {}", e)))?;

            let file_record: FileStateRecord = serde_json::from_value(data_value)
                .map_err(|e| ChangeDetectionError::Serialization(format!("Failed to parse file state record: {}", e)))?;

            paths.push(PathBuf::from(file_record.relative_path));
        }

        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SurrealDbConfig;

    #[test]
    fn test_file_state_record_conversion() {
        let path = Path::new("test/file.md");
        let state = FileState {
            file_hash: "abc123".to_string(),
            modified_time: SystemTime::now(),
            file_size: 1234,
        };

        let record = FileStateRecord::from_file_state(path, &state).unwrap();
        assert_eq!(record.relative_path, "test/file.md");
        assert_eq!(record.file_hash, "abc123");
        assert_eq!(record.file_size, 1234);

        let back = record.to_file_state();
        assert_eq!(back.file_hash, state.file_hash);
        assert_eq!(back.file_size, state.file_size);
    }

    /// Bug #4 RED: Test that change detection retrieves stored file states
    ///
    /// This test currently FAILS because the query in get_file_state() uses
    /// incorrect parameter binding ($path with positional array instead of named map)
    #[tokio::test]
    async fn test_store_and_retrieve_file_state() {
        // Given: A SurrealDB client with in-memory storage
        let config = SurrealDbConfig {
            path: ":memory:".to_string(),
            namespace: "test_change_detection".to_string(),
            database: "test_change_detection".to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(30),
        };
        let client = crate::SurrealClient::new(config).await.unwrap();
        let store = SurrealChangeDetectionStore::new(client);

        // And: A file path and state
        let path = Path::new("test/note.md");
        let state = FileState {
            file_hash: "abc123def456".to_string(),
            modified_time: SystemTime::now(),
            file_size: 5678,
        };

        // When: We store the file state
        store.store_file_state(path, state.clone()).await.unwrap();

        // Then: We should be able to retrieve it
        let retrieved = store.get_file_state(path).await.unwrap();

        assert!(
            retrieved.is_some(),
            "BUG #4: get_file_state() should return the stored state but returns None due to parameter binding error"
        );

        let retrieved_state = retrieved.unwrap();
        assert_eq!(retrieved_state.file_hash, state.file_hash);
        assert_eq!(retrieved_state.file_size, state.file_size);
    }

    /// Bug #4 RED: Test that files are skipped when unchanged
    ///
    /// This test verifies the end-to-end change detection behavior
    #[tokio::test]
    async fn test_skip_unchanged_files() {
        // Given: A store with a tracked file
        let config = SurrealDbConfig {
            path: ":memory:".to_string(),
            namespace: "test_skip".to_string(),
            database: "test_skip".to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(30),
        };
        let client = crate::SurrealClient::new(config).await.unwrap();
        let store = SurrealChangeDetectionStore::new(client);

        let path = Path::new("docs/unchanged.md");
        let state = FileState {
            file_hash: "stable_hash_123".to_string(),
            modified_time: SystemTime::now(),
            file_size: 1000,
        };

        // When: We store the state and immediately retrieve it
        store.store_file_state(path, state.clone()).await.unwrap();
        let retrieved = store.get_file_state(path).await.unwrap();

        // Then: The state should match (file hasn't changed)
        assert!(
            retrieved.is_some(),
            "BUG #4: Should retrieve unchanged file state"
        );

        let retrieved_state = retrieved.unwrap();
        assert_eq!(
            retrieved_state.file_hash, state.file_hash,
            "Hash should match for unchanged file"
        );
        assert_eq!(
            retrieved_state.file_size, state.file_size,
            "Size should match for unchanged file"
        );
    }
}
