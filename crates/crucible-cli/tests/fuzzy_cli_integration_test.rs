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
    // 1. `cru fuzzy` should open interactive picker (default)
    // 2. `cru fuzzy --oneshot "query"` should run old one-shot mode

    // We can't easily test interactive mode in automated tests
    // Manual testing will verify this works
}

/// Test: CLI fuzzy command with --oneshot flag runs non-interactive search
#[tokio::test]
async fn test_fuzzy_command_oneshot_mode() {
    let kiln = create_test_kiln().unwrap();
    let config = create_test_config(&kiln.path()).unwrap();

    // Create a simple test by directly calling the fuzzy execute function
    // with oneshot mode enabled
    use crucible_cli::commands::fuzzy;

    // Run one-shot mode (old behavior)
    let result = fuzzy::execute(
        config,
        "note".to_string(),
        true,  // content
        true,  // tags
        true,  // paths
        10,    // limit
    )
    .await;

    // Should complete without error
    assert!(result.is_ok(), "oneshot mode should work: {:?}", result);
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

/// Test: Verify CLI structure has oneshot flag
#[test]
fn test_cli_has_oneshot_flag() {
    use clap::CommandFactory;
    use crucible_cli::cli::Cli;

    let cli = Cli::command();
    let fuzzy_cmd = cli
        .get_subcommands()
        .find(|cmd| cmd.get_name() == "fuzzy")
        .expect("fuzzy command should exist");

    // Verify oneshot flag exists
    let oneshot_arg = fuzzy_cmd
        .get_arguments()
        .find(|arg| arg.get_id() == "oneshot");

    assert!(
        oneshot_arg.is_some(),
        "fuzzy command should have --oneshot flag"
    );
}
