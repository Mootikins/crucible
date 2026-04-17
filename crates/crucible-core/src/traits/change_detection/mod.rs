//! Traits for content hashing and change detection
//!
//! This module defines the core abstractions for content hashing and change detection
//! throughout the Crucible system. These traits enable dependency inversion by allowing
//! higher-level modules to depend on abstractions rather than concrete implementations.
//!
//! ## Architecture
//!
//! The traits are designed to support the file system operations refactoring:
//! - `ContentHasher`: Pure hashing operations for files and content blocks
//! - `HashLookupStorage`: Database operations for storing and retrieving hashes
//! - `ChangeDetector`: High-level change detection logic
//!
//! ## Usage Pattern
//!

mod change_detector;
mod content_hasher;
mod hash_lookup;

#[cfg(test)]
mod tests;

pub use change_detector::{
    ChangeDetectionMetrics, ChangeDetectionResult, ChangeDetector, ChangeSet, ChangeStatistics,
    ChangeSummary,
};
pub use content_hasher::ContentHasher;
pub use hash_lookup::{
    BatchLookupConfig, CacheEntry, CacheStats, HashLookupCache, HashLookupResult,
    HashLookupStorage, HashLookupSummary, StoredHash,
};
