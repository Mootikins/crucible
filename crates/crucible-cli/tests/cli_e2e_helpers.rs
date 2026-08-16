//! Shared test infrastructure for CLI E2E tests.
//!
//! Provides daemon isolation, command helpers, and config fixtures
//! used by cli_e2e_internal, cli_e2e_acp, and cli_e2e_delegation tests.

use assert_cmd::Command;
use crucible_core::test_support::hermetic_env_pairs;
use std::fs;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as StdCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const DAEMON_READY_TIMEOUT: Duration = Duration::from_secs(5);
const DAEMON_READY_POLL: Duration = Duration::from_millis(25);

/// Create a `cru` CLI command via assert_cmd.
pub fn cru() -> Command {
    assert_cmd::cargo_bin_cmd!("cru")
}

/// Escape a path for embedding in TOML string values (Windows backslash handling).
pub fn toml_escape(path: &Path) -> String {
    path.display().to_string().replace('\\', "\\\\")
}

/// Parse the session ID out of `cru session create`'s stdout.
///
/// Two output shapes, because the CLI has two. Interactively it prints
/// `Created session: <id>` followed by usage hints; piped — which is what a
/// test harness always is, since `is_interactive()` reads the tty — it prints
/// the bare id so a script can consume it.
///
/// Matching only the interactive form made every one of these tests
/// unpassable under `cargo test` and `cargo nextest`, which capture stdout by
/// construction. They failed on the shape of the output, never reaching the
/// behaviour they exist to check.
pub fn extract_session_id(stdout: &[u8]) -> String {
    let text = String::from_utf8_lossy(stdout);

    // `--format json` first — it is unambiguous, and the tests that need to
    // assert on more than the id ask for it.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text.trim()) {
        if let Some(id) = v["session_id"].as_str() {
            return id.to_string();
        }
    }

    text.lines()
        .find_map(|line| {
            let line = line.trim();
            line.strip_prefix("Created session: ")
                .map(str::trim)
                // The quiet form: the id alone on its own line. Session ids
                // carry no spaces, which is what distinguishes it from the
                // usage hints the interactive form prints after it.
                .or_else(|| (!line.is_empty() && !line.contains(' ')).then_some(line))
        })
        .map(str::to_string)
        .expect("expected a session id in `session create` output")
}

/// Write a minimal config.toml with correct `kiln_path` format.
/// Returns the config file path.
pub fn write_config(dir: &Path, extra_toml: &str) -> PathBuf {
    let kiln_path = dir.join("kiln");
    fs::create_dir_all(&kiln_path).expect("create kiln dir");

    let config_path = dir.join("config.toml");
    let config = format!(
        "kiln_path = \"{}\"\n\n[llm]\ndefault = \"ollama\"\n\n[llm.providers.ollama]\ntype = \"ollama\"\ndefault_model = \"llama3.2\"\n{}",
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

        // Single binary: daemon runs via `cru daemon serve`
        let cru_exe = env!("CARGO_BIN_EXE_cru");

        // Hermetic child env: the daemon must never inherit the developer's
        // real provider credentials (it could make real API calls) or real
        // ~/.crucible state. env_clear + allowlist, rooted in the TempDir.
        let mut daemon_cmd = StdCommand::new(cru_exe);
        daemon_cmd.env_clear();
        for (k, v) in hermetic_env_pairs(temp_dir.path()) {
            daemon_cmd.env(k, v);
        }
        // Diagnosing a daemon-side race means reading the daemon's own log, and
        // these fixtures discard it. `CRUCIBLE_TEST_DAEMON_LOG=<dir>` writes each
        // daemon's stderr to `<dir>/<pid>.log`; unset, nothing changes.
        let log_sink = std::env::var_os("CRUCIBLE_TEST_DAEMON_LOG").map(|dir| {
            let dir = PathBuf::from(dir);
            fs::create_dir_all(&dir).expect("create daemon log dir");
            dir
        });
        let stderr = match &log_sink {
            Some(dir) => {
                let path = dir.join(format!("daemon-{}.log", std::process::id()));
                Stdio::from(fs::File::create(path).expect("create daemon log"))
            }
            None => Stdio::null(),
        };
        if log_sink.is_some() {
            daemon_cmd.env("RUST_LOG", "warn,crucible_daemon=debug");
        }
        let mut process = daemon_cmd
            .args(["--config", config_path.to_str().unwrap(), "daemon", "serve"])
            .env("CRUCIBLE_SOCKET", &socket_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(stderr)
            .spawn()
            .expect("failed to spawn cru daemon serve");

        let deadline = Instant::now() + DAEMON_READY_TIMEOUT;
        while Instant::now() < deadline {
            if UnixStream::connect(&socket_path).is_ok() {
                return Self {
                    socket_path,
                    config_path,
                    _temp_dir: temp_dir,
                    process,
                };
            }
            thread::sleep(DAEMON_READY_POLL);
        }
        let _ = process.kill();
        let _ = process.wait();
        panic!(
            "daemon socket at {} did not become connectable within {:?}",
            socket_path.display(),
            DAEMON_READY_TIMEOUT
        );
    }

    /// Create a `cru` command pre-wired with CRUCIBLE_SOCKET and --config,
    /// in the same hermetic environment as the daemon (no real credentials).
    pub fn command(&self) -> Command {
        let mut cmd = cru();
        cmd.env_clear();
        for (k, v) in hermetic_env_pairs(self._temp_dir.path()) {
            cmd.env(k, v);
        }
        cmd.env("CRUCIBLE_SOCKET", &self.socket_path)
            .arg("--config")
            .arg(&self.config_path);
        cmd
    }

    /// Where this daemon writes sessions. Sessions live under the daemon's own
    /// data root now rather than inside a kiln, so a test that reads or seeds a
    /// transcript on disk has to ask the daemon rather than compose a kiln path.
    ///
    /// The hermetic env gives the child `HOME = <temp>` and no `CRUCIBLE_HOME`,
    /// so the daemon resolves its data root to `<temp>/.crucible`.
    #[allow(dead_code)]
    pub fn sessions_root(&self) -> PathBuf {
        self._temp_dir.path().join(".crucible").join("sessions")
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
