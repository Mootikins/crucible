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

pub mod block;
pub mod builder;
pub mod change_application;
pub mod deduplication_traits;
pub mod deduplicator;
pub mod diff;
pub mod error;
pub mod factory;
pub mod memory;
pub mod merkle;
pub mod traits;

// Re-export main types for convenience
pub use block::{BlockProcessor, BlockSize, HashedBlock};
pub use builder::{
    ContentAddressedStorageBuilder, HasherConfig, ProcessingConfig, StorageBackendType,
};
pub use change_application::{
    ApplicationConfig, ApplicationStats, AppliedChange, CacheStats as AppCacheStats,
    ChangeApplicationResult, ChangeApplicationSystem, FailedChange, RollbackInfo,
};
pub use deduplication_traits::{
    BlockInfo, DeduplicationCapable, DeduplicationStorage, DuplicateBlockInfo, StorageUsageStats,
};
pub use deduplicator::{
    BlockUsagePattern, DeduplicationAnalysis, DeduplicationStats, Deduplicator,
    DefaultDeduplicator, DocumentDuplicationPattern, DocumentSimilarity, DuplicateGroup,
    StorageSavings, StorageSavingsByType, UsageEvent, UsageEventType,
};
pub use diff::{
    CacheStats as DiffCacheStats, ChangeMetadata, ChangeSource, DiffConfig, EnhancedChangeDetector,
    EnhancedTreeChange, MovedBlockInfo,
};
pub use error::{StorageError, StorageResult};
pub use factory::{BackendConfig, HashAlgorithm, StorageConfig, StorageFactory};
pub use memory::{
    MemoryStorage, MemoryStorageBuilder, MemoryStorageConfig, MemoryStorageShutdown,
    MemoryStorageSnapshot, StorageEvent,
};
pub use merkle::{MerkleNode, MerkleTree, TreeChange};
pub use traits::{ContentAddressedStorage, ContentHasher, StorageBackend};
