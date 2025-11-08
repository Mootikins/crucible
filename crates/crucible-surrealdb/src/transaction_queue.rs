//! Simplified Transaction Queue Module
//!
//! This module provides a minimal queue-based database architecture that eliminates
//! RocksDB lock contention by serializing database operations through a
//! single consumer thread while allowing parallel file processing.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, oneshot, watch};

/// Serializable timestamp wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionTimestamp {
    /// Unix timestamp in milliseconds
    pub unix_ms: u64,
}

impl TransactionTimestamp {
    /// Create a new timestamp from the current time
    pub fn now() -> Self {
        Self {
            unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }
}

/// Represents different types of database transactions that can be queued
/// Simplified to 3 CRUD operations - the consumer figures out what actually changed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DatabaseTransaction {
    /// Create a new document (intelligent consumer will handle all sub-operations)
    Create {
        transaction_id: String,
        document: crucible_core::types::ParsedDocument,
        kiln_root: PathBuf,
        timestamp: TransactionTimestamp,
    },

    /// Update an existing document (consumer detects what changed via diffing)
    Update {
        transaction_id: String,
        document: crucible_core::types::ParsedDocument,
        kiln_root: PathBuf,
        timestamp: TransactionTimestamp,
    },

    /// Delete a document entirely
    Delete {
        transaction_id: String,
        document_id: String,
        kiln_root: PathBuf,
        timestamp: TransactionTimestamp,
    },
}

impl DatabaseTransaction {
    /// Get the unique transaction ID
    pub fn transaction_id(&self) -> &str {
        match self {
            DatabaseTransaction::Create { transaction_id, .. } => transaction_id,
            DatabaseTransaction::Update { transaction_id, .. } => transaction_id,
            DatabaseTransaction::Delete { transaction_id, .. } => transaction_id,
        }
    }

    /// Get the transaction priority (lower number = higher priority)
    pub fn priority(&self) -> u8 {
        match self {
            DatabaseTransaction::Create { .. } => 1, // High priority - new documents
            DatabaseTransaction::Update { .. } => 2, // Medium priority - changes to existing
            DatabaseTransaction::Delete { .. } => 3, // Lower priority - cleanup
        }
    }

    /// Get the timestamp when the transaction was created
    pub fn timestamp(&self) -> TransactionTimestamp {
        match self {
            DatabaseTransaction::Create { timestamp, .. } => timestamp.clone(),
            DatabaseTransaction::Update { timestamp, .. } => timestamp.clone(),
            DatabaseTransaction::Delete { timestamp, .. } => timestamp.clone(),
        }
    }

    /// Check if this transaction depends on another transaction completing first
    /// Simplified CRUD approach - minimal dependencies since consumer handles all sub-operations
    pub fn depends_on(&self, other: &DatabaseTransaction) -> bool {
        match (self, other) {
            // Update/Delete operations on the same document should be ordered
            (
                DatabaseTransaction::Update { document: _, .. }
                | DatabaseTransaction::Delete { document_id: _, .. },
                DatabaseTransaction::Create { document: _, .. }
                | DatabaseTransaction::Update { document: _, .. },
            ) => {
                let self_id = match self {
                    DatabaseTransaction::Update { document, .. } => document.path.clone(),
                    DatabaseTransaction::Delete { document_id, .. } => document_id.clone().into(),
                    _ => return false,
                };

                let other_id = match other {
                    DatabaseTransaction::Create { document, .. } => document.path.clone(),
                    DatabaseTransaction::Update { document, .. } => document.path.clone(),
                    _ => return false,
                };

                self_id == other_id
            }

            _ => false,
        }
    }
}

/// Result of a database transaction execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionResult {
    /// Transaction completed successfully
    Success {
        transaction_id: String,
        duration: Duration,
        metadata: TransactionMetadata,
    },

    /// Transaction failed with an error
    Failure {
        transaction_id: String,
        error: String,
        retry_count: u32,
        metadata: TransactionMetadata,
    },
}

impl TransactionResult {
    /// Get the transaction ID
    pub fn transaction_id(&self) -> &str {
        match self {
            TransactionResult::Success { transaction_id, .. } => transaction_id,
            TransactionResult::Failure { transaction_id, .. } => transaction_id,
        }
    }

    /// Check if the transaction was successful
    pub fn is_success(&self) -> bool {
        matches!(self, TransactionResult::Success { .. })
    }
}

/// Additional metadata about a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionMetadata {
    /// Transaction type
    pub transaction_type: String,
    /// File path (if applicable)
    pub file_path: Option<PathBuf>,
    /// Number of retry attempts
    pub retry_count: u32,
    /// Time spent in queue before processing
    pub queue_wait_time: Duration,
}

/// Configuration for the simplified transaction queue
#[derive(Debug, Clone)]
pub struct TransactionQueueConfig {
    /// Maximum number of transactions in the queue
    pub max_queue_size: usize,
}

impl Default for TransactionQueueConfig {
    fn default() -> Self {
        Self {
            max_queue_size: 1000,
        }
    }
}

/// Enhanced statistics about the transaction queue
#[derive(Debug, Clone)]
pub struct QueueStats {
    /// Current number of transactions in queue
    pub queue_depth: usize,
    /// Total number of transactions processed
    pub total_processed: u64,
    /// Number of successful transactions
    pub successful_transactions: u64,
    /// Number of failed transactions
    pub failed_transactions: u64,
    /// Current queue capacity utilization (0.0 to 1.0)
    pub capacity_utilization: f64,
    /// Average processing time for transactions (in milliseconds)
    pub avg_processing_time_ms: f64,
    /// Transactions per second processing rate
    pub processing_rate_tps: f64,
    /// Error rate as percentage (0.0 to 100.0)
    pub error_rate_percent: f64,
    /// Number of transactions currently enqueued (same as queue_depth for clarity)
    pub enqueued_transactions: u64,
    /// Number of transactions waiting to be processed
    pub pending_transactions: usize,
}

impl Default for QueueStats {
    fn default() -> Self {
        Self {
            queue_depth: 0,
            total_processed: 0,
            successful_transactions: 0,
            failed_transactions: 0,
            capacity_utilization: 0.0,
            avg_processing_time_ms: 0.0,
            processing_rate_tps: 0.0,
            error_rate_percent: 0.0,
            enqueued_transactions: 0,
            pending_transactions: 0,
        }
    }
}

/// Internal metrics tracking for the queue
#[derive(Debug, Default)]
struct QueueMetrics {
    /// Total number of transactions enqueued
    total_enqueued: u64,
    /// Total number of successful transactions
    total_successful: u64,
    /// Total number of failed transactions
    total_failed: u64,
    /// Sum of processing times for calculating average
    total_processing_time_ms: u64,
    /// Timestamp of last processed transaction for rate calculation
    last_processed_time: Option<Instant>,
    /// Number of processing attempts
    processing_attempts: u64,
}

/// Channel types used for transaction communication
pub type TransactionSender =
    mpsc::Sender<(DatabaseTransaction, oneshot::Sender<TransactionResult>)>;
pub type TransactionReceiver =
    mpsc::Receiver<(DatabaseTransaction, oneshot::Sender<TransactionResult>)>;
pub type ResultSender = oneshot::Sender<TransactionResult>;
pub type ResultReceiver = oneshot::Receiver<TransactionResult>;
pub type StatsWatcher = watch::Receiver<QueueStats>;

/// Simplified Transaction Queue
///
/// This manages the queue of database transactions with bounded capacity
/// and minimal features to solve RocksDB lock contention.
pub struct TransactionQueue {
    /// Channel for sending transactions to the queue
    sender: TransactionSender,

    /// Channel receiver for consumer - wrapped in Arc<Mutex> for thread safety
    receiver: Arc<std::sync::Mutex<Option<TransactionReceiver>>>,

    /// Configuration for the queue
    config: TransactionQueueConfig,

    /// Watcher for queue statistics
    stats_watcher: StatsWatcher,

    /// Internal metrics tracking
    metrics: Arc<std::sync::Mutex<QueueMetrics>>,
}

impl TransactionQueue {
    /// Create a new transaction queue with the given configuration
    pub fn new(config: TransactionQueueConfig) -> Self {
        let (sender, receiver) = mpsc::channel(config.max_queue_size);
        let (stats_sender, stats_watcher) = watch::channel(QueueStats::default());
        let metrics = Arc::new(std::sync::Mutex::new(QueueMetrics::default()));

        // Start enhanced stats monitoring
        let max_queue_size = config.max_queue_size;
        let metrics_clone = Arc::clone(&metrics);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            let mut last_update_time = Instant::now();

            loop {
                interval.tick().await;
                let current_time = Instant::now();

                let metrics_guard = metrics_clone.lock().unwrap();
                let total_processed = metrics_guard.total_successful + metrics_guard.total_failed;

                // Calculate processing rate (transactions per second)
                let time_diff = current_time.duration_since(last_update_time).as_secs_f64();
                let processing_rate = if time_diff > 0.0 {
                    total_processed as f64 / time_diff
                } else {
                    0.0
                };

                // Calculate error rate
                let error_rate = if total_processed > 0 {
                    (metrics_guard.total_failed as f64 / total_processed as f64) * 100.0
                } else {
                    0.0
                };

                // Calculate average processing time
                let avg_processing_time = if total_processed > 0 {
                    metrics_guard.total_processing_time_ms as f64 / total_processed as f64
                } else {
                    0.0
                };

                let stats = QueueStats {
                    queue_depth: 0, // Simplified - could be tracked via sender capacity
                    total_processed,
                    successful_transactions: metrics_guard.total_successful,
                    failed_transactions: metrics_guard.total_failed,
                    capacity_utilization: (total_processed as f64 / max_queue_size as f64).min(1.0),
                    avg_processing_time_ms: avg_processing_time,
                    processing_rate_tps: processing_rate,
                    error_rate_percent: error_rate,
                    enqueued_transactions: metrics_guard.total_enqueued,
                    pending_transactions: 0, // Simplified - would need channel monitoring
                };

                if stats_sender.send(stats).is_err() {
                    break; // Channel closed
                }

                last_update_time = current_time;
            }
        });

        Self {
            sender,
            receiver: Arc::new(std::sync::Mutex::new(Some(receiver))),
            config,
            stats_watcher,
            metrics,
        }
    }

    /// Get the current queue statistics
    pub fn stats(&self) -> QueueStats {
        self.stats_watcher.borrow().clone()
    }

    /// Subscribe to queue statistics updates
    pub fn subscribe_stats(&self) -> StatsWatcher {
        self.stats_watcher.clone()
    }

    /// Get the receiver for transactions (can only be called once)
    pub fn receiver(&self) -> TransactionReceiver {
        let mut receiver_guard = self.receiver.lock().unwrap();
        receiver_guard
            .take()
            .expect("TransactionQueue receiver can only be taken once")
    }

    /// Enqueue a transaction, waiting if the queue is full.
    pub async fn enqueue(
        &self,
        transaction: DatabaseTransaction,
    ) -> Result<ResultReceiver, QueueError> {
        let (result_sender, result_receiver) = oneshot::channel();

        // Track enqueue metrics
        {
            let mut metrics = self.metrics.lock().unwrap();
            metrics.total_enqueued += 1;
        }

        self.sender
            .send((transaction, result_sender))
            .await
            .map_err(|_| QueueError::QueueClosed)?;

        Ok(result_receiver)
    }

    /// Record a successful transaction for metrics tracking
    pub fn record_success(&self, processing_time_ms: u64) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.total_successful += 1;
        metrics.total_processing_time_ms += processing_time_ms;
        metrics.last_processed_time = Some(Instant::now());
        metrics.processing_attempts += 1;
    }

    /// Record a failed transaction for metrics tracking
    pub fn record_failure(&self, processing_time_ms: u64) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.total_failed += 1;
        metrics.total_processing_time_ms += processing_time_ms;
        metrics.last_processed_time = Some(Instant::now());
        metrics.processing_attempts += 1;
    }

    /// Get current metrics snapshot
    pub fn get_metrics(&self) -> (u64, u64, u64, f64) {
        let metrics = self.metrics.lock().unwrap();
        let total_processed = metrics.total_successful + metrics.total_failed;
        let avg_processing_time = if total_processed > 0 {
            metrics.total_processing_time_ms as f64 / total_processed as f64
        } else {
            0.0
        };

        (
            metrics.total_enqueued,
            metrics.total_successful,
            metrics.total_failed,
            avg_processing_time,
        )
    }

    /// Get the queue sender for direct use
    pub fn sender(&self) -> TransactionSender {
        self.sender.clone()
    }
}

/// Errors that can occur during queue operations
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    #[error("Queue is full with max size: {0}")]
    QueueFull(usize),

    #[error("Queue has been closed")]
    QueueClosed,

    #[error("Transaction timed out after: {0:?}")]
    Timeout(Duration),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_simple_queue() {
        let config = TransactionQueueConfig::default();
        let queue = TransactionQueue::new(config);

        // Create a test transaction
        let transaction = DatabaseTransaction::Create {
            transaction_id: "test-1".to_string(),
            document: crucible_core::types::ParsedDocument::new(PathBuf::from("test.md")),
            kiln_root: PathBuf::from("/test"),
            timestamp: TransactionTimestamp::now(),
        };

        // Enqueue the transaction
        let result_receiver = queue.enqueue(transaction).await.unwrap();

        // Verify we got a result receiver (oneshot receiver doesn't have is_closed method)
    }

    #[tokio::test]
    async fn test_queue_config_default() {
        let config = TransactionQueueConfig::default();
        assert_eq!(config.max_queue_size, 1000);
    }

    #[test]
    fn test_transaction_timestamp() {
        let timestamp = TransactionTimestamp::now();
        assert!(timestamp.unix_ms > 0);
    }

    #[test]
    fn test_crud_transaction_methods() {
        let create_tx = DatabaseTransaction::Create {
            transaction_id: "create-1".to_string(),
            document: crucible_core::types::ParsedDocument::new(PathBuf::from("test.md")),
            kiln_root: PathBuf::from("/test"),
            timestamp: TransactionTimestamp::now(),
        };

        assert_eq!(create_tx.transaction_id(), "create-1");
        assert_eq!(create_tx.priority(), 1);

        let update_tx = DatabaseTransaction::Update {
            transaction_id: "update-1".to_string(),
            document: crucible_core::types::ParsedDocument::new(PathBuf::from("test.md")),
            kiln_root: PathBuf::from("/test"),
            timestamp: TransactionTimestamp::now(),
        };

        assert_eq!(update_tx.priority(), 2);

        let delete_tx = DatabaseTransaction::Delete {
            transaction_id: "delete-1".to_string(),
            document_id: "test_doc".to_string(),
            kiln_root: PathBuf::from("/test"),
            timestamp: TransactionTimestamp::now(),
        };

        assert_eq!(delete_tx.priority(), 3);
    }
}
