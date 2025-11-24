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
//! ## Architecture - SOLID Principles (Phase 5)
//!
//! This crate follows the Dependency Inversion Principle:
//! - Concrete types (SurrealClient, EAVGraphStore, MerklePersistence, etc.) are PRIVATE
//! - Public API provides trait objects and factory functions via the `adapters` module
//! - CLI code depends on abstractions, not implementations
//!
//! ## Usage
//!
//! ```rust,no_run
//! use crucible_surrealdb::adapters;
//! use crucible_surrealdb::SurrealDbConfig;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = SurrealDbConfig {
//!         path: "./cache.db".to_string(),
//!         namespace: "crucible".to_string(),
//!         database: "kiln".to_string(),
//!         max_connections: Some(10),
//!         timeout_seconds: Some(30),
//!     };
//!
//!     // Use factory functions from adapters module
//!     let client = adapters::create_surreal_client(config).await?;
//!     let merkle_store = adapters::create_merkle_store(client.clone());
//!
//!     Ok(())
//! }
//! ```

use crucible_enrichment;

// ============================================================================
// SOLID ARCHITECTURE: Phase 5 - Private Infrastructure Types
// ============================================================================
// Concrete implementations should NOT be imported directly.
// Use factory functions from the `adapters` module instead.

// Public adapters module - provides factory functions for creating trait objects
pub mod adapters;

// Public configuration and data types
pub mod types;
pub use types::{
    BatchOperation,
    BatchOperationType,
    DatabaseStats,
    DbError,
    DbResult,
    EmbeddingData,
    EmbeddingDocument,
    EmbeddingMetadata,
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

// Public observability (metrics)
pub mod metrics;
pub use metrics::{
    get_global_metrics, get_health_status, get_system_health, get_system_health_report,
    record_transaction_failure, record_transaction_success, update_queue_depth, HealthReport,
    HealthStatus, SystemMetrics, SystemMetricsSnapshot,
};

// Public utilities
pub mod hash_lookup;
pub use hash_lookup::{
    check_file_needs_update, lookup_changed_files_since, lookup_file_hash,
    lookup_file_hashes_batch, lookup_file_hashes_batch_cached, lookup_files_by_content_hashes,
    BatchLookupConfig, CacheStats, HashLookupCache, HashLookupResult, StoredFileHash,
};

// Public deduplication functionality
pub mod deduplication_detector;
pub use deduplication_detector::{DeduplicationDetector, SurrealDeduplicationDetector};

pub mod deduplication_reporting;
pub use deduplication_reporting::{
    DeduplicationReport, DeduplicationReportGenerator, ExecutiveSummary, ExportFormat,
    ImplementationEffort, Recommendation, RecommendationPriority, ReportMetadata, ReportOptions,
};

// Public schema types
pub mod schema_types;
pub use schema_types::*;

// Public database and storage APIs (high-level interfaces)
pub mod database;
pub use database::SurrealEmbeddingDatabase;

pub mod content_addressed_storage;
pub use content_addressed_storage::{ContentAddressedStorageSurrealDB, DocumentBlockRecord};

pub mod kiln_store;
pub use kiln_store::{InMemoryKilnStore, KilnStore};

// Public transaction queue types (data/config types, not implementations)
pub mod transaction_queue;
pub use transaction_queue::{
    DatabaseTransaction, QueueError, QueueStats, ResultReceiver, ResultSender, StatsWatcher,
    TransactionQueueConfig, TransactionReceiver, TransactionResult, TransactionSender,
    TransactionTimestamp,
};

#[cfg(feature = "embeddings")]
pub use transaction_consumer::{
    ConsumerConfig, ConsumerStats, DatabaseTransactionConsumer, ShutdownReceiver, ShutdownSender,
};

// Public kiln integration utilities (these are utility functions, not types)
#[cfg(feature = "embeddings")]
pub mod kiln_integration;

// ============================================================================
// PRIVATE infrastructure modules - use factory functions instead!
// ============================================================================

pub(crate) mod surreal_client;
pub(crate) mod eav_graph;
pub(crate) mod merkle_persistence;
pub(crate) mod change_detection_store;
pub(crate) mod batch_aware_client;
pub(crate) mod consistency;
pub(crate) mod migration;
pub(crate) mod query;
pub(crate) mod utils;

#[cfg(feature = "embeddings")]
pub(crate) mod transaction_consumer;
#[cfg(feature = "embeddings")]
pub(crate) mod embedding;

// Internal re-exports for use within this crate only
pub(crate) use surreal_client::SurrealClient;
pub(crate) use eav_graph::{EAVGraphStore, NoteIngestor};
pub(crate) use merkle_persistence::MerklePersistence;
