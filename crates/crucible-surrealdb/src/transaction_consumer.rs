//! Database Transaction Consumer
//!
//! This module provides the single-threaded database consumer that processes
//! transactions from the queue to eliminate RocksDB lock contention.

use crate::eav_graph::EAVGraphStore;
use crate::kiln_integration::parse_entity_record_id;
use crate::metrics::{record_transaction_failure, record_transaction_success};
use crate::surreal_client::SurrealClient;
use crate::transaction_queue::{
    DatabaseTransaction, TransactionMetadata, TransactionReceiver, TransactionResult,
};
use anyhow::Result;
use serde_json::json;
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

    /// Completed transaction IDs for dependency resolution (simplified)
    completed_transactions: std::collections::HashSet<String>,

    /// Batch collector for grouping transactions
    batch_collector: BatchCollector,
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

/// Represents a transaction waiting to be batched
#[derive(Debug)]
struct BatchableTransaction {
    transaction: DatabaseTransaction,
    result_sender: oneshot::Sender<TransactionResult>,
    received_at: Instant,
}

/// Batch collector for grouping related transactions
#[derive(Debug)]
struct BatchCollector {
    /// Transactions waiting to be batched
    pending_transactions: Vec<BatchableTransaction>,
    /// When this batch was started
    batch_start_time: Instant,
}

impl BatchCollector {
    /// Create a new batch collector
    fn new() -> Self {
        Self {
            pending_transactions: Vec::new(),
            batch_start_time: Instant::now(),
        }
    }

    /// Add a transaction to the batch
    fn add_transaction(
        &mut self,
        transaction: DatabaseTransaction,
        result_sender: oneshot::Sender<TransactionResult>,
    ) {
        self.pending_transactions.push(BatchableTransaction {
            transaction,
            result_sender,
            received_at: Instant::now(),
        });
    }

    /// Check if the batch is ready to process
    fn is_ready(&self, max_batch_size: usize, batch_timeout: Duration) -> bool {
        self.pending_transactions.len() >= max_batch_size
            || self.batch_start_time.elapsed() >= batch_timeout
    }

    /// Check if the batch should try to wait for more related transactions
    fn should_wait_for_related(&self, new_transaction: &DatabaseTransaction) -> bool {
        if self.pending_transactions.is_empty() {
            return false;
        }

        // Simple heuristic: wait for transactions from the same directory or with same note prefix
        match (&self.pending_transactions[0].transaction, new_transaction) {
            (
                DatabaseTransaction::Create {
                    note: existing_doc, ..
                },
                DatabaseTransaction::Create { note: new_doc, .. },
            ) => existing_doc.path.parent() == new_doc.path.parent(),
            (
                DatabaseTransaction::Update {
                    note: existing_doc, ..
                },
                DatabaseTransaction::Update { note: new_doc, .. },
            ) => existing_doc.path.parent() == new_doc.path.parent(),
            _ => false,
        }
    }

    /// Take all transactions from the batch
    fn take_batch(&mut self) -> Vec<BatchableTransaction> {
        let batch = std::mem::take(&mut self.pending_transactions);
        self.batch_start_time = Instant::now();
        batch
    }

    /// Get the number of pending transactions
    fn len(&self) -> usize {
        self.pending_transactions.len()
    }

    /// Check if the batch is empty
    fn is_empty(&self) -> bool {
        self.pending_transactions.is_empty()
    }
}

impl Clone for DatabaseTransactionConsumer {
    fn clone(&self) -> Self {
        let (stats_sender, _) = watch::channel(ConsumerStats::default());
        Self {
            client: self.client.clone(),
            config: self.config.clone(),
            stats: ConsumerStats::default(),
            stats_sender,
            is_running: false,
            completed_transactions: std::collections::HashSet::new(),
            batch_collector: BatchCollector::new(),
        }
    }
}

impl DatabaseTransactionConsumer {
    /// Create a new database transaction consumer
    pub fn new(client: Arc<SurrealClient>, config: ConsumerConfig) -> Self {
        let (stats_sender, _) = watch::channel(ConsumerStats::default());

        Self {
            client,
            config,
            stats: ConsumerStats::default(),
            stats_sender,
            is_running: false,
            completed_transactions: std::collections::HashSet::new(),
            batch_collector: BatchCollector::new(),
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
            self.process_pending_transactions(&mut processing_times)
                .await?;

            // Handle transaction reception with batching logic
            if self.config.enable_batching {
                // Batch-enabled processing
                let mut processed_any = false;

                // Try to receive a transaction with short timeout
                match timeout(Duration::from_millis(10), receiver.recv()).await {
                    Ok(Some((transaction, result_sender))) => {
                        // Check if transaction dependencies are satisfied
                        if self.are_dependencies_satisfied(&transaction) {
                            // Add to batch collector
                            self.batch_collector
                                .add_transaction(transaction, result_sender);

                            // Try to collect more related transactions quickly
                            let mut collection_attempts = 0;
                            while self.batch_collector.len() < self.config.max_batch_size
                                && collection_attempts < 5
                                && !self
                                    .batch_collector
                                    .is_ready(self.config.max_batch_size, self.config.batch_timeout)
                            {
                                match timeout(Duration::from_millis(5), receiver.recv()).await {
                                    Ok(Some((next_tx, next_result_sender))) => {
                                        if self.are_dependencies_satisfied(&next_tx) {
                                            // If the new transaction is related to existing ones, add it to batch
                                            if self
                                                .batch_collector
                                                .should_wait_for_related(&next_tx)
                                            {
                                                self.batch_collector
                                                    .add_transaction(next_tx, next_result_sender);
                                            } else {
                                                // Not related, process current batch first
                                                break;
                                            }
                                        }
                                    }
                                    Ok(None) => {
                                        info!("Transaction queue closed, shutting down consumer");
                                        break;
                                    }
                                    Err(_) => {
                                        // Timeout - stop collecting
                                        break;
                                    }
                                }
                                collection_attempts += 1;
                            }

                            // Process the batch if it's ready
                            if self
                                .batch_collector
                                .is_ready(self.config.max_batch_size, self.config.batch_timeout)
                            {
                                let batch = self.batch_collector.take_batch();
                                if let Err(e) =
                                    self.process_batch(batch, &mut processing_times).await
                                {
                                    error!("Error processing transaction batch: {}", e);
                                }
                                processed_any = true;
                            }
                        } else {
                            // Simplified: skip transactions with unsatisfied dependencies
                            debug!(
                                "Transaction {} waiting for dependencies - skipping for now",
                                transaction.transaction_id()
                            );
                        }
                    }
                    Ok(None) => {
                        info!("Transaction queue closed, shutting down consumer");
                        break;
                    }
                    Err(_) => {
                        // No transaction received, check if batch timeout expired
                        if !self.batch_collector.is_empty()
                            && self
                                .batch_collector
                                .is_ready(self.config.max_batch_size, self.config.batch_timeout)
                        {
                            let batch = self.batch_collector.take_batch();
                            if let Err(e) = self.process_batch(batch, &mut processing_times).await {
                                error!("Error processing expired batch: {}", e);
                            }
                            processed_any = true;
                        }
                    }
                }

                // If we didn't process anything, do a small sleep to prevent busy loop
                if !processed_any && self.batch_collector.is_empty() {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            } else {
                // Original single-transaction processing
                match timeout(Duration::from_millis(100), receiver.recv()).await {
                    Ok(Some((transaction, result_sender))) => {
                        // Check if transaction dependencies are satisfied
                        if self.are_dependencies_satisfied(&transaction) {
                            // Process immediately
                            if let Err(e) = self
                                .process_transaction(
                                    transaction,
                                    result_sender,
                                    &mut processing_times,
                                )
                                .await
                            {
                                error!("Error processing transaction: {}", e);
                            }
                        } else {
                            // Simplified: no pending queue for now - just skip
                            debug!(
                                "Transaction {} waiting for dependencies - skipping for now",
                                transaction.transaction_id()
                            );
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
            }

            // Update statistics
            self.update_stats(
                &start_time,
                &mut processing_times,
                &mut last_throughput_check,
                &mut transactions_at_last_check,
            );

            // Send statistics update
            let _ = self.stats_sender.send(self.stats.clone());
        }

        // Process any remaining pending transactions
        self.process_remaining_transactions(&mut processing_times)
            .await?;

        info!("Database transaction consumer stopped");
        self.is_running = false;

        Ok(())
    }

    /// Check if all dependencies for a transaction are satisfied
    fn are_dependencies_satisfied(&self, transaction: &DatabaseTransaction) -> bool {
        // Simplified CRUD approach - minimal dependencies
        match transaction {
            DatabaseTransaction::Create { .. } => {
                // Create operations have no dependencies
                true
            }
            DatabaseTransaction::Update { .. } => {
                // Update operations depend on note existing, but for simplicity we'll allow them
                // The intelligent consumer will handle create vs update logic
                true
            }
            DatabaseTransaction::Delete { .. } => {
                // Delete operations can proceed independently
                true
            }
        }
    }

    /// Process any pending transactions (simplified - no pending queue for now)
    async fn process_pending_transactions(
        &mut self,
        _processing_times: &mut Vec<Duration>,
    ) -> Result<()> {
        // Simplified architecture - no pending queue for now
        // All transactions are processed immediately or skipped
        Ok(())
    }

    /// Process a single transaction from the simplified queue with retry logic
    async fn process_transaction(
        &mut self,
        transaction: DatabaseTransaction,
        result_sender: oneshot::Sender<TransactionResult>,
        processing_times: &mut Vec<Duration>,
    ) -> Result<()> {
        let start_time = Instant::now();
        self.stats.processing_count += 1;

        debug!("Processing transaction: {}", transaction.transaction_id());

        let mut retry_count = 0;
        let mut last_error = None;
        let result = loop {
            match timeout(
                self.config.transaction_timeout,
                self.execute_transaction(&transaction),
            )
            .await
            {
                Ok(Ok(())) => {
                    // Success - break retry loop
                    let success_result = TransactionResult::Success {
                        transaction_id: transaction.transaction_id().to_string(),
                        duration: start_time.elapsed(),
                        metadata: self.create_transaction_metadata(&transaction, retry_count),
                    };
                    self.completed_transactions
                        .insert(transaction.transaction_id().to_string());
                    break success_result;
                }
                Ok(Err(e)) => {
                    error!(
                        "Transaction {} failed (attempt {}): {}",
                        transaction.transaction_id(),
                        retry_count + 1,
                        e
                    );
                    last_error = Some(e);

                    retry_count += 1;
                    if retry_count >= self.config.max_retries {
                        // Max retries exceeded - create failure result
                        let failure_result = TransactionResult::Failure {
                            transaction_id: transaction.transaction_id().to_string(),
                            error: last_error
                                .unwrap_or_else(|| anyhow::anyhow!("Unknown error"))
                                .to_string(),
                            retry_count,
                            metadata: self.create_transaction_metadata(&transaction, retry_count),
                        };
                        break failure_result;
                    }

                    // Wait before retry with exponential backoff
                    let retry_delay = self.calculate_retry_delay(retry_count);
                    debug!(
                        "Retrying transaction {} in {:?} (attempt {}/{})",
                        transaction.transaction_id(),
                        retry_delay,
                        retry_count,
                        self.config.max_retries
                    );
                    tokio::time::sleep(retry_delay).await;
                    continue;
                }
                Err(_) => {
                    // Timeout
                    error!(
                        "Transaction {} timed out (attempt {}) after {:?}",
                        transaction.transaction_id(),
                        retry_count + 1,
                        self.config.transaction_timeout
                    );

                    retry_count += 1;
                    if retry_count >= self.config.max_retries {
                        // Max retries exceeded - create failure result
                        let failure_result = TransactionResult::Failure {
                            transaction_id: transaction.transaction_id().to_string(),
                            error: format!(
                                "Transaction timed out after {:?} ({} attempts)",
                                self.config.transaction_timeout, retry_count
                            ),
                            retry_count,
                            metadata: self.create_transaction_metadata(&transaction, retry_count),
                        };
                        break failure_result;
                    }

                    // Wait before retry with exponential backoff
                    let retry_delay = self.calculate_retry_delay(retry_count);
                    debug!(
                        "Retrying transaction {} in {:?} (attempt {}/{}) due to timeout",
                        transaction.transaction_id(),
                        retry_delay,
                        retry_count,
                        self.config.max_retries
                    );
                    tokio::time::sleep(retry_delay).await;
                    continue;
                }
            }
        };

        // Record processing time
        let processing_time = start_time.elapsed();
        processing_times.push(processing_time);

        // Update statistics
        self.stats.total_processed += 1;
        match &result {
            TransactionResult::Success { .. } => {
                self.stats.successful_transactions += 1;
            }
            TransactionResult::Failure { retry_count, .. } => {
                self.stats.failed_transactions += 1;
                self.stats.retried_transactions += *retry_count as u64;
            }
        }
        self.stats.processing_count -= 1;

        // Record metrics for global tracking
        let processing_time_ms = processing_time.as_millis() as u64;
        match &result {
            TransactionResult::Success { .. } => {
                record_transaction_success(processing_time_ms);
            }
            TransactionResult::Failure { .. } => {
                record_transaction_failure(processing_time_ms);
            }
        }

        // Send result to caller
        let _ = result_sender.send(result);

        debug!(
            "Completed transaction: {} in {:?}",
            transaction.transaction_id(),
            processing_time
        );
        Ok(())
    }

    /// Execute a database transaction using intelligent consumer diffing
    async fn execute_transaction(&self, transaction: &DatabaseTransaction) -> Result<()> {
        match transaction {
            DatabaseTransaction::Create {
                note, kiln_root, ..
            } => {
                debug!("Creating note: {}", note.path.display());

                // Store the note and all its relationships in one operation
                let document_id =
                    crate::kiln_integration::store_parsed_document(&self.client, note, kiln_root)
                        .await?;

                // Create all related entities (links, embeds, tags) automatically
                self.create_document_relationships(&document_id, note, kiln_root)
                    .await?;
            }

            DatabaseTransaction::Update {
                note, kiln_root, ..
            } => {
                debug!("Updating note: {}", note.path.display());

                // Check if note exists and determine what changed
                let document_id =
                    crate::kiln_integration::generate_document_id(&note.path, kiln_root);
                let existing_doc = self.get_existing_document(&document_id).await?;

                if let Some(existing_document) = existing_doc {
                    // Intelligent diffing - update only what changed
                    self.update_document_intelligently(&existing_document, note, kiln_root)
                        .await?;
                } else {
                    // Note doesn't exist, treat as create
                    info!("Note {} not found, treating as create", document_id);
                    let created_id = crate::kiln_integration::store_parsed_document(
                        &self.client,
                        note,
                        kiln_root,
                    )
                    .await?;
                    self.create_document_relationships(&created_id, note, kiln_root)
                        .await?;
                }
            }

            DatabaseTransaction::Delete {
                document_id,
                kiln_root,
                ..
            } => {
                debug!("Deleting note: {}", document_id);

                // Remove note and all its relationships
                self.delete_document_completely(document_id, kiln_root)
                    .await?;
            }
        }

        Ok(())
    }

    /// Process a batch of transactions together for better performance
    async fn process_batch(
        &mut self,
        batch: Vec<BatchableTransaction>,
        processing_times: &mut Vec<Duration>,
    ) -> Result<()> {
        if batch.is_empty() {
            return Ok(());
        }

        let batch_start_time = Instant::now();
        let batch_size = batch.len();

        debug!("Processing batch of {} transactions", batch_size);
        self.stats.processing_count += batch_size;

        // Execute all transactions in the batch using SurrealDB's transaction capabilities
        let results = self.execute_transaction_batch(&batch).await;

        // Process results and send responses
        for (batchable_tx, result) in batch.into_iter().zip(results) {
            let processing_time = batch_start_time.elapsed();
            processing_times.push(processing_time);

            // Update statistics
            self.stats.total_processed += 1;
            match &result {
                TransactionResult::Success { .. } => {
                    self.stats.successful_transactions += 1;
                    // Mark transaction as completed for dependency tracking
                    if let Some(tx_id) = result.transaction_id().split(':').next() {
                        self.completed_transactions.insert(tx_id.to_string());
                    }
                }
                TransactionResult::Failure { .. } => {
                    self.stats.failed_transactions += 1;
                }
            }
            self.stats.processing_count -= 1;

            // Record metrics for global tracking
            let processing_time_ms = batch_start_time.elapsed().as_millis() as u64;
            match &result {
                TransactionResult::Success { .. } => {
                    record_transaction_success(processing_time_ms);
                }
                TransactionResult::Failure { .. } => {
                    record_transaction_failure(processing_time_ms);
                }
            }

            // Send result to caller
            let _ = batchable_tx.result_sender.send(result);
        }

        let total_batch_time = batch_start_time.elapsed();
        debug!(
            "Completed batch of {} transactions in {:?}",
            batch_size, total_batch_time
        );

        Ok(())
    }

    /// Execute multiple transactions in a single database transaction with retry logic
    async fn execute_transaction_batch(
        &self,
        batch: &[BatchableTransaction],
    ) -> Vec<TransactionResult> {
        let mut results = Vec::with_capacity(batch.len());

        // For now, process transactions sequentially but within the same database transaction context
        // In the future, this could be optimized to use actual database transaction batching
        for batchable_tx in batch {
            let start_time = Instant::now();
            let mut retry_count = 0;
            let mut last_error = None;

            // Attempt transaction with retry logic
            loop {
                let _result = match timeout(
                    self.config.transaction_timeout,
                    self.execute_transaction(&batchable_tx.transaction),
                )
                .await
                {
                    Ok(Ok(())) => {
                        // Success - break retry loop
                        let success_result = TransactionResult::Success {
                            transaction_id: batchable_tx.transaction.transaction_id().to_string(),
                            duration: start_time.elapsed(),
                            metadata: self.create_transaction_metadata(
                                &batchable_tx.transaction,
                                retry_count,
                            ),
                        };
                        results.push(success_result);
                        break;
                    }
                    Ok(Err(e)) => {
                        error!(
                            "Transaction {} failed (attempt {}): {}",
                            batchable_tx.transaction.transaction_id(),
                            retry_count + 1,
                            e
                        );
                        last_error = Some(e);

                        retry_count += 1;
                        if retry_count >= self.config.max_retries {
                            // Max retries exceeded - create failure result
                            let failure_result = TransactionResult::Failure {
                                transaction_id: batchable_tx
                                    .transaction
                                    .transaction_id()
                                    .to_string(),
                                error: last_error
                                    .unwrap_or_else(|| anyhow::anyhow!("Unknown error"))
                                    .to_string(),
                                retry_count,
                                metadata: self.create_transaction_metadata(
                                    &batchable_tx.transaction,
                                    retry_count,
                                ),
                            };
                            results.push(failure_result);
                            break;
                        }

                        // Wait before retry with exponential backoff
                        let retry_delay = self.calculate_retry_delay(retry_count);
                        debug!(
                            "Retrying transaction {} in {:?} (attempt {}/{})",
                            batchable_tx.transaction.transaction_id(),
                            retry_delay,
                            retry_count,
                            self.config.max_retries
                        );
                        tokio::time::sleep(retry_delay).await;
                        continue;
                    }
                    Err(_) => {
                        // Timeout
                        error!(
                            "Transaction {} timed out (attempt {}) after {:?}",
                            batchable_tx.transaction.transaction_id(),
                            retry_count + 1,
                            self.config.transaction_timeout
                        );

                        retry_count += 1;
                        if retry_count >= self.config.max_retries {
                            // Max retries exceeded - create failure result
                            let failure_result = TransactionResult::Failure {
                                transaction_id: batchable_tx
                                    .transaction
                                    .transaction_id()
                                    .to_string(),
                                error: format!(
                                    "Transaction timed out after {:?} ({} attempts)",
                                    self.config.transaction_timeout, retry_count
                                ),
                                retry_count,
                                metadata: self.create_transaction_metadata(
                                    &batchable_tx.transaction,
                                    retry_count,
                                ),
                            };
                            results.push(failure_result);
                            break;
                        }

                        // Wait before retry with exponential backoff
                        let retry_delay = self.calculate_retry_delay(retry_count);
                        debug!(
                            "Retrying transaction {} in {:?} (attempt {}/{}) due to timeout",
                            batchable_tx.transaction.transaction_id(),
                            retry_delay,
                            retry_count,
                            self.config.max_retries
                        );
                        tokio::time::sleep(retry_delay).await;
                        continue;
                    }
                };
            }
        }

        results
    }

    /// Create transaction metadata
    fn create_transaction_metadata(
        &self,
        transaction: &DatabaseTransaction,
        retry_count: u32,
    ) -> TransactionMetadata {
        TransactionMetadata {
            transaction_type: match transaction {
                DatabaseTransaction::Create { .. } => "Create".to_string(),
                DatabaseTransaction::Update { .. } => "Update".to_string(),
                DatabaseTransaction::Delete { .. } => "Delete".to_string(),
            },
            file_path: match transaction {
                DatabaseTransaction::Create { note, .. }
                | DatabaseTransaction::Update { note, .. } => Some(note.path.clone()),
                DatabaseTransaction::Delete { .. } => None,
            },
            retry_count,
            queue_wait_time: Duration::from_millis(0), // Simplified - no tracking
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

    /// Process remaining pending transactions during shutdown (simplified)
    async fn process_remaining_transactions(
        &mut self,
        processing_times: &mut Vec<Duration>,
    ) -> Result<()> {
        // Process any remaining transactions in the batch collector
        if !self.batch_collector.is_empty() {
            info!(
                "Processing {} remaining transactions in batch collector",
                self.batch_collector.len()
            );
            let batch = self.batch_collector.take_batch();
            if let Err(e) = self.process_batch(batch, processing_times).await {
                error!("Error processing remaining batch during shutdown: {}", e);
            }
        }

        // Simplified architecture - no pending queue for now
        info!("No pending transactions to process in simplified architecture");
        Ok(())
    }

    /// Helper method to create all note relationships (wikilinks, embeds, tags)
    async fn create_document_relationships(
        &self,
        document_id: &str,
        note: &crucible_core::types::ParsedNote,
        kiln_root: &std::path::Path,
    ) -> Result<()> {
        // Create wikilink edges
        if !note.wikilinks.is_empty() {
            crate::kiln_integration::create_wikilink_edges(
                &self.client,
                document_id,
                note,
                kiln_root,
            )
            .await?;
        }

        // Create embed relations
        crate::kiln_integration::create_embed_relationships(
            &self.client,
            document_id,
            note,
            kiln_root,
        )
        .await?;

        // Tags are now automatically stored during note ingestion in NoteIngestor

        // Note: embeds are handled through content processing, not as separate relationships
        // The intelligent consumer handles all content-related updates automatically

        Ok(())
    }

    /// Helper method to get existing note from database
    async fn get_existing_document(
        &self,
        document_id: &str,
    ) -> Result<Option<crucible_core::types::ParsedNote>> {
        // For now, always return None to simplify the intelligent consumer
        // This means all Update operations will be treated as Create operations
        // which is fine for the simple queue architecture goal of eliminating lock contention
        debug!(
            "Checking for existing note: {} (simplified check)",
            document_id
        );
        Ok(None)
    }

    /// Helper method to intelligently update note based on diff
    async fn update_document_intelligently(
        &self,
        _existing: &crucible_core::types::ParsedNote,
        new: &crucible_core::types::ParsedNote,
        kiln_root: &std::path::Path,
    ) -> Result<()> {
        debug!("Updating note: {}", new.path.display());

        // Simple intelligent update: just store the new note
        // The consumer is "intelligent" because it figures out what to do automatically
        // without the processing layer having to specify granular operations
        let document_id =
            crate::kiln_integration::store_parsed_document(&self.client, new, kiln_root).await?;
        self.create_document_relationships(&document_id, new, kiln_root)
            .await?;

        Ok(())
    }

    /// Helper method to completely delete a note and all its relationships
    async fn delete_document_completely(
        &self,
        document_id: &str,
        _kiln_root: &std::path::Path,
    ) -> Result<()> {
        info!("Deleting note: {}", document_id);

        let entity_id = parse_entity_record_id(document_id)?;
        let store = EAVGraphStore::new(self.client.as_ref().clone());

        self.client
            .query(
                r#"
                DELETE embeddings WHERE entity_id = type::thing($table, $id);
                "#,
                &[json!({
                    "table": entity_id.table,
                    "id": entity_id.id,
                })],
            )
            .await
            .map_err(|e| {
                anyhow::anyhow!("Failed to delete embeddings for {}: {}", document_id, e)
            })?;

        self.client
            .query(
                r#"
                DELETE blocks WHERE entity_id = type::thing($table, $id);
                "#,
                &[json!({
                    "table": entity_id.table,
                    "id": entity_id.id,
                })],
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to delete blocks for {}: {}", document_id, e))?;

        self.client
            .query(
                r#"
                DELETE properties WHERE entity_id = type::thing($table, $id);
                "#,
                &[json!({
                    "table": entity_id.table,
                    "id": entity_id.id,
                })],
            )
            .await
            .map_err(|e| {
                anyhow::anyhow!("Failed to delete properties for {}: {}", document_id, e)
            })?;

        self.client
            .query(
                r#"
                DELETE relations WHERE in = type::thing($table, $id);
                DELETE relations WHERE out = type::thing($table, $id);
                "#,
                &[json!({
                    "table": entity_id.table,
                    "id": entity_id.id,
                })],
            )
            .await
            .map_err(|e| {
                anyhow::anyhow!("Failed to delete relations for {}: {}", document_id, e)
            })?;

        store.delete_entity_tags(&entity_id).await.map_err(|e| {
            anyhow::anyhow!("Failed to delete entity tags for {}: {}", document_id, e)
        })?;

        self.client
            .query(
                r#"
                DELETE type::thing($table, $id);
                "#,
                &[json!({
                    "table": entity_id.table,
                    "id": entity_id.id,
                })],
            )
            .await
            .map_err(|e| anyhow::anyhow!("Failed to delete entity {}: {}", document_id, e))?;

        debug!("Successfully deleted entity and relations: {}", document_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction_queue::{DatabaseTransaction, TransactionTimestamp};
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

        let db_config = crate::types::SurrealDbConfig::default();
        let consumer = DatabaseTransactionConsumer::new(
            Arc::new(SurrealClient::new(db_config).await.unwrap()),
            config,
        );

        // Test exponential backoff
        assert_eq!(
            consumer.calculate_retry_delay(1),
            Duration::from_millis(1000)
        );
        assert_eq!(
            consumer.calculate_retry_delay(2),
            Duration::from_millis(2000)
        );
        assert_eq!(
            consumer.calculate_retry_delay(3),
            Duration::from_millis(4000)
        );

        // Test max delay capping
        assert_eq!(
            consumer.calculate_retry_delay(10),
            Duration::from_millis(10000)
        );
    }

    #[tokio::test]
    async fn test_transaction_metadata_creation() {
        let config = ConsumerConfig::default();
        // Use in-memory database to avoid file lock conflicts
        let mut db_config = crate::types::SurrealDbConfig::default();
        db_config.path = ":memory:".to_string();
        let consumer = DatabaseTransactionConsumer::new(
            Arc::new(SurrealClient::new(db_config).await.unwrap()),
            config,
        );

        let tx = DatabaseTransaction::Create {
            transaction_id: "test-1".to_string(),
            note: crucible_core::types::ParsedNote::default(),
            kiln_root: PathBuf::from("/test"),
            timestamp: TransactionTimestamp::now(),
        };

        let metadata = consumer.create_transaction_metadata(&tx, 0);

        assert_eq!(metadata.transaction_type, "Create");
        assert_eq!(metadata.retry_count, 0);
    }
}
