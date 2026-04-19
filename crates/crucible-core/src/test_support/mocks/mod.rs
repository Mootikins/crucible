//! Mock Implementations for Testing
//!
//! This module provides comprehensive mock implementations of core traits for testing purposes.
//! These mocks are designed to be:
//!
//! - **Deterministic**: Always produce the same results for the same inputs
//! - **Fast**: In-memory operations with no I/O overhead
//! - **Configurable**: Support error injection and custom behaviors
//! - **Observable**: Track all operations for test assertions
//! - **Isolated**: No external dependencies or side effects
//!
//! # Design Principles
//!
//! - **Simplicity**: Straightforward implementations without production complexity
//! - **Predictability**: Deterministic behavior for reliable test results
//! - **Observability**: Call tracking for verifying test expectations
//! - **Error Testing**: Support for simulating various error conditions
//!
//! # Examples
//!
//! ## Mock Hashing Algorithm
//!
//! ```rust
//! use crucible_core::test_support::mocks::MockHashingAlgorithm;
//! use crucible_core::hashing::algorithm::HashingAlgorithm;
//!
//! let hasher = MockHashingAlgorithm::new();
//! let hash = hasher.hash(b"test data");
//!
//! // Mock hasher produces deterministic, simple hashes
//! assert_eq!(hash.len(), 32);
//! assert_eq!(hasher.algorithm_name(), "MockHash");
//! ```
//!
//! ## Mock Storage
//!
//! ```rust
//! use crucible_core::test_support::mocks::MockStorage;
//!
//! let storage = MockStorage::new();
//!
//! // Access statistics
//! let stats = storage.stats();
//! assert_eq!(stats.store_count, 0);
//!
//! // Configure error simulation
//! storage.set_simulate_errors(true, "Storage full");
//! ```
//!
//! ## Mock Content Hasher
//!
//! ```rust
//! use crucible_core::test_support::mocks::MockContentHasher;
//! use crucible_core::traits::change_detection::ContentHasher;
//! use std::path::Path;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let hasher = MockContentHasher::new();
//!
//! // Configure to return specific hash for a path
//! hasher.set_file_hash("test.md", vec![1u8; 32]);
//!
//! // Hash file operations use configured values
//! let hash = hasher.hash_file(Path::new("test.md")).await?;
//! assert_eq!(hash.as_bytes().len(), 32);
//! # Ok(())
//! # }
//! ```

mod change_detector;
mod completion;
mod content_hasher;
mod enrichment;
mod event_emitter;
mod hash_lookup;
mod hashing;
mod storage;

pub use change_detector::MockChangeDetector;
pub use completion::MockCompletionBackend;
pub use content_hasher::MockContentHasher;
pub use enrichment::MockEnrichmentService;
pub use event_emitter::{MockEmitterBehavior, MockEventEmitter, MockEventEmitterStats};
pub use hash_lookup::MockHashLookupStorage;
pub use hashing::MockHashingAlgorithm;
pub use storage::{MockStorage, MockStorageStats};
