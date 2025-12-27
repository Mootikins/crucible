//! Integration tests for zellij-inbox

use std::process::Command;
use tempfile::TempDir;

fn zellij_inbox() -> Command {
    Command::new(env!("CARGO_BIN_EXE_zellij-inbox"))
}

#[test]
fn cli_add_list_remove() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("test.md");
    let file_arg = format!("--file={}", file.display());

    // Add item (--file must come before subcommand)
    let output = zellij_inbox()
        .args([&file_arg, "add", "test: hello", "--pane", "42", "--project", "test"])
        .output()
        .unwrap();
    assert!(output.status.success(), "add failed: {:?}", output);

    // List
    let output = zellij_inbox()
        .args([&file_arg, "list"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test: hello"));
    assert!(stdout.contains("pane:42"));

    // List JSON
    let output = zellij_inbox()
        .args([&file_arg, "list", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"pane_id\": 42"));

    // Remove
    let output = zellij_inbox()
        .args([&file_arg, "remove", "--pane", "42"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // List empty
    let output = zellij_inbox()
        .args([&file_arg, "list"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("(no items)"));
}

#[test]
fn cli_clear() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("test.md");
    let file_arg = format!("--file={}", file.display());

    // Add items
    zellij_inbox()
        .args([&file_arg, "add", "a", "--pane", "1", "--project", "p"])
        .output()
        .unwrap();
    zellij_inbox()
        .args([&file_arg, "add", "b", "--pane", "2", "--project", "p"])
        .output()
        .unwrap();

    // Clear
    let output = zellij_inbox()
        .args([&file_arg, "clear"])
        .output()
        .unwrap();
    assert!(output.status.success());

    // Verify empty
    let output = zellij_inbox()
        .args([&file_arg, "list"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("(no items)"));
}

#[test]
fn cli_upsert_same_pane() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("test.md");
    let file_arg = format!("--file={}", file.display());

    // Add item
    zellij_inbox()
        .args([&file_arg, "add", "first", "--pane", "42", "--project", "p"])
        .output()
        .unwrap();

    // Update same pane
    zellij_inbox()
        .args([&file_arg, "add", "second", "--pane", "42", "--project", "p"])
        .output()
        .unwrap();

    // Should have only one item with "second"
    let output = zellij_inbox()
        .args([&file_arg, "list"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("second"));
    assert!(!stdout.contains("first"));
}
