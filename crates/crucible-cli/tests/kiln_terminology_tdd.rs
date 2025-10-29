//! Terminology regression tests for the CLI. Ensure user-facing output refers to the
//! workspace as a "kiln" and no longer mentions the legacy "vault" wording.

use anyhow::{Context, Result};
use std::{path::PathBuf, process::Command, time::Duration};
use tempfile::TempDir;
use tokio::time::timeout;

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

fn assert_not_contains_vault(output: &str) {
    let lower = output.to_lowercase();
    assert!(
        !lower.contains("vault"),
        "Output should not reference legacy 'vault' terminology, got: {}",
        output
    );
}

fn assert_mentions_kiln_and_not_vault(output: &str) {
    let lower = output.to_lowercase();
    assert!(
        lower.contains("kiln"),
        "Expected output to reference 'kiln', got: {}",
        output
    );
    assert_not_contains_vault(output);
}

async fn run_cli_command(args: &[&str]) -> Result<String> {
    let binary_path = cli_binary_path();
    let mut cmd = Command::new(binary_path);
    cmd.args(args);

    // Isolate CLI from user environment so it can create config/history files safely.
    let temp_home = TempDir::new().context("Failed to create temporary HOME directory")?;
    cmd.env("HOME", temp_home.path());
    cmd.env("XDG_CONFIG_HOME", temp_home.path());
    cmd.env("XDG_DATA_HOME", temp_home.path());

    let output = timeout(Duration::from_secs(30), async move {
        let _guard = temp_home; // Keep temp directory alive while command runs
        tokio::task::spawn_blocking(move || cmd.output())
            .await
            .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
            .map_err(|e| anyhow::anyhow!("Command execution failed: {}", e))
    })
    .await
    .map_err(|_| anyhow::anyhow!("Command timed out"))??;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(anyhow::anyhow!("CLI command failed: {}", stderr));
    }

    Ok(if stderr.is_empty() {
        stdout
    } else {
        format!("{}{}", stderr, stdout)
    })
}

async fn run_cli_command_allow_failure(args: &[&str]) -> Result<String> {
    let binary_path = cli_binary_path();
    let mut cmd = Command::new(binary_path);
    cmd.args(args);

    let temp_home = TempDir::new().context("Failed to create temporary HOME directory")?;
    cmd.env("HOME", temp_home.path());
    cmd.env("XDG_CONFIG_HOME", temp_home.path());
    cmd.env("XDG_DATA_HOME", temp_home.path());

    let output = timeout(Duration::from_secs(30), async move {
        let _guard = temp_home;
        tokio::task::spawn_blocking(move || cmd.output())
            .await
            .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
            .map_err(|e| anyhow::anyhow!("Command execution failed: {}", e))
    })
    .await
    .map_err(|_| anyhow::anyhow!("Command timed out"))??;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok(if stderr.is_empty() {
        stdout
    } else {
        format!("{}{}", stderr, stdout)
    })
}

#[tokio::test]
async fn help_command_mentions_kiln_not_vault() -> Result<()> {
    let output = run_cli_command(&["--help"]).await?;
    assert_mentions_kiln_and_not_vault(&output);
    Ok(())
}

#[tokio::test]
async fn search_help_mentions_kiln_not_vault() -> Result<()> {
    let output = run_cli_command(&["search", "--help"]).await?;
    assert_mentions_kiln_and_not_vault(&output);
    Ok(())
}

#[tokio::test]
async fn semantic_help_mentions_kiln_not_vault() -> Result<()> {
    let output = run_cli_command(&["semantic", "--help"]).await?;
    assert_mentions_kiln_and_not_vault(&output);
    Ok(())
}

#[tokio::test]
async fn stats_help_mentions_kiln_not_vault() -> Result<()> {
    let output = run_cli_command(&["stats", "--help"]).await?;
    assert_mentions_kiln_and_not_vault(&output);
    Ok(())
}

#[tokio::test]
async fn config_help_does_not_reference_vault() -> Result<()> {
    let output = run_cli_command(&["config", "--help"]).await?;
    assert_not_contains_vault(&output);
    Ok(())
}

#[tokio::test]
async fn primary_help_commands_consistently_use_kiln() -> Result<()> {
    let commands: &[(&[&str], bool)] = &[
        (&["--help"], true),
        (&["search", "--help"], true),
        (&["semantic", "--help"], true),
        (&["stats", "--help"], true),
        (&["config", "--help"], false),
        (&["note", "--help"], false),
    ];

    for (args, expect_kiln) in commands {
        let output = run_cli_command(args).await?;
        if *expect_kiln {
            assert_mentions_kiln_and_not_vault(&output);
        } else {
            assert_not_contains_vault(&output);
        }
    }

    Ok(())
}

#[tokio::test]
async fn error_output_uses_kiln_not_vault() -> Result<()> {
    // Trigger an error by running semantic search without arguments.
    let output = run_cli_command_allow_failure(&["semantic"]).await?;
    assert_not_contains_vault(&output);
    Ok(())
}
