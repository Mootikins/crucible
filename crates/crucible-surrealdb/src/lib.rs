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

pub mod database;
pub mod multi_client;
pub mod query;
pub mod schema_types;
pub mod types;
pub mod vault_integration;
pub mod vault_pipeline_connector;
pub mod vault_processor;
pub mod vault_scanner;

// Embedding modules
pub mod embedding;
pub mod embedding_config;
pub mod embedding_pipeline;
pub mod embedding_pool;

pub use database::SurrealEmbeddingDatabase;
pub use multi_client::SurrealClient;
pub use schema_types::*;
pub use types::*;

// Re-export embedding functionality with specific exports to avoid conflicts
pub use embedding_config::{
    DocumentEmbedding, EmbeddingConfig, EmbeddingError, EmbeddingModel, EmbeddingProcessingResult,
    PrivacyMode, ThreadPoolMetrics,
};
pub use embedding_pipeline::EmbeddingPipeline;
pub use embedding_pool::EmbeddingThreadPool;

// Vault scanner exports
pub use vault_processor::{
    process_document_embeddings, process_incremental_changes, process_vault_files,
    process_vault_files_with_error_handling, scan_vault_directory,
};
pub use vault_scanner::{
    create_vault_scanner, create_vault_scanner_with_embeddings, parse_file_to_document,
    validate_vault_scanner_config, ChangeDetectionMethod, ErrorHandlingMode, VaultFileInfo,
    VaultProcessError, VaultProcessResult, VaultScanError, VaultScanResult, VaultScanner,
    VaultScannerConfig, VaultScannerErrorType, VaultScannerMetrics, VaultScannerState,
};

// Vault pipeline connector exports
pub use vault_pipeline_connector::{
    generate_document_id_from_path, get_parsed_documents_from_scan,
    transform_parsed_document_to_embedding_inputs, BatchProcessingResult, DocumentProcessingResult,
    VaultPipelineConfig, VaultPipelineConnector,
};
