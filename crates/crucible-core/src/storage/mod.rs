//! Content-Addressed Storage Module
//!
//! This module provides trait abstractions and implementations for content-addressed storage
//! with Merkle tree support. It enables efficient change detection, incremental updates,
//! and cryptographic integrity verification for stored documents.
//!
//! ## Key Components
//!
//! - **ContentAddressedStorage**: Core trait for storage backends
//! - **ContentHasher**: Trait for pluggable hash algorithms
//! - **MerkleTree**: Binary Merkle tree for block-level integrity
//! - **HashedBlock**: Individual content blocks with cryptographic hashes
//!
//! ## Architecture
//!
//! The system follows a dependency inversion pattern where business logic depends on
//! trait abstractions rather than concrete implementations. This enables:
//! - Comprehensive unit testing with mock implementations
//! - Multiple storage backends (SurrealDB, in-memory, file-based)
//! - Pluggable hashing algorithms
//! - Clean separation of concerns

pub mod traits;
pub mod merkle;
pub mod block;
pub mod error;
pub mod builder;
pub mod diff;
pub mod change_application;
pub mod memory;
pub mod deduplicator;
pub mod deduplication_traits;

// Re-export main types for convenience
pub use traits::{ContentAddressedStorage, ContentHasher, StorageBackend};
pub use merkle::{MerkleTree, MerkleNode, TreeChange};
pub use block::{HashedBlock, BlockProcessor, BlockSize};
pub use error::{StorageError, StorageResult};
pub use builder::{ContentAddressedStorageBuilder, StorageBackendType, HasherConfig, ProcessingConfig};
pub use diff::{
    EnhancedChangeDetector, EnhancedTreeChange, ChangeMetadata, ChangeSource,
    DiffConfig, MovedBlockInfo, CacheStats as DiffCacheStats
};
pub use change_application::{
    ChangeApplicationSystem, ChangeApplicationResult, ApplicationConfig,
    AppliedChange, FailedChange, RollbackInfo, ApplicationStats, CacheStats as AppCacheStats
};
pub use memory::{
    MemoryStorage, MemoryStorageConfig, MemoryStorageBuilder, MemoryStorageSnapshot,
    StorageEvent, MemoryStorageShutdown
};
pub use deduplicator::{
    Deduplicator, DefaultDeduplicator, DeduplicationAnalysis, DuplicateGroup,
    DeduplicationStats, StorageSavings, BlockUsagePattern, UsageEvent, UsageEventType,
    DocumentDuplicationPattern, DocumentSimilarity, StorageSavingsByType
};
pub use deduplication_traits::{
    DeduplicationStorage, DeduplicationCapable, BlockInfo, DuplicateBlockInfo,
    StorageUsageStats
};