//! `init.lua` is the config, and `config.toml` is not read.
//!
//! `config.toml` was the seed layer under `init.lua` for one release. The
//! v0.30.0 daemon warned at every boot that the file was deprecated and
//! named `cru config migrate`; this is the release where the reader goes.
//! `cru config migrate` still reads the file — that is its whole job — so
//! the parse path stays. Nothing else reads it.
//!
//! Two properties, and each has been a defect in some product that dropped a
//! format: the file must stop SETTING anything (a value that still applies is
//! a migration nobody finishes), and it must stop BREAKING anything (a file
//! the daemon no longer needs must not be able to refuse the boot).

use crucible_daemon::daemon_plugins::{
    boot_input_hash, evaluate_boot_config_with_paths, BootConfig, PluginPathsFn,
};
use crucible_lua::PluginSource;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A config directory holding whatever the case needs, and nothing the
/// developer's own machine supplies.
fn write_config(tmp: &Path, files: &[(&str, &str)]) -> PathBuf {
    let config_dir = tmp.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    for (name, body) in files {
        std::fs::write(config_dir.join(name), body).unwrap();
    }
    config_dir
}

/// Boot against the fixture. The plugin search is injected as a value, so no
/// test here can reach the developer's real plugin directories.
async fn boot(config_dir: &Path) -> anyhow::Result<BootConfig> {
    let paths: PluginPathsFn = Arc::new(|_rtp: &[PathBuf]| Vec::<(PathBuf, PluginSource)>::new());
    evaluate_boot_config_with_paths(Some(config_dir.join("config.toml")), None, None, paths).await
}

/// The file sets nothing. A key it names, that `init.lua` does not, holds
/// its default.
#[tokio::test]
async fn a_config_toml_key_does_not_reach_the_effective_config() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = write_config(
        tmp.path(),
        &[
            ("config.toml", "default_kiln = \"from-toml\"\n"),
            ("init.lua", "-- the human's file sets nothing\n"),
        ],
    );

    let booted = boot(&config_dir).await.expect("the boot must succeed");

    assert_eq!(
        booted.config.default_kiln, None,
        "config.toml is not a config source"
    );
}

/// A file nothing reads cannot refuse the boot. Before the reader went, this
/// TOML failed to parse and the daemon stopped.
#[tokio::test]
async fn a_malformed_config_toml_does_not_stop_the_boot() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = write_config(
        tmp.path(),
        &[
            ("config.toml", "this is not = = toml\n"),
            ("init.lua", "cru.config.set({ default_kiln = \"mine\" })\n"),
        ],
    );

    let booted = match boot(&config_dir).await {
        Ok(booted) => booted,
        Err(e) => panic!("a file the daemon does not read must not stop it: {e:#}"),
    };

    assert_eq!(booted.config.default_kiln.as_deref(), Some("mine"));
}

/// The boot hash answers "restart to apply". Editing a file the boot does not
/// read is not a reason to restart.
#[test]
fn the_boot_hash_ignores_config_toml() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = write_config(
        tmp.path(),
        &[("config.toml", "default_kiln = \"a\"\n"), ("init.lua", "")],
    );
    let before = boot_input_hash(&config_dir.join("config.toml"));

    std::fs::write(config_dir.join("config.toml"), "default_kiln = \"b\"\n").unwrap();

    assert_eq!(
        before,
        boot_input_hash(&config_dir.join("config.toml")),
        "config.toml is not a boot input"
    );
}

/// `settings.json` IS a boot input: `load_settings_layer` reads it at every
/// boot, and `settings_file.rs` says a hand edit survives. A hand edit that
/// did not move the hash left `cru doctor` calling a stale daemon current.
#[test]
fn the_boot_hash_covers_settings_json() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = write_config(tmp.path(), &[("init.lua", "")]);
    let source = config_dir.join("config.toml");

    let absent = boot_input_hash(&source);

    std::fs::write(
        config_dir.join("settings.json"),
        "{ \"default_kiln\": \"a\" }\n",
    )
    .unwrap();
    let created = boot_input_hash(&source);
    assert_ne!(
        absent, created,
        "creating settings.json changes what the boot reads"
    );

    std::fs::write(
        config_dir.join("settings.json"),
        "{ \"default_kiln\": \"b\" }\n",
    )
    .unwrap();
    assert_ne!(
        created,
        boot_input_hash(&source),
        "editing settings.json changes what the boot reads"
    );
}
