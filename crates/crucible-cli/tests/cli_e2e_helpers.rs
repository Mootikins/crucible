//! Shared test infrastructure for CLI E2E tests.
//!
//! Provides daemon isolation, command helpers, and config fixtures
//! used by cli_e2e_internal, cli_e2e_acp, and cli_e2e_delegation tests.

// Deprecation warning is from assert_cmd's cargo_bin() - intentional, matches existing tests
#![allow(deprecated)]

use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as StdCommand, Stdio};
use std::thread;
use std::time::Duration;

/// Create a `cru` CLI command via assert_cmd.
pub fn cru() -> Command {
    Command::cargo_bin("cru").unwrap()
}

/// Escape a path for embedding in TOML string values (Windows backslash handling).
pub fn toml_escape(path: &Path) -> String {
    path.display().to_string().replace('\\', "\\\\")
}

/// Parse session ID from CLI stdout containing "Created session: <id>".
pub fn extract_session_id(stdout: &[u8]) -> String {
    let text = String::from_utf8_lossy(stdout);
    text.lines()
        .find_map(|line| line.strip_prefix("Created session: "))
        .map(|s| s.trim().to_string())
        .expect("expected 'Created session: <id>' in output")
}

/// Write a minimal config.toml with correct `kiln_path` format.
/// Returns the config file path.
pub fn write_config(dir: &Path, extra_toml: &str) -> PathBuf {
    let kiln_path = dir.join("kiln");
    fs::create_dir_all(&kiln_path).expect("create kiln dir");

    let config_path = dir.join("config.toml");
    let config = format!(
        "kiln_path = \"{}\"\n\n[chat]\nprovider = \"ollama\"\nmodel = \"llama3.2\"\n{}",
        toml_escape(&kiln_path),
        extra_toml,
    );
    fs::write(&config_path, config).expect("write config");
    config_path
}

/// Isolated daemon fixture with RAII cleanup.
pub struct TestDaemon {
    pub socket_path: PathBuf,
    pub config_path: PathBuf,
    _temp_dir: tempfile::TempDir,
    process: Child,
}

impl TestDaemon {
    /// Start an isolated daemon with a minimal config.
    pub fn start() -> Self {
        Self::start_with_extra_config("")
    }

    /// Start an isolated daemon with extra TOML appended to the config.
    pub fn start_with_extra_config(extra_toml: &str) -> Self {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let socket_path = temp_dir.path().join("daemon.sock");
        let config_path = write_config(temp_dir.path(), extra_toml);

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
                    socket_path,
                    config_path,
                    _temp_dir: temp_dir,
                    process,
                };
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!("daemon socket did not appear within 5 seconds");
    }

    /// Create a `cru` command pre-wired with CRUCIBLE_SOCKET and --config.
    pub fn command(&self) -> Command {
        let mut cmd = cru();
        cmd.env("CRUCIBLE_SOCKET", &self.socket_path)
            .arg("--config")
            .arg(&self.config_path);
        cmd
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
