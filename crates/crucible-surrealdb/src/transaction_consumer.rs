//! Database Transaction Consumer
//!
//! This module provides the single-threaded database consumer that processes
//! transactions from the queue to eliminate RocksDB lock contention.

use crate::transaction_queue::{
    DatabaseTransaction, QueuedTransaction, TransactionMetadata,
    TransactionResult, TransactionReceiver,
};
use crate::surreal_client::SurrealClient;
use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{oneshot, watch};
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

/// Database transaction consumer
///
/// Processes transactions from a queue in a single thread to eliminate
/// RocksDB lock contention while providing transaction ordering and
/// dependency resolution.
pub struct DatabaseTransactionConsumer {
    /// Database client for executing transactions
    client: Arc<SurrealClient>,

    /// Configuration for transaction processing
    config: ConsumerConfig,

    /// Current processing statistics
    stats: ConsumerStats,

    /// Channel sender for broadcasting statistics updates
    stats_sender: watch::Sender<ConsumerStats>,

    /// Whether the consumer is currently running
    is_running: bool,

    /// Pending transactions that are waiting for dependencies
    pending_transactions: Vec<QueuedTransaction>,

    /// Completed transaction IDs for dependency resolution
    completed_transactions: std::collections::HashSet<String>,
}

/// Configuration for the transaction consumer
#[derive(Debug, Clone)]
pub struct ConsumerConfig {
    /// Maximum time to wait for a single transaction
    pub transaction_timeout: Duration,

    /// Maximum time to wait for batch completion
    pub batch_timeout: Duration,

    /// Maximum number of transactions to batch together
    pub max_batch_size: usize,

    /// Number of retry attempts for failed transactions
    pub max_retries: u32,

    /// Base delay for exponential backoff
    pub base_retry_delay: Duration,

    /// Maximum delay for exponential backoff
    pub max_retry_delay: Duration,

    /// Whether to enable transaction batching
    pub enable_batching: bool,
}

impl Default for ConsumerConfig {
    fn default() -> Self {
        Self {
            transaction_timeout: Duration::from_secs(30),
            batch_timeout: Duration::from_millis(100),
            max_batch_size: 10,
            max_retries: 3,
            base_retry_delay: Duration::from_millis(1000),
            max_retry_delay: Duration::from_millis(30000),
            enable_batching: true,
        }
    }
}

/// Statistics for the transaction consumer
#[derive(Debug, Clone)]
pub struct ConsumerStats {
    /// Number of transactions currently processing
    pub processing_count: usize,

    /// Number of transactions pending dependencies
    pub pending_count: usize,

    /// Total number of transactions processed
    pub total_processed: u64,

    /// Number of successful transactions
    pub successful_transactions: u64,

    /// Number of failed transactions
    pub failed_transactions: u64,

    /// Number of transactions retried
    pub retried_transactions: u64,

    /// Average processing time per transaction
    pub avg_processing_time: Duration,

    /// Current processing rate (transactions per second)
    pub processing_rate: f64,

    /// Uptime of the consumer
    pub uptime: Duration,
}

impl Default for ConsumerStats {
    fn default() -> Self {
        Self {
            processing_count: 0,
            pending_count: 0,
            total_processed: 0,
            successful_transactions: 0,
            failed_transactions: 0,
            retried_transactions: 0,
            avg_processing_time: Duration::from_millis(0),
            processing_rate: 0.0,
            uptime: Duration::from_millis(0),
        }
    }
}

/// Shutdown signal for the consumer
pub type ShutdownSender = oneshot::Sender<()>;
pub type ShutdownReceiver = oneshot::Receiver<()>;

impl Clone for DatabaseTransactionConsumer {
    fn clone(&self) -> Self {
        let (stats_sender, _) = watch::channel(ConsumerStats::default());
        Self {
            client: self.client.clone(),
            config: self.config.clone(),
            stats: ConsumerStats::default(),
            stats_sender,
            is_running: false,
            pending_transactions: Vec::new(),
            completed_transactions: std::collections::HashSet::new(),
        }
    }
}

impl DatabaseTransactionConsumer {
    /// Create a new database transaction consumer
    pub fn new(
        client: Arc<SurrealClient>,
        config: ConsumerConfig,
    ) -> Self {
        let (stats_sender, _) = watch::channel(ConsumerStats::default());

        Self {
            client,
            config,
            stats: ConsumerStats::default(),
            stats_sender,
            is_running: false,
            pending_transactions: Vec::new(),
            completed_transactions: std::collections::HashSet::new(),
        }
    }

    /// Get a receiver for consumer statistics updates
    pub fn subscribe_stats(&self) -> watch::Receiver<ConsumerStats> {
        self.stats_sender.subscribe()
    }

    /// Get current consumer statistics
    pub fn stats(&self) -> ConsumerStats {
        self.stats.clone()
    }

    /// Start the consumer with the given transaction receiver
    pub async fn start(
        &mut self,
        mut receiver: TransactionReceiver,
        shutdown: ShutdownReceiver,
    ) -> Result<()> {
        if self.is_running {
            warn!("Transaction consumer is already running");
            return Ok(());
        }

        info!("Starting database transaction consumer");
        self.is_running = true;
        let start_time = Instant::now();

        // Main consumer loop
        let mut shutdown = shutdown;
        let mut processing_times = Vec::new();
        let mut last_throughput_check = Instant::now();
        let mut transactions_at_last_check = 0u64;

        while self.is_running {
            // Check for shutdown signal
            if shutdown.try_recv().is_ok() {
                info!("Shutdown signal received, stopping transaction consumer");
                break;
            }

            // Process any pending transactions whose dependencies might now be satisfied
            self.process_pending_transactions(&mut processing_times).await?;

            // Wait for the next transaction or timeout
            match timeout(Duration::from_millis(100), receiver.recv()).await {
                Ok(Some(queued_tx)) => {
                    // Check if transaction dependencies are satisfied
                    if self.are_dependencies_satisfied(&queued_tx.transaction) {
                        // Process immediately
                        if let Err(e) = self.process_transaction(queued_tx, &mut processing_times).await {
                            error!("Error processing transaction: {}", e);
                        }
                    } else {
                        // Add to pending queue
                        debug!("Transaction {} waiting for dependencies", queued_tx.transaction_id());
                        self.pending_transactions.push(queued_tx);
                        self.stats.pending_count = self.pending_transactions.len();
                    }
                }
                Ok(None) => {
                    info!("Transaction queue closed, shutting down consumer");
                    break;
                }
                Err(_) => {
                    // Timeout - continue loop to check pending transactions and shutdown
                    continue;
                }
            }

            // Update statistics
            self.update_stats(&start_time, &mut processing_times, &mut last_throughput_check, &mut transactions_at_last_check);

            // Send statistics update
            let _ = self.stats_sender.send(self.stats.clone());
        }

        // Process any remaining pending transactions
        self.process_remaining_transactions(&mut processing_times).await?;

        info!("Database transaction consumer stopped");
        self.is_running = false;

        Ok(())
    }

    /// Check if all dependencies for a transaction are satisfied
    fn are_dependencies_satisfied(&self, transaction: &DatabaseTransaction) -> bool {
        // For now, we'll implement simple dependency checking
        // Later, this can be enhanced with more sophisticated dependency tracking

        match transaction {
            DatabaseTransaction::StoreDocument { .. } => {
                // StoreDocument has no dependencies
                true
            }
            DatabaseTransaction::CreateWikilinkEdges { document, .. } |
            DatabaseTransaction::CreateEmbedRelationships { document, .. } |
            DatabaseTransaction::CreateTagAssociations { document, .. } |
            DatabaseTransaction::ProcessEmbeddings { document, .. } => {
                // These operations depend on the document being stored first
                let store_tx_id = format!("store-{}", document.path.display());
                self.completed_transactions.contains(&store_tx_id)
            }
            DatabaseTransaction::UpdateTimestamp { .. } => {
                // Timestamp updates depend on document being stored
                // For now, we'll assume this is satisfied if any store operation has completed
                !self.completed_transactions.is_empty()
            }
        }
    }

    /// Process any pending transactions whose dependencies are now satisfied
    async fn process_pending_transactions(&mut self, processing_times: &mut Vec<Duration>) -> Result<()> {
        let mut ready_indices = Vec::new();

        for (i, queued_tx) in self.pending_transactions.iter().enumerate() {
            if self.are_dependencies_satisfied(&queued_tx.transaction) {
                ready_indices.push(i);
            }
        }

        // Process ready transactions in order
        for &index in ready_indices.iter().rev() {
            let queued_tx = self.pending_transactions.remove(index);
            debug!("Processing pending transaction: {}", queued_tx.transaction_id());
            if let Err(e) = self.process_transaction(queued_tx, processing_times).await {
                error!("Error processing pending transaction: {}", e);
            }
        }

        self.stats.pending_count = self.pending_transactions.len();
        Ok(())
    }

    /// Process a single transaction
    async fn process_transaction(
        &mut self,
        mut queued_tx: QueuedTransaction,
        processing_times: &mut Vec<Duration>,
    ) -> Result<()> {
        let start_time = Instant::now();
        self.stats.processing_count += 1;

        debug!("Processing transaction: {}", queued_tx.transaction_id());

        let result = match timeout(self.config.transaction_timeout, self.execute_transaction(&queued_tx.transaction)).await {
            Ok(Ok(())) => {
                // Success
                self.completed_transactions.insert(queued_tx.transaction_id().to_string());

                TransactionResult::Success {
                    transaction_id: queued_tx.transaction_id().to_string(),
                    duration: start_time.elapsed(),
                    metadata: self.create_transaction_metadata(&queued_tx, 0),
                }
            }
            Ok(Err(e)) => {
                // Transaction failed - retry if possible
                if queued_tx.retry_count < self.config.max_retries {
                    queued_tx.retry_count += 1;
                    self.stats.retried_transactions += 1;

                    let retry_delay = self.calculate_retry_delay(queued_tx.retry_count);
                    warn!("Transaction {} failed, retrying in {:?} (attempt {}/{}): {}",
                          queued_tx.transaction_id(), retry_delay, queued_tx.retry_count, self.config.max_retries, e);

                    tokio::time::sleep(retry_delay).await;

                    // Re-queue for retry
                    return Err(e); // Will be handled by caller
                } else {
                    // Max retries exceeded
                    error!("Transaction {} failed after {} retries: {}",
                           queued_tx.transaction_id(), self.config.max_retries, e);

                    TransactionResult::Failure {
                        transaction_id: queued_tx.transaction_id().to_string(),
                        error: e.to_string(),
                        retry_count: queued_tx.retry_count,
                        metadata: self.create_transaction_metadata(&queued_tx, queued_tx.retry_count),
                    }
                }
            }
            Err(_) => {
                // Timeout
                error!("Transaction {} timed out after {:?}", queued_tx.transaction_id(), self.config.transaction_timeout);

                TransactionResult::Failure {
                    transaction_id: queued_tx.transaction_id().to_string(),
                    error: format!("Transaction timed out after {:?}", self.config.transaction_timeout),
                    retry_count: queued_tx.retry_count,
                    metadata: self.create_transaction_metadata(&queued_tx, queued_tx.retry_count),
                }
            }
        };

        // Record processing time
        let processing_time = start_time.elapsed();
        processing_times.push(processing_time);

        // Update statistics
        self.stats.total_processed += 1;
        match result {
            TransactionResult::Success { .. } => {
                self.stats.successful_transactions += 1;
            }
            TransactionResult::Failure { .. } => {
                self.stats.failed_transactions += 1;
            }
            TransactionResult::Skipped { .. } => {
                // Skipped transactions don't count as failures
            }
        }
        self.stats.processing_count -= 1;

        // Send result to caller
        if let Some(sender) = queued_tx.take_result_sender() {
            let _ = sender.send(result);
        }

        debug!("Completed transaction: {} in {:?}", queued_tx.transaction_id(), processing_time);
        Ok(())
    }

    /// Execute a database transaction
    async fn execute_transaction(&self, transaction: &DatabaseTransaction) -> Result<()> {
        match transaction {
            DatabaseTransaction::StoreDocument { document, kiln_root, .. } => {
                // This will be implemented when we create the transaction builder
                // For now, we'll use the existing store_parsed_document function
                debug!("Storing document: {}", document.path.display());

                // Call existing kiln_integration function
                crate::kiln_integration::store_parsed_document(
                    &self.client,
                    document,
                    kiln_root
                ).await?;
            }

            DatabaseTransaction::CreateWikilinkEdges { document_id, document, .. } => {
                debug!("Creating wikilink edges for document: {}", document_id);

                // This will be implemented when we create the transaction builder
                // For now, we'll use existing functions
                crate::kiln_integration::create_wikilink_edges(
                    &self.client,
                    document_id,
                    document
                ).await?;
            }

            DatabaseTransaction::CreateEmbedRelationships { document_id, document, .. } => {
                debug!("Creating embed relationships for document: {}", document_id);

                crate::kiln_integration::create_embed_relationships(
                    &self.client,
                    document_id,
                    document
                ).await?;
            }

            DatabaseTransaction::CreateTagAssociations { document_id, document, .. } => {
                debug!("Creating tag associations for document: {}", document_id);

                crate::kiln_integration::create_tag_associations(
                    &self.client,
                    document_id,
                    document
                ).await?;
            }

            DatabaseTransaction::ProcessEmbeddings { document_id, document, .. } => {
                debug!("Processing embeddings for document: {}", document_id);

                // TODO: Implement embedding processing when transaction builder is ready
                // For now, we'll skip embedding processing
                warn!("Embedding processing not yet implemented for document: {}", document_id);
            }

            DatabaseTransaction::UpdateTimestamp { document_id, .. } => {
                debug!("Updating timestamp for document: {}", document_id);

                // This will be implemented when we create the transaction builder
                // For now, we'll skip timestamp updates
                warn!("Timestamp update not yet implemented for document: {}", document_id);
            }
        }

        Ok(())
    }

    /// Create transaction metadata
    fn create_transaction_metadata(&self, queued_tx: &QueuedTransaction, retry_count: u32) -> TransactionMetadata {
        TransactionMetadata {
            transaction_type: match queued_tx.transaction {
                DatabaseTransaction::StoreDocument { .. } => "StoreDocument".to_string(),
                DatabaseTransaction::CreateWikilinkEdges { .. } => "CreateWikilinkEdges".to_string(),
                DatabaseTransaction::CreateEmbedRelationships { .. } => "CreateEmbedRelationships".to_string(),
                DatabaseTransaction::CreateTagAssociations { .. } => "CreateTagAssociations".to_string(),
                DatabaseTransaction::ProcessEmbeddings { .. } => "ProcessEmbeddings".to_string(),
                DatabaseTransaction::UpdateTimestamp { .. } => "UpdateTimestamp".to_string(),
            },
            file_path: match &queued_tx.transaction {
                DatabaseTransaction::StoreDocument { document, .. } => Some(document.path.clone()),
                DatabaseTransaction::CreateWikilinkEdges { document, .. } |
                DatabaseTransaction::CreateEmbedRelationships { document, .. } |
                DatabaseTransaction::CreateTagAssociations { document, .. } |
                DatabaseTransaction::ProcessEmbeddings { document, .. } => Some(document.path.clone()),
                DatabaseTransaction::UpdateTimestamp { .. } => None,
            },
            queue_depth: 0, // This would be set by the queue
            retry_count,
            queue_wait_time: queued_tx.queue_wait_time(),
        }
    }

    /// Calculate retry delay with exponential backoff
    fn calculate_retry_delay(&self, retry_count: u32) -> Duration {
        let delay_ms = self.config.base_retry_delay.as_millis() as u64 * 2_u64.pow(retry_count - 1);
        let delay_ms = delay_ms.min(self.config.max_retry_delay.as_millis() as u64);
        Duration::from_millis(delay_ms)
    }

    /// Update consumer statistics
    fn update_stats(
        &mut self,
        start_time: &Instant,
        processing_times: &mut Vec<Duration>,
        last_throughput_check: &mut Instant,
        transactions_at_last_check: &mut u64,
    ) {
        self.stats.uptime = start_time.elapsed();

        // Update average processing time
        if !processing_times.is_empty() {
            let total_time: Duration = processing_times.iter().sum();
            self.stats.avg_processing_time = total_time / processing_times.len() as u32;

            // Keep only recent processing times
            const MAX_SAMPLES: usize = 1000;
            if processing_times.len() > MAX_SAMPLES {
                let drop_count = processing_times.len() - MAX_SAMPLES;
                processing_times.drain(0..drop_count);
            }
        }

        // Update processing rate (every 10 seconds)
        if last_throughput_check.elapsed() >= Duration::from_secs(10) {
            let elapsed = last_throughput_check.elapsed();
            let transaction_diff = self.stats.total_processed - *transactions_at_last_check;

            self.stats.processing_rate = transaction_diff as f64 / elapsed.as_secs_f64();

            *last_throughput_check = Instant::now();
            *transactions_at_last_check = self.stats.total_processed;
        }
    }

    /// Process remaining pending transactions during shutdown
    async fn process_remaining_transactions(&mut self, processing_times: &mut Vec<Duration>) -> Result<()> {
        info!("Processing {} remaining pending transactions", self.pending_transactions.len());

        let remaining = std::mem::take(&mut self.pending_transactions);

        for queued_tx in remaining {
            debug!("Processing remaining transaction: {}", queued_tx.transaction_id());
            if let Err(e) = self.process_transaction(queued_tx, processing_times).await {
                error!("Error processing remaining transaction: {}", e);
            }
        }

        self.stats.pending_count = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction_queue::{TransactionQueue, TransactionQueueConfig, DatabaseTransaction, QueuedTransaction, TransactionTimestamp};
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_consumer_config_default() {
        let config = ConsumerConfig::default();
        assert_eq!(config.transaction_timeout, Duration::from_secs(30));
        assert_eq!(config.max_retries, 3);
        assert!(config.enable_batching);
    }

    #[tokio::test]
    async fn test_consumer_stats_default() {
        let stats = ConsumerStats::default();
        assert_eq!(stats.processing_count, 0);
        assert_eq!(stats.total_processed, 0);
        assert_eq!(stats.processing_rate, 0.0);
    }

    #[tokio::test]
    async fn test_retry_delay_calculation() {
        let config = ConsumerConfig {
            base_retry_delay: Duration::from_millis(1000),
            max_retry_delay: Duration::from_millis(10000),
            ..Default::default()
        };

        let consumer = DatabaseTransactionConsumer::new(
            Arc::new(SurrealClient::new("test").await.unwrap()),
            config,
        );

        // Test exponential backoff
        assert_eq!(consumer.calculate_retry_delay(1), Duration::from_millis(1000));
        assert_eq!(consumer.calculate_retry_delay(2), Duration::from_millis(2000));
        assert_eq!(consumer.calculate_retry_delay(3), Duration::from_millis(4000));

        // Test max delay capping
        assert_eq!(consumer.calculate_retry_delay(10), Duration::from_millis(10000));
    }

    #[tokio::test]
    async fn test_transaction_metadata_creation() {
        let config = ConsumerConfig::default();
        let consumer = DatabaseTransactionConsumer::new(
            Arc::new(SurrealClient::new("test").await.unwrap()),
            config,
        );

        let tx = DatabaseTransaction::StoreDocument {
            transaction_id: "test-1".to_string(),
            document: crucible_core::types::ParsedDocument::default(),
            kiln_root: PathBuf::from("/test"),
            timestamp: TransactionTimestamp::now(),
        };

        let (queued_tx, _) = QueuedTransaction::new(tx);
        let metadata = consumer.create_transaction_metadata(&queued_tx, 0);

        assert_eq!(metadata.transaction_type, "StoreDocument");
        assert_eq!(metadata.retry_count, 0);
    }
}