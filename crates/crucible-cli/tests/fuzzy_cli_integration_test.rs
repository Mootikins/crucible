//! CLI Integration Tests for Interactive Fuzzy Search
//!
//! These tests verify that the interactive fuzzy search integrates properly with the CLI.

mod common;

use anyhow::Result;
use crucible_core::test_support::{create_kiln_with_files, kiln_path_str};
use crucible_cli::config::CliConfig;
use std::path::Path;

/// Helper to create a test kiln for CLI integration tests
fn create_test_kiln() -> Result<tempfile::TempDir> {
    create_kiln_with_files(&[
        ("note1.md", "# Note 1\n\nThis is about Rust programming."),
        ("note2.md", "# Note 2\n\nAnother note about algorithms."),
        ("todo.md", "# TODO\n\n- Task 1\n- Task 2"),
    ])
}

/// Helper to create test config for CLI
fn create_test_config(kiln_path: &Path) -> Result<CliConfig> {
    CliConfig::builder()
        .kiln_path(kiln_path)
        .embedding_url("mock")
        .embedding_model("mock-test-model")
        .build()
}

// ============================================================================
// Phase 8: CLI Integration Tests
// ============================================================================

/// Test: CLI fuzzy command defaults to interactive mode
/// This is a placeholder test - interactive mode requires terminal which we can't test easily
#[tokio::test]
#[ignore] // Ignored because interactive mode requires a real terminal
async fn test_fuzzy_command_interactive_mode() {
    let kiln = create_test_kiln().unwrap();
    let kiln_path = kiln_path_str(kiln.path());

    // In a real scenario, this would open an interactive picker
    // For now, we just verify the command structure exists

    // This test serves as documentation that:
    // `cru fuzzy` opens interactive picker with Ctrl+M mode toggle

    // We can't easily test interactive mode in automated tests
    // Manual testing will verify this works
}

/// Test: fuzzy_interactive module can be called directly
#[tokio::test]
async fn test_fuzzy_interactive_module_callable() {
    use crucible_cli::commands::fuzzy_interactive;

    let kiln = create_test_kiln().unwrap();
    let config = create_test_config(&kiln.path()).unwrap();

    // Verify the interactive module exists and is callable
    // Note: This won't actually show a picker in tests, but verifies the API
    let result = fuzzy_interactive::execute(
        config,
        "note".to_string(),
        10,
    )
    .await;

    // Currently this will just list files and exit
    // When we integrate nucleo-picker, it will open interactive UI
    assert!(result.is_ok(), "fuzzy_interactive::execute should be callable");
}
