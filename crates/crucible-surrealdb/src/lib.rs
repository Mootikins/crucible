//! # Crucible SurrealDB Backend
//!
//! This crate provides a SurrealDB backend implementation for the Crucible knowledge
//! management system. It uses SurrealDB with RocksDB storage for enhanced features and performance.
//!
//! ## Features
//!
//! - **Native Vector Storage**: Efficient storage of embeddings as arrays
//! - **Graph Relations**: Support for document relationships
//! - **ACID Transactions**: Full transaction support
//! - **Live Queries**: Real-time cache updates
//! - **Schema Flexibility**: Dynamic fields without migrations
//!
//! ## Usage
//!
//! ```rust,no_run
//! use crucible_surrealdb::SurrealEmbeddingDatabase;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let db = SurrealEmbeddingDatabase::new("./cache.db").await?;
//!     db.initialize().await?;
//!
//!     // Use the database...
//!
//!     Ok(())
//! }
//! ```

pub mod batch_aware_client;
pub mod consistency;
pub mod content_addressed_storage;
pub mod database;
pub mod kiln_integration;
pub mod kiln_pipeline_connector;
pub mod kiln_processor;
pub mod kiln_scanner;
pub mod kiln_store;
pub mod query;
pub mod schema_types;
pub mod surreal_client;
pub mod transaction_queue;
pub mod transaction_consumer;
pub mod simple_integration;
pub mod metrics;
pub mod migration;
pub mod types;
pub mod hash_lookup;
// Block storage module (currently has compilation errors, being refactored)
// pub mod block_storage;

// Embedding modules
pub mod embedding;
pub mod embedding_config;
pub mod embedding_pipeline;
pub mod embedding_pool;

pub use content_addressed_storage::ContentAddressedStorageSurrealDB;
pub use database::SurrealEmbeddingDatabase;
pub use kiln_store::{InMemoryKilnStore, KilnStore};
pub use schema_types::*;
pub use surreal_client::SurrealClient;
pub use transaction_queue::{
    DatabaseTransaction, QueueStats, TransactionQueueConfig, TransactionResult,
    TransactionReceiver, TransactionSender, ResultReceiver, ResultSender, StatsWatcher,
    TransactionTimestamp, QueueError,
};
pub use transaction_consumer::{
    DatabaseTransactionConsumer, ConsumerConfig, ConsumerStats, ShutdownSender, ShutdownReceiver,
};
// Re-export database types for external use
// Note: Use local type definitions but provide aliases for compatibility
pub use types::{
    DatabaseStats, DbError, DbResult, QueryResult, Record, RecordId, SelectQuery, TableSchema,
    SurrealDbConfig,
    // Legacy embedding types for tests
    Document, EmbeddingDocument, EmbeddingData, EmbeddingMetadata, SearchFilters, SearchQuery,
    BatchOperation, BatchOperationType,
};

// Re-export embedding functionality with specific exports to avoid conflicts
pub use embedding_config::{
    DocumentEmbedding, EmbeddingConfig, EmbeddingError, EmbeddingModel, EmbeddingProcessingResult,
    PrivacyMode, ThreadPoolMetrics,
};
pub use embedding_pipeline::EmbeddingPipeline;
pub use embedding_pool::{EmbeddingSignature, EmbeddingThreadPool};

// Kiln scanner exports
pub use kiln_processor::{
    process_document_embeddings, process_incremental_changes, process_kiln_delta,
    process_kiln_files, process_kiln_files_with_error_handling, scan_kiln_directory,
};
pub use kiln_scanner::{
    create_kiln_scanner, create_kiln_scanner_with_embeddings, parse_file_to_document,
    validate_kiln_scanner_config, ChangeDetectionMethod, ErrorHandlingMode, KilnFileInfo,
    KilnProcessError, KilnProcessResult, KilnScanError, KilnScanResult, KilnScanner,
    KilnScannerConfig, KilnScannerErrorType, KilnScannerMetrics, KilnScannerState,
    ChangeDetectionSummary,
};

// Kiln pipeline connector exports
pub use kiln_pipeline_connector::{
    generate_document_id_from_path, get_parsed_documents_from_scan,
    transform_parsed_document_to_embedding_inputs, BatchProcessingResult, DocumentProcessingResult,
    KilnPipelineConfig, KilnPipelineConnector,
};

// Simple integration exports (replaces complex QueueBasedProcessor)
pub use simple_integration::{
    enqueue_document, enqueue_document_deletion, enqueue_documents, get_queue_status,
};

// Metrics exports
pub use metrics::{
    SystemMetrics, SystemMetricsSnapshot, HealthStatus, HealthReport, get_global_metrics,
    record_transaction_success, record_transaction_failure, update_queue_depth, get_system_health,
    get_system_health_report, get_health_status,
};

// Hash lookup exports
pub use hash_lookup::{
    lookup_file_hash, lookup_file_hashes_batch, lookup_file_hashes_batch_cached, lookup_files_by_content_hashes,
    lookup_changed_files_since, check_file_needs_update, BatchLookupConfig, HashLookupResult,
    StoredFileHash, HashLookupCache, CacheStats,
};

// Block storage functionality is integrated into ContentAddressedStorageSurrealDB
pub use content_addressed_storage::DocumentBlockRecord;

// Deduplication detection functionality
pub mod deduplication_detector;
pub use deduplication_detector::{DeduplicationDetector, SurrealDeduplicationDetector};

// Deduplication reporting functionality
pub mod deduplication_reporting;
pub use deduplication_reporting::{
    DeduplicationReportGenerator, DeduplicationReport, ReportOptions, ExportFormat,
    ReportMetadata, ExecutiveSummary, Recommendation, RecommendationPriority,
    ImplementationEffort
};
