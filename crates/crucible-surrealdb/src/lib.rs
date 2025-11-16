//! # Crucible SurrealDB Backend
//!
//! This crate provides a SurrealDB backend implementation for the Crucible knowledge
//! management system. It uses SurrealDB with RocksDB storage for enhanced features and performance.
//!
//! ## Features
//!
//! - **Native Vector Storage**: Efficient storage of embeddings as arrays
//! - **Graph Relations**: Support for note relationships
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

use crucible_enrichment;

pub mod batch_aware_client;
pub mod consistency;
pub mod content_addressed_storage;
pub mod database;
pub mod eav_graph;
pub mod hash_lookup;
#[cfg(feature = "embeddings")]
pub mod kiln_integration;
// TODO: These modules (kiln_pipeline_connector, kiln_scanner) are part of the old
// embedding_pool polling architecture and need to be updated to use the new
// EnrichmentPipeline + NoteIngestor pattern. Disabled for now.
// #[cfg(feature = "embeddings")]
// pub mod kiln_pipeline_connector;
// #[cfg(feature = "embeddings")]
// pub mod kiln_scanner;
pub mod kiln_store;
pub mod merkle_persistence;
pub mod metrics;
pub mod migration;
pub mod query;
pub mod schema_types;
#[cfg(feature = "embeddings")]
pub mod simple_integration;
pub mod surreal_client;
#[cfg(feature = "embeddings")]
pub mod transaction_consumer;
pub mod transaction_queue;
pub mod types;

// Embedding modules
#[cfg(feature = "embeddings")]
pub mod embedding;
#[cfg(feature = "embeddings")]
#[cfg(feature = "embeddings")]

pub use content_addressed_storage::ContentAddressedStorageSurrealDB;
pub use database::SurrealEmbeddingDatabase;
pub use kiln_store::{InMemoryKilnStore, KilnStore};
pub use schema_types::*;
pub use surreal_client::SurrealClient;
#[cfg(feature = "embeddings")]
pub use transaction_consumer::{
    ConsumerConfig, ConsumerStats, DatabaseTransactionConsumer, ShutdownReceiver, ShutdownSender,
};
pub use transaction_queue::{
    DatabaseTransaction, QueueError, QueueStats, ResultReceiver, ResultSender, StatsWatcher,
    TransactionQueueConfig, TransactionReceiver, TransactionResult, TransactionSender,
    TransactionTimestamp,
};
// Re-export database types for external use
// Note: Use local type definitions but provide aliases for compatibility
pub use types::{
    BatchOperation,
    BatchOperationType,
    DatabaseStats,
    DbError,
    DbResult,
    EmbeddingData,
    EmbeddingDocument,
    EmbeddingMetadata,
    // Legacy embedding types for tests
    Note,
    QueryResult,
    Record,
    RecordId,
    SearchFilters,
    SearchQuery,
    SelectQuery,
    SurrealDbConfig,
    TableSchema,
};

// Re-export embedding functionality from crucible-enrichment
pub use crucible_enrichment::{
};
#[cfg(feature = "embeddings")]

// Kiln scanner exports (requires embeddings feature)
#[cfg(feature = "embeddings")]
#[cfg(feature = "embeddings")]
pub use kiln_scanner::{
    create_kiln_scanner, create_kiln_scanner_with_embeddings, parse_file_to_document,
    validate_kiln_scanner_config, ChangeDetectionMethod, ChangeDetectionSummary, ErrorHandlingMode,
    KilnFileInfo, KilnProcessError, KilnProcessResult, KilnScanError, KilnScanResult, KilnScanner,
    KilnScannerConfig, KilnScannerErrorType, KilnScannerMetrics, KilnScannerState,
};

// Kiln pipeline connector exports (requires embeddings feature)
#[cfg(feature = "embeddings")]
pub use kiln_pipeline_connector::{
    generate_document_id_from_path, get_parsed_documents_from_scan,
    transform_parsed_document_to_embedding_inputs, BatchProcessingResult, KilnPipelineConfig,
    KilnPipelineConnector, NoteProcessingResult,
};

// Simple integration exports (replaces complex QueueBasedProcessor)
#[cfg(feature = "embeddings")]
pub use simple_integration::{
    enqueue_document, enqueue_document_deletion, enqueue_documents, get_queue_status,
};

// Metrics exports
pub use metrics::{
    get_global_metrics, get_health_status, get_system_health, get_system_health_report,
    record_transaction_failure, record_transaction_success, update_queue_depth, HealthReport,
    HealthStatus, SystemMetrics, SystemMetricsSnapshot,
};

// Hash lookup exports
pub use hash_lookup::{
    check_file_needs_update, lookup_changed_files_since, lookup_file_hash,
    lookup_file_hashes_batch, lookup_file_hashes_batch_cached, lookup_files_by_content_hashes,
    BatchLookupConfig, CacheStats, HashLookupCache, HashLookupResult, StoredFileHash,
};

// Merkle tree persistence exports
pub use merkle_persistence::{
    HybridTreeRecord, MerklePersistence, SectionRecord, VirtualSectionRecord,
};

// Block storage functionality is integrated into ContentAddressedStorageSurrealDB
pub use content_addressed_storage::DocumentBlockRecord;

// Deduplication detection functionality
pub mod deduplication_detector;
pub use deduplication_detector::{DeduplicationDetector, SurrealDeduplicationDetector};

// Deduplication reporting functionality
pub mod deduplication_reporting;
pub use deduplication_reporting::{
    DeduplicationReport, DeduplicationReportGenerator, ExecutiveSummary, ExportFormat,
    ImplementationEffort, Recommendation, RecommendationPriority, ReportMetadata, ReportOptions,
};
