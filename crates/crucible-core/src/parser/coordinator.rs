//! Parser-Storage Coordinator
//!
//! This module provides the coordinator that handles complex operations between the parser
//! and storage systems. It manages batch processing, error handling, and provides a unified
//! interface for parsing + storage operations with advanced capabilities.
//!
//! ## Architecture
//!
//! The coordinator follows the orchestrator pattern:
//! - **Coordination**: Manages interactions between parser and storage
//! - **Batch Processing**: Handles multiple documents efficiently
//! - **Error Recovery**: Graceful error handling and rollback capabilities
//! - **Performance Optimization**: Parallel processing and caching
//! - **Transaction Support**: Atomic operations across parser and storage

use crate::hashing::blake3::Blake3Hasher;
use crate::parser::error::ParserResult;
use crate::parser::storage_bridge::{
    StorageAwareMarkdownParser, StorageAwareParseResult, StorageAwareParserConfig,
};
use crate::storage::{ContentAddressedStorage, EnhancedTreeChange, MerkleTree, StorageResult};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{RwLock, Semaphore};

/// Configuration for the parser-storage coordinator
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    /// Maximum number of concurrent parsing operations
    pub max_concurrent_operations: usize,
    /// Enable parallel processing for batch operations
    pub enable_parallel_processing: bool,
    /// Timeout for individual operations (in seconds)
    pub operation_timeout_seconds: u64,
    /// Enable operation rollback on errors
    pub enable_rollback: bool,
    /// Cache size for frequently accessed documents
    pub cache_size: usize,
    /// Enable detailed operation logging
    pub enable_logging: bool,
    /// Enable transaction support for batch operations
    pub enable_transactions: bool,
    /// Maximum number of documents in a single batch
    pub max_batch_size: usize,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            max_concurrent_operations: 10,
            enable_parallel_processing: true,
            operation_timeout_seconds: 300, // 5 minutes
            enable_rollback: true,
            cache_size: 1000,
            enable_logging: true,
            enable_transactions: true,
            max_batch_size: 100,
        }
    }
}

/// Represents a parsing operation with its context
#[derive(Debug, Clone)]
pub struct ParsingOperation {
    /// Unique identifier for the operation
    pub id: String,
    /// Source path of the document
    pub source_path: PathBuf,
    /// Content to parse (if parsing from string)
    pub content: Option<String>,
    /// Operation type
    pub operation_type: OperationType,
    /// Priority of the operation
    pub priority: OperationPriority,
    /// Metadata associated with the operation
    pub metadata: OperationMetadata,
}

/// Types of parsing operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationType {
    /// Parse from file path
    FromFile,
    /// Parse from string content
    FromContent,
    /// Parse and compare with previous result
    CompareWithPrevious,
    /// Re-parse with change detection
    ReparseWithChanges,
}

/// Priority levels for operations
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OperationPriority {
    Low = 1,
    Normal = 2,
    High = 3,
    Critical = 4,
}

/// Metadata associated with parsing operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationMetadata {
    /// Timestamp when operation was created
    pub created_at: DateTime<Utc>,
    /// User or system that initiated the operation
    pub initiator: String,
    /// Tags for categorizing the operation
    pub tags: Vec<String>,
    /// Additional key-value metadata
    pub custom_fields: HashMap<String, String>,
}

impl Default for OperationMetadata {
    fn default() -> Self {
        Self {
            created_at: Utc::now(),
            initiator: "system".to_string(),
            tags: Vec::new(),
            custom_fields: HashMap::new(),
        }
    }
}

/// Result of a parsing operation
#[derive(Debug, Clone)]
pub struct OperationResult {
    /// Operation identifier
    pub operation_id: String,
    /// Result of parsing operation
    pub parse_result: StorageAwareParseResult,
    /// Duration of the operation in milliseconds
    pub duration_ms: u64,
    /// Whether the operation succeeded
    pub success: bool,
    /// Error information if operation failed
    pub error: Option<String>,
    /// Changes detected during the operation
    pub changes: Vec<EnhancedTreeChange>,
}

/// Batch operation result
#[derive(Debug, Clone)]
pub struct BatchOperationResult {
    /// Batch identifier
    pub batch_id: String,
    /// Results for individual operations
    pub operation_results: Vec<OperationResult>,
    /// Total duration of the batch in milliseconds
    pub total_duration_ms: u64,
    /// Number of successful operations
    pub successful_operations: usize,
    /// Number of failed operations
    pub failed_operations: usize,
    /// Overall success status
    pub success: bool,
    /// Aggregate statistics
    pub aggregate_statistics: BatchStatistics,
}

/// Aggregate statistics for batch operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchStatistics {
    /// Total number of documents processed
    pub total_documents: usize,
    /// Total content size in bytes
    pub total_content_size: usize,
    /// Total number of blocks created
    pub total_blocks: usize,
    /// Total unique blocks (after deduplication)
    pub total_unique_blocks: usize,
    /// Average deduplication ratio
    pub average_deduplication_ratio: f32,
    /// Average parse time per document
    pub average_parse_time_ms: f64,
    /// Average storage time per document
    pub average_storage_time_ms: f64,
    /// Total number of changes detected
    pub total_changes: usize,
    /// Timestamp when batch was completed
    pub completed_at: DateTime<Utc>,
}

/// Transaction context for batch operations
#[derive(Debug)]
pub struct TransactionContext {
    /// Transaction identifier
    pub id: String,
    /// Operations in this transaction
    pub operations: Vec<String>,
    /// Storage operations performed
    pub storage_operations: Vec<StorageOperation>,
    /// Start time of the transaction
    pub start_time: SystemTime,
    /// Whether transaction can be rolled back
    pub rollback_enabled: bool,
}

/// Storage operation performed within a transaction
#[derive(Debug, Clone)]
pub enum StorageOperation {
    StoreBlock { hash: String },
    StoreTree { root_hash: String },
    DeleteBlock { hash: String },
    DeleteTree { root_hash: String },
}

/// Parser-Storage Coordinator trait
///
/// This trait defines the interface for coordinating complex parsing and storage operations
/// with support for batch processing, transactions, and error recovery.
#[async_trait]
pub trait ParserStorageCoordinator: Send + Sync {
    /// Process a single parsing operation
    ///
    /// # Arguments
    /// * `operation` - The parsing operation to process
    ///
    /// # Returns
    /// Result of the operation
    async fn process_operation(&self, operation: ParsingOperation)
        -> ParserResult<OperationResult>;

    /// Process multiple operations in a batch
    ///
    /// # Arguments
    /// * `operations` - List of operations to process
    /// * `transaction_enabled` - Whether to process as a transaction
    ///
    /// # Returns
    /// Batch operation result
    async fn process_batch(
        &self,
        operations: Vec<ParsingOperation>,
        transaction_enabled: bool,
    ) -> ParserResult<BatchOperationResult>;

    /// Start a new transaction
    ///
    /// # Returns
    /// Transaction context
    async fn begin_transaction(&self) -> ParserResult<TransactionContext>;

    /// Commit a transaction
    ///
    /// # Arguments
    /// * `context` - Transaction context to commit
    ///
    /// # Returns
    /// Success status
    async fn commit_transaction(&self, context: TransactionContext) -> ParserResult<bool>;

    /// Rollback a transaction
    ///
    /// # Arguments
    /// * `context` - Transaction context to rollback
    ///
    /// # Returns
    /// Success status
    async fn rollback_transaction(&self, context: TransactionContext) -> ParserResult<bool>;

    /// Get operation status
    ///
    /// # Arguments
    /// * `operation_id` - Operation identifier
    ///
    /// # Returns
    /// Current operation status
    async fn get_operation_status(
        &self,
        operation_id: &str,
    ) -> ParserResult<Option<OperationResult>>;

    /// Cancel an ongoing operation
    ///
    /// # Arguments
    /// * `operation_id` - Operation identifier
    ///
    /// # Returns
    /// Success status
    async fn cancel_operation(&self, operation_id: &str) -> ParserResult<bool>;

    /// Get coordinator statistics
    ///
    /// # Returns
    /// Coordinator performance statistics
    async fn get_statistics(&self) -> ParserResult<CoordinatorStatistics>;

    /// Clear operation cache
    ///
    /// # Returns
    /// Success status
    async fn clear_cache(&self) -> ParserResult<bool>;
}

/// Coordinator performance statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorStatistics {
    /// Total number of operations processed
    pub total_operations: u64,
    /// Number of successful operations
    pub successful_operations: u64,
    /// Number of failed operations
    pub failed_operations: u64,
    /// Average operation time in milliseconds
    pub average_operation_time_ms: f64,
    /// Current cache size
    pub current_cache_size: usize,
    /// Number of active operations
    pub active_operations: usize,
    /// Number of pending operations
    pub pending_operations: usize,
    /// Timestamp of last operation
    pub last_operation_time: Option<DateTime<Utc>>,
}

/// Default implementation of the Parser-Storage Coordinator
pub struct DefaultParserStorageCoordinator {
    /// Storage-aware parser
    parser: Arc<dyn StorageAwareMarkdownParser>,
    /// Storage backend
    storage: Arc<dyn ContentAddressedStorage>,
    /// Coordinator configuration
    config: CoordinatorConfig,
    /// Operation cache
    operation_cache: Arc<RwLock<HashMap<String, OperationResult>>>,
    /// Semaphore for limiting concurrent operations
    operation_semaphore: Arc<Semaphore>,
    /// Active operations tracking
    active_operations: Arc<RwLock<HashSet<String>>>,
    /// Statistics tracking
    statistics: Arc<RwLock<CoordinatorStatistics>>,
}

impl DefaultParserStorageCoordinator {
    /// Create a new coordinator with the given parser and storage
    ///
    /// # Arguments
    /// * `parser` - Storage-aware parser instance
    /// * `storage` - Storage backend instance
    /// * `config` - Coordinator configuration
    ///
    /// # Returns
    /// New coordinator instance
    pub fn new(
        parser: Arc<dyn StorageAwareMarkdownParser>,
        storage: Arc<dyn ContentAddressedStorage>,
        config: CoordinatorConfig,
    ) -> Self {
        let max_concurrent = config.max_concurrent_operations;
        Self {
            parser,
            storage,
            config,
            operation_cache: Arc::new(RwLock::new(HashMap::new())),
            operation_semaphore: Arc::new(Semaphore::new(max_concurrent)),
            active_operations: Arc::new(RwLock::new(HashSet::new())),
            statistics: Arc::new(RwLock::new(CoordinatorStatistics {
                total_operations: 0,
                successful_operations: 0,
                failed_operations: 0,
                average_operation_time_ms: 0.0,
                current_cache_size: 0,
                active_operations: 0,
                pending_operations: 0,
                last_operation_time: None,
            })),
        }
    }

    /// Create a coordinator with default components
    ///
    /// # Arguments
    /// * `config` - Coordinator configuration
    ///
    /// # Returns
    /// New coordinator instance or error
    pub async fn with_default_components(config: CoordinatorConfig) -> ParserResult<Self> {
        // Create default parser
        let base_parser = Box::new(crate::parser::bridge::ParserAdapter::new());
        let parser = Arc::new(
            crate::parser::storage_bridge::StorageAwareParser::with_config(
                base_parser,
                StorageAwareParserConfig::default(),
                Arc::new(Blake3Hasher::new()),
            ),
        );

        // Create default storage (mock for now)
        let storage = Arc::new(MockStorageBackend::new());

        Ok(Self::new(parser, storage, config))
    }

    /// Generate a unique operation ID
    #[allow(dead_code)] // Reserved for future operation tracking
    fn generate_operation_id(&self) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let count = COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("op_{}_{}", count, Utc::now().timestamp_millis())
    }

    /// Update statistics after an operation
    async fn update_statistics(&self, duration_ms: u64, success: bool) {
        let mut stats = self.statistics.write().await;
        stats.total_operations += 1;

        if success {
            stats.successful_operations += 1;
        } else {
            stats.failed_operations += 1;
        }

        // Update average operation time
        let total_time = stats.average_operation_time_ms * (stats.total_operations - 1) as f64;
        stats.average_operation_time_ms =
            (total_time + duration_ms as f64) / stats.total_operations as f64;
        stats.last_operation_time = Some(Utc::now());
    }

    /// Cache an operation result
    async fn cache_operation_result(&self, operation_id: &str, result: OperationResult) {
        let mut cache = self.operation_cache.write().await;

        // Check cache size limit
        if cache.len() >= self.config.cache_size {
            // Remove oldest entry (simple LRU)
            if let Some(oldest_key) = cache.keys().next().cloned() {
                cache.remove(&oldest_key);
            }
        }

        cache.insert(operation_id.to_string(), result);

        // Update cache size in statistics
        let mut stats = self.statistics.write().await;
        stats.current_cache_size = cache.len();
    }
}

#[async_trait]
impl ParserStorageCoordinator for DefaultParserStorageCoordinator {
    async fn process_operation(
        &self,
        operation: ParsingOperation,
    ) -> ParserResult<OperationResult> {
        let start_time = SystemTime::now();
        let operation_id = operation.id.clone();

        // Check concurrency limit
        let _permit = match self.operation_semaphore.try_acquire() {
            Ok(permit) => permit,
            Err(_) => {
                // Wait for available slot
                self.operation_semaphore.acquire().await.map_err(|_| {
                    crate::parser::error::ParserError::ParseFailed(
                        "Failed to acquire operation permit".to_string(),
                    )
                })?
            }
        };

        // Mark operation as active
        {
            let mut active = self.active_operations.write().await;
            active.insert(operation_id.clone());

            // Update statistics
            let mut stats = self.statistics.write().await;
            stats.active_operations = active.len();
        }

        let result = match operation.operation_type {
            OperationType::FromFile => {
                let parse_result = self
                    .parser
                    .parse_file_with_storage(
                        &operation.source_path,
                        Some(Arc::clone(&self.storage)),
                    )
                    .await;

                match parse_result {
                    Ok(result) => OperationResult {
                        operation_id: operation_id.clone(),
                        parse_result: result,
                        duration_ms: 0, // Will be set below
                        success: true,
                        error: None,
                        changes: Vec::new(),
                    },
                    Err(e) => OperationResult {
                        operation_id: operation_id.clone(),
                        parse_result: StorageAwareParseResult::default(), // Will be ignored due to success=false
                        duration_ms: 0,
                        success: false,
                        error: Some(e.to_string()),
                        changes: Vec::new(),
                    },
                }
            }
            OperationType::FromContent => {
                let content = operation.content.ok_or_else(|| {
                    crate::parser::error::ParserError::ParseFailed(
                        "Content not provided for FromContent operation".to_string(),
                    )
                })?;

                let parse_result = self
                    .parser
                    .parse_content_with_storage(
                        &content,
                        &operation.source_path,
                        Some(Arc::clone(&self.storage)),
                    )
                    .await;

                match parse_result {
                    Ok(result) => OperationResult {
                        operation_id: operation_id.clone(),
                        parse_result: result,
                        duration_ms: 0,
                        success: true,
                        error: None,
                        changes: Vec::new(),
                    },
                    Err(e) => OperationResult {
                        operation_id: operation_id.clone(),
                        parse_result: StorageAwareParseResult::default(),
                        duration_ms: 0,
                        success: false,
                        error: Some(e.to_string()),
                        changes: Vec::new(),
                    },
                }
            }
            OperationType::CompareWithPrevious | OperationType::ReparseWithChanges => {
                // These would require previous results from cache
                OperationResult {
                    operation_id: operation_id.clone(),
                    parse_result: StorageAwareParseResult::default(),
                    duration_ms: 0,
                    success: false,
                    error: Some("Operation type not yet implemented".to_string()),
                    changes: Vec::new(),
                }
            }
        };

        // Calculate duration
        let duration = SystemTime::now()
            .duration_since(start_time)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut final_result = result;
        final_result.duration_ms = duration;

        // Mark operation as complete
        {
            let mut active = self.active_operations.write().await;
            active.remove(&operation_id);

            // Update statistics
            let mut stats = self.statistics.write().await;
            stats.active_operations = active.len();
        }

        // Cache the result
        self.cache_operation_result(&operation_id, final_result.clone())
            .await;

        // Update statistics
        self.update_statistics(duration, final_result.success).await;

        Ok(final_result)
    }

    async fn process_batch(
        &self,
        operations: Vec<ParsingOperation>,
        transaction_enabled: bool,
    ) -> ParserResult<BatchOperationResult> {
        let start_time = SystemTime::now();
        let batch_id = format!("batch_{}", Utc::now().timestamp_millis());

        if operations.is_empty() {
            return Err(crate::parser::error::ParserError::ParseFailed(
                "Empty operation batch".to_string(),
            ));
        }

        if operations.len() > self.config.max_batch_size {
            return Err(crate::parser::error::ParserError::ParseFailed(format!(
                "Batch size {} exceeds maximum {}",
                operations.len(),
                self.config.max_batch_size
            )));
        }

        // Begin transaction if enabled
        let transaction_context = if transaction_enabled && self.config.enable_transactions {
            Some(self.begin_transaction().await?)
        } else {
            None
        };

        // Process operations (sequential for now to avoid futures dependency)
        let mut operation_results = Vec::new();
        for operation in operations {
            match self.process_operation(operation).await {
                Ok(result) => operation_results.push(result),
                Err(e) => {
                    // Add failed result
                    operation_results.push(OperationResult {
                        operation_id: "unknown".to_string(),
                        parse_result: StorageAwareParseResult::default(),
                        duration_ms: 0,
                        success: false,
                        error: Some(e.to_string()),
                        changes: Vec::new(),
                    });
                }
            }
        }

        // Calculate statistics
        let successful_count = operation_results.iter().filter(|r| r.success).count();
        let failed_count = operation_results.len() - successful_count;
        let total_duration = SystemTime::now()
            .duration_since(start_time)
            .unwrap_or_default()
            .as_millis() as u64;

        // Calculate aggregate statistics
        let aggregate_stats = self.calculate_batch_statistics(&operation_results);

        let batch_result = BatchOperationResult {
            batch_id,
            operation_results,
            total_duration_ms: total_duration,
            successful_operations: successful_count,
            failed_operations: failed_count,
            success: failed_count == 0,
            aggregate_statistics: aggregate_stats,
        };

        // Commit or rollback transaction
        if let Some(context) = transaction_context {
            if batch_result.success {
                self.commit_transaction(context).await?;
            } else {
                self.rollback_transaction(context).await?;
            }
        }

        Ok(batch_result)
    }

    async fn begin_transaction(&self) -> ParserResult<TransactionContext> {
        let transaction_id = format!("tx_{}", Utc::now().timestamp_millis());

        Ok(TransactionContext {
            id: transaction_id,
            operations: Vec::new(),
            storage_operations: Vec::new(),
            start_time: SystemTime::now(),
            rollback_enabled: self.config.enable_rollback,
        })
    }

    async fn commit_transaction(&self, _context: TransactionContext) -> ParserResult<bool> {
        // In a real implementation, this would finalize all storage operations
        // and make them permanent
        Ok(true)
    }

    async fn rollback_transaction(&self, _context: TransactionContext) -> ParserResult<bool> {
        // In a real implementation, this would undo all storage operations
        // performed within the transaction
        Ok(true)
    }

    async fn get_operation_status(
        &self,
        operation_id: &str,
    ) -> ParserResult<Option<OperationResult>> {
        let cache = self.operation_cache.read().await;
        Ok(cache.get(operation_id).cloned())
    }

    async fn cancel_operation(&self, operation_id: &str) -> ParserResult<bool> {
        let mut active = self.active_operations.write().await;

        if active.remove(operation_id) {
            // Update statistics
            let mut stats = self.statistics.write().await;
            stats.active_operations = active.len();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn get_statistics(&self) -> ParserResult<CoordinatorStatistics> {
        let stats = self.statistics.read().await;
        Ok(stats.clone())
    }

    async fn clear_cache(&self) -> ParserResult<bool> {
        {
            let mut cache = self.operation_cache.write().await;
            cache.clear();
        }

        let mut stats = self.statistics.write().await;
        stats.current_cache_size = 0;

        Ok(true)
    }
}

impl DefaultParserStorageCoordinator {
    /// Calculate aggregate statistics for batch operations
    pub fn calculate_batch_statistics(&self, results: &[OperationResult]) -> BatchStatistics {
        let total_documents = results.len();
        let mut total_content_size = 0usize;
        let mut total_blocks = 0usize;
        let mut total_unique_blocks = 0usize;
        let mut total_deduplication_ratio = 0.0f32;
        let mut total_parse_time = 0.0f64;
        let mut total_storage_time = 0.0f64;
        let mut total_changes = 0usize;

        for result in results {
            if result.success {
                total_content_size += result.parse_result.statistics.content_size_bytes;
                total_blocks += result.parse_result.statistics.block_count;
                total_unique_blocks += result.parse_result.statistics.unique_blocks;
                total_deduplication_ratio += result.parse_result.statistics.deduplication_ratio;
                total_parse_time += result.parse_result.statistics.parse_time_ms as f64;
                total_storage_time += result.parse_result.statistics.storage_time_ms as f64;
                total_changes += result.changes.len();
            }
        }

        BatchStatistics {
            total_documents,
            total_content_size,
            total_blocks,
            total_unique_blocks,
            average_deduplication_ratio: if total_documents > 0 {
                total_deduplication_ratio / total_documents as f32
            } else {
                0.0
            },
            average_parse_time_ms: if total_documents > 0 {
                total_parse_time / total_documents as f64
            } else {
                0.0
            },
            average_storage_time_ms: if total_documents > 0 {
                total_storage_time / total_documents as f64
            } else {
                0.0
            },
            total_changes,
            completed_at: Utc::now(),
        }
    }
}

/// Mock storage backend for testing
#[derive(Debug)]
#[allow(dead_code)] // Test infrastructure
pub struct MockStorageBackend {
    name: String,
}

impl MockStorageBackend {
    pub fn new() -> Self {
        Self {
            name: "mock_storage".to_string(),
        }
    }
}

#[async_trait::async_trait]
impl crate::storage::traits::BlockOperations for MockStorageBackend {
    async fn store_block(&self, _hash: &str, _data: &[u8]) -> StorageResult<()> {
        Ok(())
    }

    async fn get_block(&self, _hash: &str) -> StorageResult<Option<Vec<u8>>> {
        Ok(None)
    }

    async fn block_exists(&self, _hash: &str) -> StorageResult<bool> {
        Ok(false)
    }

    async fn delete_block(&self, _hash: &str) -> StorageResult<bool> {
        Ok(false)
    }
}

#[async_trait::async_trait]
impl crate::storage::traits::TreeOperations for MockStorageBackend {
    async fn store_tree(&self, _root_hash: &str, _tree: &MerkleTree) -> StorageResult<()> {
        Ok(())
    }

    async fn get_tree(&self, _root_hash: &str) -> StorageResult<Option<MerkleTree>> {
        Ok(None)
    }

    async fn tree_exists(&self, _root_hash: &str) -> StorageResult<bool> {
        Ok(false)
    }

    async fn delete_tree(&self, _root_hash: &str) -> StorageResult<bool> {
        Ok(false)
    }
}

#[async_trait::async_trait]
impl crate::storage::traits::StorageManagement for MockStorageBackend {
    async fn get_stats(&self) -> StorageResult<crate::storage::traits::StorageStats> {
        Ok(crate::storage::traits::StorageStats {
            backend: crate::storage::traits::StorageBackend::InMemory,
            block_count: 0,
            block_size_bytes: 0,
            tree_count: 0,
            deduplication_savings: 0,
            average_block_size: 0.0,
            largest_block_size: 0,
            evicted_blocks: 0,
            quota_usage: None,
        })
    }

    async fn maintenance(&self) -> StorageResult<()> {
        Ok(())
    }
}

// Blanket implementation for the composite trait
impl ContentAddressedStorage for MockStorageBackend {}

/// Factory functions for creating coordinators
pub mod factory {
    use super::*;

    /// Create a coordinator with default configuration
    ///
    /// # Arguments
    /// * `parser` - Storage-aware parser
    /// * `storage` - Storage backend
    ///
    /// # Returns
    /// New coordinator instance
    pub fn create_coordinator(
        parser: Arc<dyn StorageAwareMarkdownParser>,
        storage: Arc<dyn ContentAddressedStorage>,
    ) -> impl ParserStorageCoordinator {
        DefaultParserStorageCoordinator::new(parser, storage, CoordinatorConfig::default())
    }

    /// Create a coordinator with custom configuration
    ///
    /// # Arguments
    /// * `parser` - Storage-aware parser
    /// * `storage` - Storage backend
    /// * `config` - Coordinator configuration
    ///
    /// # Returns
    /// New coordinator instance
    pub fn create_coordinator_with_config(
        parser: Arc<dyn StorageAwareMarkdownParser>,
        storage: Arc<dyn ContentAddressedStorage>,
        config: CoordinatorConfig,
    ) -> impl ParserStorageCoordinator {
        DefaultParserStorageCoordinator::new(parser, storage, config)
    }

    /// Create a coordinator with default components
    ///
    /// # Arguments
    /// * `config` - Optional coordinator configuration
    ///
    /// # Returns
    /// New coordinator instance
    pub async fn create_default_coordinator(
        config: Option<CoordinatorConfig>,
    ) -> ParserResult<impl ParserStorageCoordinator> {
        let config = config.unwrap_or_default();
        DefaultParserStorageCoordinator::with_default_components(config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::storage_bridge::factory as parser_factory;

    #[test]
    fn test_coordinator_config_default() {
        let config = CoordinatorConfig::default();

        assert_eq!(config.max_concurrent_operations, 10);
        assert!(config.enable_parallel_processing);
        assert_eq!(config.operation_timeout_seconds, 300);
        assert!(config.enable_rollback);
        assert_eq!(config.cache_size, 1000);
        assert!(config.enable_logging);
        assert!(config.enable_transactions);
        assert_eq!(config.max_batch_size, 100);
    }

    #[test]
    fn test_operation_metadata_default() {
        let metadata = OperationMetadata::default();

        assert_eq!(metadata.initiator, "system");
        assert!(metadata.tags.is_empty());
        assert!(metadata.custom_fields.is_empty());
        assert!(metadata.created_at <= Utc::now());
    }

    #[test]
    fn test_parsing_operation_creation() {
        let operation = ParsingOperation {
            id: "test_op".to_string(),
            source_path: PathBuf::from("test.md"),
            content: Some("# Test".to_string()),
            operation_type: OperationType::FromContent,
            priority: OperationPriority::Normal,
            metadata: OperationMetadata::default(),
        };

        assert_eq!(operation.id, "test_op");
        assert_eq!(operation.source_path, PathBuf::from("test.md"));
        assert_eq!(operation.operation_type, OperationType::FromContent);
        assert_eq!(operation.priority, OperationPriority::Normal);
    }

    #[test]
    fn test_operation_priority_ordering() {
        assert!(OperationPriority::Critical > OperationPriority::High);
        assert!(OperationPriority::High > OperationPriority::Normal);
        assert!(OperationPriority::Normal > OperationPriority::Low);
    }

    #[tokio::test]
    async fn test_create_default_coordinator() {
        let result = factory::create_default_coordinator(None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_coordinator_statistics_initialization() {
        let coordinator = factory::create_default_coordinator(None).await.unwrap();
        let stats = coordinator.get_statistics().await.unwrap();

        assert_eq!(stats.total_operations, 0);
        assert_eq!(stats.successful_operations, 0);
        assert_eq!(stats.failed_operations, 0);
        assert_eq!(stats.average_operation_time_ms, 0.0);
        assert_eq!(stats.current_cache_size, 0);
        assert_eq!(stats.active_operations, 0);
        assert_eq!(stats.pending_operations, 0);
        assert!(stats.last_operation_time.is_none());
    }

    #[tokio::test]
    async fn test_clear_cache() {
        let coordinator = factory::create_default_coordinator(None).await.unwrap();
        let result = coordinator.clear_cache().await;

        assert!(result.is_ok());
        assert!(result.unwrap());

        // Verify cache is cleared
        let stats = coordinator.get_statistics().await.unwrap();
        assert_eq!(stats.current_cache_size, 0);
    }

    #[tokio::test]
    async fn test_begin_transaction() {
        let coordinator = factory::create_default_coordinator(None).await.unwrap();
        let result = coordinator.begin_transaction().await;

        assert!(result.is_ok());
        let context = result.unwrap();
        assert!(!context.id.is_empty());
        assert!(context.rollback_enabled);
    }

    #[tokio::test]
    async fn test_process_empty_batch() {
        let coordinator = factory::create_default_coordinator(None).await.unwrap();
        let result = coordinator.process_batch(vec![], false).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Empty operation batch"));
    }

    #[tokio::test]
    async fn test_process_batch_oversized() {
        let coordinator = factory::create_coordinator_with_config(
            Arc::new(parser_factory::create_storage_aware_parser()),
            Arc::new(MockStorageBackend::new()),
            CoordinatorConfig {
                max_batch_size: 2,
                ..Default::default()
            },
        );

        let operations = vec![
            ParsingOperation {
                id: "op1".to_string(),
                source_path: PathBuf::from("test1.md"),
                content: Some("content1".to_string()),
                operation_type: OperationType::FromContent,
                priority: OperationPriority::Normal,
                metadata: OperationMetadata::default(),
            },
            ParsingOperation {
                id: "op2".to_string(),
                source_path: PathBuf::from("test2.md"),
                content: Some("content2".to_string()),
                operation_type: OperationType::FromContent,
                priority: OperationPriority::Normal,
                metadata: OperationMetadata::default(),
            },
            ParsingOperation {
                id: "op3".to_string(),
                source_path: PathBuf::from("test3.md"),
                content: Some("content3".to_string()),
                operation_type: OperationType::FromContent,
                priority: OperationPriority::Normal,
                metadata: OperationMetadata::default(),
            },
        ];

        let result = coordinator.process_batch(operations, false).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds maximum"));
    }

    #[tokio::test]
    async fn test_get_nonexistent_operation_status() {
        let coordinator = factory::create_default_coordinator(None).await.unwrap();
        let result = coordinator.get_operation_status("nonexistent").await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_cancel_nonexistent_operation() {
        let coordinator = factory::create_default_coordinator(None).await.unwrap();
        let result = coordinator.cancel_operation("nonexistent").await;

        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_calculate_batch_statistics_empty() {
        let coordinator = factory::create_coordinator_with_config(
            Arc::new(parser_factory::create_storage_aware_parser()),
            Arc::new(MockStorageBackend::new()),
            CoordinatorConfig::default(),
        );

        let results: Vec<OperationResult> = vec![];
        // Use a concrete coordinator for testing
        let concrete_coordinator = DefaultParserStorageCoordinator::new(
            Arc::new(parser_factory::create_storage_aware_parser()),
            Arc::new(MockStorageBackend::new()),
            CoordinatorConfig::default(),
        );
        let stats = concrete_coordinator.calculate_batch_statistics(&results);

        assert_eq!(stats.total_documents, 0);
        assert_eq!(stats.total_content_size, 0);
        assert_eq!(stats.total_blocks, 0);
        assert_eq!(stats.total_unique_blocks, 0);
        assert_eq!(stats.average_deduplication_ratio, 0.0);
        assert_eq!(stats.average_parse_time_ms, 0.0);
        assert_eq!(stats.average_storage_time_ms, 0.0);
        assert_eq!(stats.total_changes, 0);
    }

    #[test]
    fn test_batch_statistics_serialization() {
        let stats = BatchStatistics {
            total_documents: 10,
            total_content_size: 10240,
            total_blocks: 20,
            total_unique_blocks: 18,
            average_deduplication_ratio: 0.1,
            average_parse_time_ms: 50.5,
            average_storage_time_ms: 25.3,
            total_changes: 5,
            completed_at: Utc::now(),
        };

        // Test that we can serialize and deserialize
        let json = serde_json::to_string(&stats).unwrap();
        let deserialized: BatchStatistics = serde_json::from_str(&json).unwrap();

        assert_eq!(stats.total_documents, deserialized.total_documents);
        assert_eq!(stats.total_content_size, deserialized.total_content_size);
        assert_eq!(
            stats.average_deduplication_ratio,
            deserialized.average_deduplication_ratio
        );
    }
}
