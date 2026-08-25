//! `cru config migrate` (T5.5): the one-time TOML → Lua generator, its
//! machine-state split, its abort discipline, and the acceptance flow.

use assert_cmd::Command;
use crucible_core::test_support::hermetic_env_pairs;
use std::path::Path;

/// Create a `cru` CLI command via assert_cmd.
fn cru() -> Command {
    assert_cmd::cargo_bin_cmd!("cru")
}

/// A hermetic `cru` rooted at `home`, socket pinned inside it.
fn hermetic(home: &Path) -> Command {
    let mut cmd = cru();
    cmd.env_clear();
    for (k, v) in hermetic_env_pairs(home) {
        cmd.env(k, v);
    }
    cmd.env("CRUCIBLE_SOCKET", home.join("daemon.sock"));
    cmd
}

fn write_config(home: &Path, body: &str) -> std::path::PathBuf {
    let config_dir = home.join(".config").join("crucible");
    std::fs::create_dir_all(&config_dir).unwrap();
    let path = config_dir.join("config.toml");
    std::fs::write(&path, body).unwrap();
    path
}

/// The split: `auto = true` entries and the machine default land in
/// `kilns.json`; the hand-written entry stays in the emitted Lua; the TOML
/// is renamed and the Lua becomes `init.lua`.
#[test]
fn migrate_splits_auto_kilns_into_state_and_keeps_hand_written_ones() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let config_path = write_config(
        home,
        r#"
default_kiln = "scratch"

[kilns]
notes = "/tmp/migrate-gate/notes"

[kilns.scratch]
path = "/tmp/migrate-gate/scratch"
auto = true
"#,
    );

    hermetic(home)
        .args(["config", "migrate"])
        .assert()
        .success();

    // The TOML retired; the Lua took its place.
    assert!(!config_path.exists(), "config.toml must be renamed away");
    assert!(config_path.with_extension("toml.migrated").exists());
    let init = config_path.parent().unwrap().join("init.lua");
    let lua = std::fs::read_to_string(&init).expect("init.lua written");
    assert!(lua.contains("notes"), "the hand-written kiln stays: {lua}");
    assert!(
        !lua.contains("scratch"),
        "the auto kiln must not stay in the Lua: {lua}"
    );

    // kilns.json holds exactly the auto entry, and the machine default
    // moved with it.
    let state: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(home.join(".crucible").join("kilns.json"))
            .expect("kilns.json written"),
    )
    .unwrap();
    let kilns = state["kilns"].as_object().unwrap();
    assert_eq!(
        kilns.keys().collect::<Vec<_>>(),
        vec!["scratch"],
        "exactly the auto entries: {state}"
    );
    assert_eq!(kilns["scratch"]["path"], "/tmp/migrate-gate/scratch");
    assert_eq!(kilns["scratch"]["auto"], true);
    assert_eq!(state["default_kiln"], "scratch");
}

/// An existing `init.lua` is never edited: the chunk becomes a module and
/// the command prints the one line to add.
#[test]
fn migrate_never_edits_an_existing_init_lua() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let config_path = write_config(home, "default_kiln = \"notes\"\n[kilns]\nnotes = \"/n\"\n");
    let init = config_path.parent().unwrap().join("init.lua");
    let user_body = "-- the user's own file\n";
    std::fs::write(&init, user_body).unwrap();

    let assert = hermetic(home)
        .args(["config", "migrate"])
        .assert()
        .success();

    assert_eq!(
        std::fs::read_to_string(&init).unwrap(),
        user_body,
        "the user's init.lua must not change"
    );
    let module = config_path
        .parent()
        .unwrap()
        .join("lua")
        .join("migrated_config.lua");
    assert!(module.exists(), "the chunk becomes a module instead");
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(
        stdout.contains("require(\"migrated_config\")"),
        "the instruction names the require line: {stdout}"
    );
}

/// A name `kilns.json` already points elsewhere is a refusal, and the
/// refusal leaves NOTHING behind: no Lua file, the TOML untouched.
#[test]
fn a_repoint_refusal_leaves_no_file_behind() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let config_path = write_config(
        home,
        "[kilns.scratch]\npath = \"/tmp/migrate-gate/new-place\"\nauto = true\n",
    );

    // Pre-seed the state with the same name at a DIFFERENT path.
    let data_home = home.join(".crucible");
    std::fs::create_dir_all(&data_home).unwrap();
    std::fs::write(
        data_home.join("kilns.json"),
        r#"{"version":1,"kilns":{"scratch":{"path":"/tmp/migrate-gate/old-place","auto":true,"registered_at":"2026-01-01T00:00:00Z"}}}"#,
    )
    .unwrap();

    let assert = hermetic(home)
        .args(["config", "migrate"])
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(stderr.contains("scratch"), "{stderr}");
    assert!(stderr.contains("re-point"), "{stderr}");
    assert!(config_path.exists(), "the TOML must be untouched");
    assert!(
        !config_path.parent().unwrap().join("init.lua").exists(),
        "no Lua file may be left behind"
    );
}

/// The acceptance flow: migrate, then `cru kiln register` against a daemon
/// booted from the migrated config, then the name resolves in the listing —
/// green by construction under the M4-first order (registration is additive
/// daemon state).
#[test]
fn migrate_then_register_then_the_name_resolves() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let kiln_dir = home.join("kilns").join("fresh");
    std::fs::create_dir_all(&kiln_dir).unwrap();
    write_config(
        home,
        "[kilns.migrated]\npath = \"/tmp/migrate-gate/migrated\"\nauto = true\n",
    );

    hermetic(home)
        .args(["config", "migrate"])
        .assert()
        .success();

    // A real daemon on the migrated root.
    let socket = home.join("daemon.sock");
    let mut daemon = std::process::Command::new(env!("CARGO_BIN_EXE_cru"));
    daemon.env_clear();
    for (k, v) in hermetic_env_pairs(home) {
        daemon.env(k, v);
    }
    let daemon = daemon
        .env("CRUCIBLE_SOCKET", &socket)
        .args(["daemon", "serve"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn daemon");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !socket.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    // RAII cleanup: the daemon dies even when an assertion below panics.
    struct KillOnDrop(std::process::Child);
    impl Drop for KillOnDrop {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let _daemon = KillOnDrop(daemon);

    hermetic(home)
        .args(["kiln", "register", "fresh"])
        .arg(&kiln_dir)
        .assert()
        .success();

    let listing = hermetic(home).args(["kiln", "list"]).assert().success();
    let stdout = String::from_utf8_lossy(&listing.get_output().stdout).to_string();
    assert!(stdout.contains("fresh"), "the new name resolves: {stdout}");
    assert!(
        stdout.contains("migrated"),
        "the migrated auto kiln resolves too: {stdout}"
    );
}
