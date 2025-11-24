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

        // Query by relative_path using the unique index
        let query = "SELECT * FROM file_state WHERE relative_path = $path LIMIT 1";

        let params = vec![Value::String(relative_path.clone())];

        let result = self
            .client
            .query(query, &params)
            .await
            .map_err(|e| ChangeDetectionError::Storage(format!("Failed to query file state: {}", e)))?;

        // Parse the first result
        if result.records.is_empty() {
            trace!("No stored state found for: {}", relative_path);
            return Ok(None);
        }

        let data_value = serde_json::to_value(&result.records[0].data)
            .map_err(|e| ChangeDetectionError::Serialization(format!("Failed to convert record data: {}", e)))?;

        let record: FileStateRecord = serde_json::from_value(data_value)
            .map_err(|e| ChangeDetectionError::Serialization(format!("Failed to parse file state record: {}", e)))?;

        trace!("Found stored state for {}: hash={}", relative_path, &record.file_hash[..8]);
        Ok(Some(record.to_file_state()))
    }

    async fn store_file_state(&self, path: &Path, state: FileState) -> ChangeDetectionResult<()> {
        let record = FileStateRecord::from_file_state(path, &state)
            .map_err(|e| ChangeDetectionError::InvalidPath(e))?;

        trace!("Storing file state for {}: hash={}", record.relative_path, &record.file_hash[..8]);

        // Use UPSERT pattern with parameterized record ID
        // Convert path to a safe record ID by URL-encoding
        let record_id = urlencoding::encode(&record.relative_path).to_string();

        let query = r#"
            UPDATE type::thing('file_state', $record_id) CONTENT {
                relative_path: $relative_path,
                file_hash: $file_hash,
                modified_time: type::datetime($modified_time),
                file_size: $file_size
            }
        "#;

        let params = vec![
            Value::String(record_id),
            Value::String(record.relative_path.clone()),
            Value::String(record.file_hash.clone()),
            Value::Number(record.modified_time.into()),
            Value::Number(record.file_size.into()),
        ];

        self.client
            .query(query, &params)
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
}
