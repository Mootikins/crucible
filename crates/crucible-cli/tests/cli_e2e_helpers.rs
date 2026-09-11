//! Shared test infrastructure for CLI E2E tests.
//!
//! Provides daemon isolation, command helpers, and config fixtures
//! used by cli_e2e_internal, cli_e2e_acp, and cli_e2e_delegation tests.

use assert_cmd::Command;
use crucible_core::test_support::hermetic_env_pairs;
use std::fs;
use std::ops::{Deref, DerefMut};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as StdCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const DAEMON_READY_TIMEOUT: Duration = Duration::from_secs(5);
const DAEMON_READY_POLL: Duration = Duration::from_millis(25);

/// A `cru` CLI command with no environment of its own.
///
/// Only for a caller that applies its own hermetic environment straight
/// afterwards, such as [`TestDaemon::command`]. Every other caller uses
/// [`cru`], which is hermetic already.
#[allow(dead_code)]
pub fn cru_bare() -> Command {
    assert_cmd::cargo_bin_cmd!("cru")
}

/// A hermetic `cru` command together with the temporary home it writes into.
///
/// The directory must outlive the child process, so the guard owns it. `Deref`
/// exposes the [`Command`] itself, which keeps the call sites unchanged: a
/// temporary guard lives to the end of the statement, and `assert()` and
/// `output()` both run the child before that point.
#[allow(dead_code)]
pub struct HermeticCru {
    cmd: Command,
    _home: tempfile::TempDir,
}

impl Deref for HermeticCru {
    type Target = Command;

    fn deref(&self) -> &Command {
        &self.cmd
    }
}

impl DerefMut for HermeticCru {
    fn deref_mut(&mut self) -> &mut Command {
        &mut self.cmd
    }
}

/// Create a `cru` CLI command that cannot read or write the developer's home.
///
/// `cru chat`, `cru acp` and `cru mcp --stdio` open a log file under
/// `~/.crucible/` before they parse anything else, and every command reads the
/// real config and credential files. `env_clear` plus the allowlist from
/// `hermetic_env_pairs` roots all of that in a temporary directory instead.
#[allow(dead_code)]
pub fn cru() -> HermeticCru {
    let home = tempfile::tempdir().expect("create temp home for cru");
    let mut cmd = cru_bare();
    cmd.env_clear();
    for (key, value) in hermetic_env_pairs(home.path()) {
        cmd.env(key, value);
    }
    // The log path follows `HOME` today. Pin it as well, so a later change to
    // that default cannot send the log back to the developer's home directory.
    cmd.env("CRUCIBLE_LOG_FILE", home.path().join("cru.log"));
    // Keep the daemon socket inside the sandbox too: a daemon that another
    // test leaked on the shared default socket must not answer this child.
    cmd.env("CRUCIBLE_SOCKET", home.path().join("daemon.sock"));
    HermeticCru { cmd, _home: home }
}

/// Escape a path for embedding in a Lua string literal (Windows backslashes).
pub fn path_literal(path: &Path) -> String {
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
#[allow(dead_code)]
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

/// Write a minimal `init.lua` with a correct `kiln_path`, plus whatever Lua
/// the caller appends. Returns the config file path.
///
/// `init.lua` and not `config.toml`: the daemon stopped reading TOML, so a
/// fixture written that way configures nothing and every value in it is a
/// silent default.
pub fn write_config(dir: &Path, extra_lua: &str) -> PathBuf {
    let kiln_path = dir.join("kiln");
    fs::create_dir_all(&kiln_path).expect("create kiln dir");

    let config_path = dir.join("init.lua");
    let config = format!(
        "cru.config.set({{\n  kiln_path = \"{}\",\n  llm = {{ default = \"ollama\", providers = {{ ollama = {{ type = \"ollama\", default_model = \"llama3.2\" }} }} }},\n}})\n{}",
        path_literal(&kiln_path),
        extra_lua,
    );
    fs::write(&config_path, config).expect("write config");
    config_path
}

/// Isolated daemon fixture with RAII cleanup.
pub struct TestDaemon {
    pub socket_path: PathBuf,
    // `common` is compiled into each integration binary separately, so a
    // helper only some of them use reads as dead code in the others.
    #[allow(dead_code)]
    pub config_path: PathBuf,
    _temp_dir: tempfile::TempDir,
    process: Child,
}

impl TestDaemon {
    /// Start an isolated daemon with a minimal config.
    pub fn start() -> Self {
        Self::start_with_extra_config("")
    }

    /// Start an isolated daemon with extra Lua appended to the config.
    pub fn start_with_extra_config(extra_lua: &str) -> Self {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let socket_path = temp_dir.path().join("daemon.sock");
        let config_path = write_config(temp_dir.path(), extra_lua);

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

    /// The hermetic HOME this daemon runs under.
    #[allow(dead_code)]
    pub fn home(&self) -> &Path {
        self._temp_dir.path()
    }

    /// [`Self::command`] without the `--config` argument, for a test that
    /// supplies its own (e.g. the root-mismatch refusal).
    #[allow(dead_code)]
    pub fn command_without_config(&self) -> Command {
        let mut cmd = cru_bare();
        cmd.env_clear();
        for (k, v) in hermetic_env_pairs(self._temp_dir.path()) {
            cmd.env(k, v);
        }
        cmd.env("CRUCIBLE_SOCKET", &self.socket_path);
        cmd
    }

    /// Create a `cru` command pre-wired with CRUCIBLE_SOCKET and --config,
    /// in the same hermetic environment as the daemon (no real credentials).
    #[allow(dead_code)]
    pub fn command(&self) -> Command {
        let mut cmd = cru_bare();
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
        self.data_root().join("sessions")
    }

    /// Where this daemon writes its registries — `kilns.json`, `llm.json`,
    /// `projects.json`. A test that asserts a registration landed has to read
    /// the daemon's file, not the user's config: that separation is the whole
    /// point of those files.
    #[allow(dead_code)]
    pub fn data_root(&self) -> PathBuf {
        self._temp_dir.path().join(".crucible")
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
