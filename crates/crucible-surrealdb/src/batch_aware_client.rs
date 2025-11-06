//! Batch-Aware Database Client
//!
//! This module provides a wrapper around SurrealClient that can check for pending
//! batch operations before performing database reads, ensuring consistency
//! between the database state and in-flight batch processing.

use crate::consistency::{ConsistencyLevel, FlushResult, PendingOperationsResult};
use crate::surreal_client::SurrealClient;
use crate::types::QueryResult;
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Trait for batch-aware database reads
#[async_trait::async_trait]
pub trait BatchAwareRead {
    /// Query with consistency guarantees
    async fn query_with_consistency(
        &self,
        sql: &str,
        params: &[serde_json::Value],
        consistency: ConsistencyLevel,
    ) -> Result<QueryResult>;

    /// Query a specific file with consistency guarantees
    async fn query_file_with_consistency(
        &self,
        file_path: &Path,
        consistency: ConsistencyLevel,
    ) -> Result<Option<FileDocumentState>>;

    /// Get pending operations for a file
    async fn get_pending_operations(
        &self,
        file_path: &Path,
    ) -> PendingOperationsResult;

    /// Force flush operations for specific files
    async fn flush_files(&self, file_paths: &[&Path]) -> Result<FlushResult>;

    /// Get current batch processing status
    async fn get_batch_status(&self) -> crate::consistency::FlushStatus;
}

/// Document state from database including consistency information
#[derive(Debug, Clone)]
pub struct FileDocumentState {
    /// Path to the file
    pub path: String,

    /// Document content hash
    pub file_hash: Option<String>,

    /// Document metadata (from database)
    pub metadata: Option<serde_json::Value>,

    /// Whether the document exists in database
    pub exists: bool,

    /// Pending operations affecting this file
    pub pending_operations: PendingOperationsResult,

    /// Timestamp when this state was captured
    pub captured_at: std::time::Instant,
}

/// Batch-aware wrapper around SurrealClient
pub struct BatchAwareSurrealClient {
    /// Inner SurrealClient for database operations
    client: SurrealClient,

    /// Event processor for checking pending operations
    event_processor: Option<Arc<dyn EventProcessor>>,
}

/// Trait for event processor integration
#[async_trait::async_trait]
pub trait EventProcessor: Send + Sync {
    /// Get pending operations for a file
    async fn get_pending_operations_for_file(
        &self,
        file_path: &Path,
    ) -> PendingOperationsResult;

    /// Force flush operations for specific files
    async fn flush_for_files(&self, file_paths: &[&Path]) -> Result<FlushResult>;

    /// Get current batch processing status
    async fn get_batch_status(&self) -> crate::consistency::FlushStatus;
}

impl BatchAwareSurrealClient {
    /// Create a new batch-aware client without event processor integration
    pub fn new(client: SurrealClient) -> Self {
        Self {
            client,
            event_processor: None,
        }
    }

    /// Create a new batch-aware client with event processor integration
    pub fn with_event_processor(
        client: SurrealClient,
        event_processor: Arc<dyn EventProcessor>,
    ) -> Self {
        Self {
            client,
            event_processor: Some(event_processor),
        }
    }

    /// Check if there are pending operations for a file
    async fn has_pending_operations(&self, file_path: &Path) -> bool {
        if let Some(processor) = &self.event_processor {
            let pending = processor.get_pending_operations_for_file(file_path).await;
            pending.has_pending()
        } else {
            false // No event processor = no pending operations
        }
    }

    /// Merge pending operations with database state
    async fn merge_state_with_pending(
        &self,
        file_path: &Path,
        db_state: Option<FileDocumentState>,
        pending_ops: PendingOperationsResult,
    ) -> FileDocumentState {
        let base_state = db_state.unwrap_or_else(|| FileDocumentState {
            path: file_path.to_string_lossy().to_string(),
            file_hash: None,
            metadata: None,
            exists: false,
            pending_operations: PendingOperationsResult::none(),
            captured_at: std::time::Instant::now(),
        });

        FileDocumentState {
            pending_operations: pending_ops,
            captured_at: std::time::Instant::now(),
            ..base_state
        }
    }

    /// Wait for pending operations to complete (with timeout)
    async fn wait_for_pending_completion(
        &self,
        file_path: &Path,
        timeout: Duration,
    ) -> Result<()> {
        let start = std::time::Instant::now();

        while start.elapsed() < timeout {
            if !self.has_pending_operations(file_path).await {
                return Ok(());
            }

            // Check status periodically
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Timeout reached, force flush
        if let Some(processor) = &self.event_processor {
            processor.flush_for_files(&[file_path]).await?;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl BatchAwareRead for BatchAwareSurrealClient {
    async fn query_with_consistency(
        &self,
        sql: &str,
        params: &[serde_json::Value],
        consistency: ConsistencyLevel,
    ) -> Result<QueryResult> {
        match consistency {
            ConsistencyLevel::Eventual => {
                // Standard database query
                self.client.query(sql, params).await.map_err(|e| anyhow::anyhow!("{}", e))
            }
            ConsistencyLevel::ReadAfterWrite => {
                // For general queries, we can't easily check pending operations
                // So we just do the query for now
                // In a full implementation, this would analyze the SQL to determine affected tables
                self.client.query(sql, params).await.map_err(|e| anyhow::anyhow!("{}", e))
            }
            ConsistencyLevel::Strong => {
                // Force flush of all pending operations before query
                if let Some(processor) = &self.event_processor {
                    // Get all files with pending operations and flush them
                    // This is a simplified implementation
                    let _flush_result = processor.get_batch_status().await;
                    // In a full implementation, we'd extract files and flush them
                }

                self.client.query(sql, params).await.map_err(|e| anyhow::anyhow!("{}", e))
            }
        }
    }

    async fn query_file_with_consistency(
        &self,
        file_path: &Path,
        consistency: ConsistencyLevel,
    ) -> Result<Option<FileDocumentState>> {
        match consistency {
            ConsistencyLevel::Eventual => {
                // Just query from database
                self.query_file_from_database(file_path).await
            }
            ConsistencyLevel::ReadAfterWrite => {
                // Check for pending operations first
                let pending_ops = self.get_pending_operations(file_path).await;

                if pending_ops.has_pending() {
                    // Wait for pending operations or timeout
                    self.wait_for_pending_completion(file_path, Duration::from_secs(2)).await?;
                }

                // Query from database
                self.query_file_from_database(file_path).await
            }
            ConsistencyLevel::Strong => {
                // Force flush immediately
                if let Some(processor) = &self.event_processor {
                    processor.flush_for_files(&[file_path]).await?;
                }

                // Query from database
                self.query_file_from_database(file_path).await
            }
        }
    }

    async fn get_pending_operations(
        &self,
        file_path: &Path,
    ) -> PendingOperationsResult {
        if let Some(processor) = &self.event_processor {
            processor.get_pending_operations_for_file(file_path).await
        } else {
            PendingOperationsResult::none()
        }
    }

    async fn flush_files(&self, file_paths: &[&Path]) -> Result<FlushResult> {
        if let Some(processor) = &self.event_processor {
            // Convert references to owned paths for the processor
            let path_refs: Vec<&Path> = file_paths.iter().copied().collect();
            processor.flush_for_files(&path_refs).await
        } else {
            Ok(FlushResult {
                operations_flushed: 0,
                flush_duration: Duration::from_millis(0),
                success_rate: 1.0,
            })
        }
    }

    async fn get_batch_status(&self) -> crate::consistency::FlushStatus {
        if let Some(processor) = &self.event_processor {
            processor.get_batch_status().await
        } else {
            crate::consistency::FlushStatus {
                pending_batches: 0,
                processing_events: 0,
                estimated_completion: None,
            }
        }
    }
}

impl BatchAwareSurrealClient {
    /// Query file state directly from database
    async fn query_file_from_database(&self, file_path: &Path) -> Result<Option<FileDocumentState>> {
        // Query for document by path
        let sql = "SELECT * FROM notes WHERE path = $path";
        let params = vec![
            serde_json::json!({"path": file_path.to_string_lossy()})
        ];

        match self.client.query(sql, &params).await {
            Ok(query_result) => {
                if !query_result.records.is_empty() {
                    let record = &query_result.records[0];
                    Ok(Some(FileDocumentState {
                        path: file_path.to_string_lossy().to_string(),
                        file_hash: record.data.get("file_hash").and_then(|v| v.as_str()).map(|s| s.to_string()),
                        metadata: record.data.get("metadata").cloned(),
                        exists: true,
                        pending_operations: PendingOperationsResult::none(),
                        captured_at: std::time::Instant::now(),
                    }))
                } else {
                    Ok(Some(FileDocumentState {
                        path: file_path.to_string_lossy().to_string(),
                        file_hash: None,
                        metadata: None,
                        exists: false,
                        pending_operations: PendingOperationsResult::none(),
                        captured_at: std::time::Instant::now(),
                    }))
                }
            }
            Err(e) => Err(anyhow::anyhow!("Database query failed: {}", e)),
        }
    }
}

/// Extension trait for SurrealClient to create batch-aware clients
pub trait SurrealClientBatchAware {
    /// Convert to batch-aware client
    fn batch_aware(self) -> BatchAwareSurrealClient;

    /// Convert to batch-aware client with event processor
    fn batch_aware_with_processor(
        self,
        event_processor: Arc<dyn EventProcessor>,
    ) -> BatchAwareSurrealClient;
}

impl SurrealClientBatchAware for SurrealClient {
    fn batch_aware(self) -> BatchAwareSurrealClient {
        BatchAwareSurrealClient::new(self)
    }

    fn batch_aware_with_processor(
        self,
        event_processor: Arc<dyn EventProcessor>,
    ) -> BatchAwareSurrealClient {
        BatchAwareSurrealClient::with_event_processor(self, event_processor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surreal_client::SurrealClient;

    #[tokio::test]
    async fn test_batch_aware_client_creation() -> Result<()> {
        let client = SurrealClient::new_memory().await?;
        let batch_client = client.batch_aware();

        // Test that we can create a batch aware client and check its status
        let status = batch_client.get_batch_status().await;
        assert_eq!(status.pending_batches, 0);
        assert_eq!(status.processing_events, 0);

        // Test file state query (this is the main use case)
        let file_path = std::path::Path::new("/non/existent/file.md");
        let state = batch_client
            .query_file_with_consistency(file_path, ConsistencyLevel::Eventual)
            .await?;

        assert!(state.is_some());
        assert!(!state.unwrap().exists);

        Ok(())
    }

    #[tokio::test]
    async fn test_file_state_query() -> Result<()> {
        let client = SurrealClient::new_memory().await?;
        let batch_client = client.batch_aware();

        // Test querying non-existent file
        let file_path = std::path::Path::new("/non/existent/file.md");
        let state = batch_client
            .query_file_with_consistency(file_path, ConsistencyLevel::Eventual)
            .await?;

        assert!(state.is_some());
        assert!(!state.unwrap().exists);

        Ok(())
    }

    // Mock event processor for testing
    struct MockEventProcessor;

    #[async_trait::async_trait]
    impl EventProcessor for MockEventProcessor {
        async fn get_pending_operations_for_file(
            &self,
            _file_path: &Path,
        ) -> PendingOperationsResult {
            PendingOperationsResult::none()
        }

        async fn flush_for_files(&self, _file_paths: &[&Path]) -> Result<FlushResult> {
            Ok(FlushResult {
                operations_flushed: 0,
                flush_duration: Duration::from_millis(10),
                success_rate: 1.0,
            })
        }

        async fn get_batch_status(&self) -> crate::consistency::FlushStatus {
            crate::consistency::FlushStatus {
                pending_batches: 0,
                processing_events: 0,
                estimated_completion: None,
            }
        }
    }

    #[tokio::test]
    async fn test_batch_aware_with_processor() -> Result<()> {
        let client = SurrealClient::new_memory().await?;
        let processor = Arc::new(MockEventProcessor);
        let batch_client = client.batch_aware_with_processor(processor);

        // Test that we can get batch status
        let status = batch_client.get_batch_status().await;
        assert_eq!(status.pending_batches, 0);
        assert_eq!(status.processing_events, 0);

        Ok(())
    }
}