//! CLI binary E2E tests for ACP session lifecycle and `--agent` handling.
//!
//! These tests validate `cru session create --agent <profile>` behavior at the
//! binary boundary, including help text, built-in profile resolution, unknown
//! profile errors, and a full create -> send -> end lifecycle with a mock ACP
//! agent profile.

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as StdCommand, Stdio};
use std::thread;
use std::time::Duration;

fn cru() -> Command {
    Command::cargo_bin("cru").unwrap()
}

fn toml_escape(path: &Path) -> String {
    path.display().to_string().replace('\\', "\\\\")
}

fn mock_agent_path() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_mock-acp-agent") {
        return PathBuf::from(path);
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/mock-acp-agent")
}

fn daemon_binary_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_cru-server") {
        return PathBuf::from(path);
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/cru-server")
}

struct IsolatedDaemon {
    _temp_dir: tempfile::TempDir,
    socket_path: PathBuf,
    config_path: PathBuf,
    process: Option<Child>,
}

impl IsolatedDaemon {
    fn start(with_mock_profile: bool) -> Self {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let socket_path = temp_dir.path().join("daemon.sock");
        let kiln_path = temp_dir.path().join("kiln");
        fs::create_dir_all(&kiln_path).expect("create kiln dir");

        let config_path = temp_dir.path().join("config.toml");
        let mut config = format!(
            "[kiln]\npath = \"{}\"\n\n[chat]\nprovider = \"ollama\"\nmodel = \"llama3.2\"\n",
            toml_escape(&kiln_path)
        );

        if with_mock_profile {
            let mock_path = mock_agent_path();
            assert!(
                mock_path.exists(),
                "mock-acp-agent binary not found at {}",
                mock_path.display()
            );

            config.push_str(&format!(
                "\n[acp.agents.mock]\ncommand = \"{}\"\nargs = []\ndescription = \"Mock ACP agent for CLI E2E tests\"\n",
                toml_escape(&mock_path)
            ));
        }

        fs::write(&config_path, config).expect("write config");

        let daemon_exe = daemon_binary_path();
        assert!(
            daemon_exe.exists(),
            "cru-server binary not found at {}",
            daemon_exe.display()
        );
        let process = StdCommand::new(daemon_exe)
            .env("CRUCIBLE_SOCKET", &socket_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn daemon");

        for _ in 0..50 {
            if socket_path.exists() {
                thread::sleep(Duration::from_millis(50));
                return Self {
                    _temp_dir: temp_dir,
                    socket_path,
                    config_path,
                    process: Some(process),
                };
            }
            thread::sleep(Duration::from_millis(100));
        }

        panic!("daemon failed to start within 5 seconds");
    }

    fn command(&self) -> Command {
        let mut cmd = cru();
        cmd.env("CRUCIBLE_SOCKET", &self.socket_path)
            .arg("--config")
            .arg(&self.config_path);
        cmd
    }
}

impl Drop for IsolatedDaemon {
    fn drop(&mut self) {
        if let Some(mut child) = self.process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn extract_session_id(stdout: &[u8]) -> String {
    let text = String::from_utf8_lossy(stdout);
    for line in text.lines() {
        if let Some(session_id) = line.strip_prefix("Created session: ") {
            return session_id.trim().to_string();
        }
    }

    panic!("could not parse session id from output: {text}");
}

#[test]
fn session_create_help_shows_agent_flag() {
    cru()
        .args(["session", "create", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-a, --agent <AGENT>"))
        .stdout(predicate::str::contains("ACP agent profile"));
}

#[test]
#[serial]
#[ignore = "requires daemon"]
fn session_create_rejects_unknown_agent_profile() {
    let daemon = IsolatedDaemon::start(false);

    daemon
        .command()
        .args(["session", "create", "--agent", "nonexistent-profile"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Unknown ACP agent profile: nonexistent-profile",
        ));
}

#[test]
#[serial]
#[ignore = "requires daemon"]
fn session_create_rejects_empty_agent_profile() {
    let daemon = IsolatedDaemon::start(false);

    daemon
        .command()
        .args(["session", "create", "--agent", ""])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unknown ACP agent profile"));
}

#[test]
#[serial]
#[ignore = "requires daemon"]
fn session_create_accepts_builtin_acp_profiles() {
    let daemon = IsolatedDaemon::start(false);

    for profile in ["claude", "opencode", "gemini", "codex", "cursor"] {
        daemon
            .command()
            .args(["session", "create", "--agent", profile])
            .assert()
            .success()
            .stdout(predicate::str::contains(format!(
                "Configured agent: {} (acp)",
                profile
            )));
    }
}

#[test]
#[serial]
#[ignore = "requires daemon and mock-acp-agent binary"]
fn session_acp_lifecycle_with_mock_agent_profile() {
    let daemon = IsolatedDaemon::start(true);

    let create_output = daemon
        .command()
        .args(["session", "create", "--agent", "mock"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Configured agent: mock (acp)"))
        .get_output()
        .stdout
        .clone();

    let session_id = extract_session_id(&create_output);

    daemon
        .command()
        .args([
            "session",
            "send",
            &session_id,
            "hello from cli e2e acp test",
        ])
        .assert()
        .success();

    daemon
        .command()
        .args(["session", "end", &session_id])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "Ended session: {}",
            session_id
        )));
}
