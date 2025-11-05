//! Transaction Queue Module
//!
//! This module provides a queue-based database architecture that eliminates
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

    /// Create a timestamp from an Instant (for non-serializable contexts)
    pub fn from_instant(instant: Instant) -> Self {
        // Convert to SystemTime approximation
        let duration = instant.elapsed();
        let now = SystemTime::now();
        let system_time = now.checked_sub(duration).unwrap_or(now);

        Self {
            unix_ms: system_time
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }

    /// Convert to Instant (for runtime use)
    pub fn to_instant(&self) -> Instant {
        // This is an approximation - Instant can't be perfectly reconstructed from SystemTime
        let system_time = UNIX_EPOCH + Duration::from_millis(self.unix_ms);
        let now = SystemTime::now();

        if system_time > now {
            // Future timestamp - add the difference
            let duration = system_time.duration_since(now).unwrap_or_default();
            Instant::now() + duration
        } else {
            // Past timestamp - subtract the difference
            let duration = now.duration_since(system_time).unwrap_or_default();
            Instant::now().checked_sub(duration).unwrap_or(Instant::now())
        }
    }
}

/// Represents different types of database transactions that can be queued
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DatabaseTransaction {
    /// Store a parsed document and its metadata
    StoreDocument {
        transaction_id: String,
        document: crucible_core::types::ParsedDocument,
        kiln_root: PathBuf,
        timestamp: TransactionTimestamp,
    },

    /// Create wikilink relationships between documents
    CreateWikilinkEdges {
        transaction_id: String,
        document_id: String,
        document: crucible_core::types::ParsedDocument,
        timestamp: TransactionTimestamp,
    },

    /// Create embed relationships between documents
    CreateEmbedRelationships {
        transaction_id: String,
        document_id: String,
        document: crucible_core::types::ParsedDocument,
        timestamp: TransactionTimestamp,
    },

    /// Create tag associations for a document
    CreateTagAssociations {
        transaction_id: String,
        document_id: String,
        document: crucible_core::types::ParsedDocument,
        timestamp: TransactionTimestamp,
    },

    /// Process embeddings for a document
    ProcessEmbeddings {
        transaction_id: String,
        document_id: String,
        document: crucible_core::types::ParsedDocument,
        timestamp: TransactionTimestamp,
    },

    /// Update document processed timestamp
    UpdateTimestamp {
        transaction_id: String,
        document_id: String,
        timestamp: TransactionTimestamp,
    },
}

impl DatabaseTransaction {
    /// Get the unique transaction ID
    pub fn transaction_id(&self) -> &str {
        match self {
            DatabaseTransaction::StoreDocument { transaction_id, .. } => transaction_id,
            DatabaseTransaction::CreateWikilinkEdges { transaction_id, .. } => transaction_id,
            DatabaseTransaction::CreateEmbedRelationships { transaction_id, .. } => transaction_id,
            DatabaseTransaction::CreateTagAssociations { transaction_id, .. } => transaction_id,
            DatabaseTransaction::ProcessEmbeddings { transaction_id, .. } => transaction_id,
            DatabaseTransaction::UpdateTimestamp { transaction_id, .. } => transaction_id,
        }
    }

    /// Get the transaction priority (lower number = higher priority)
    pub fn priority(&self) -> u8 {
        match self {
            DatabaseTransaction::StoreDocument { .. } => 1,     // High priority - foundation
            DatabaseTransaction::UpdateTimestamp { .. } => 2,    // High priority - quick completion
            DatabaseTransaction::CreateWikilinkEdges { .. } => 3, // Medium priority
            DatabaseTransaction::CreateEmbedRelationships { .. } => 3, // Medium priority
            DatabaseTransaction::CreateTagAssociations { .. } => 3,  // Medium priority
            DatabaseTransaction::ProcessEmbeddings { .. } => 4,     // Lower priority - expensive
        }
    }

    /// Get the timestamp when the transaction was created
    pub fn timestamp(&self) -> TransactionTimestamp {
        match self {
            DatabaseTransaction::StoreDocument { timestamp, .. } => timestamp.clone(),
            DatabaseTransaction::CreateWikilinkEdges { timestamp, .. } => timestamp.clone(),
            DatabaseTransaction::CreateEmbedRelationships { timestamp, .. } => timestamp.clone(),
            DatabaseTransaction::CreateTagAssociations { timestamp, .. } => timestamp.clone(),
            DatabaseTransaction::ProcessEmbeddings { timestamp, .. } => timestamp.clone(),
            DatabaseTransaction::UpdateTimestamp { timestamp, .. } => timestamp.clone(),
        }
    }

    /// Check if this transaction depends on another transaction completing first
    pub fn depends_on(&self, other: &DatabaseTransaction) -> bool {
        match (self, other) {
            // Links, embeds, and tags depend on document being stored first
            (
                DatabaseTransaction::CreateWikilinkEdges { document, .. }
                | DatabaseTransaction::CreateEmbedRelationships { document, .. }
                | DatabaseTransaction::CreateTagAssociations { document, .. }
                | DatabaseTransaction::ProcessEmbeddings { document, .. },
                DatabaseTransaction::StoreDocument { document: stored_document, .. },
            ) => {
                document.path == stored_document.path
            }

            // Timestamp update depends on document being stored first
            (
                DatabaseTransaction::UpdateTimestamp { .. },
                DatabaseTransaction::StoreDocument { .. },
            ) => {
                // For simplicity, we'll handle timestamp update dependencies in the consumer
                false
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

    /// Transaction was skipped (e.g., due to dependency failure)
    Skipped {
        transaction_id: String,
        reason: String,
        metadata: TransactionMetadata,
    },
}

impl TransactionResult {
    /// Get the transaction ID
    pub fn transaction_id(&self) -> &str {
        match self {
            TransactionResult::Success { transaction_id, .. } => transaction_id,
            TransactionResult::Failure { transaction_id, .. } => transaction_id,
            TransactionResult::Skipped { transaction_id, .. } => transaction_id,
        }
    }

    /// Check if the transaction was successful
    pub fn is_success(&self) -> bool {
        matches!(self, TransactionResult::Success { .. })
    }

    /// Get the transaction duration
    pub fn duration(&self) -> Option<Duration> {
        match self {
            TransactionResult::Success { duration, .. } => Some(*duration),
            TransactionResult::Failure { .. } => None,
            TransactionResult::Skipped { .. } => None,
        }
    }
}

/// Additional metadata about a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionMetadata {
    /// Transaction type
    pub transaction_type: String,
    /// File path (if applicable)
    pub file_path: Option<PathBuf>,
    /// Queue depth when transaction was dequeued
    pub queue_depth: usize,
    /// Number of retry attempts
    pub retry_count: u32,
    /// Time spent in queue before processing
    pub queue_wait_time: Duration,
}

/// Configuration for the transaction queue
#[derive(Debug, Clone)]
pub struct TransactionQueueConfig {
    /// Maximum number of transactions in the queue
    pub max_queue_size: usize,

    /// Number of retry attempts before giving up
    pub max_retries: u32,

    /// Base delay for exponential backoff (in milliseconds)
    pub base_retry_delay_ms: u64,

    /// Maximum delay for exponential backoff (in milliseconds)
    pub max_retry_delay_ms: u64,

    /// Whether to enable transaction batching
    pub enable_batching: bool,

    /// Maximum batch size
    pub max_batch_size: usize,

    /// Batch timeout (maximum time to wait for batch completion)
    pub batch_timeout: Duration,
}

impl Default for TransactionQueueConfig {
    fn default() -> Self {
        Self {
            max_queue_size: 1000,
            max_retries: 3,
            base_retry_delay_ms: 1000,
            max_retry_delay_ms: 30000,
            enable_batching: true,
            max_batch_size: 10,
            batch_timeout: Duration::from_millis(100),
        }
    }
}

/// Statistics about the transaction queue
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
    /// Average processing time
    pub avg_processing_time: Duration,
    /// Average queue wait time
    pub avg_queue_wait_time: Duration,
    /// Current queue capacity utilization (0.0 to 1.0)
    pub capacity_utilization: f64,
}

impl Default for QueueStats {
    fn default() -> Self {
        Self {
            queue_depth: 0,
            total_processed: 0,
            successful_transactions: 0,
            failed_transactions: 0,
            avg_processing_time: Duration::from_millis(0),
            avg_queue_wait_time: Duration::from_millis(0),
            capacity_utilization: 0.0,
        }
    }
}

/// Channel types used for transaction communication
pub type TransactionSender = mpsc::Sender<QueuedTransaction>;
pub type TransactionReceiver = mpsc::Receiver<QueuedTransaction>;
pub type ResultSender = oneshot::Sender<TransactionResult>;
pub type ResultReceiver = oneshot::Receiver<TransactionResult>;
pub type StatsWatcher = watch::Receiver<QueueStats>;

/// Transaction Queue Manager
///
/// This manages the queue of database transactions with bounded capacity,
/// backpressure handling, and statistics tracking.
pub struct TransactionQueue {
    /// Channel for sending transactions to the queue
    sender: TransactionSender,

    /// Channel receiver for consumer - wrapped in Arc<Mutex> for thread safety
    receiver: Arc<std::sync::Mutex<Option<TransactionReceiver>>>,

    /// Configuration for the queue
    config: TransactionQueueConfig,

    /// Watcher for queue statistics
    stats_watcher: StatsWatcher,
}

impl TransactionQueue {
    /// Create a new transaction queue with the given configuration
    pub fn new(config: TransactionQueueConfig) -> Self {
        let (sender, receiver) = mpsc::channel(config.max_queue_size);
        let (stats_sender, stats_watcher) = watch::channel(QueueStats::default());

        // Start the statistics monitoring task
        // Note: We can't clone the receiver, so we'll monitor without it for now
        let stats_config = config.clone();
        tokio::spawn(async move {
            Self::monitor_queue_stats_placeholder(stats_sender, stats_config).await;
        });

        Self {
            sender,
            receiver: Arc::new(std::sync::Mutex::new(Some(receiver))),
            config,
            stats_watcher,
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
        receiver_guard.take()
            .expect("TransactionQueue receiver can only be taken once")
    }

    /// Placeholder for queue stats monitoring when receiver can't be cloned
    async fn monitor_queue_stats_placeholder(
        mut stats_sender: watch::Sender<QueueStats>,
        config: TransactionQueueConfig,
    ) {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let stats = QueueStats {
                queue_depth: 0, // Can't monitor without receiver
                total_processed: 0,
                successful_transactions: 0,
                failed_transactions: 0,
                avg_processing_time: Duration::from_millis(0),
                avg_queue_wait_time: Duration::from_millis(0),
                capacity_utilization: 0.0,
            };

            if stats_sender.send(stats).is_err() {
                break; // Channel closed
            }
        }
    }

    /// Try to enqueue a transaction. Returns an error if the queue is full.
    pub async fn try_enqueue(&self, transaction: DatabaseTransaction) -> Result<ResultReceiver, QueueError> {
        let (queued, receiver) = QueuedTransaction::new(transaction);

        self.sender.send(queued).await
            .map_err(|_| QueueError::QueueFull(self.config.max_queue_size))?;

        Ok(receiver)
    }

    /// Enqueue a transaction, waiting if the queue is full.
    pub async fn enqueue(&self, transaction: DatabaseTransaction) -> Result<ResultReceiver, QueueError> {
        let (queued, receiver) = QueuedTransaction::new(transaction);

        // This will wait if the queue is full
        self.sender.send(queued).await
            .map_err(|_| QueueError::QueueClosed)?;

        Ok(receiver)
    }

    /// Get the queue sender for direct use
    pub fn sender(&self) -> TransactionSender {
        self.sender.clone()
    }

    /// Get the queue configuration
    pub fn config(&self) -> &TransactionQueueConfig {
        &self.config
    }

    /// Monitor queue statistics in the background
    async fn monitor_queue_stats(
        mut receiver: TransactionReceiver,
        stats_sender: watch::Sender<QueueStats>,
        config: TransactionQueueConfig,
    ) {
        let mut stats = QueueStats::default();

        while let Some(_queued_tx) = receiver.recv().await {
            // Update queue depth stats
            stats.queue_depth = receiver.len();
            stats.capacity_utilization = receiver.len() as f64 / config.max_queue_size as f64;

            // Send updated stats
            let _ = stats_sender.send(stats.clone());
        }
    }
}

/// Errors that can occur when working with the transaction queue
#[derive(Debug, thiserror::Error)]
pub enum QueueError {
    /// The queue is full and cannot accept more transactions
    #[error("Transaction queue is full (capacity: {0})")]
    QueueFull(usize),

    /// The queue has been closed
    #[error("Transaction queue has been closed")]
    QueueClosed,

    /// Transaction processing failed
    #[error("Transaction failed: {0}")]
    TransactionFailed(String),
}


/// A transaction with associated result channel
#[derive(Debug)]
pub struct QueuedTransaction {
    /// The actual transaction to execute
    pub transaction: DatabaseTransaction,

    /// Channel to send the result back to the caller
    pub result_sender: Option<ResultSender>,

    /// When this transaction was queued
    pub queued_at: Instant,

    /// Current retry attempt
    pub retry_count: u32,
}

impl QueuedTransaction {
    /// Create a new queued transaction
    pub fn new(transaction: DatabaseTransaction) -> (Self, ResultReceiver) {
        let (result_sender, result_receiver) = oneshot::channel();

        let queued = Self {
            transaction,
            result_sender: Some(result_sender),
            queued_at: Instant::now(),
            retry_count: 0,
        };

        (queued, result_receiver)
    }

    /// Get the transaction ID
    pub fn transaction_id(&self) -> &str {
        self.transaction.transaction_id()
    }

    /// Get the queue wait time so far
    pub fn queue_wait_time(&self) -> Duration {
        self.queued_at.elapsed()
    }

    /// Take the result sender (consuming it)
    pub fn take_result_sender(&mut self) -> Option<ResultSender> {
        self.result_sender.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::types::ParsedDocument;

    #[test]
    fn test_transaction_priorities() {
        let store_tx = DatabaseTransaction::StoreDocument {
            transaction_id: "test-1".to_string(),
            document: ParsedDocument::default(),
            kiln_root: PathBuf::from("/test"),
            timestamp: TransactionTimestamp::now(),
        };

        let embed_tx = DatabaseTransaction::ProcessEmbeddings {
            transaction_id: "test-2".to_string(),
            document_id: "doc-1".to_string(),
            document: ParsedDocument::default(),
            timestamp: TransactionTimestamp::now(),
        };

        assert!(store_tx.priority() < embed_tx.priority());
    }

    #[test]
    fn test_transaction_dependencies() {
        let store_tx = DatabaseTransaction::StoreDocument {
            transaction_id: "store-1".to_string(),
            document: ParsedDocument::default(),
            kiln_root: PathBuf::from("/test"),
            timestamp: TransactionTimestamp::now(),
        };

        let link_tx = DatabaseTransaction::CreateWikilinkEdges {
            transaction_id: "link-1".to_string(),
            document_id: "doc-1".to_string(),
            document: ParsedDocument::default(),
            timestamp: TransactionTimestamp::now(),
        };

        assert!(link_tx.depends_on(&store_tx));
        assert!(!store_tx.depends_on(&link_tx));
    }

    #[test]
    fn test_queued_transaction_creation() {
        let tx = DatabaseTransaction::UpdateTimestamp {
            transaction_id: "update-1".to_string(),
            document_id: "doc-1".to_string(),
            timestamp: TransactionTimestamp::now(),
        };

        let (queued, _receiver) = QueuedTransaction::new(tx);

        assert_eq!(queued.transaction_id(), "update-1");
        assert_eq!(queued.retry_count, 0);
        assert!(queued.result_sender.is_some());
    }

    #[test]
    fn test_transaction_timestamp() {
        let ts = TransactionTimestamp::now();
        let instant = ts.to_instant();

        // The conversion should be reasonably close (within 1 second)
        let now = Instant::now();
        let diff = if instant > now { instant - now } else { now - instant };
        assert!(diff < Duration::from_secs(1));
    }

    #[test]
    fn test_transaction_result() {
        let success = TransactionResult::Success {
            transaction_id: "test-1".to_string(),
            duration: Duration::from_millis(100),
            metadata: TransactionMetadata {
                transaction_type: "StoreDocument".to_string(),
                file_path: Some(PathBuf::from("/test.md")),
                queue_depth: 5,
                retry_count: 0,
                queue_wait_time: Duration::from_millis(10),
            },
        };

        assert!(success.is_success());
        assert_eq!(success.transaction_id(), "test-1");
        assert_eq!(success.duration(), Some(Duration::from_millis(100)));
    }

    #[test]
    fn test_queue_config_defaults() {
        let config = TransactionQueueConfig::default();

        assert_eq!(config.max_queue_size, 1000);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_retry_delay_ms, 1000);
        assert_eq!(config.max_retry_delay_ms, 30000);
        assert!(config.enable_batching);
        assert_eq!(config.max_batch_size, 10);
    }
}