mod test_utilities;
use assert_cmd::Command;
use predicates::prelude::*;
use test_utilities::TestKiln;

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
    cmd.arg("--version").assert().success();
}

#[test]
fn test_stats_command_with_empty_kiln() {
    let kiln = TestKiln::new().unwrap();

    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("--kiln-path")
        .arg(kiln.kiln_path_str())
        .arg("--db-path")
        .arg(kiln.db_path_str())
        .arg("stats")
        .assert()
        .success();
}

#[test]
fn test_note_list_empty() {
    let kiln = TestKiln::new().unwrap();

    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("--kiln-path")
        .arg(kiln.kiln_path_str())
        .arg("--db-path")
        .arg(kiln.db_path_str())
        .arg("note")
        .arg("list")
        .assert()
        .success();
}

#[test]
fn test_note_create() {
    let kiln = TestKiln::new().unwrap();
    let note_path = "test-note.md";

    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("--kiln-path")
        .arg(kiln.kiln_path_str())
        .arg("--db-path")
        .arg(kiln.db_path_str())
        .arg("note")
        .arg("create")
        .arg(note_path)
        .arg("--content")
        .arg("# Test Note")
        .assert()
        .success()
        .stdout(predicate::str::contains("Created"));

    // Verify file was created
    assert!(kiln.kiln_path.join(note_path).exists());
}

#[test]
fn test_commands_list() {
    let _kiln = TestKiln::new().unwrap();

    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("commands")
        .assert()
        .success()
        .stdout(predicate::str::contains("Available Rune Commands"));
}

#[test]
fn test_invalid_command() {
    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("nonexistent-command").assert().failure();
}

#[test]
fn test_missing_kiln_path() {
    let kiln = TestKiln::new().unwrap();

    // Use a nonexistent kiln path
    let mut cmd = Command::cargo_bin("crucible-cli").unwrap();
    cmd.arg("--kiln-path")
        .arg("/nonexistent/path")
        .arg("--db-path")
        .arg(kiln.db_path_str())
        .arg("stats")
        .assert()
        .success(); // Should succeed but show empty stats
}
