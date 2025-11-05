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
pub mod types;

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
};

// Kiln pipeline connector exports
pub use kiln_pipeline_connector::{
    generate_document_id_from_path, get_parsed_documents_from_scan,
    transform_parsed_document_to_embedding_inputs, BatchProcessingResult, DocumentProcessingResult,
    KilnPipelineConfig, KilnPipelineConnector,
};
