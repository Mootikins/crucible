//! Integration test module for Obsidian client
//!
//! This module contains integration tests for the Obsidian HTTP client,
//! testing against a mock server to validate API interactions.

pub mod e2e_workflow_test;
pub mod error_scenarios_test;
pub mod http_client_test;
pub mod mock_server;
pub mod obsidian_integration_test;
pub mod test_data;

use std::path::PathBuf;
use tempfile::TempDir;

/// Common test setup - creates a temporary directory for test data
pub fn setup_test_env() -> TempDir {
    tempfile::tempdir().expect("Failed to create temp directory")
}

/// Helper to create a test database path
pub fn test_db_path(temp_dir: &TempDir) -> PathBuf {
    temp_dir.path().join("test.db")
}

/// Clean up test resources
pub fn cleanup() {
    // Additional cleanup if needed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup() {
        let temp_dir = setup_test_env();
        assert!(temp_dir.path().exists());
    }
}
