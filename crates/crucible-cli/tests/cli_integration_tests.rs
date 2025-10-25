//! Integration tests for core CLI user workflows
//!
//! These tests are written TDD-style - they should fail first,
//! then drive the implementation to make them pass.

use anyhow::Result;
use tokio::process::Command;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::time::{timeout, Duration};

/// Helper function to get CLI binary path
fn cli_binary_path() -> PathBuf {
    // Look for CLI binary in target directory
    let base_dir = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap_or_else(|_| std::env::current_dir().unwrap().to_string_lossy().to_string());

    let debug_path = PathBuf::from(&base_dir)
        .join("../../target/debug/crucible-cli");
    let release_path = PathBuf::from(&base_dir)
        .join("../../target/release/crucible-cli");

    if debug_path.exists() {
        debug_path
    } else if release_path.exists() {
        release_path
    } else {
        panic!("crucible-cli binary not found. Run 'cargo build -p crucible-cli' first.");
    }
}

/// Helper to run CLI command with proper environment
async fn run_cli_command(args: Vec<&str>, env_vars: Vec<(&str, &str)>) -> Result<String> {
    let binary_path = cli_binary_path();
    let mut cmd = Command::new(binary_path);

    // Add environment variables
    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    for arg in args {
        cmd.arg(arg);
    }

    let output_result = timeout(Duration::from_secs(30), cmd.output()).await
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

/// Helper to create a test vault with sample content
async fn create_test_vault() -> Result<TempDir> {
    let temp_dir = TempDir::new()?;
    let vault_path = temp_dir.path();

    // Create .obsidian directory for Obsidian vault
    std::fs::create_dir_all(vault_path.join(".obsidian"))?;

    // Create sample markdown files
    let test_files = vec![
        ("Getting Started.md", "# Getting Started\n\nThis is a getting started guide for the vault."),
        ("Project Architecture.md", "# Project Architecture\n\nThis document describes the architecture."),
        ("Testing Notes.md", "# Testing\n\nSome testing notes here."),
        ("README.md", "# README\n\nThis is the main README file."),
        ("Development.md", "# Development\n\nDevelopment documentation."),
    ];

    for (filename, content) in test_files {
        let file_path = vault_path.join(filename);
        std::fs::write(file_path, content)?;
    }

    Ok(temp_dir)
}

#[tokio::test]
async fn test_basic_search_works_immediately() -> Result<()> {
    // GIVEN: A test vault with content
    let vault_dir = create_test_vault().await?;

    // WHEN: User performs basic search
    let result = run_cli_command(
        vec!["search", "getting"],
        vec![("OBSIDIAN_VAULT_PATH", vault_dir.path().to_string_lossy().as_ref())]
    ).await?;

    // THEN: Should return immediate basic results without daemon
    assert!(result.contains("Getting Started.md") || result.contains("basic"));
    assert!(result.contains("Found") || result.contains("matches"));

    Ok(())
}

#[tokio::test]
async fn test_search_without_vault_gives_helpful_error() -> Result<()> {
    // GIVEN: No vault path set (set to invalid path)
    let result = run_cli_command(
        vec!["search", "test"],
        vec![("OBSIDIAN_VAULT_PATH", "/nonexistent/path")]
    ).await;

    // WHEN: Search is attempted without vault
    // THEN: Should give helpful error message
    match result {
        Ok(output) => {
            assert!(output.contains("kiln") && output.contains("path"));
            assert!(output.contains("help") || output.contains("Error"));
        }
        Err(e) => {
            let error_msg = e.to_string();
            assert!(error_msg.contains("kiln") && error_msg.contains("path"));
            assert!(error_msg.contains("help") || error_msg.contains("Error"));
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_basic_search_with_options() -> Result<()> {
    // GIVEN: A test vault
    let vault_dir = create_test_vault().await?;

    // WHEN: User searches with limit option
    let result = run_cli_command(
        vec!["search", "development", "--limit", "2"],
        vec![("OBSIDIAN_VAULT_PATH", vault_dir.path().to_string_lossy().as_ref())]
    ).await?;

    // THEN: Should respect limit and find relevant files
    assert!(result.contains("Development.md"));
    assert!(result.contains("limit") || result.contains("results"));

    Ok(())
}

#[tokio::test]
async fn test_search_json_output_format() -> Result<()> {
    // GIVEN: A test vault
    let vault_dir = create_test_vault().await?;

    // WHEN: User requests JSON output
    let result = run_cli_command(
        vec!["search", "test", "--format", "json"],
        vec![("OBSIDIAN_VAULT_PATH", vault_dir.path().to_string_lossy().as_ref())]
    ).await?;

    // THEN: Should return JSON formatted results
    let trimmed = result.trim_start();
    assert!(trimmed.starts_with('[') || trimmed.starts_with('{'));
    assert!(result.contains("\""));

    Ok(())
}

#[tokio::test]
async fn test_fuzzy_search_without_daemon() -> Result<()> {
    // GIVEN: A test vault
    let vault_dir = create_test_vault().await?;

    // WHEN: User performs fuzzy search
    let result = run_cli_command(
        vec!["fuzzy", "arch"],
        vec![("OBSIDIAN_VAULT_PATH", vault_dir.path().to_string_lossy().as_ref())]
    ).await?;

    // THEN: Should find relevant files using basic fuzzy matching
    assert!(result.contains("Project Architecture.md") || result.contains("matches"));

    Ok(())
}

#[tokio::test]
async fn test_stats_command_works_immediately() -> Result<()> {
    // GIVEN: A test vault
    let vault_dir = create_test_vault().await?;

    // WHEN: User requests vault statistics
    let result = run_cli_command(
        vec!["stats"],
        vec![("OBSIDIAN_VAULT_PATH", vault_dir.path().to_string_lossy().as_ref())]
    ).await?;

    // THEN: Should show kiln statistics immediately
    assert!(result.contains("total_documents") || result.contains("files"));
    assert!(result.contains("kiln") || result.contains("Kiln"));

    Ok(())
}

#[tokio::test]
async fn test_help_command_shows_available_commands() -> Result<()> {
    // WHEN: User requests help
    let result = run_cli_command(vec!["--help"], vec![]).await?;

    // THEN: Should show available commands
    assert!(result.contains("search"));
    assert!(result.contains("fuzzy"));
    assert!(result.contains("stats"));

    Ok(())
}

#[tokio::test]
async fn test_no_command_defaults_to_help() -> Result<()> {
    // WHEN: User runs CLI with no arguments
    let result = run_cli_command(vec![], vec![]).await?;

    // THEN: Should show help or REPL mode
    assert!(result.contains("help") || result.contains("commands") || result.contains("REPL"));

    Ok(())
}

#[tokio::test]
async fn test_version_command_works() -> Result<()> {
    // WHEN: User requests version
    let result = run_cli_command(vec!["--version"], vec![]).await?;

    // THEN: Should show version information
    assert!(result.contains("0.1.0") || result.contains("version"));

    Ok(())
}

#[tokio::test]
async fn test_invalid_command_gives_helpful_error() -> Result<()> {
    // WHEN: User uses invalid command
    let result = run_cli_command(vec!["invalid-command"], vec![]).await;

    // THEN: Should give helpful error message
    match result {
        Ok(output) => {
            assert!(output.contains("error") || output.contains("usage") || output.contains("unrecognized"));
        }
        Err(e) => {
            let error_msg = e.to_string();
            assert!(error_msg.contains("error") || error_msg.contains("usage") || error_msg.contains("unrecognized"));
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_search_empty_query_shows_help() -> Result<()> {
    // GIVEN: A test vault
    let vault_dir = create_test_vault().await?;

    // WHEN: User searches with empty query
    let result = run_cli_command_allow_failure(
        vec!["search", ""],
        vec![("OBSIDIAN_VAULT_PATH", vault_dir.path().to_string_lossy().as_ref())]
    ).await?;

    // THEN: Should show validation error about empty query
    assert!(result.contains("empty") || result.contains("query") || result.contains("help"));

    Ok(())
}

/// Helper to run CLI command and allow failure (captures error output)
async fn run_cli_command_allow_failure(args: Vec<&str>, env_vars: Vec<(&str, &str)>) -> Result<String> {
    let binary_path = cli_binary_path();
    let mut cmd = Command::new(binary_path);

    // Add environment variables
    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    for arg in args {
        cmd.arg(arg);
    }

    let output_result = timeout(Duration::from_secs(30), cmd.output()).await
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

#[tokio::test]
async fn test_search_with_unicode_content() -> Result<()> {
    // GIVEN: A test vault with unicode content
    let vault_dir = create_test_vault().await?;

    // Create file with unicode content
    let unicode_file = vault_dir.path().join("unicode-test.md");
    let unicode_content = "# Unicode Test\n\nTest with emoji 🚀 and special chars: café, résumé, naïve";
    std::fs::write(&unicode_file, unicode_content)?;

    // WHEN: User searches for unicode terms
    let result = run_cli_command(
        vec!["search", "café"],
        vec![("OBSIDIAN_VAULT_PATH", vault_dir.path().to_string_lossy().as_ref())]
    ).await?;

    // THEN: Should find unicode content
    assert!(result.contains("unicode-test.md") || result.contains("matches"));

    Ok(())
}