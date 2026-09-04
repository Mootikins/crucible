//! Tests for `cru chat --replay` flag-combination validation (Task 2.1).
//!
//! These tests invoke the `cru` binary via `assert_cmd` and verify that
//! `--replay` combined with any incompatible flag exits non-zero with a
//! clear error message. The flag-validation check runs at the top of
//! `chat::execute`, before any daemon/replay work, so these tests do not
//! require a running daemon.

use assert_cmd::Command;
use crucible_core::test_support::hermetic_env_pairs;
use predicates::prelude::*;
use tempfile::TempDir;

/// Creates an empty temp file to pass as `--replay`. The file-parsing step
/// never runs because flag validation fails first, so empty content is fine.
fn make_fake_replay() -> (TempDir, std::path::PathBuf) {
    let tmpdir = TempDir::new().unwrap();
    let fake_replay = tmpdir.path().join("test.jsonl");
    std::fs::write(&fake_replay, "").unwrap();
    (tmpdir, fake_replay)
}

/// A `cru` command with every home directory rooted at `home`.
///
/// `cru chat` opens a log file before it validates the flags, and the default
/// path of that file is the developer's `~/.crucible/chat.log`. Without this
/// environment each of the five tests below appends to that real file. The
/// caller must keep `home` alive until the child process ends.
fn hermetic_cru(home: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.env_clear();
    for (key, value) in hermetic_env_pairs(home) {
        cmd.env(key, value);
    }
    // The log path follows `HOME` today. Pin it as well, so a later change to
    // that default cannot send the log back to the developer's home directory.
    cmd.env("CRUCIBLE_LOG_FILE", home.join("chat.log"));
    // Keep the daemon socket inside the sandbox too: a daemon that another
    // test leaked on the shared default socket must not answer this child.
    cmd.env("CRUCIBLE_SOCKET", home.join("daemon.sock"));
    cmd
}

#[test]
fn replay_with_query_errors() {
    let (tmpdir, fake_replay) = make_fake_replay();

    let mut cmd = hermetic_cru(tmpdir.path());
    cmd.arg("chat")
        .arg("--replay")
        .arg(&fake_replay)
        .arg("some query text");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("cannot be combined with a query"));
}

#[test]
fn replay_with_record_errors() {
    let (tmpdir, fake_replay) = make_fake_replay();
    let tmpdir2 = TempDir::new().unwrap();
    let record_path = tmpdir2.path().join("record.jsonl");

    let mut cmd = hermetic_cru(tmpdir.path());
    cmd.arg("chat")
        .arg("--replay")
        .arg(&fake_replay)
        .arg("--record")
        .arg(&record_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("cannot be combined with --record"));
}

#[test]
fn replay_with_resume_errors() {
    let (tmpdir, fake_replay) = make_fake_replay();

    let mut cmd = hermetic_cru(tmpdir.path());
    cmd.arg("chat")
        .arg("--replay")
        .arg(&fake_replay)
        .arg("--resume")
        .arg("some-session-id");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("cannot be combined with --resume"));
}

#[test]
fn replay_with_agent_errors() {
    let (tmpdir, fake_replay) = make_fake_replay();

    let mut cmd = hermetic_cru(tmpdir.path());
    cmd.arg("chat")
        .arg("--replay")
        .arg(&fake_replay)
        .arg("--agent")
        .arg("claude");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("cannot be combined with --agent"));
}

#[test]
fn replay_with_nonexistent_file_errors() {
    let tmpdir = TempDir::new().unwrap();
    // A path inside the sandbox that nothing creates.
    let missing = tmpdir.path().join("crucible-replay-does-not-exist.jsonl");

    let mut cmd = hermetic_cru(tmpdir.path());
    cmd.arg("chat").arg("--replay").arg(&missing);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("replay file not found"));
}
