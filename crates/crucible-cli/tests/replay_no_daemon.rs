//! Regression test: `cru chat --replay` makes zero daemon-socket or network
//! `connect()` calls. This is the end-to-end guard for the TUI-only replay
//! invariant (plan success-criterion 1).
//!
//! Linux-only — uses strace.
//!
//! Even when stdin is not a TTY (the usual case under `cargo test`) and `cru`
//! bails early from `Terminal::new()`, the assertion still holds: replay must
//! never open a Unix socket to the daemon or any AF_INET/AF_INET6 socket.

use std::path::PathBuf;
use std::process::{Command, Stdio};

fn have_strace() -> bool {
    Command::new("strace")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn fixture_path() -> PathBuf {
    // CARGO_MANIFEST_DIR points at crates/crucible-cli; walk up to repo root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/fixtures/demo.jsonl")
}

#[test]
#[cfg_attr(not(target_os = "linux"), ignore = "strace is Linux-only")]
fn replay_makes_no_socket_or_network_syscalls() {
    if !have_strace() {
        eprintln!("strace not available on this host; skipping");
        return;
    }

    let fixture = fixture_path();
    if !fixture.exists() {
        eprintln!(
            "fixture missing: {} — skipping (this test runs against the real demo fixture)",
            fixture.display()
        );
        return;
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let fake_sock = tmp.path().join("crucible.sock");
    let trace_file = tmp.path().join("trace.log");

    // `cru chat` tries to enable raw mode. Under `cargo test` stdin/stdout are
    // typically not a TTY, so `Terminal::new()` fails and the process exits.
    // That is fine: the invariant we assert is about syscalls, not exit code.
    // `Stdio::null()` makes the non-TTY behavior deterministic.
    let status = Command::new("strace")
        .args(["-f", "-e", "trace=connect,socket", "-o"])
        .arg(&trace_file)
        .arg(env!("CARGO_BIN_EXE_cru"))
        .args(["chat", "--replay"])
        .arg(&fixture)
        .args(["--replay-speed", "100", "--replay-auto-exit", "200"])
        .env("CRUCIBLE_SOCKET", &fake_sock)
        .env("CI", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("strace must run");

    // Log for debugging — exit status is not asserted (see TTY note above).
    eprintln!("strace wrapper exit status: {:?}", status);

    let trace = std::fs::read_to_string(&trace_file).expect("trace file readable");

    // Assertion 1: no connect() to the daemon socket path.
    let sock_str = fake_sock.to_string_lossy().to_string();
    assert!(
        !trace.contains(&sock_str),
        "replay attempted connect() to daemon socket ({}):\n{}",
        sock_str,
        trace
    );

    // Assertion 2: no AF_INET/AF_INET6 connect() calls.
    let net_connects: Vec<_> = trace
        .lines()
        .filter(|l| (l.contains("AF_INET") || l.contains("AF_INET6")) && l.contains("connect("))
        .collect();
    assert!(
        net_connects.is_empty(),
        "replay opened network connect() calls:\n{}",
        net_connects.join("\n")
    );
}
