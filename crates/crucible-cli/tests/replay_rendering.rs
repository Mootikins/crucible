//! Regression test: replay of the demo fixture renders cleanly — no RPC error
//! strings, no sanitized-error strings, no warning badge.
//!
//! Plan success-criterion 3. The replay must not surface any daemon-communication
//! errors or warning badges in its rendered output.
//!
//! This test spawns `cru` in a PTY (via expectrl) because the TUI enables
//! crossterm raw mode on startup, which requires a controlling terminal.

use std::io::Read;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use expectrl::session::Session;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/fixtures/demo.jsonl")
}

/// Drains all PTY output until EOF or the deadline passes.
///
/// Returns whatever bytes were produced. If the child is still running when the
/// deadline expires, we give up reading; the ending `--replay-auto-exit` should
/// mean this never happens in practice, but we cap at a generous bound.
fn drain_until_done<S: Read>(stream: &mut S, deadline: Instant) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 * 1024);
    let mut buf = [0u8; 8192];
    while Instant::now() < deadline {
        match stream.read(&mut buf) {
            Ok(0) => break, // EOF
            Ok(n) => out.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => break,
        }
    }
    out
}

#[test]
#[cfg_attr(not(unix), ignore = "PTY-based test requires a Unix host")]
fn demo_fixture_renders_without_rpc_error() {
    let fixture = fixture_path();
    if !fixture.exists() {
        eprintln!("fixture missing: {} — skipping", fixture.display());
        return;
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let fake_sock = tmp.path().join("crucible.sock");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cru"));
    // Hermetic child env: no real credentials or config reach the PTY child.
    cmd.env_clear();
    for (k, v) in crucible_core::test_support::hermetic_env_pairs(tmp.path()) {
        cmd.env(k, v);
    }
    cmd.args(["chat", "--replay"])
        .arg(&fixture)
        .args(["--replay-speed", "100", "--replay-auto-exit", "200"])
        .env("CRUCIBLE_SOCKET", &fake_sock)
        .env("TERM", "xterm-256color")
        .env("CI", "1");

    let mut session = match Session::spawn(cmd) {
        Ok(s) => s,
        Err(e) => {
            // If the PTY layer is unavailable (e.g. sandbox), skip rather than fail.
            eprintln!("could not spawn PTY session: {e} — skipping");
            return;
        }
    };

    // Auto-exit is 200ms after replay completes; the whole drain should finish
    // well under 10s. We use that as a hard ceiling.
    let deadline = Instant::now() + Duration::from_secs(10);
    let bytes = drain_until_done(&mut session, deadline);
    let output = String::from_utf8_lossy(&bytes).into_owned();

    assert!(
        !output.contains("Communication error"),
        "RPC 'Communication error' leaked into replay output:\n{output}"
    );
    assert!(
        !output.contains("Internal server error"),
        "'Internal server error' leaked into replay output:\n{output}"
    );
    assert!(
        !output.contains("WARN 1"),
        "warning badge 'WARN 1' rendered during replay:\n{output}"
    );
}
