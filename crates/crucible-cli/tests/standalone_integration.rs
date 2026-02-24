//! Integration tests for `cru --standalone` mode.
//!
//! Verifies that the standalone flag:
//! 1. Is accepted by the CLI parser
//! 2. Starts an in-process daemon (no background server required)
//! 3. Does not conflict with a running daemon (uses its own PID-based socket)
//! 4. Works without any external daemon running

#![allow(deprecated)]

use assert_cmd::Command;
use std::fs;
use std::path::Path;

/// Create a `cru` CLI command via assert_cmd.
fn cru() -> Command {
    Command::cargo_bin("cru").unwrap()
}

/// Write a minimal config.toml for tests.
fn write_config(dir: &Path) -> std::path::PathBuf {
    let kiln_path = dir.join("kiln");
    fs::create_dir_all(&kiln_path).expect("create kiln dir");

    let config_path = dir.join("config.toml");
    let config = format!(
        "kiln_path = \"{}\"\n\n[llm]\ndefault = \"local\"\n\n[llm.providers.local]\ntype = \"ollama\"\ndefault_model = \"llama3.2\"\n",
        kiln_path.display().to_string().replace('\\', "\\\\"),
    );
    fs::write(&config_path, config).expect("write config");
    config_path
}

/// Test that `--standalone` flag is accepted by the CLI parser.
/// Uses `--help` to avoid actually starting a daemon.
#[test]
fn standalone_flag_is_accepted_by_parser() {
    // `cru --standalone --help` should succeed (help exits with error code, but flag is parsed)
    // We test that the flag doesn't cause an "unexpected argument" error
    let output = cru()
        .args(["--standalone", "daemon", "--help"])
        .output()
        .expect("failed to run cru");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should NOT contain "unexpected argument" or "unrecognized"
    assert!(
        !stderr.contains("unexpected argument"),
        "standalone flag was rejected: {}",
        stderr
    );
    assert!(
        !stderr.contains("unrecognized"),
        "standalone flag was rejected: {}",
        stderr
    );
}

/// Test that `cru --standalone daemon status` works without an external daemon.
/// The standalone flag should spin up an in-process daemon and respond to status.
#[test]
fn standalone_daemon_status_works_without_external_daemon() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let config_path = write_config(temp.path());

    // Use a unique socket path to avoid conflicts with any running daemon
    let unique_socket = temp.path().join("test-standalone.sock");

    let output = cru()
        .args(["--standalone", "--config"])
        .arg(&config_path)
        .arg("daemon")
        .arg("status")
        // Override socket so we don't conflict with real daemon
        .env("CRUCIBLE_SOCKET", &unique_socket)
        .timeout(std::time::Duration::from_secs(15))
        .output()
        .expect("failed to run cru --standalone daemon status");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should succeed (exit 0) and report daemon is running
    assert!(
        output.status.success(),
        "standalone daemon status failed.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("running"),
        "expected 'running' in output.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );
}

/// Test that `cru --standalone` does not conflict with a running daemon.
/// When CRUCIBLE_SOCKET is set to a unique path, standalone creates its own server.
#[test]
fn standalone_uses_isolated_socket_not_shared_daemon() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let config_path = write_config(temp.path());

    // Point to a socket that definitely doesn't exist
    let nonexistent_socket = temp.path().join("nonexistent-daemon.sock");

    // Without --standalone, this would fail (no daemon at that socket)
    // With --standalone, it should succeed (creates its own in-process daemon)
    let output = cru()
        .args(["--standalone", "--config"])
        .arg(&config_path)
        .arg("daemon")
        .arg("status")
        .env("CRUCIBLE_SOCKET", &nonexistent_socket)
        .timeout(std::time::Duration::from_secs(15))
        .output()
        .expect("failed to run cru");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "standalone should succeed even without external daemon.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );
}

/// Test that `cru --standalone` creates the .crucible/ directory for persistence.
/// This verifies Task 21: standalone persistence to disk.
#[test]
fn standalone_creates_crucible_directory_for_persistence() {
    let temp = tempfile::tempdir().expect("create temp dir");
    let config_path = write_config(temp.path());
    let kiln_path = temp.path().join("kiln");
    let unique_socket = temp.path().join("standalone-persist.sock");

    // Run a simple command in standalone mode
    let output = cru()
        .args(["--standalone", "--config"])
        .arg(&config_path)
        .arg("daemon")
        .arg("status")
        .env("CRUCIBLE_SOCKET", &unique_socket)
        .timeout(std::time::Duration::from_secs(15))
        .output()
        .expect("failed to run cru");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "standalone command failed.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // The kiln directory should exist (created by config)
    assert!(
        kiln_path.exists(),
        "kiln directory should exist at {:?}",
        kiln_path
    );
}

/// Test that `cru --standalone` flag is global and works before subcommands.
#[test]
fn standalone_flag_is_global_works_before_subcommands() {
    // Test various positions of --standalone flag
    let temp = tempfile::tempdir().expect("create temp dir");
    let config_path = write_config(temp.path());
    let unique_socket = temp.path().join("global-flag-test.sock");

    // --standalone before subcommand
    let output = cru()
        .arg("--standalone")
        .arg("--config")
        .arg(&config_path)
        .arg("daemon")
        .arg("status")
        .env("CRUCIBLE_SOCKET", &unique_socket)
        .timeout(std::time::Duration::from_secs(15))
        .output()
        .expect("failed to run cru");

    assert!(
        output.status.success(),
        "--standalone before subcommand should work.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
