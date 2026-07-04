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
//! ## Mock Storage
//!
//! ```rust
//! use crate::test_support::mocks::MockStorage;
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

mod completion;
mod event_emitter;
mod storage;

pub use completion::MockCompletionBackend;
pub use event_emitter::{MockEmitterBehavior, MockEventEmitter, MockEventEmitterStats};
pub use storage::{MockStorage, MockStorageStats};
