mod common;

use assert_cmd::Command;
use common::TestVault;
use predicates::prelude::*;

#[test]
fn test_help_command() {
    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Crucible CLI"));
}

#[test]
fn test_version_command() {
    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("--version")
        .assert()
        .success();
}

#[test]
fn test_stats_command_with_empty_vault() {
    let vault = TestVault::new().unwrap();
    
    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("--vault-path").arg(vault.vault_path_str())
        .arg("--database").arg(vault.db_path_str())
        .arg("stats")
        .assert()
        .success();
}

#[test]
fn test_note_list_empty() {
    let vault = TestVault::new().unwrap();
    
    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("--vault-path").arg(vault.vault_path_str())
        .arg("--database").arg(vault.db_path_str())
        .arg("note")
        .arg("list")
        .assert()
        .success();
}

#[test]
fn test_note_create() {
    let vault = TestVault::new().unwrap();
    let note_path = "test-note.md";
    
    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("--vault-path").arg(vault.vault_path_str())
        .arg("--database").arg(vault.db_path_str())
        .arg("note")
        .arg("create")
        .arg(note_path)
        .arg("--content")
        .arg("# Test Note")
        .assert()
        .success()
        .stdout(predicate::str::contains("Created"));
    
    // Verify file was created
    assert!(vault.vault_path.join(note_path).exists());
}

#[test]
fn test_commands_list() {
    let _vault = TestVault::new().unwrap();
    
    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("commands")
        .assert()
        .success()
        .stdout(predicate::str::contains("Available Rune Commands"));
}

#[test]
fn test_invalid_command() {
    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("nonexistent-command")
        .assert()
        .failure();
}

#[test]
fn test_missing_vault_path() {
    let vault = TestVault::new().unwrap();
    
    // Use a nonexistent vault path
    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("--vault-path").arg("/nonexistent/path")
        .arg("--database").arg(vault.db_path_str())
        .arg("stats")
        .assert()
        .success(); // Should succeed but show empty stats
}
