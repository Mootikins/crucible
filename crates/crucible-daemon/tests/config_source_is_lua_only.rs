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

use crucible_core::test_support::EnvVarGuard;
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

/// Boot against the fixture. The plugin search is injected as a value, and
/// the runtime tree is pinned to this checkout, so no test here reads the
/// developer's real plugin directories or a tree a release install left on
/// the machine.
async fn boot(config_dir: &Path) -> anyhow::Result<BootConfig> {
    let _pin = pin_the_runtime_to_this_repository();
    boot_with_the_machines_runtime(config_dir).await
}

/// [`boot`] WITHOUT the runtime pin: the shipped defaults come from whatever
/// tree the machine resolves, an installed one included.
///
/// Only [`an_installed_defaults_file_does_not_reach_the_effective_config`]
/// uses this, and it uses it to prove that the pin above has work to do.
async fn boot_with_the_machines_runtime(config_dir: &Path) -> anyhow::Result<BootConfig> {
    let paths: PluginPathsFn = Arc::new(|_rtp: &[PathBuf]| Vec::<(PathBuf, PluginSource)>::new());
    evaluate_boot_config_with_paths(Some(config_dir.join("config.toml")), None, None, paths).await
}

/// This repository's `runtime/` tree — the defaults this build ships.
///
/// A test binary runs from `target/debug/deps`, so both exe-relative roots
/// `runtime_roots::for_current_exe` builds are absent and resolution falls
/// through to what a RELEASE INSTALL left in the user's directories. Those
/// files belong to another build.
fn repo_runtime_root() -> PathBuf {
    let root = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../runtime"));
    assert!(
        root.join("defaults").join("init.luau").is_file(),
        "the repository runtime tree must hold the shipped defaults: {}",
        root.display()
    );
    root
}

/// Pin the defaults resolution to this repository while the guard lives.
///
/// `$CRUCIBLE_RUNTIME` outranks every root the resolver discovers. Hold the
/// guard across the boot: the defaults file is read during the boot.
#[must_use]
fn pin_the_runtime_to_this_repository() -> EnvVarGuard {
    EnvVarGuard::set(
        "CRUCIBLE_RUNTIME",
        repo_runtime_root().display().to_string(),
    )
}

/// Plant a runtime tree at the roots an INSTALLED Crucible owns, and say
/// which `default_kiln` its defaults file sets.
///
/// Both roots come from `dirs`, so the guards redirect them under `home` and
/// nothing touches the developer's own directories.
fn plant_an_installed_runtime(home: &Path) -> (&'static str, Vec<EnvVarGuard>) {
    const KILN: &str = "from-an-installed-tree";
    let guards = vec![
        // An exported value on the developer's machine would outrank the
        // planted tree and make the test prove nothing.
        EnvVarGuard::remove("CRUCIBLE_RUNTIME"),
        EnvVarGuard::set("XDG_CONFIG_HOME", home.join("config").display().to_string()),
        EnvVarGuard::set("XDG_DATA_HOME", home.join("data").display().to_string()),
    ];

    let roots = [
        crucible_core::runtime_roots::user_runtime(),
        crucible_core::runtime_roots::bundled_runtime_dir(),
    ];
    for root in roots.into_iter().flatten() {
        let defaults = root.join("defaults");
        std::fs::create_dir_all(&defaults).unwrap();
        std::fs::write(
            defaults.join("init.luau"),
            format!("cru.config.set({{ default_kiln = \"{KILN}\" }})\n"),
        )
        .unwrap();
    }
    (KILN, guards)
}

/// A tree a release install left on the machine must set nothing here.
///
/// The file every boot reads before `init.lua` is `defaults/init.luau`, and
/// resolution finds it off the running binary. A test binary resolves no tree
/// of its own, so an installed one used to answer — and a value it set looked
/// exactly like a value this suite is about.
///
/// Both halves matter. The first proves the planted tree is a root the
/// resolver reads, so the second cannot pass for the wrong reason.
#[tokio::test]
async fn an_installed_defaults_file_does_not_reach_the_effective_config() {
    let tmp = tempfile::tempdir().unwrap();
    let (planted_kiln, _guards) = plant_an_installed_runtime(tmp.path());
    let config_dir = write_config(
        tmp.path(),
        &[("init.lua", "-- the human's file sets nothing\n")],
    );

    let ambient = boot_with_the_machines_runtime(&config_dir)
        .await
        .expect("the boot must succeed");
    assert_eq!(
        ambient.config.default_kiln.as_deref(),
        Some(planted_kiln),
        "precondition: the planted tree must be a root the resolver reads"
    );

    let booted = boot(&config_dir).await.expect("the boot must succeed");

    assert_eq!(
        booted.config.default_kiln, None,
        "the pinned boot must read this repository's defaults, not an \
         installed tree's"
    );
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
