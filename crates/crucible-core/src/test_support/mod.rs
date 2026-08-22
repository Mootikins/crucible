//! Shared testing utilities for Crucible components.
//!
//! This module provides comprehensive testing infrastructure including:
//!
//! - **Temporary Kilns**: Helper functions for creating test file structures
//! - **Mock Implementations**: Deterministic, observable mocks for all core traits
//! - **Test Utilities**: Common helpers for test setup and teardown
//!
//! ## Modules
//!
//! - [`mocks`]: Mock implementations of traits (storage, completion, event emitter)

pub mod env_guard;
pub mod fixtures;
pub mod hermetic_env;
pub mod local_env;
pub mod mocks;

// Re-export common fixture types and utilities for convenience
pub use env_guard::EnvVarGuard;
pub use fixtures::{create_basic_kiln, create_kiln, create_kiln_with_files, KilnFixture};
pub use hermetic_env::hermetic_env_pairs;
pub use local_env::{require_test_env, test_env};

use std::path::Path;

/// Convenience helper to convert a kiln path into a string for configuration.
pub fn kiln_path_str(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

/// Returns a cross-platform path that doesn't exist.
///
/// Use this for tests that verify error handling for missing files/directories.
/// The path is constructed using `std::env::temp_dir()` which works on all platforms.
///
/// # Example
///
/// ```rust
/// use crucible_core::test_support::nonexistent_path;
///
/// let path = nonexistent_path("missing_note.md");
/// assert!(!path.exists());
/// ```
pub fn nonexistent_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("crucible_nonexistent_{}", name))
}
