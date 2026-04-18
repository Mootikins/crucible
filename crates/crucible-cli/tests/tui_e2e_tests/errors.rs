//! Error message and exit-code tests.

use super::shared::{find_binary, require_binary};
use super::tui_e2e_harness::{TuiTestConfig, TuiTestSession};

// =============================================================================
// Error Message Tests
// =============================================================================

/// Test that invalid option shows helpful error
#[test]
fn error_invalid_option_value() {
    require_binary!();

    let config = TuiTestConfig::new("daemon --invalid-option-xyz");
    let mut session = TuiTestSession::spawn(config).expect("Failed to spawn");

    session
        .expect_regex(r"(?i)(error|unexpected)")
        .expect("Should show invalid option error");
    session.expect_eof().expect("Should exit");
}

// =============================================================================
// Exit Code Tests
// =============================================================================

/// Test that --version exits with code 0
#[test]
fn exit_code_version_success() {
    require_binary!();

    let binary = find_binary().expect("Binary should exist");
    let output = std::process::Command::new(&binary)
        .arg("--version")
        .output()
        .expect("Failed to run command");

    assert!(
        output.status.success(),
        "--version should exit with code 0, got: {:?}",
        output.status.code()
    );
}

/// Test that --help exits with code 0
#[test]
fn exit_code_help_success() {
    require_binary!();

    let binary = find_binary().expect("Binary should exist");
    let output = std::process::Command::new(&binary)
        .arg("--help")
        .output()
        .expect("Failed to run command");

    assert!(
        output.status.success(),
        "--help should exit with code 0, got: {:?}",
        output.status.code()
    );
}

/// Test that invalid subcommand exits with non-zero code
#[test]
fn exit_code_invalid_subcommand_failure() {
    require_binary!();

    let binary = find_binary().expect("Binary should exist");
    let output = std::process::Command::new(&binary)
        .arg("nonexistent_command_xyz")
        .output()
        .expect("Failed to run command");

    assert!(
        !output.status.success(),
        "Invalid subcommand should exit with non-zero code, got: {:?}",
        output.status.code()
    );
}

/// Test that invalid option exits with non-zero code
#[test]
fn exit_code_invalid_option_failure() {
    require_binary!();

    let binary = find_binary().expect("Binary should exist");
    let output = std::process::Command::new(&binary)
        .arg("--nonexistent-option-xyz")
        .output()
        .expect("Failed to run command");

    assert!(
        !output.status.success(),
        "Invalid option should exit with non-zero code, got: {:?}",
        output.status.code()
    );
}

/// Test that subcommand --help exits with code 0
#[test]
fn exit_code_subcommand_help_success() {
    require_binary!();

    let binary = find_binary().expect("Binary should exist");

    for subcommand in &[
        "config", "daemon", "storage", "stats", "mcp", "chat", "process",
    ] {
        let output = std::process::Command::new(&binary)
            .args([*subcommand, "--help"])
            .output()
            .expect("Failed to run command");

        assert!(
            output.status.success(),
            "{} --help should exit with code 0, got: {:?}",
            subcommand,
            output.status.code()
        );
    }
}

/// Test that process command on nonexistent path completes without hanging
#[test]
fn exit_code_process_nonexistent_path() {
    require_binary!();

    let binary = find_binary().expect("Binary should exist");
    let output = std::process::Command::new(&binary)
        .args(["process", "--kiln", "/nonexistent/path/that/does/not/exist"])
        .output()
        .expect("Failed to run command");

    let _stderr = String::from_utf8_lossy(&output.stderr);
    let _stdout = String::from_utf8_lossy(&output.stdout);
}
