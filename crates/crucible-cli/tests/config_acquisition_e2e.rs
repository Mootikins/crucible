//! Per-command config acquisition (T5.3): a bootstrap command completes
//! with no daemon and spawns none; a daemon-backed command against a daemon
//! on a DIFFERENT config root is refused, naming both roots.

mod cli_e2e_helpers;

use cli_e2e_helpers::*;
use crucible_core::test_support::hermetic_env_pairs;
use std::fs;

/// Run `cru` hermetically under `home`, with the daemon socket pinned inside
/// it, and return the socket path — so "no daemon spawned" is observable.
fn hermetic_cru(home: &std::path::Path) -> (assert_cmd::Command, std::path::PathBuf) {
    let socket = home.join("daemon.sock");
    let mut cmd = cru();
    cmd.env_clear();
    for (k, v) in hermetic_env_pairs(home) {
        cmd.env(k, v);
    }
    cmd.env("CRUCIBLE_SOCKET", &socket);
    (cmd, socket)
}

/// `cru daemon status` needs neither a daemon nor a config VM: it completes
/// with no daemon running and starts none.
#[test]
fn daemon_status_completes_with_no_daemon_and_spawns_none() {
    let temp = tempfile::tempdir().unwrap();
    let (mut cmd, socket) = hermetic_cru(temp.path());

    cmd.args(["daemon", "status"]).assert().success();

    assert!(
        !socket.exists(),
        "`cru daemon status` must not spawn a daemon"
    );
}

/// `cru config init` writes the example file locally: no daemon, none spawned.
#[test]
fn config_init_completes_with_no_daemon_and_spawns_none() {
    let temp = tempfile::tempdir().unwrap();
    let (mut cmd, socket) = hermetic_cru(temp.path());
    let target = temp.path().join("init.lua");

    cmd.args(["config", "init", "--path"])
        .arg(&target)
        .assert()
        .success();

    assert!(target.exists(), "config init must write the example file");
    assert!(
        !socket.exists(),
        "`cru config init` must not spawn a daemon"
    );
}

/// A daemon-backed command whose resolved config root differs from the
/// running daemon's is refused, naming both roots and the remedy.
#[test]
fn a_daemon_on_a_different_config_root_is_refused() {
    let daemon = TestDaemon::start();

    // A second config root inside the same hermetic home.
    let other_root = daemon.home().join("other-root");
    fs::create_dir_all(&other_root).unwrap();
    let other_config = other_root.join("config.toml");
    fs::write(&other_config, "").unwrap();

    let mut cmd = daemon.command_without_config();
    cmd.arg("--config")
        .arg(&other_config)
        .args(["models", "--format", "json"]);

    let assert = cmd.assert().failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains("config root"),
        "the refusal must name the mismatch: {stderr}"
    );
    assert!(
        stderr.contains("other-root"),
        "the refusal must name the invocation's root: {stderr}"
    );
    assert!(
        stderr.contains("cru daemon restart"),
        "the refusal must name the remedy: {stderr}"
    );
}
