//! Tests for `cru chat --replay` flag-combination validation (Task 2.1).
//!
//! These tests invoke the `cru` binary via `assert_cmd` and verify that
//! `--replay` combined with any incompatible flag exits non-zero with a
//! clear error message. The flag-validation check runs at the top of
//! `chat::execute`, before any daemon/replay work, so these tests do not
//! require a running daemon.

use assert_cmd::Command;
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

#[test]
fn replay_with_query_errors() {
    let (_tmpdir, fake_replay) = make_fake_replay();

    let mut cmd = Command::cargo_bin("cru").unwrap();
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
    let (_tmpdir, fake_replay) = make_fake_replay();
    let tmpdir2 = TempDir::new().unwrap();
    let record_path = tmpdir2.path().join("record.jsonl");

    let mut cmd = Command::cargo_bin("cru").unwrap();
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
    let (_tmpdir, fake_replay) = make_fake_replay();

    let mut cmd = Command::cargo_bin("cru").unwrap();
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
    let (_tmpdir, fake_replay) = make_fake_replay();

    let mut cmd = Command::cargo_bin("cru").unwrap();
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
    // Pick a path that almost certainly does not exist.
    let missing = std::env::temp_dir().join("crucible-replay-does-not-exist-7f3a9b2c.jsonl");
    // Ensure it is absent (in case a prior run somehow created it).
    let _ = std::fs::remove_file(&missing);

    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("chat").arg("--replay").arg(&missing);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("replay file not found"));
}
