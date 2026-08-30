//! Plugin configuration: `setup()` wiring and the shipped plugins' config layer.
//!
//! Two mechanisms have to agree for plugin config to work:
//!
//! 1. `[plugins.<name>]` from config.toml reaches the plugin runtime as
//!    `cru.plugin.config.get("<name>.<key>")`, and is handed to the plugin's
//!    `setup()` function at load time.
//! 2. The plugin's own config module resolves in a defined order (Lua beats
//!    TOML): `setup()` → explicit TOML → declared defaults → caller fallback.
//!
//! These tests pin both, plus the module-cache isolation that keeps two
//! plugins with a same-named local module (`config.lua`) from sharing one.

use crucible_lua::PluginSource;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Shipped-plugin config precedence (pure Lua, no daemon)
// ---------------------------------------------------------------------------

fn plugins_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("runtime")
        .join("plugins")
}

/// Load a shipped plugin's `lua/config.lua` with a stand-in for the daemon's
/// `cru.plugin.config.get("<plugin>.<key>")` backed by `toml`.
fn shipped_config_module(plugin: &str, toml: serde_json::Value) -> (mlua::Lua, mlua::Table) {
    use mlua::LuaSerdeExt;

    let lua = mlua::Lua::new();
    let crucible = lua.create_table().unwrap();
    let config = lua.create_table().unwrap();
    let get = lua
        .create_function(move |lua, key: String| {
            let Some((ns, sub)) = key.split_once('.') else {
                return Ok(mlua::Value::Nil);
            };
            match toml.get(ns).and_then(|v| v.get(sub)) {
                Some(v) => lua.to_value(v),
                None => Ok(mlua::Value::Nil),
            }
        })
        .unwrap();
    config.set("get", get).unwrap();
    crucible.set("config", config).unwrap();
    lua.globals().set("crucible", crucible).unwrap();

    // `cru.config` is the *app* config store — a pair of get/set functions,
    // never a per-plugin table. Present here so a plugin reaching for
    // `cru.config[<plugin>]` finds the same nil the daemon would hand it.
    lua.load(r#"cru = { config = { get = function() return nil end, set = function() end } }"#)
        .exec()
        .unwrap();

    // The plugin's own `lua/` directory, exactly as the runtime scopes it:
    // Luau has no `package.path`, and the host resolver is what `require`
    // reads. The guard lives as long as the returned VM does.
    let modules = crucible_lua::ModuleRegistry::install(&lua).unwrap();
    let scope = modules
        .enter_plugin_root(&plugins_root().join(plugin))
        .unwrap();
    std::mem::forget(scope);

    let module: mlua::Table = lua.load(r#"return require("config")"#).eval().unwrap();
    (lua, module)
}

fn get_i64(module: &mlua::Table, key: &str) -> i64 {
    module
        .get::<mlua::Function>("get")
        .unwrap()
        .call::<i64>(key)
        .unwrap()
}

fn get_i64_with_fallback(module: &mlua::Table, key: &str, fallback: i64) -> i64 {
    module
        .get::<mlua::Function>("get")
        .unwrap()
        .call::<i64>((key, fallback))
        .unwrap()
}

fn get_bool_with_fallback(module: &mlua::Table, key: &str, fallback: bool) -> bool {
    module
        .get::<mlua::Function>("get")
        .unwrap()
        .call::<bool>((key, fallback))
        .unwrap()
}

fn init(module: &mlua::Table, lua: &mlua::Lua, cfg: &str) {
    let table: mlua::Table = lua.load(format!("return {cfg}")).eval().unwrap();
    module
        .get::<mlua::Function>("init")
        .unwrap()
        .call::<()>(table)
        .unwrap();
}

#[test]
fn declared_default_beats_caller_fallback() {
    // The caller's fallback is a last resort, not an override. `timeout`
    // is declared as 120 in reflection's defaults, so a call site passing
    // 999 must still see 120.
    let (_lua, module) = shipped_config_module("reflection", serde_json::json!({}));
    assert_eq!(get_i64(&module, "timeout"), 120);
    assert_eq!(get_i64_with_fallback(&module, "timeout", 999), 120);
}

#[test]
fn caller_fallback_applies_only_to_undeclared_keys() {
    let (_lua, module) = shipped_config_module("reflection", serde_json::json!({}));
    let val: String = module
        .get::<mlua::Function>("get")
        .unwrap()
        .call::<String>(("no_such_key", "fb"))
        .unwrap();
    assert_eq!(val, "fb");
}

#[test]
fn setup_values_beat_declared_defaults_and_fallback() {
    let (lua, module) = shipped_config_module("reflection", serde_json::json!({}));
    init(&module, &lua, "{ timeout = 7 }");
    assert_eq!(get_i64(&module, "timeout"), 7);
    assert_eq!(get_i64_with_fallback(&module, "timeout", 120), 7);
}

// `setup_kilns_are_visible_through_the_kilns_accessor` lived here. It drove
// `kiln-expert`'s `M.kilns()` — the one shipped accessor with a non-nil table
// default — to prove the fallback did not permanently shadow a `setup()` value.
// The plugin is gone and no other declares such an accessor. The behaviour it
// guarded is still covered generally by `setup_values_beat_explicit_toml` and
// the `timeout` cases above.

/// Lua beats TOML — the Neovim convention. The daemon seeds `setup()` with
/// the TOML section at load, so TOML applies as the base; a user's later
/// `setup{}` call (their init.lua runs after plugins load) must win. This
/// used to be backwards: TOML silently overrode every setup() value, so
/// configuring a plugin from Lua was impossible whenever a TOML key existed.
#[test]
fn setup_values_beat_explicit_toml() {
    let (lua, module) = shipped_config_module(
        "reflection",
        serde_json::json!({ "reflection": { "timeout": 99 } }),
    );
    // Before any setup() call, TOML is the resolved value.
    assert_eq!(get_i64(&module, "timeout"), 99);
    init(&module, &lua, "{ timeout = 7 }");
    assert_eq!(get_i64(&module, "timeout"), 7);
}

#[test]
fn reflection_setup_can_flip_the_enabled_switch() {
    let (lua, module) = shipped_config_module("reflection", serde_json::json!({}));
    assert!(get_bool_with_fallback(&module, "enabled", true));

    init(&module, &lua, "{ enabled = false }");
    assert!(!get_bool_with_fallback(&module, "enabled", true));
}

#[test]
fn reflection_toml_can_flip_the_enabled_switch() {
    let (_lua, module) = shipped_config_module(
        "reflection",
        serde_json::json!({ "reflection": { "enabled": false } }),
    );
    assert!(!get_bool_with_fallback(&module, "enabled", true));
}

#[test]
fn reflection_setup_lowers_min_turns() {
    let (lua, module) = shipped_config_module("reflection", serde_json::json!({}));
    init(&module, &lua, "{ min_turns = 1 }");
    assert_eq!(get_i64_with_fallback(&module, "min_turns", 3), 1);
}

// ---------------------------------------------------------------------------
// setup() wiring through the daemon plugin loader
// ---------------------------------------------------------------------------

fn write_plugin(root: &Path, name: &str, init_lua: &str) {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("plugin.yaml"),
        format!("name: {name}\nversion: \"0.1.0\"\nmain: init.lua\n"),
    )
    .unwrap();
    std::fs::write(dir.join("init.lua"), init_lua).unwrap();
}

fn write_plugin_module(root: &Path, plugin: &str, module: &str, body: &str) {
    let dir = root.join(plugin).join("lua");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("{module}.lua")), body).unwrap();
}

async fn load_from(
    root: &Path,
    config: std::collections::HashMap<String, serde_json::Value>,
) -> crucible_daemon::daemon_plugins::DaemonPluginLoader {
    let mut loader = crucible_daemon::daemon_plugins::DaemonPluginLoader::new(config).unwrap();
    loader
        .load_plugins(&[(root.to_path_buf(), PluginSource::EnvPath)])
        .await
        .unwrap();
    loader
}

/// Run the one-VM boot against a fixture: `config.toml` and `init.lua` under
/// `<tmp>/config/`, plugins under `root` — the plugin-path resolution is
/// injected as a value, so nothing reaches outside the fixture. Returns the
/// loader with `init.lua` already evaluated and plugins ACTIVATED against
/// the extracted config (the deferred phase, as the daemon runs it).
async fn boot_and_activate(
    tmp: &Path,
    root: &Path,
    config_toml: &str,
    init_lua: &str,
) -> (
    crucible_core::config::CliAppConfig,
    crucible_daemon::daemon_plugins::DaemonPluginLoader,
) {
    use crucible_daemon::daemon_plugins::PluginPathsFn;

    let config_dir = tmp.join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("config.toml"), config_toml).unwrap();
    std::fs::write(config_dir.join("init.lua"), init_lua).unwrap();

    let fixture_root = root.to_path_buf();
    let paths: PluginPathsFn = std::sync::Arc::new(move |rtp: &[PathBuf]| {
        let mut dirs = vec![(fixture_root.clone(), PluginSource::EnvPath)];
        for entry in rtp {
            let plugins = entry.join("plugins");
            if plugins.exists() {
                dirs.push((plugins, PluginSource::Runtime));
            }
        }
        dirs
    });

    let boot = crucible_daemon::daemon_plugins::evaluate_boot_config_with_paths(
        Some(config_dir.join("config.toml")),
        None,
        None,
        std::sync::Arc::clone(&paths),
    )
    .await
    .unwrap();

    let mut loader = boot.loader;
    loader
        .load_plugins(&paths(&boot.config.runtimepath))
        .await
        .unwrap();
    (boot.config, loader)
}

/// The idiom, end to end under the boot inversion: init.lua runs FIRST, its
/// `require("prefs").setup{...}` works through the live search space, the
/// deferred activation reuses that same module instance (the file is never
/// evaluated twice), and the user's call OWNS the setup — the default
/// `setup(cfg)` with the TOML section is skipped, so setup runs exactly once
/// and the user's value stands.
#[tokio::test]
async fn user_init_lua_setup_owns_the_plugin_and_runs_once() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("plugins");
    write_plugin(
        &root,
        "prefs",
        r#"
_G.__prefs_setups = 0
return {
    name = "prefs",
    setup = function(cfg)
        _G.__prefs_setups = _G.__prefs_setups + 1
        _G.__prefs_config = cfg
    end,
}
"#,
    );

    let (_config, loader) = boot_and_activate(
        tmp.path(),
        &root,
        "[plugins.prefs]\ngreeting = \"from-toml\"\n",
        r#"require("prefs").setup({ greeting = "from-lua" })"#,
    )
    .await;

    let resolved = loader.eval("return __prefs_config.greeting").await.unwrap();
    assert_eq!(
        resolved, "from-lua",
        "the user's direct setup() call owns this plugin's config"
    );
    let count = loader.eval("return __prefs_setups").await.unwrap();
    assert_eq!(count, "1", "setup must run exactly once — the user's call");
}

/// The store form: `cru.config.set{ plugins = { prefs = {...} } }` in
/// init.lua merges over the TOML seed and feeds the activation phase's
/// default `setup(cfg)` call for a plugin the user did not set up directly.
#[tokio::test]
async fn store_form_plugin_config_reaches_the_default_setup() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("plugins");
    write_plugin(
        &root,
        "prefs",
        r#"
_G.__prefs_setups = 0
return {
    name = "prefs",
    setup = function(cfg)
        _G.__prefs_setups = _G.__prefs_setups + 1
        _G.__prefs_config = cfg
    end,
}
"#,
    );

    let (_config, loader) = boot_and_activate(
        tmp.path(),
        &root,
        "",
        r#"cru.config.set({ plugins = { prefs = { greeting = "from-store" } } })"#,
    )
    .await;

    let resolved = loader.eval("return __prefs_config.greeting").await.unwrap();
    assert_eq!(
        resolved, "from-store",
        "a store-form plugin config must reach the default setup(cfg)"
    );
    let count = loader.eval("return __prefs_setups").await.unwrap();
    assert_eq!(count, "1");
}

/// A `runtimepath` set in init.lua reaches plugin discovery: the plugin
/// lives ONLY under a directory init.lua names, and it still activates.
#[tokio::test]
async fn a_runtimepath_set_in_init_lua_reaches_discovery() {
    let tmp = tempfile::tempdir().unwrap();
    let empty_root = tmp.path().join("plugins");
    std::fs::create_dir_all(&empty_root).unwrap();
    let rtp = tmp.path().join("extra");
    write_plugin(
        &rtp.join("plugins"),
        "fromrtp",
        r#"
return {
    name = "fromrtp",
    setup = function(cfg) _G.__fromrtp_active = true end,
}
"#,
    );

    let init = format!(
        r#"cru.config.set({{ runtimepath = {{ [[{}]] }} }})"#,
        rtp.display()
    );
    let (config, loader) = boot_and_activate(tmp.path(), &empty_root, "", &init).await;

    assert_eq!(config.runtimepath, vec![rtp.clone()]);
    let active = loader.eval("return _G.__fromrtp_active").await.unwrap();
    assert_eq!(
        active, "true",
        "a plugin under an init.lua-declared runtimepath must activate"
    );
}

/// A plugin whose DECLARED name differs from its directory name is
/// required under the directory-derived module name — outside the boot
/// searcher's claim shape. Activation must still execute its file exactly
/// once: re-execution doubles every top-level hook, and the require-time
/// registrations are unattributed, so nothing could clear the first copy.
#[tokio::test]
async fn a_name_mismatched_plugin_executes_once_and_hooks_once() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("plugins");
    let dir = root.join("plainmod");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("plugin.yaml"),
        "name: fancy-name\nversion: \"0.1.0\"\nmain: init.lua\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("init.lua"),
        r#"
_G.__plainmod_execs = (_G.__plainmod_execs or 0) + 1
cru.on("turn:complete", function() end)
return { name = "fancy-name" }
"#,
    )
    .unwrap();

    let (_config, loader) =
        boot_and_activate(tmp.path(), &root, "", r#"require("plainmod")"#).await;

    let execs = loader.eval("return _G.__plainmod_execs").await.unwrap();
    assert_eq!(
        execs, "1",
        "the file package.loaded already holds must not execute again"
    );
    let handlers = loader.plugin_handlers();
    let count = handlers
        .runtime_handlers()
        .lock()
        .unwrap()
        .iter()
        .filter(|h| h.event_type == "turn:complete")
        .count();
    assert_eq!(count, 1, "a re-execution would register the hook twice");
}

/// The same entry FILE reached under a different module name — a dotted
/// `require("dotmod.init")` resolves `<root>/dotmod/init.lua` through the
/// standard loader, outside the searcher's claim shape. Only the post-eval
/// sweep records it, and without that record activation executes the file
/// a second time.
#[tokio::test]
async fn a_dotted_require_of_the_entry_file_still_activates_once() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("plugins");
    write_plugin(
        &root,
        "dotmod",
        r#"
_G.__dotmod_execs = (_G.__dotmod_execs or 0) + 1
cru.on("turn:complete", function() end)
return { name = "dotmod" }
"#,
    );

    let (_config, loader) =
        boot_and_activate(tmp.path(), &root, "", r#"require("dotmod.init")"#).await;

    let execs = loader.eval("return _G.__dotmod_execs").await.unwrap();
    assert_eq!(
        execs, "1",
        "one file, one execution, whatever name reached it"
    );
}

#[tokio::test]
async fn a_both_forms_plugin_gets_one_supersession_notice() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("plugins");
    write_plugin(
        &root,
        "prefs",
        r#"
return { name = "prefs", setup = function(cfg) _G.__prefs_config = cfg end }
"#,
    );

    let (_config, loader) = boot_and_activate(
        tmp.path(),
        &root,
        "[plugins.prefs]\nclip = 4\n",
        r#"require("prefs").setup({ greeting = "from-lua" })"#,
    )
    .await;

    let notices = loader.supersession_notices();
    assert_eq!(notices.len(), 1, "one notice per plugin: {notices:?}");
    assert!(notices[0].contains("prefs"), "{notices:?}");
    assert!(
        notices[0].contains("plugins.prefs"),
        "the notice must name the ignored section: {notices:?}"
    );
    assert!(
        notices[0].contains("move those keys into the setup call"),
        "the notice must name the remedy: {notices:?}"
    );
}

/// The notice fires ONLY for the both-forms combination.
#[tokio::test]
async fn a_single_form_plugin_gets_no_supersession_notice() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("plugins");
    write_plugin(
        &root,
        "prefs",
        r#"
return { name = "prefs", setup = function(cfg) _G.__prefs_config = cfg end }
"#,
    );

    // Direct form only.
    let (_config, loader) = boot_and_activate(
        tmp.path(),
        &root,
        "",
        r#"require("prefs").setup({ greeting = "from-lua" })"#,
    )
    .await;
    assert!(
        loader.supersession_notices().is_empty(),
        "a direct-only plugin is not superseding anything"
    );

    // Store form only.
    let tmp2 = tempfile::tempdir().unwrap();
    let root2 = tmp2.path().join("plugins");
    write_plugin(
        &root2,
        "prefs",
        r#"
return { name = "prefs", setup = function(cfg) _G.__prefs_config = cfg end }
"#,
    );
    let (_config, loader) =
        boot_and_activate(tmp2.path(), &root2, "[plugins.prefs]\nclip = 4\n", "").await;
    assert!(
        loader.supersession_notices().is_empty(),
        "a store-only plugin gets its default setup, nothing is ignored"
    );
}

/// A plugin the user merely `require`d — no setup call — still receives the
/// default `setup(cfg)` at activation. This is the arm that silently
/// regresses into an unconfigured plugin if the recorder over-records.
#[tokio::test]
async fn a_require_without_setup_still_gets_the_default_setup_call() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("plugins");
    write_plugin(
        &root,
        "prefs",
        r#"
_G.__prefs_setups = 0
return {
    name = "prefs",
    setup = function(cfg)
        _G.__prefs_setups = _G.__prefs_setups + 1
        _G.__prefs_config = cfg
    end,
}
"#,
    );

    let (_config, loader) = boot_and_activate(
        tmp.path(),
        &root,
        "[plugins.prefs]\ngreeting = \"from-toml\"\n",
        r#"local _ = require("prefs")"#,
    )
    .await;

    let count = loader.eval("return __prefs_setups").await.unwrap();
    assert_eq!(
        count, "1",
        "a require without a setup call must still get the default setup"
    );
    let greeting = loader.eval("return __prefs_config.greeting").await.unwrap();
    assert_eq!(greeting, "from-toml", "the section must reach that call");
}

/// The boot's setup recorder is a boot-phase device only: after boot, the
/// module table in `package.loaded` holds the plugin's OWN function again.
/// A plugin that stores `M.setup` and compares it later must see itself.
#[tokio::test]
async fn a_wrapped_setup_is_restored_to_the_plugins_own_function_after_boot() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("plugins");
    write_plugin(
        &root,
        "prefs",
        r#"
local function my_setup(cfg)
    _G.__prefs_config = cfg
end
_G.__prefs_original_setup = my_setup
return { name = "prefs", setup = my_setup }
"#,
    );

    let (_config, loader) = boot_and_activate(
        tmp.path(),
        &root,
        "",
        // During the evaluation the recorder is in place, so identity does
        // not hold yet — the fixture records that too, as the contrast.
        r#"
local m = require("prefs")
cru.config.set({ probe = { wrapped_during_boot = not rawequal(m.setup, _G.__prefs_original_setup) } })
m.setup({ marker = "user-owned" })
"#,
    )
    .await;

    let store = crucible_lua::get_app_config().expect("store live");
    assert_eq!(
        store["probe"]["wrapped_during_boot"],
        serde_json::json!(true),
        "precondition: the recorder was in place during the evaluation"
    );
    let restored = loader
        .eval(r#"return tostring(rawequal(require("prefs").setup, _G.__prefs_original_setup))"#)
        .await
        .unwrap();
    assert_eq!(
        restored, "true",
        "after boot the table must hold the plugin's own setup, not the recorder"
    );
}

/// A DISABLED plugin: the user's `require` still loads its module (as in
/// Neovim), but activation registers none of its hooks or exports.
#[tokio::test]
async fn a_disabled_plugin_loads_as_a_module_but_activates_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("plugins");
    write_plugin(
        &root,
        "offplug",
        r#"
_G.__offplug_module_loaded = true
cru.on("turn:complete", function() end)
return {
    name = "offplug",
    setup = function(cfg) _G.__offplug_setup_ran = true end,
    commands = {
        ["offplug.hello"] = { desc = "hello", fn = function() return {} end },
    },
}
"#,
    );

    let (_config, loader) = boot_and_activate(
        tmp.path(),
        &root,
        "[plugins.offplug]\nenabled = false\n",
        r#"require("offplug")"#,
    )
    .await;

    let loaded = loader
        .eval("return _G.__offplug_module_loaded")
        .await
        .unwrap();
    assert_eq!(loaded, "true", "require must still load the module");
    let setup = loader
        .eval("return tostring(_G.__offplug_setup_ran)")
        .await
        .unwrap();
    assert_eq!(setup, "nil", "a disabled plugin gets no setup call");
    assert_eq!(
        loader.plugin_handlers().plugin_handler_count("offplug"),
        0,
        "a disabled plugin's boot-require hooks must be cleared"
    );
    let command = loader
        .plugin_registry()
        .run_command("offplug.hello", serde_json::json!({}))
        .await
        .unwrap();
    assert!(
        command.is_none(),
        "a disabled plugin's commands must not register"
    );
}

/// A broken user init.lua is user configuration, not a gate: the boot warns,
/// continues on the seed, and activation still hands the plugin its TOML
/// section. The daemon never goes down for it.
#[tokio::test]
async fn broken_user_init_lua_fails_open() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("plugins");
    write_plugin(
        &root,
        "prefs",
        r#"
return {
    name = "prefs",
    setup = function(cfg) _G.__prefs_config = cfg end,
}
"#,
    );

    let (config, loader) = boot_and_activate(
        tmp.path(),
        &root,
        "default_kiln = \"seeded\"\n\n[plugins.prefs]\ngreeting = \"from-toml\"\n",
        "this is not lua (",
    )
    .await;

    assert_eq!(
        config.default_kiln.as_deref(),
        Some("seeded"),
        "the extracted config must be the seed"
    );
    let resolved = loader.eval("return __prefs_config.greeting").await.unwrap();
    assert_eq!(resolved, "from-toml");
}

#[tokio::test]
async fn setup_receives_the_plugins_toml_section() {
    let tmp = tempfile::tempdir().unwrap();
    write_plugin(
        tmp.path(),
        "cfgprobe",
        r#"
return {
  name = "cfgprobe",
  version = "0.1.0",
  setup = function(cfg)
    _G.__probe_called = true
    _G.__probe_greeting = cfg.greeting
  end,
}
"#,
    );

    let config = std::collections::HashMap::from([(
        "cfgprobe".to_string(),
        serde_json::json!({ "greeting": "hi" }),
    )]);
    let loader = load_from(tmp.path(), config).await;

    assert_eq!(loader.eval("=__probe_called").await.unwrap(), "true");
    assert_eq!(loader.eval("=__probe_greeting").await.unwrap(), "hi");
}

#[tokio::test]
async fn setup_runs_with_an_empty_table_when_the_plugin_has_no_config() {
    let tmp = tempfile::tempdir().unwrap();
    write_plugin(
        tmp.path(),
        "cfgprobe",
        r#"
return {
  name = "cfgprobe",
  version = "0.1.0",
  setup = function(cfg)
    _G.__probe_type = type(cfg)
  end,
}
"#,
    );

    let loader = load_from(tmp.path(), std::collections::HashMap::new()).await;
    assert_eq!(loader.eval("=__probe_type").await.unwrap(), "table");
}

#[tokio::test]
async fn plugins_do_not_share_a_same_named_local_module() {
    // Both shipped plugins carry their own `lua/config.lua`. A single
    // `package.loaded["config"]` across the plugin VM would hand the second
    // plugin the first one's config module.
    let tmp = tempfile::tempdir().unwrap();
    for name in ["alpha", "beta"] {
        write_plugin(
            tmp.path(),
            name,
            &format!(
                r#"
local shared = require("shared")
_G.__who_{name} = shared.who
return {{ name = "{name}", version = "0.1.0" }}
"#
            ),
        );
        write_plugin_module(
            tmp.path(),
            name,
            "shared",
            &format!(r#"return {{ who = "{name}" }}"#),
        );
    }

    let loader = load_from(tmp.path(), std::collections::HashMap::new()).await;
    assert_eq!(loader.eval("=__who_alpha").await.unwrap(), "alpha");
    assert_eq!(loader.eval("=__who_beta").await.unwrap(), "beta");
}

#[tokio::test]
async fn toml_config_resolves_via_crucible_config_get() {
    // Regression guard: the dotted-key lookup is the only working path today
    // and stays the highest-precedence layer.
    let tmp = tempfile::tempdir().unwrap();
    write_plugin(
        tmp.path(),
        "cfgprobe",
        r#"return { name = "cfgprobe", version = "0.1.0" }"#,
    );

    let config = std::collections::HashMap::from([(
        "cfgprobe".to_string(),
        serde_json::json!({ "greeting": "hi", "count": 7, "nested": { "deep": "found" } }),
    )]);
    let loader = load_from(tmp.path(), config).await;

    assert_eq!(
        loader
            .eval(r#"=cru.plugin.config.get("cfgprobe.greeting")"#)
            .await
            .unwrap(),
        "hi"
    );
    // Every dot segment descends — this used to split on the FIRST dot only,
    // so nested TOML tables were unreachable past one level.
    assert_eq!(
        loader
            .eval(r#"=cru.plugin.config.get("cfgprobe.nested.deep")"#)
            .await
            .unwrap(),
        "found"
    );
    assert_eq!(
        loader
            .eval(r#"=cru.plugin.config.get("cfgprobe.count")"#)
            .await
            .unwrap(),
        "7"
    );
    assert_eq!(
        loader
            .eval(r#"=cru.plugin.config.get("cfgprobe.missing")"#)
            .await
            .unwrap(),
        "nil"
    );
}

// ---------------------------------------------------------------------------
// The shipped auto-title plugin, configured the documented way
// ---------------------------------------------------------------------------

/// Copy the shipped `auto-title` plugin into `root` so the loader sees a real
/// plugin without also loading every other shipped one.
fn copy_shipped_plugin(root: &Path, plugin: &str) {
    fn copy_tree(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
    copy_tree(&plugins_root().join(plugin), &root.join(plugin));
}

/// Record what `auto-title` asks `cru.session.complete` for, and answer.
const RECORD_COMPLETIONS: &str = r#"
__completion_opts = nil
cru.session = cru.session or {}
cru.session.complete = function(session_id, opts)
    __completion_opts = opts
    return "A perfectly good title"
end
"#;

/// Run `auto-title.generate` the way the daemon does — through the command
/// handle the loader captured at load — and answer with the recorded options.
async fn generate_title(
    loader: &crucible_daemon::daemon_plugins::DaemonPluginLoader,
    user: &str,
) -> (String, String, String) {
    loader.eval(RECORD_COMPLETIONS).await.unwrap();
    let result = loader
        .plugin_registry()
        .run_command(
            "auto-title.generate",
            serde_json::json!({ "session_id": "chat-1", "user": user }),
        )
        .await
        .unwrap()
        .expect("auto-title must declare the command it publishes");
    let title = result["title"].as_str().unwrap().to_string();
    let system = loader.eval("=__completion_opts.system").await.unwrap();
    let prompt = loader.eval("=__completion_opts.prompt").await.unwrap();
    (title, system, prompt)
}

/// The documented Lua config path, end to end under the boot inversion: the
/// user's init.lua reaches the shipped plugin by `require` BEFORE activation,
/// activation reuses that same module instance, and the command the daemon
/// calls sees what the user set.
#[tokio::test]
async fn user_init_lua_configures_the_shipped_auto_title_plugin() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("plugins");
    std::fs::create_dir_all(&root).unwrap();
    copy_shipped_plugin(&root, "auto-title");

    let (_config, loader) = boot_and_activate(
        tmp.path(),
        &root,
        "",
        r#"require("auto-title").setup({ prompt = "Name it.", clip = 4 })"#,
    )
    .await;

    let (title, system, prompt) = generate_title(&loader, "abcdefgh").await;
    assert_eq!(
        system, "Name it.",
        "the configured prompt must be the one asked with"
    );
    assert_eq!(
        prompt, "User: abcd",
        "the configured clip must bound the exchange"
    );
    assert_eq!(title, "A perfectly good title");
}

/// With no init.lua configuration at all, the shipped defaults stand.
#[tokio::test]
async fn the_shipped_auto_title_defaults_stand_without_user_config() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("plugins");
    std::fs::create_dir_all(&root).unwrap();
    copy_shipped_plugin(&root, "auto-title");

    let (_config, loader) = boot_and_activate(tmp.path(), &root, "", "").await;

    let (_, default_system, _) = generate_title(&loader, "help me fix the auth flow").await;
    assert!(
        default_system.contains("3 to 7 words"),
        "the shipped prompt is the base: {default_system}"
    );
}

/// A plugin configured BOTH ways takes the direct call: the user's
/// `require("auto-title").setup{...}` owns this plugin's setup, so the
/// `[plugins.auto-title]` TOML section is NOT applied — the docs say pick
/// one form per plugin. (Before the boot inversion the two composed per
/// key; the composition rule is now ownership, not layering.)
#[tokio::test]
async fn a_direct_setup_call_owns_the_plugin_over_the_toml_section() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("plugins");
    std::fs::create_dir_all(&root).unwrap();
    copy_shipped_plugin(&root, "auto-title");

    let (_config, loader) = boot_and_activate(
        tmp.path(),
        &root,
        "[plugins.auto-title]\nprompt = \"From TOML.\"\nclip = 4\n",
        r#"require("auto-title").setup({ prompt = "From Lua." })"#,
    )
    .await;

    let (_, system, prompt) = generate_title(&loader, "abcdefgh").await;
    assert_eq!(system, "From Lua.", "the direct call owns the setup");
    assert_eq!(
        prompt, "User: abcdefgh",
        "the TOML clip is not applied — the direct call owns the whole setup"
    );
}

/// The channel publishes exactly once, whichever path loads the plugin.
///
/// The user's `require` at boot runs the plugin body (which publishes, under
/// the plugin's own binding via the boot searcher); activation then reuses
/// the instance and must not run the body — or publish — a second time.
#[tokio::test]
async fn configuring_auto_title_does_not_republish_the_channel() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("plugins");
    std::fs::create_dir_all(&root).unwrap();
    copy_shipped_plugin(&root, "auto-title");

    let (_config, loader) = boot_and_activate(
        tmp.path(),
        &root,
        "",
        r#"require("auto-title").setup({ prompt = "Name it." })"#,
    )
    .await;

    let titlers: Vec<String> = loader
        .publications()
        .get("session_title")
        .into_iter()
        .map(|(plugin, _)| plugin)
        .collect();
    assert_eq!(
        titlers,
        vec!["auto-title".to_string()],
        "one load, one publication, attributed to the plugin itself"
    );
}
