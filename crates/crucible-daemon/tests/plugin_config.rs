//! Plugin configuration: `setup()` wiring and the shipped plugins' config layer.
//!
//! Two mechanisms have to agree for plugin config to work:
//!
//! 1. `[plugins.<name>]` from config.toml reaches the plugin runtime as
//!    `crucible.config.get("<name>.<key>")`, and is handed to the plugin's
//!    `setup()` function at load time.
//! 2. The plugin's own config module resolves in a defined order:
//!    explicit TOML → `setup()` → declared defaults → caller fallback.
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
/// `crucible.config.get("<plugin>.<key>")` backed by `toml`.
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

    let dir = plugins_root().join(plugin).join("lua");
    lua.load(format!(
        r#"package.path = {:?} .. "/?.lua;" .. package.path"#,
        dir.to_string_lossy()
    ))
    .exec()
    .unwrap();

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
    // is declared as 30 in kiln-expert's defaults, so a call site passing
    // 999 must still see 30.
    let (_lua, module) = shipped_config_module("kiln-expert", serde_json::json!({}));
    assert_eq!(get_i64(&module, "timeout"), 30);
    assert_eq!(get_i64_with_fallback(&module, "timeout", 999), 30);
}

#[test]
fn caller_fallback_applies_only_to_undeclared_keys() {
    let (_lua, module) = shipped_config_module("kiln-expert", serde_json::json!({}));
    let val: String = module
        .get::<mlua::Function>("get")
        .unwrap()
        .call::<String>(("no_such_key", "fb"))
        .unwrap();
    assert_eq!(val, "fb");
}

#[test]
fn setup_values_beat_declared_defaults_and_fallback() {
    let (lua, module) = shipped_config_module("kiln-expert", serde_json::json!({}));
    init(&module, &lua, "{ timeout = 7 }");
    assert_eq!(get_i64(&module, "timeout"), 7);
    assert_eq!(get_i64_with_fallback(&module, "timeout", 30), 7);
}

#[test]
fn setup_kilns_are_visible_through_the_kilns_accessor() {
    // `M.kilns()` is `M.get("kilns", {})` — the non-nil fallback used to
    // shadow every setup() value permanently.
    let (lua, module) = shipped_config_module("kiln-expert", serde_json::json!({}));
    init(&module, &lua, r#"{ kilns = { docs = "/tmp/docs" } }"#);

    let kilns: mlua::Table = module
        .get::<mlua::Function>("kilns")
        .unwrap()
        .call::<mlua::Table>(())
        .unwrap();
    assert_eq!(kilns.get::<String>("docs").unwrap(), "/tmp/docs");
}

/// Lua beats TOML — the Neovim convention. The daemon seeds `setup()` with
/// the TOML section at load, so TOML applies as the base; a user's later
/// `setup{}` call (their init.lua runs after plugins load) must win. This
/// used to be backwards: TOML silently overrode every setup() value, so
/// configuring a plugin from Lua was impossible whenever a TOML key existed.
#[test]
fn setup_values_beat_explicit_toml() {
    let (lua, module) = shipped_config_module(
        "kiln-expert",
        serde_json::json!({ "kiln-expert": { "timeout": 99 } }),
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

/// End-to-end through the loader: TOML seeds setup() at load, and the user's
/// init.lua — evaluated AFTER plugins load — calls setup() again and wins.
/// Lua beats TOML; the ordering IS the precedence mechanism.
#[tokio::test]
async fn user_init_lua_setup_overrides_toml() {
    let tmp = tempfile::tempdir().unwrap();
    write_plugin(
        tmp.path(),
        "prefs",
        r#"
return {
    name = "prefs",
    setup = function(cfg) _G.__prefs_config = cfg end,
}
"#,
    );
    let config = std::collections::HashMap::from([(
        "prefs".to_string(),
        serde_json::json!({ "greeting": "from-toml" }),
    )]);
    let loader = load_from(tmp.path(), config).await;

    let seeded = loader.eval("return __prefs_config.greeting").await.unwrap();
    assert_eq!(seeded, "from-toml", "TOML is the base at load");

    let init_path = tmp.path().join("init.lua");
    std::fs::write(
        &init_path,
        r#"require("prefs").setup({ greeting = "from-lua" })"#,
    )
    .unwrap();
    loader.eval_user_init(&init_path).await;

    let resolved = loader.eval("return __prefs_config.greeting").await.unwrap();
    assert_eq!(
        resolved, "from-lua",
        "the user's init.lua setup() call must override the TOML seed"
    );
}

/// A broken user init.lua is user configuration, not a gate: it must warn
/// and leave the TOML-seeded config intact, never take the daemon down.
#[tokio::test]
async fn broken_user_init_lua_fails_open() {
    let tmp = tempfile::tempdir().unwrap();
    write_plugin(
        tmp.path(),
        "prefs",
        r#"
return {
    name = "prefs",
    setup = function(cfg) _G.__prefs_config = cfg end,
}
"#,
    );
    let config = std::collections::HashMap::from([(
        "prefs".to_string(),
        serde_json::json!({ "greeting": "from-toml" }),
    )]);
    let loader = load_from(tmp.path(), config).await;

    let init_path = tmp.path().join("init.lua");
    std::fs::write(&init_path, "this is not lua (").unwrap();
    loader.eval_user_init(&init_path).await;

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
        serde_json::json!({ "greeting": "hi", "count": 7 }),
    )]);
    let loader = load_from(tmp.path(), config).await;

    assert_eq!(
        loader
            .eval(r#"=crucible.config.get("cfgprobe.greeting")"#)
            .await
            .unwrap(),
        "hi"
    );
    assert_eq!(
        loader
            .eval(r#"=crucible.config.get("cfgprobe.count")"#)
            .await
            .unwrap(),
        "7"
    );
    assert_eq!(
        loader
            .eval(r#"=crucible.config.get("cfgprobe.missing")"#)
            .await
            .unwrap(),
        "nil"
    );
}
