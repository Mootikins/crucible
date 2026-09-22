//! The boot's failure rule for the user's config.
//!
//! `init.lua` is the only config language a human writes, so the two ways it
//! can fail are not one failure. A file that does not PARSE states no intent:
//! the daemon refuses to start and names the line. A file that parses and
//! then RAISES states an intent that ran part way: the boot rolls the whole
//! state back, warns, and continues on the seed.
//!
//! The rule is not one level deep. A module under the user's own `lua/`
//! directory is config too, and a syntax error in it is fatal for the same
//! reason. A plugin's file is not config — the user did not write it and
//! cannot fix its line — so a plugin that does not parse stays fail-open.

use crucible_core::test_support::EnvVarGuard;
use crucible_daemon::daemon_plugins::{evaluate_boot_config_with_paths, BootConfig, PluginPathsFn};
use crucible_lua::PluginSource;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A config directory holding `settings.json` and `init.lua`, and nothing
/// the developer's own machine supplies.
///
/// `settings.json` is the seed: the layer below `init.lua`, which is what a
/// rollback lands on. `serde_json::Value::Null` writes no file.
fn write_config(tmp: &Path, settings: serde_json::Value, init_lua: &str) -> PathBuf {
    let config_dir = tmp.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    if !settings.is_null() {
        std::fs::write(
            config_dir.join("settings.json"),
            serde_json::to_string_pretty(&settings).unwrap(),
        )
        .unwrap();
    }
    std::fs::write(config_dir.join("init.lua"), init_lua).unwrap();
    config_dir
}

/// Boot against the fixture. The plugin search is injected as a value, and
/// the runtime tree is pinned to this checkout, so no test here reads the
/// developer's real plugin directories or a tree a release install left on
/// the machine.
async fn boot(config_dir: &Path, plugin_root: Option<PathBuf>) -> anyhow::Result<BootConfig> {
    let _pin = pin_the_runtime_to_this_repository();
    boot_with_the_machines_runtime(config_dir, plugin_root).await
}

/// [`boot`] WITHOUT the runtime pin: the shipped defaults come from whatever
/// tree the machine resolves, an installed one included.
///
/// Only [`an_installed_defaults_file_does_not_survive_the_rollback`] uses
/// this, and it uses it to prove that the pin above has work to do.
async fn boot_with_the_machines_runtime(
    config_dir: &Path,
    plugin_root: Option<PathBuf>,
) -> anyhow::Result<BootConfig> {
    let paths: PluginPathsFn = Arc::new(move |_rtp: &[PathBuf]| match &plugin_root {
        Some(root) => vec![(root.clone(), PluginSource::EnvPath)],
        None => Vec::new(),
    });
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

/// What a rollback lands on is the seed, and a tree a release install left
/// on the machine is not part of it.
///
/// The defaults file runs before `init.lua` in the same VM, so a value it
/// sets outlives the rollback and reads exactly like a seeded value. A test
/// binary resolves no runtime tree of its own, so an installed one used to
/// supply that file.
///
/// Both halves matter. The first proves the planted tree is a root the
/// resolver reads, so the second cannot pass for the wrong reason.
#[tokio::test]
async fn an_installed_defaults_file_does_not_survive_the_rollback() {
    let tmp = tempfile::tempdir().unwrap();
    let (planted_kiln, _guards) = plant_an_installed_runtime(tmp.path());
    let config_dir = write_config(
        tmp.path(),
        serde_json::Value::Null,
        "cru.config.set({ default_kiln = \"from-init\" })\nerror(\"boom\")\n",
    );

    let ambient = boot_with_the_machines_runtime(&config_dir, None)
        .await
        .expect("a config file that raises must still boot");
    assert_eq!(
        ambient.config.default_kiln.as_deref(),
        Some(planted_kiln),
        "precondition: the planted tree must be a root the resolver reads"
    );

    let booted = boot(&config_dir, None)
        .await
        .expect("a config file that raises must still boot");

    assert_eq!(
        booted.config.default_kiln, None,
        "the seed a rollback lands on must come from this repository's \
         defaults, not from an installed tree"
    );
}

/// A file that does not parse states no intent, so the boot stops and says
/// which line to fix.
#[tokio::test]
async fn a_syntax_error_in_init_lua_refuses_the_boot_and_names_the_line() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = write_config(
        tmp.path(),
        serde_json::json!({ "default_kiln": "seeded" }),
        "cru.config.set({ default_kiln = \"from-init\" })\nthis is not lua (\n",
    );

    let Err(error) = boot(&config_dir, None).await else {
        panic!("a config file that does not parse must not boot");
    };

    let message = format!("{error:#}");
    assert!(
        message.contains("init.lua:2"),
        "the refusal must name the file and the line: {message}"
    );
}

/// A file that parses and then raises ran part way. The boot rolls the WHOLE
/// state back — not "the seed plus whatever ran before the error line" — and
/// continues, so a broken config means exactly what the warning says.
#[tokio::test]
async fn a_runtime_error_in_init_lua_rolls_the_state_back_and_boots() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = write_config(
        tmp.path(),
        serde_json::json!({ "default_kiln": "seeded" }),
        "cru.config.set({ default_kiln = \"from-init\" })\nerror(\"boom\")\n",
    );

    let booted = match boot(&config_dir, None).await {
        Ok(booted) => booted,
        Err(e) => panic!("a config file that raises must still boot: {e:#}"),
    };

    assert_eq!(
        booted.config.default_kiln.as_deref(),
        Some("seeded"),
        "the write before the error line must roll back with everything else"
    );
    assert!(
        booted.eval_error.is_some(),
        "the boot must report the failure it continued past"
    );
}

/// The rule is not one level deep. A file `init.lua` loads through the host
/// arrives as a runtime error one level up, so the classification happens at
/// the load: a required config module that does not parse is fatal too.
#[tokio::test]
async fn a_syntax_error_in_a_required_config_module_refuses_the_boot() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = write_config(
        tmp.path(),
        serde_json::json!({ "default_kiln": "seeded" }),
        "require(\"keymaps\")\n",
    );
    let user_lua = config_dir.join("lua");
    std::fs::create_dir_all(&user_lua).unwrap();
    std::fs::write(user_lua.join("keymaps.lua"), "local x =\n").unwrap();

    let Err(error) = boot(&config_dir, None).await else {
        panic!("a config module that does not parse must not boot");
    };

    let message = format!("{error:#}");
    assert!(
        message.contains("keymaps.lua:"),
        "the refusal must name the included file and its line: {message}"
    );
}

/// A plugin is not the user's config: the user did not write it and cannot
/// fix its line. A plugin module that does not parse stays fail-open, so the
/// classification must not follow every syntax error it can reach.
#[tokio::test]
async fn a_syntax_error_in_a_plugin_module_still_boots() {
    let tmp = tempfile::tempdir().unwrap();
    let plugin_root = tmp.path().join("plugins");
    let plugin_dir = plugin_root.join("broken");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(plugin_dir.join("init.lua"), "return {}\n").unwrap();
    std::fs::write(plugin_dir.join("sub.lua"), "local x =\n").unwrap();

    let config_dir = write_config(
        tmp.path(),
        serde_json::json!({ "default_kiln": "seeded" }),
        "require(\"broken.sub\")\n",
    );

    let booted = match boot(&config_dir, Some(plugin_root)).await {
        Ok(booted) => booted,
        Err(e) => panic!("a plugin that does not parse must not stop the daemon: {e:#}"),
    };

    assert!(
        booted.eval_error.is_some(),
        "the boot must still report the plugin failure it continued past"
    );
}
