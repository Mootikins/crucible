//! `cru init` registers with the daemon, not with the user's config file.
//!
//! This crosses the process boundary on purpose. The daemon-side handlers have
//! their own tests and the bind-time overlay has its own; what nothing else
//! proves is that a real `cru init`, run as a separate process against a real
//! daemon over a real socket, puts the registration in the daemon's state file.
//! That is the whole claim of T4.3 for the kiln half.

mod cli_e2e_helpers;

use cli_e2e_helpers::TestDaemon;

/// The kiln lands in `<data_home>/kilns.json`, and the user's config is not
/// touched.
///
/// The negative half matters as much as the positive one. `cru init` used to
/// write a `[kilns]` entry into the config file, and the failure mode of a
/// half-done migration is BOTH writers running — which looks fine until a
/// config reload drops one of them.
#[test]
fn init_registers_the_kiln_with_the_daemon_and_leaves_the_config_alone() {
    let daemon = TestDaemon::start();
    // Outside the daemon's temp dir: that directory holds the daemon's own
    // `.crucible` data root, so `cru init` reads anything under it as being
    // inside an existing kiln and refuses.
    let workspace = tempfile::tempdir().expect("workspace");
    let kiln_dir = workspace.path().join("notes-kiln");
    std::fs::create_dir_all(&kiln_dir).expect("kiln dir");

    let config_before =
        std::fs::read_to_string(&daemon.config_path).expect("the fixture wrote a config");

    let output = daemon
        .command()
        .args(["init", "-p", kiln_dir.to_str().unwrap(), "-y"])
        .output()
        .expect("run cru init");
    assert!(
        output.status.success(),
        "cru init failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let state_file = daemon.data_root().join("kilns.json");
    let state = std::fs::read_to_string(&state_file).unwrap_or_else(|e| {
        panic!(
            "kilns.json must exist at {}: {e}\nstdout: {}",
            state_file.display(),
            String::from_utf8_lossy(&output.stdout)
        )
    });
    let parsed: serde_json::Value = serde_json::from_str(&state).expect("kilns.json is JSON");
    assert_eq!(parsed["version"], 1);
    let registered = parsed["kilns"]
        .as_object()
        .expect("a kilns table")
        .values()
        .any(|entry| {
            entry["path"]
                .as_str()
                .is_some_and(|p| p.ends_with("notes-kiln"))
        });
    assert!(registered, "the kiln must be registered: {state}");

    assert_eq!(
        std::fs::read_to_string(&daemon.config_path).expect("config still readable"),
        config_before,
        "cru init must not write the user's config file any more"
    );
}
