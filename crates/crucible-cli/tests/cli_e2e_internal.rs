//! CLI binary-level E2E tests for internal session lifecycle commands.
//!
//! These tests validate flat `cru session ...` command behavior in non-interactive mode,
//! including help output, graceful failures when daemon connectivity is unavailable, and
//! a real daemon-backed lifecycle flow (`create -> list -> show -> send -> pause ->
//! unpause -> end`) using isolated sockets.

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as StdCommand, Stdio};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

fn cru() -> Command {
    Command::cargo_bin("cru").unwrap()
}

struct TestKiln {
    _temp_dir: TempDir,
    path: PathBuf,
}

impl TestKiln {
    fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp kiln dir");
        let path = temp_dir.path().to_path_buf();
        let crucible_dir = path.join(".crucible");

        fs::create_dir_all(crucible_dir.join("sessions")).expect("failed to create sessions dir");
        fs::create_dir_all(crucible_dir.join("plugins")).expect("failed to create plugins dir");

        fs::write(
            crucible_dir.join("config.toml"),
            r#"[kiln]
path = "."

[chat]
provider = "ollama"
model = "llama3.2"
"#,
        )
        .expect("failed to write test config");

        Self {
            _temp_dir: temp_dir,
            path,
        }
    }
}

struct TestDaemon {
    _temp_dir: TempDir,
    socket_path: PathBuf,
    process: Child,
}

impl TestDaemon {
    fn start() -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create daemon temp dir");
        let socket_path = temp_dir.path().join("daemon.sock");
        let daemon_exe =
            std::env::var("CARGO_BIN_EXE_cru-server").unwrap_or_else(|_| "cru-server".to_string());

        let process = StdCommand::new(daemon_exe)
            .env("CRUCIBLE_SOCKET", &socket_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn cru-server");

        for _ in 0..50 {
            if socket_path.exists() {
                thread::sleep(Duration::from_millis(50));
                return Self {
                    _temp_dir: temp_dir,
                    socket_path,
                    process,
                };
            }
            thread::sleep(Duration::from_millis(100));
        }

        panic!("daemon socket did not appear within timeout");
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

fn command_with_env(kiln_path: &Path, socket_path: &Path) -> Command {
    let mut cmd = cru();
    cmd.current_dir(kiln_path)
        .env("CRUCIBLE_SOCKET", socket_path)
        .arg("--no-process");
    cmd
}

fn invalid_socket_path() -> (TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("failed to create temp dir for invalid socket path");
    let too_long_name = "s".repeat(220);
    let socket = temp.path().join(too_long_name);
    (temp, socket)
}

fn parse_created_session_id(output: &str) -> String {
    output
        .lines()
        .find_map(|line| line.strip_prefix("Created session: "))
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .expect("missing 'Created session:' line in output")
        .to_string()
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
    let kiln = TestKiln::new();
    let (_socket_temp, bad_socket) = invalid_socket_path();

    command_with_env(&kiln.path, &bad_socket)
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
    let kiln = TestKiln::new();
    let (_socket_temp, bad_socket) = invalid_socket_path();

    command_with_env(&kiln.path, &bad_socket)
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
    let kiln = TestKiln::new();
    let daemon = TestDaemon::start();

    let create = command_with_env(&kiln.path, &daemon.socket_path)
        .args(["session", "create", "-t", "chat"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let create_stdout = String::from_utf8(create).expect("create stdout was not utf-8");
    let session_id = parse_created_session_id(&create_stdout);

    command_with_env(&kiln.path, &daemon.socket_path)
        .args(["session", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&session_id));

    command_with_env(&kiln.path, &daemon.socket_path)
        .args(["session", "show", &session_id])
        .assert()
        .success()
        .stdout(predicate::str::contains(&session_id));

    command_with_env(&kiln.path, &daemon.socket_path)
        .args(["session", "pause", &session_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Paused session"));

    let mut send_cmd = command_with_env(&kiln.path, &daemon.socket_path);
    send_cmd
        .args(["session", "send", &session_id, "hello from cli e2e"])
        .timeout(Duration::from_secs(20));
    let send_output = send_cmd.output().expect("failed to run session send");
    let send_stderr = String::from_utf8_lossy(&send_output.stderr);
    assert!(
        !send_stderr.contains("panicked"),
        "session send should fail gracefully when it fails"
    );

    command_with_env(&kiln.path, &daemon.socket_path)
        .args(["session", "unpause", &session_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Resumed session"));

    command_with_env(&kiln.path, &daemon.socket_path)
        .args(["session", "end", &session_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Ended session"));
}
