//! Integration tests for Crucible CLI

#![allow(clippy::field_reassign_with_default, deprecated)]

//!
//! These tests verify end-to-end functionality of CLI commands

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Cross-platform test path helper
fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("crucible_test_{}", name))
}

// ============================================================================
// Basic CLI Tests
// ============================================================================

#[test]
fn test_cli_version() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("--version");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("0.1.0")); // Check for version number
}

#[test]
fn test_cli_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Usage:"));
}

// ============================================================================
// Global Flag Tests
// ============================================================================

#[test]
fn test_global_verbose_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("--verbose").arg("--help");

    cmd.assert().success();
}

#[test]
fn test_global_verbose_short_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("-v").arg("--help");

    cmd.assert().success();
}

#[test]
fn test_global_config_flag() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("test-config.toml");
    let kiln_path = test_path("test-kiln");

    // Create a minimal config file
    fs::write(
        &config_path,
        format!(
            r#"
[kiln]
path = "{}"
"#,
            kiln_path.to_string_lossy().replace('\\', "\\\\")
        ),
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("--config")
        .arg(config_path.to_str().unwrap())
        .arg("--help");

    cmd.assert().success();
}

#[test]
fn test_global_embedding_url_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("--embedding-url")
        .arg("https://example.com")
        .arg("--help");

    cmd.assert().success();
}

#[test]
fn test_global_embedding_model_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("--embedding-model").arg("test-model").arg("--help");

    cmd.assert().success();
}

#[test]
fn test_global_format_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("--format").arg("json").arg("--help");

    cmd.assert().success();
}

#[test]
fn test_global_no_process_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("--no-process").arg("--help");

    cmd.assert().success();
}

#[test]
fn test_global_process_timeout_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("--process-timeout").arg("120").arg("--help");

    cmd.assert().success();
}

#[test]
fn test_multiple_global_flags() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("--verbose")
        .arg("--no-process")
        .arg("--format")
        .arg("json")
        .arg("--help");

    cmd.assert().success();
}

// ============================================================================
// Chat Command Tests
// ============================================================================

#[test]
fn test_chat_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("chat").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Interactive AI chat"))
        .stdout(predicate::str::contains("--plan"))
        .stdout(predicate::str::contains("plan"));
}

#[test]
fn test_chat_with_agent_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("chat")
        .arg("--agent")
        .arg("claude-code")
        .arg("--help");

    cmd.assert().success();
}

#[test]
fn test_chat_with_no_context_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("chat").arg("--no-context").arg("--help");

    cmd.assert().success();
}

#[test]
fn test_chat_with_context_size_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("chat")
        .arg("--context-size")
        .arg("10")
        .arg("--help");

    cmd.assert().success();
}

#[test]
fn test_chat_with_plan_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("chat").arg("--plan").arg("--help");

    cmd.assert().success();
}

#[test]
fn test_chat_all_flags_combined() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("chat")
        .arg("--agent")
        .arg("claude-code")
        .arg("--no-context")
        .arg("--context-size")
        .arg("10")
        .arg("--plan")
        .arg("--help");

    cmd.assert().success();
}

// ============================================================================
// Process Command Tests
// ============================================================================

#[test]
fn test_process_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("process").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "Process markdown files through the pipeline",
        ))
        .stdout(predicate::str::contains("--force"))
        .stdout(predicate::str::contains("--watch"));
}

#[test]
fn test_process_with_force_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("process").arg("--force").arg("--help");

    cmd.assert().success();
}

#[test]
fn test_process_with_watch_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("process").arg("--watch").arg("--help");

    cmd.assert().success();
}

#[test]
fn test_process_with_short_watch_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("process").arg("-w").arg("--help");

    cmd.assert().success();
}

// ============================================================================
// Stats Command Tests
// ============================================================================

#[test]
fn test_stats_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("stats").arg("--help");

    cmd.assert().success().stdout(predicate::str::contains(
        "Display comprehensive kiln statistics",
    ));
}

// ============================================================================
// Config Command Tests
// ============================================================================

#[test]
fn test_config_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("config").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Manage Crucible configuration"));
}

#[test]
fn test_config_init_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("config").arg("init").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Initialize a new config file"));
}

#[test]
fn test_config_show_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("config").arg("show").arg("--help");

    cmd.assert().success().stdout(predicate::str::contains(
        "Show the current effective configuration",
    ));
}

#[test]
fn test_config_init_with_path_flag() {
    let config_path = test_path("test-config.toml");
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("config")
        .arg("init")
        .arg("--path")
        .arg(config_path.to_str().unwrap())
        .arg("--help");

    cmd.assert().success();
}

#[test]
fn test_config_init_with_force_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("config").arg("init").arg("--force").arg("--help");

    cmd.assert().success();
}

#[test]
fn test_config_show_with_format_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("config")
        .arg("show")
        .arg("--format")
        .arg("json")
        .arg("--help");

    cmd.assert().success();
}

// Diff command removed in Phase 1 CLI consolidation

// ============================================================================
// Status Command Tests
// ============================================================================

#[test]
fn test_status_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("status").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Show storage status"));
}

#[test]
fn test_status_with_format_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("status").arg("--format").arg("json").arg("--help");

    cmd.assert().success();
}

#[test]
fn test_status_with_detailed_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("status").arg("--detailed").arg("--help");

    cmd.assert().success();
}

#[test]
fn test_status_with_recent_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("status").arg("--recent").arg("--help");

    cmd.assert().success();
}

// ============================================================================
// Storage Command Tests
// ============================================================================

#[test]
fn test_storage_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("storage").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Manage storage operations"));
}

#[test]
fn test_storage_stats_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("storage").arg("stats").arg("--help");

    cmd.assert().success();
}

#[test]
fn test_storage_verify_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("storage").arg("verify").arg("--help");

    cmd.assert().success();
}

#[test]
fn test_storage_cleanup_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("storage").arg("cleanup").arg("--help");

    cmd.assert().success();
}

#[test]
fn test_storage_backup_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("storage").arg("backup").arg("--help");

    cmd.assert().success();
}

#[test]
fn test_storage_restore_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("storage").arg("restore").arg("--help");

    cmd.assert().success();
}

// ============================================================================
// Edge Cases and Error Scenarios
// ============================================================================

#[test]
fn test_invalid_command() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("invalid-command");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("error:").or(predicate::str::contains("unrecognized")));
}

#[test]
fn test_invalid_global_flag() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("--invalid-flag").arg("--help");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument"));
}

#[test]
fn test_invalid_process_timeout_value() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("--process-timeout")
        .arg("not-a-number")
        .arg("--help");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn test_invalid_context_size_value() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("chat").arg("--context-size").arg("not-a-number");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn test_config_flag_with_nonexistent_file() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("--config")
        .arg("/nonexistent/path/config.toml")
        .arg("--help");

    // Should succeed for --help even with invalid config path
    cmd.assert().success();
}

// Note: Full integration tests for chat/process would require:
// 1. Setting up a test kiln with markdown files
// 2. Mocking the ACP agent subprocess
// 3. Setting up a test database
// These are deferred to when the full implementations are completed
