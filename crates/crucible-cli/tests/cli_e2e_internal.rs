//! CLI binary-level E2E tests for internal session lifecycle commands.
//!
//! These tests validate flat `cru session ...` command behavior in non-interactive mode,
//! including help output, graceful failures when daemon connectivity is unavailable, and
//! a real daemon-backed lifecycle flow (`create -> list -> show -> send -> pause ->
//! unpause -> end`) using isolated sockets.

#[allow(deprecated)]
mod cli_e2e_helpers;

use cli_e2e_helpers::*;
use predicates::prelude::*;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

fn invalid_socket_path() -> (TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("failed to create temp dir for invalid socket path");
    let too_long_name = "s".repeat(220);
    let socket = temp.path().join(too_long_name);
    (temp, socket)
}

#[test]
fn session_help_shows_flat_internal_lifecycle_subcommands() {
    cru()
        .args(["session", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("show"))
        .stdout(predicate::str::contains("send"))
        .stdout(predicate::str::contains("pause"))
        .stdout(predicate::str::contains("unpause"))
        .stdout(predicate::str::contains("end"));
}

#[test]
fn session_list_help_shows_all_and_state_flags() {
    cru()
        .args(["session", "list", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--all"))
        .stdout(predicate::str::contains("--state"));
}

#[test]
fn session_create_help_shows_agent_flag() {
    cru()
        .args(["session", "create", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-a, --agent"));
}

#[test]
fn session_daemon_subcommand_is_removed() {
    cru()
        .args(["session", "daemon", "list"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("unrecognized subcommand")
                .or(predicate::str::contains("unexpected argument")),
        );
}

#[test]
fn session_list_without_daemon_is_graceful_error() {
    let temp = tempfile::tempdir().expect("failed to create temp dir");
    let config_path = write_config(temp.path(), "");
    let (_socket_temp, bad_socket) = invalid_socket_path();

    cru()
        .env("CRUCIBLE_SOCKET", &bad_socket)
        .arg("--config")
        .arg(&config_path)
        .args(["session", "list"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Failed to connect to daemon")
                .or(predicate::str::contains("Error:")),
        )
        .stderr(predicate::str::contains("panicked").not());
}

#[test]
fn session_show_without_daemon_for_missing_id_is_graceful_error() {
    let temp = tempfile::tempdir().expect("failed to create temp dir");
    let config_path = write_config(temp.path(), "");
    let (_socket_temp, bad_socket) = invalid_socket_path();

    cru()
        .env("CRUCIBLE_SOCKET", &bad_socket)
        .arg("--config")
        .arg(&config_path)
        .args(["session", "show", "chat-20260221-0000-dead"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Session not found").or(predicate::str::contains("Error:")),
        )
        .stderr(predicate::str::contains("panicked").not());
}

#[test]
#[ignore = "requires daemon"]
fn session_internal_lifecycle_with_real_daemon() {
    let daemon = TestDaemon::start();

    let create = daemon
        .command()
        .args(["session", "create", "-t", "chat"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let session_id = extract_session_id(&create);

    daemon
        .command()
        .args(["session", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&session_id));

    daemon
        .command()
        .args(["session", "show", &session_id])
        .assert()
        .success()
        .stdout(predicate::str::contains(&session_id));

    daemon
        .command()
        .args(["session", "pause", &session_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Paused session"));

    let mut send_cmd = daemon.command();
    send_cmd
        .args(["session", "send", &session_id, "hello from cli e2e"])
        .timeout(Duration::from_secs(20));
    let send_output = send_cmd.output().expect("failed to run session send");
    let send_stderr = String::from_utf8_lossy(&send_output.stderr);
    assert!(
        !send_stderr.contains("panicked"),
        "session send should fail gracefully when it fails"
    );

    daemon
        .command()
        .args(["session", "unpause", &session_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Resumed session"));

    daemon
        .command()
        .args(["session", "end", &session_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Ended session"));
}
