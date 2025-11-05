//! Transaction Result System
//!
//! This module provides high-level result handling for database transactions
//! in the queue-based architecture. It bridges the low-level oneshot channels
//! from the transaction queue with a more user-friendly result handling interface.

use crate::transaction_queue::{TransactionResult, ResultReceiver, QueueError};
use anyhow::Result;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, warn, error};

/// High-level transaction result handler
///
/// Provides a convenient interface for handling transaction results
/// with timeout support and error conversion.
pub struct TransactionResultHandler {
    /// Timeout for waiting for results
    default_timeout: Duration,
}

impl TransactionResultHandler {
    /// Create a new result handler with default timeout
    pub fn new() -> Self {
        Self {
            default_timeout: Duration::from_secs(30),
        }
    }

    /// Create a new result handler with custom timeout
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            default_timeout: timeout,
        }
    }

    /// Wait for a transaction result with the default timeout
    pub async fn wait_for_result(&self, receiver: ResultReceiver) -> Result<TransactionResult> {
        self.wait_for_result_with_timeout(receiver, self.default_timeout).await
    }

    /// Wait for a transaction result with a custom timeout
    pub async fn wait_for_result_with_timeout(
        &self,
        receiver: ResultReceiver,
        timeout_duration: Duration,
    ) -> Result<TransactionResult> {
        match timeout(timeout_duration, receiver).await {
            Ok(result) => match result {
                Ok(transaction_result) => {
                    debug!("Received transaction result: {}", transaction_result.transaction_id());
                    Ok(transaction_result)
                }
                Err(e) => {
                    error!("Error receiving transaction result: {}", e);
                    Err(anyhow::anyhow!("Failed to receive transaction result: {}", e))
                }
            },
            Err(_) => {
                warn!("Transaction result timed out after {:?}", timeout_duration);
                Err(QueueError::TransactionFailed(format!("Transaction timed out after {:?}", timeout_duration)).into())
            }
        }
    }

    /// Wait for multiple transaction results concurrently
    pub async fn wait_for_multiple_results(
        &self,
        receivers: Vec<ResultReceiver>,
    ) -> Vec<Result<TransactionResult>> {
        let futures: Vec<_> = receivers
            .into_iter()
            .map(|receiver| self.wait_for_result(receiver))
            .collect();

        futures::future::join_all(futures).await
    }

    /// Wait for multiple transaction results with individual timeouts
    pub async fn wait_for_multiple_results_with_timeouts(
        &self,
        receivers_with_timeouts: Vec<(ResultReceiver, Duration)>,
    ) -> Vec<Result<TransactionResult>> {
        let futures: Vec<_> = receivers_with_timeouts
            .into_iter()
            .map(|(receiver, timeout_duration)| self.wait_for_result_with_timeout(receiver, timeout_duration))
            .collect();

        futures::future::join_all(futures).await
    }
}

impl Default for TransactionResultHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about transaction results
#[derive(Debug, Clone)]
pub struct TransactionResultStats {
    /// Number of successful transactions
    pub successful: usize,

    /// Number of failed transactions
    pub failed: usize,

    /// Number of skipped transactions
    pub skipped: usize,

    /// Number of timeout transactions
    pub timeouts: usize,

    /// Total number of transactions
    pub total: usize,

    /// Average processing time for successful transactions
    pub avg_processing_time: Option<Duration>,

    /// Total processing time
    pub total_processing_time: Duration,
}

impl TransactionResultStats {
    /// Create empty statistics
    pub fn new() -> Self {
        Self {
            successful: 0,
            failed: 0,
            skipped: 0,
            timeouts: 0,
            total: 0,
            avg_processing_time: None,
            total_processing_time: Duration::from_millis(0),
        }
    }

    /// Add a transaction result to the statistics
    pub fn add_result(&mut self, result: &TransactionResult) {
        self.total += 1;

        match result {
            TransactionResult::Success { duration, .. } => {
                self.successful += 1;
                self.total_processing_time += *duration;
                self.update_avg_processing_time();
            }
            TransactionResult::Failure { .. } => {
                self.failed += 1;
            }
            TransactionResult::Skipped { .. } => {
                self.skipped += 1;
            }
        }
    }

    /// Add a timeout to the statistics
    pub fn add_timeout(&mut self) {
        self.total += 1;
        self.timeouts += 1;
    }

    /// Update the average processing time
    fn update_avg_processing_time(&mut self) {
        if self.successful > 0 {
            self.avg_processing_time = Some(self.total_processing_time / self.successful as u32);
        }
    }

    /// Get the success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.successful as f64 / self.total as f64) * 100.0
        }
    }

    /// Get the failure rate as a percentage
    pub fn failure_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.failed as f64 / self.total as f64) * 100.0
        }
    }
}

impl Default for TransactionResultStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Aggregates transaction results and provides statistics
pub struct TransactionResultAggregator {
    stats: TransactionResultStats,
}

impl TransactionResultAggregator {
    /// Create a new aggregator
    pub fn new() -> Self {
        Self {
            stats: TransactionResultStats::new(),
        }
    }

    /// Process a transaction result
    pub fn process_result(&mut self, result: &TransactionResult) {
        self.stats.add_result(result);
    }

    /// Process a timeout
    pub fn process_timeout(&mut self) {
        self.stats.add_timeout();
    }

    /// Get the current statistics
    pub fn stats(&self) -> &TransactionResultStats {
        &self.stats
    }

    /// Reset the statistics
    pub fn reset(&mut self) {
        self.stats = TransactionResultStats::new();
    }

    /// Check if the error rate is above a threshold
    pub fn high_error_rate(&self, threshold: f64) -> bool {
        self.stats.failure_rate() > threshold
    }

    /// Get a summary string
    pub fn summary(&self) -> String {
        format!(
            "Total: {}, Success: {:.1}%, Failed: {:.1}%, Skipped: {}, Timeouts: {}, Avg Time: {:?}",
            self.stats.total,
            self.stats.success_rate(),
            self.stats.failure_rate(),
            self.stats.skipped,
            self.stats.timeouts,
            self.stats.avg_processing_time.unwrap_or(Duration::from_millis(0))
        )
    }
}

impl Default for TransactionResultAggregator {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch result collector for processing multiple transactions together
pub struct BatchResultCollector {
    results: Vec<TransactionResult>,
    timeouts: usize,
}

impl BatchResultCollector {
    /// Create a new batch collector
    pub fn new() -> Self {
        Self {
            results: Vec::new(),
            timeouts: 0,
        }
    }

    /// Add a successful result
    pub fn add_result(&mut self, result: TransactionResult) {
        self.results.push(result);
    }

    /// Add a timeout
    pub fn add_timeout(&mut self) {
        self.timeouts += 1;
    }

    /// Get all successful results
    pub fn successful_results(&self) -> Vec<&TransactionResult> {
        self.results.iter().filter(|r| r.is_success()).collect()
    }

    /// Get all failed results
    pub fn failed_results(&self) -> Vec<&TransactionResult> {
        self.results.iter().filter(|r| !r.is_success()).collect()
    }

    /// Check if the batch was completely successful
    pub fn is_completely_successful(&self) -> bool {
        self.failed_results().is_empty() && self.timeouts == 0
    }

    /// Get the total number of transactions in the batch
    pub fn total_count(&self) -> usize {
        self.results.len() + self.timeouts
    }

    /// Get the success count
    pub fn success_count(&self) -> usize {
        self.successful_results().len()
    }

    /// Get the failure count
    pub fn failure_count(&self) -> usize {
        self.failed_results().len() + self.timeouts
    }

    /// Convert to statistics
    pub fn to_stats(&self) -> TransactionResultStats {
        let mut stats = TransactionResultStats::new();

        for result in &self.results {
            stats.add_result(result);
        }

        for _ in 0..self.timeouts {
            stats.add_timeout();
        }

        stats
    }

    /// Take all results and reset the collector
    pub fn take_results(&mut self) -> Vec<TransactionResult> {
        let results = std::mem::take(&mut self.results);
        let timeouts = std::mem::take(&mut self.timeouts);

        // Convert timeouts to failure results for consistency
        let timeout_results: Vec<TransactionResult> = (0..timeouts)
            .map(|_| TransactionResult::Failure {
                transaction_id: "timeout".to_string(),
                error: "Transaction timed out".to_string(),
                retry_count: 0,
                metadata: crate::transaction_queue::TransactionMetadata {
                    transaction_type: "unknown".to_string(),
                    file_path: None,
                    queue_depth: 0,
                    retry_count: 0,
                    queue_wait_time: Duration::from_millis(0),
                },
            })
            .collect();

        results.into_iter().chain(timeout_results).collect()
    }
}

impl Default for BatchResultCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction_queue::{TransactionMetadata, TransactionTimestamp, QueuedTransaction};
    use std::path::PathBuf;

    fn create_test_transaction_result(transaction_id: &str) -> TransactionResult {
        TransactionResult::Success {
            transaction_id: transaction_id.to_string(),
            duration: Duration::from_millis(100),
            metadata: TransactionMetadata {
                transaction_type: "StoreDocument".to_string(),
                file_path: Some(PathBuf::from("test.md")),
                queue_depth: 5,
                retry_count: 0,
                queue_wait_time: Duration::from_millis(10),
            },
        }
    }

    #[tokio::test]
    async fn test_result_handler_timeout() {
        let handler = TransactionResultHandler::with_timeout(Duration::from_millis(100));

        // Create a receiver that will never receive anything
        let (_sender, receiver) = tokio::sync::oneshot::channel();

        let result = handler.wait_for_result(receiver).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_result_handler_success() {
        let handler = TransactionResultHandler::new();

        let (sender, receiver) = tokio::sync::oneshot::channel();
        let test_result = create_test_transaction_result("test-1");

        // Send the result in a separate task
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let _ = sender.send(test_result);
        });

        let result = handler.wait_for_result(receiver).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().transaction_id(), "test-1");
    }

    #[test]
    fn test_result_stats() {
        let mut stats = TransactionResultStats::new();

        // Add some results
        stats.add_result(&create_test_transaction_result("test-1"));
        stats.add_result(&create_test_transaction_result("test-2"));
        stats.add_timeout();

        assert_eq!(stats.total, 3);
        assert_eq!(stats.successful, 2);
        assert_eq!(stats.timeouts, 1);
        assert_eq!(stats.success_rate(), 66.66666666666667);
    }

    #[test]
    fn test_result_aggregator() {
        let mut aggregator = TransactionResultAggregator::new();

        aggregator.process_result(&create_test_transaction_result("test-1"));
        aggregator.process_result(&create_test_transaction_result("test-2"));
        aggregator.process_timeout();

        assert_eq!(aggregator.stats().total, 3);
        assert_eq!(aggregator.stats().successful, 2);
        assert_eq!(aggregator.stats().timeouts, 1);

        let summary = aggregator.summary();
        assert!(summary.contains("Total: 3"));
        assert!(summary.contains("Success: 66.7%"));
    }

    #[test]
    fn test_batch_collector() {
        let mut collector = BatchResultCollector::new();

        collector.add_result(create_test_transaction_result("test-1"));
        collector.add_result(create_test_transaction_result("test-2"));
        collector.add_timeout();

        assert_eq!(collector.total_count(), 3);
        assert_eq!(collector.success_count(), 2);
        assert_eq!(collector.failure_count(), 1);
        assert!(!collector.is_completely_successful());
    }
}