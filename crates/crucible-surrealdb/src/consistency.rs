//! Database consistency levels for batch-aware operations
//!
//! This module provides consistency guarantees when reading metadata
//! from the database while batch operations may be pending.
//!
//! # Note
//! This module provides the consistency framework for the batch-aware client.
//! It's part of the ongoing queue-based processing architecture refactoring and
//! is strategically preserved until the infrastructure is fully integrated.

#![allow(dead_code)]

use anyhow::Result;
use std::path::PathBuf;
use std::time::Instant;

/// Consistency levels for database reads
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConsistencyLevel {
    /// Standard database read without checking batch queue (current behavior)
    /// May return stale data if changes are pending in batches
    #[default]
    Eventual,

    /// Check batch queue for pending operations before reading
    /// Returns merged state of database + pending changes
    ReadAfterWrite,

    /// Force flush of pending batches before reading
    /// Guarantees most up-to-date state but with higher latency
    Strong,
}

/// Status of a pending operation for a specific file
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingOperation {
    /// Path to the file being processed
    pub file_path: PathBuf,

    /// Type of operation
    pub operation_type: OperationType,

    /// Batch identifier if part of a batch
    pub batch_id: Option<uuid::Uuid>,

    /// When the operation was queued
    pub queued_at: Instant,

    /// Estimated completion time
    pub estimated_completion: Option<Instant>,

    /// Event ID for tracking
    pub event_id: uuid::Uuid,
}

/// Types of operations that can be pending
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationType {
    /// File creation
    Create,

    /// File content update
    Update,

    /// File deletion
    Delete,

    /// Metadata update
    MetadataUpdate,

    /// Embedding generation
    Embedding,
}

/// Processing status for pending operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessingStatus {
    /// No pending operations for this file
    None,

    /// Operations are queued but not yet processing
    Queued(Vec<PendingOperation>),

    /// Operations are currently being processed
    Processing(Vec<PendingOperation>),

    /// Operations are queued AND processing (concurrent batches)
    QueuedAndProcessing {
        queued: Vec<PendingOperation>,
        processing: Vec<PendingOperation>,
    },
}

/// Result of checking pending operations
#[derive(Debug, Clone)]
pub struct PendingOperationsResult {
    /// Processing status for the file
    pub status: ProcessingStatus,

    /// Total number of pending operations
    pub total_pending: usize,

    /// Estimated time until all operations complete
    pub estimated_completion: Option<Instant>,

    /// Whether any operations are currently processing
    pub has_processing: bool,
}

impl PendingOperationsResult {
    /// Create a new result with no pending operations
    pub fn none() -> Self {
        Self {
            status: ProcessingStatus::None,
            total_pending: 0,
            estimated_completion: None,
            has_processing: false,
        }
    }

    /// Create a new result with queued operations
    pub fn queued(operations: Vec<PendingOperation>) -> Self {
        let total = operations.len();
        let estimated_completion = operations
            .iter()
            .filter_map(|op| op.estimated_completion)
            .max();

        Self {
            status: ProcessingStatus::Queued(operations.clone()),
            total_pending: total,
            estimated_completion,
            has_processing: false,
        }
    }

    /// Create a new result with processing operations
    pub fn processing(operations: Vec<PendingOperation>) -> Self {
        let total = operations.len();
        let estimated_completion = operations
            .iter()
            .filter_map(|op| op.estimated_completion)
            .max();

        Self {
            status: ProcessingStatus::Processing(operations.clone()),
            total_pending: total,
            estimated_completion,
            has_processing: true,
        }
    }

    /// Create a new result with both queued and processing operations
    pub fn queued_and_processing(
        queued: Vec<PendingOperation>,
        processing: Vec<PendingOperation>,
    ) -> Self {
        let total = queued.len() + processing.len();
        let all_operations = queued.iter().chain(processing.iter());
        let estimated_completion = all_operations
            .filter_map(|op| op.estimated_completion)
            .max();

        Self {
            status: ProcessingStatus::QueuedAndProcessing {
                queued: queued.clone(),
                processing: processing.clone(),
            },
            total_pending: total,
            estimated_completion,
            has_processing: true,
        }
    }

    /// Check if there are any pending operations
    pub fn has_pending(&self) -> bool {
        self.total_pending > 0
    }

    /// Check if operations are currently being processed
    pub fn is_processing(&self) -> bool {
        self.has_processing
    }
}

/// Configuration for consistency checking
#[derive(Debug, Clone)]
pub struct ConsistencyConfig {
    /// Default consistency level for operations
    pub default_level: ConsistencyLevel,

    /// Maximum time to wait for batch processing (ms)
    pub max_wait_time_ms: u64,

    /// Whether to enable automatic strong consistency for critical operations
    pub enable_auto_strong: bool,

    /// Files that always require strong consistency
    pub critical_files: Vec<String>,
}

impl Default for ConsistencyConfig {
    fn default() -> Self {
        Self {
            default_level: ConsistencyLevel::Eventual,
            max_wait_time_ms: 5000, // 5 seconds
            enable_auto_strong: false,
            critical_files: vec![],
        }
    }
}

/// Error types for consistency operations
#[derive(Debug, thiserror::Error)]
pub enum ConsistencyError {
    #[error("Timeout waiting for batch processing: {0}ms")]
    BatchTimeout(u64),

    #[error("Flush operation failed: {0}")]
    FlushFailed(String),

    #[error("Invalid consistency level: {0}")]
    InvalidConsistencyLevel(String),

    #[error("Processing queue is full")]
    QueueFull,

    #[error("Operation cancelled")]
    Cancelled,
}

/// Result type for consistency operations
pub type ConsistencyResult<T> = Result<T, ConsistencyError>;

/// Result of a flush operation
#[derive(Debug, Clone)]
pub struct FlushResult {
    /// Number of operations flushed
    pub operations_flushed: usize,

    /// Time taken to perform the flush
    pub flush_duration: std::time::Duration,

    /// Success rate of the flush (0.0 to 1.0)
    pub success_rate: f64,
}

/// Status of batch processing
#[derive(Debug, Clone)]
pub struct FlushStatus {
    /// Number of pending batches
    pub pending_batches: usize,

    /// Number of events currently being processed
    pub processing_events: usize,

    /// Estimated completion time for current batch
    pub estimated_completion: Option<std::time::Instant>,
}

impl FlushStatus {
    /// Check if there are any pending operations
    pub fn has_pending(&self) -> bool {
        self.pending_batches > 0 || self.processing_events > 0
    }

    /// Get total number of operations in progress
    pub fn total_in_progress(&self) -> usize {
        self.pending_batches + self.processing_events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consistency_level_default_is_eventual() {
        let level = ConsistencyLevel::default();
        assert_eq!(level, ConsistencyLevel::Eventual);
    }

    #[test]
    fn consistency_level_variants_are_distinct() {
        assert_ne!(ConsistencyLevel::Eventual, ConsistencyLevel::ReadAfterWrite);
        assert_ne!(ConsistencyLevel::ReadAfterWrite, ConsistencyLevel::Strong);
        assert_ne!(ConsistencyLevel::Eventual, ConsistencyLevel::Strong);
    }

    #[test]
    fn pending_operations_result_none_has_no_pending() {
        let result = PendingOperationsResult::none();
        assert!(!result.has_pending());
        assert!(!result.is_processing());
        assert_eq!(result.total_pending, 0);
        assert!(result.estimated_completion.is_none());
        assert!(matches!(result.status, ProcessingStatus::None));
    }

    #[test]
    fn pending_operations_result_queued_reports_pending() {
        let op = PendingOperation {
            file_path: PathBuf::from("test.md"),
            operation_type: OperationType::Create,
            batch_id: None,
            queued_at: Instant::now(),
            estimated_completion: None,
            event_id: uuid::Uuid::new_v4(),
        };
        let result = PendingOperationsResult::queued(vec![op]);
        assert!(result.has_pending());
        assert!(!result.is_processing());
        assert_eq!(result.total_pending, 1);
        assert!(matches!(result.status, ProcessingStatus::Queued(_)));
    }

    #[test]
    fn pending_operations_result_processing_reports_active() {
        let op = PendingOperation {
            file_path: PathBuf::from("note.md"),
            operation_type: OperationType::Update,
            batch_id: Some(uuid::Uuid::new_v4()),
            queued_at: Instant::now(),
            estimated_completion: Some(Instant::now()),
            event_id: uuid::Uuid::new_v4(),
        };
        let result = PendingOperationsResult::processing(vec![op]);
        assert!(result.has_pending());
        assert!(result.is_processing());
        assert_eq!(result.total_pending, 1);
        assert!(result.estimated_completion.is_some());
        assert!(matches!(result.status, ProcessingStatus::Processing(_)));
    }

    #[test]
    fn pending_operations_result_queued_and_processing_combines_counts() {
        let make_op = |path: &str, op_type: OperationType| PendingOperation {
            file_path: PathBuf::from(path),
            operation_type: op_type,
            batch_id: None,
            queued_at: Instant::now(),
            estimated_completion: None,
            event_id: uuid::Uuid::new_v4(),
        };

        let queued = vec![make_op("a.md", OperationType::Create)];
        let processing = vec![
            make_op("b.md", OperationType::Update),
            make_op("c.md", OperationType::Delete),
        ];
        let result = PendingOperationsResult::queued_and_processing(queued, processing);

        assert!(result.has_pending());
        assert!(result.is_processing());
        assert_eq!(result.total_pending, 3);
        assert!(matches!(
            result.status,
            ProcessingStatus::QueuedAndProcessing { .. }
        ));
    }

    #[test]
    fn consistency_config_default_values() {
        let config = ConsistencyConfig::default();
        assert_eq!(config.default_level, ConsistencyLevel::Eventual);
        assert_eq!(config.max_wait_time_ms, 5000);
        assert!(!config.enable_auto_strong);
        assert!(config.critical_files.is_empty());
    }

    #[test]
    fn flush_status_has_pending_when_batches_exist() {
        let status = FlushStatus {
            pending_batches: 2,
            processing_events: 0,
            estimated_completion: None,
        };
        assert!(status.has_pending());
        assert_eq!(status.total_in_progress(), 2);
    }

    #[test]
    fn flush_status_has_pending_when_events_processing() {
        let status = FlushStatus {
            pending_batches: 0,
            processing_events: 3,
            estimated_completion: None,
        };
        assert!(status.has_pending());
        assert_eq!(status.total_in_progress(), 3);
    }

    #[test]
    fn flush_status_no_pending_when_empty() {
        let status = FlushStatus {
            pending_batches: 0,
            processing_events: 0,
            estimated_completion: None,
        };
        assert!(!status.has_pending());
        assert_eq!(status.total_in_progress(), 0);
    }

    #[test]
    fn flush_status_total_combines_batches_and_events() {
        let status = FlushStatus {
            pending_batches: 5,
            processing_events: 3,
            estimated_completion: Some(Instant::now()),
        };
        assert_eq!(status.total_in_progress(), 8);
    }

    #[test]
    fn operation_type_variants_exist() {
        let types = [
            OperationType::Create,
            OperationType::Update,
            OperationType::Delete,
            OperationType::MetadataUpdate,
            OperationType::Embedding,
        ];
        assert_eq!(types.len(), 5);
    }
}
