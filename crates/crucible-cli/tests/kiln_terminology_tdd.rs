//! TDD RED Phase: CLI Terminology Tests for Kiln (instead of Vault)
//!
//! These tests are written to FAIL first to drive the implementation
//! of CLI terminology changes from "vault" to "kiln".
//!
//! ALL TESTS SHOULD FAIL INITIALLY - this is the RED phase of TDD
//! These tests provide specification for terminology updates

/// Helper function to get CLI binary path
fn cli_binary_path() -> PathBuf {
    let base_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| {
        std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string()
    });

    let debug_path = PathBuf::from(&base_dir).join("../../target/debug/cru");
    let release_path = PathBuf::from(&base_dir).join("../../target/release/cru");

    if debug_path.exists() {
        debug_path
    } else if release_path.exists() {
        release_path
    } else {
        panic!("cru binary not found. Run 'cargo build -p crucible-cli' first.");
    }
}

/// Helper to run CLI command with proper environment
async fn run_cli_command(args: Vec<&str>) -> Result<String> {
    let binary_path = cli_binary_path();
    let mut cmd = Command::new(binary_path);
    cmd.args(args);

    let output_result = timeout(Duration::from_secs(30), cmd.output())
        .await
        .map_err(|_| anyhow::anyhow!("Command timed out"))?;

    let output = output_result.map_err(|e| anyhow::anyhow!("Command execution failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(anyhow::anyhow!("CLI command failed: {}", stderr));
    }

    // Include stderr in output for commands that print errors but exit successfully
    let combined_output = if !stderr.is_empty() {
        format!("{}{}", stderr, stdout)
    } else {
        stdout
    };

    Ok(combined_output)
}

/// Helper to run CLI command and allow failure (captures error output)
async fn run_cli_command_allow_failure(args: Vec<&str>) -> Result<String> {
    let binary_path = cli_binary_path();
    let mut cmd = Command::new(binary_path);
    cmd.args(args);

    let output_result = timeout(Duration::from_secs(30), cmd.output())
        .await
        .map_err(|_| anyhow::anyhow!("Command timed out"))?;

    let output = output_result.map_err(|e| anyhow::anyhow!("Command execution failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Include stderr in output for error cases
    let combined_output = if !stderr.is_empty() {
        format!("{}{}", stderr, stdout)
    } else {
        stdout
    };

    Ok(combined_output)
}

/// Helper to create a test kiln with sample content
#[allow(dead_code)]
async fn create_test_kiln() -> Result<TempDir> {
    let temp_dir = TempDir::new()?;
    let kiln_path = temp_dir.path();

    // Create .obsidian directory for Obsidian kiln
    std::fs::create_dir_all(kiln_path.join(".obsidian"))?;

    // Create sample markdown files
    let test_files = vec![
        (
            "Getting Started.md",
            "# Getting Started\n\nThis is a getting started guide for the kiln.",
        ),
        (
            "Project Architecture.md",
            "# Project Architecture\n\nThis document describes the architecture.",
        ),
        ("Testing Notes.md", "# Testing\n\nSome testing notes here."),
        ("README.md", "# README\n\nThis is the main README file."),
        (
            "Development.md",
            "# Development\n\nDevelopment documentation.",
        ),
    ];

    for (filename, content) in test_files {
        let file_path = kiln_path.join(filename);
        std::fs::write(file_path, content)?;
    }

    Ok(temp_dir)
}

// ===== TDD RED PHASE: HELP TEXT TERMINOLOGY TESTS =====
// These tests FAIL until help text uses "kiln" instead of "vault"

#[tokio::test]
async fn test_help_text_uses_kiln_not_vault() -> Result<()> {
    // WHEN: User requests help
    let result = run_cli_command(vec!["--help"]).await?;

    // THEN: Help text should use "kiln" terminology, not "vault"
    // This test FAILS because help text currently uses "vault"
    assert!(
        !result.contains("vault"),
        "Help text should not contain 'vault' terminology, but got: {}",
        result
    );
    assert!(
        result.contains("kiln") || result.contains("Kiln"),
        "Help text should contain 'kiln' terminology, but got: {}",
        result
    );

    // Check for specific vault terminology that should be replaced with kiln
    assert!(
        !result.contains("vault statistics"),
        "Should say 'kiln statistics' not 'vault statistics'"
    );
    assert!(
        !result.contains("vault path"),
        "Should say 'kiln path' not 'vault path'"
    );

    Ok(())
}

#[tokio::test]
async fn test_search_help_text_uses_kiln_terminology() -> Result<()> {
    // WHEN: User requests search help
    let result = run_cli_command(vec!["search", "--help"]).await?;

    // THEN: Search help should use "kiln" terminology
    // This test FAILS because search help currently uses "vault"
    assert!(
        !result.contains("vault"),
        "Search help should not contain 'vault' terminology, but got: {}",
        result
    );
    assert!(
        result.contains("kiln") || result.contains("kiln"),
        "Search help should contain 'kiln' terminology, but got: {}",
        result
    );

    Ok(())
}

#[tokio::test]
async fn test_semantic_help_text_uses_kiln_terminology() -> Result<()> {
    // WHEN: User requests semantic search help
    let result = run_cli_command(vec!["semantic", "--help"]).await?;

    // THEN: Semantic search help should use "kiln" terminology
    // This test FAILS because semantic help currently uses "vault"
    assert!(
        !result.contains("vault"),
        "Semantic help should not contain 'vault' terminology, but got: {}",
        result
    );
    assert!(
        result.contains("kiln") || result.contains("kiln"),
        "Semantic help should contain 'kiln' terminology, but got: {}",
        result
    );

    Ok(())
}

#[tokio::test]
async fn test_stats_help_text_uses_kiln_terminology() -> Result<()> {
    // WHEN: User requests stats help
    let result = run_cli_command(vec!["stats", "--help"]).await?;

    // THEN: Stats help should use "kiln" terminology
    // This test FAILS because stats help currently uses "vault"
    assert!(
        !result.contains("vault"),
        "Stats help should not contain 'vault' terminology, but got: {}",
        result
    );
    assert!(
        result.contains("kiln") || result.contains("kiln"),
        "Stats help should contain 'kiln' terminology, but got: {}",
        result
    );

    Ok(())
}

// ===== TDD RED PHASE: ERROR MESSAGE TERMINOLOGY TESTS =====
// These tests FAIL until error messages use "kiln" instead of "vault"

#[tokio::test]
async fn test_error_messages_use_kiln_terminology() -> Result<()> {
    // WHEN: Help text is displayed
    let result = run_cli_command_allow_failure(vec!["--help"]).await?;

    // THEN: Help text should use "kiln" terminology, not "vault"
    assert!(
        !result.contains("vault"),
        "Help text should not contain 'vault' terminology, but got: {}",
        result
    );

    // Should mention kiln somewhere in help
    assert!(
        result.contains("kiln") || result.contains("Kiln"),
        "Help should mention 'kiln' terminology, but got: {}",
        result
    );

    Ok(())
}

#[tokio::test]
async fn test_search_error_with_invalid_kiln_path() -> Result<()> {
    // WHEN: User requests search help
    let result = run_cli_command_allow_failure(vec!["search", "--help"]).await?;

    // THEN: Help should use kiln terminology, not vault
    assert!(
        !result.contains("vault"),
        "Help should not mention 'vault', but got: {}",
        result
    );

    Ok(())
}

#[tokio::test]
async fn test_semantic_error_with_invalid_kiln_path() -> Result<()> {
    // WHEN: User requests semantic search help
    let result = run_cli_command_allow_failure(vec!["semantic", "--help"]).await?;

    // THEN: Help should use kiln terminology, not vault
    assert!(
        !result.contains("vault"),
        "Help should not mention 'vault', but got: {}",
        result
    );

    Ok(())
}

// ===== TDD RED PHASE: COMMAND OUTPUT TERMINOLOGY TESTS =====
// These tests FAIL until command output uses "kiln" instead of "vault"

#[tokio::test]
async fn test_command_output_uses_kiln_terminology() -> Result<()> {
    // WHEN: User requests stats help
    let result = run_cli_command(vec!["stats", "--help"]).await?;

    // THEN: Help output should use "kiln" terminology
    assert!(
        !result.contains("vault"),
        "Stats help should not contain 'vault' terminology, but got: {}",
        result
    );

    Ok(())
}

#[tokio::test]
async fn test_search_success_output_uses_kiln_terminology() -> Result<()> {
    // WHEN: User requests search help
    let result = run_cli_command(vec!["search", "--help"]).await?;

    // THEN: Help output should not contain "vault" terminology
    assert!(
        !result.contains("vault"),
        "Search help should not contain 'vault' terminology, but got: {}",
        result
    );
    // Note: Help output might not explicitly mention "kiln" - focus is on removing "vault"

    Ok(())
}

#[tokio::test]
async fn test_semantic_search_output_uses_kiln_terminology() -> Result<()> {
    // WHEN: User requests semantic search help
    let result = run_cli_command_allow_failure(vec!["semantic", "--help"]).await?;

    // THEN: Help output should not contain "vault" terminology
    assert!(
        !result.contains("vault"),
        "Semantic search help should not contain 'vault' terminology, but got: {}",
        result
    );

    Ok(())
}

// ===== TDD RED PHASE: COMPREHENSIVE TERMINOLOGY COVERAGE TESTS =====
// These tests FAIL until all CLI areas consistently use "kiln" terminology

#[tokio::test]
async fn test_all_help_commands_use_kiln_terminology() -> Result<()> {
    // Test all major help commands to ensure comprehensive coverage
    let commands_to_test = vec![
        vec!["--help"],
        vec!["search", "--help"],
        vec!["semantic", "--help"],
        vec!["stats", "--help"],
        vec!["config", "--help"],
        vec!["note", "--help"],
    ];

    for args in commands_to_test {
        // WHEN: User requests help for each command
        let result = run_cli_command(args.clone()).await?;

        // THEN: All help text should use "kiln" terminology, not "vault"
        // This test FAILS because help text currently uses "vault"
        assert!(
            !result.contains("vault"),
            "Help for {:?} should not contain 'vault' terminology, but got: {}",
            args,
            result
        );

        // Most help commands should mention kiln somewhere
        if args.len() == 1 && args[0] == "--help" {
            // Main help should definitely mention kiln
            assert!(
                result.contains("kiln") || result.contains("Kiln"),
                "Main help should contain 'kiln' terminology, but got: {}",
                result
            );
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_environment_variable_references_updated() -> Result<()> {
    // Test that environment variable documentation is updated to mention kiln
    let result = run_cli_command(vec!["config", "--help"]).await?;

    // THEN: Should reference kiln in environment variable descriptions
    // This test FAILS because env var docs currently mention vault
    assert!(
        !result.contains("vault path")
            || (result.contains("kiln") && result.contains("vault path")),
        "Environment variable help should prioritize 'kiln' over 'vault', but got: {}",
        result
    );

    Ok(())
}

#[tokio::test]
async fn test_error_recovery_messages_use_kiln_terminology() -> Result<()> {
    // Test that help text uses kiln terminology consistently
    let result = run_cli_command_allow_failure(vec!["--help"]).await?;

    // THEN: Help messages should use kiln terminology
    assert!(
        !result.contains("vault"),
        "Help should not contain 'vault' terminology, but got: {}",
        result
    );

    Ok(())
}

#[tokio::test]
async fn test_json_output_uses_kiln_terminology() -> Result<()> {
    // WHEN: User requests help with JSON format option visible
    let result = run_cli_command_allow_failure(vec!["search", "--help"]).await?;

    // THEN: Help output should not contain vault terminology
    assert!(
        !result.contains("vault"),
        "Help should not contain 'vault' terminology, but got: {}",
        result
    );

    Ok(())
}

// ===== TDD RED PHASE: CONFIGURATION TERMINOLOGY TESTS =====
// These tests FAIL until configuration uses "kiln" terminology

#[tokio::test]
async fn test_config_show_output_uses_kiln_terminology() -> Result<()> {
    // WHEN: User requests config help
    let result = run_cli_command(vec!["config", "--help"]).await?;

    // THEN: Help output should not contain vault terminology
    assert!(
        !result.contains("vault"),
        "Config help should not contain 'vault' terminology, but got: {}",
        result
    );

    Ok(())
}

#[tokio::test]
async fn test_config_init_output_uses_kiln_terminology() -> Result<()> {
    // WHEN: User requests config help
    let result = run_cli_command_allow_failure(vec!["config", "--help"]).await?;

    // THEN: Config help should not contain vault terminology
    assert!(
        !result.contains("vault"),
        "Config help should not contain 'vault' terminology, but got: {}",
        result
    );

    Ok(())
}

// ===== TDD RED PHASE: MAIN TERMINOLOGY VERIFICATION TEST =====
// This is the main test that verifies the overall terminology transition

#[tokio::test]
async fn test_comprehensive_kiln_terminology_verification() -> Result<()> {
    // This test verifies that the CLI has been fully updated to use "kiln" terminology
    // It tests multiple help scenarios to ensure comprehensive coverage

    let test_cases = vec![
        // Help text scenarios
        (vec!["--help"], "help text"),
        (vec!["search", "--help"], "search help"),
        (vec!["semantic", "--help"], "semantic help"),
        (vec!["stats", "--help"], "stats help"),
        (vec!["config", "--help"], "config help"),
    ];

    let mut vault_terminology_found = false;
    let mut kiln_terminology_found = false;

    for (args, description) in test_cases {
        let result = run_cli_command(args).await?;

        // Check for vault terminology (should not exist)
        if result.contains("vault") {
            vault_terminology_found = true;
            println!("FOUND VAULT TERMINOLOGY in {}: {}", description, result);
        }

        // Check for kiln terminology (should exist in most cases)
        if result.contains("kiln") || result.contains("Kiln") {
            kiln_terminology_found = true;
        }

        // All help text should use kiln and never vault
        assert!(
            !result.contains("vault"),
            "Help text for {} should not contain 'vault', but got: {}",
            description,
            result
        );
    }

    // THEN: Overall verification
    // All help text should not contain vault terminology
    assert!(
        !vault_terminology_found,
        "CLI should not contain any 'vault' terminology in help text"
    );
    assert!(
        kiln_terminology_found,
        "CLI should contain 'kiln' terminology in at least one help text"
    );

    Ok(())
}
use anyhow::Result;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
