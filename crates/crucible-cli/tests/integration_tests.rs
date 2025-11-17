//! Integration tests for Crucible CLI
//!
//! These tests verify end-to-end functionality of CLI commands

use assert_cmd::Command;
use predicates::prelude::*;

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

#[test]
fn test_chat_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("chat").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Natural language chat"))
        .stdout(predicate::str::contains("--act"))
        .stdout(predicate::str::contains("plan"));
}

#[test]
fn test_process_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("process").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Process files through the pipeline"))
        .stdout(predicate::str::contains("--force"))
        .stdout(predicate::str::contains("--watch"));
}

#[test]
fn test_config_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("config").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Configuration management"));
}

// Note: Full integration tests for chat/process would require:
// 1. Setting up a test kiln with markdown files
// 2. Mocking the ACP agent subprocess
// 3. Setting up a test database
// These are deferred to when the full implementations are completed
