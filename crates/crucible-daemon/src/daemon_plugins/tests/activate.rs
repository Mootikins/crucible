//! Activation: one body for a plugin, whoever asks for it.
//!
//! A boot `require`, the spec-driven pass and a reload all reach
//! `activate`. These tests drive it through the loader's public surface and
//! read the VM to see what ran.
use super::super::*;
use crucible_lua::manifest::PluginState;

/// A loader over one fixture plugin `<tmp>/<name>/init.luau`, with the
/// search path added and the boot `require` hook installed, and WITHOUT the
/// shipped defaults, so the Builtin fragment does not interfere.
async fn loader_with_plugin(name: &str, body: &str) -> (DaemonPluginLoader, tempfile::TempDir) {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("init.luau"), body).unwrap();
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .add_plugin_paths(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .expect("paths");
    boot::install_boot_require_hook(&loader);
    (loader, tmp)
}

fn last_error(loader: &DaemonPluginLoader, name: &str) -> String {
    loader
        .loaded_plugin_info()
        .into_iter()
        .find(|p| p["name"] == name)
        .and_then(|p| p["last_error"].as_str().map(str::to_string))
        .unwrap_or_default()
}

/// init.lua requires the plugin; the loader then activates the spec. setup
/// runs once, and the module table is the same object both times.
#[tokio::test]
async fn a_boot_require_and_the_loader_share_one_activation() {
    let (mut loader, _dirs) = loader_with_plugin(
        "once",
        r#"
        local M = { calls = 0 }
        function M.setup(opts) M.calls = M.calls + 1 end
        return M"#,
    )
    .await;
    loader
        .eval_user_init(r#"local m = require("once")"#)
        .await
        .unwrap();
    loader.load_plugins_from_spec().await.unwrap();
    let calls: i64 = loader
        .lua()
        .load(r#"return require("once").calls"#)
        .eval()
        .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(loader.plugin_state("once"), Some(PluginState::Active));
}

/// The body leaves a mark in the VM when it runs. A body that raised
/// instead would pass this test through the wrong door: `activate` skips
/// `mark_error` for a disabled plugin, so the state alone cannot tell a
/// body that never ran from a body that ran and failed.
#[tokio::test]
async fn a_disabled_plugin_never_runs() {
    let (mut loader, _dirs) = loader_with_plugin("quiet", r#"_G.ran = true; return {}"#).await;
    loader
        .eval_user_init(r#"cru.plugin.setup({ { "quiet", enabled = false } })"#)
        .await
        .unwrap();
    loader.load_plugins_from_spec().await.unwrap();
    let ran: Option<bool> = loader.lua().globals().get("ran").unwrap();
    assert_eq!(ran, None, "the disabled plugin's body ran");
    assert_eq!(loader.plugin_state("quiet"), Some(PluginState::Disabled));
}

/// A reload of a plugin that `init.lua` required must read its file again.
/// The boot `require` left an instance in `package.loaded`, and `activate`
/// reuses such an instance; the reload forgets it first, so the body runs
/// a second time.
#[tokio::test]
async fn reloading_a_boot_required_plugin_reruns_its_file() {
    let (mut loader, _dirs) =
        loader_with_plugin("x", r#"_G.runs = (_G.runs or 0) + 1; return {}"#).await;
    loader.eval_user_init(r#"require("x")"#).await.unwrap();
    loader.load_plugins_from_spec().await.unwrap();
    assert_eq!(loader.plugin_state("x"), Some(PluginState::Active));

    loader.reload_plugin("x").await.expect("reload");
    let runs: i64 = loader.lua().globals().get("runs").unwrap();
    assert_eq!(runs, 2, "the reload did not re-run the plugin's file");
}

/// A body that counts its runs and returns a table with a distinct marker.
fn counted_plugin_body() -> &'static str {
    r#"_G.runs = (_G.runs or 0) + 1; return { marker = math.random() }"#
}

/// The `marker` a `require(name)` answers, and the identity of the table.
fn required_module(loader: &DaemonPluginLoader, name: &str) -> (f64, *const std::ffi::c_void) {
    let table: mlua::Table = loader
        .lua()
        .load(format!("return require('{name}')"))
        .eval()
        .expect("require");
    (
        table.get::<f64>("marker").expect("marker"),
        table.to_pointer(),
    )
}

/// The loader activated the plugin, and no `require` ran before it. A later
/// `require("x")` answers the activated module, as lazy.nvim's loader does,
/// and the file does not run a second time.
#[tokio::test]
async fn require_after_activation_returns_the_activated_module() {
    let (mut loader, _dirs) = loader_with_plugin("x", counted_plugin_body()).await;
    loader
        .eval_user_init(r#"cru.plugin.setup({ "x" })"#)
        .await
        .unwrap();
    loader.load_plugins_from_spec().await.unwrap();
    let activated = activate::activate(&mut loader, "x")
        .await
        .expect("activate");

    let (marker, identity) = required_module(&loader, "x");
    assert_eq!(
        identity,
        activated.to_pointer(),
        "require answered another table"
    );
    assert_eq!(marker, activated.get::<f64>("marker").unwrap());
    let runs: i64 = loader.lua().globals().get("runs").unwrap();
    assert_eq!(runs, 1, "the require ran the plugin's file again");
}

/// A reload forgets the seeded entry with the rest: the file runs again, and
/// `require("x")` answers the new instance.
#[tokio::test]
async fn reloading_a_loader_activated_plugin_reruns_its_file_and_reseeds_the_cache() {
    let (mut loader, _dirs) = loader_with_plugin("x", counted_plugin_body()).await;
    loader
        .eval_user_init(r#"cru.plugin.setup({ "x" })"#)
        .await
        .unwrap();
    loader.load_plugins_from_spec().await.unwrap();
    let (first_marker, first_identity) = required_module(&loader, "x");

    loader.reload_plugin("x").await.expect("reload");
    let runs: i64 = loader.lua().globals().get("runs").unwrap();
    assert_eq!(runs, 2, "the reload did not re-run the plugin's file");
    let activated = activate::activate(&mut loader, "x")
        .await
        .expect("activate");
    let (marker, identity) = required_module(&loader, "x");
    assert_ne!(
        identity, first_identity,
        "require still answers the old instance"
    );
    assert_ne!(marker, first_marker);
    assert_eq!(
        identity,
        activated.to_pointer(),
        "require answered another table"
    );
}

/// An inert plugin is not served from the cache: after `disable`, the entry
/// the activation seeded is gone.
#[tokio::test]
async fn an_inert_plugin_is_not_served_from_the_module_cache() {
    let (mut loader, _dirs) = loader_with_plugin("x", counted_plugin_body()).await;
    loader
        .eval_user_init(r#"cru.plugin.setup({ "x" })"#)
        .await
        .unwrap();
    loader.load_plugins_from_spec().await.unwrap();
    let seeded: bool = loader
        .lua()
        .load("return package.loaded['x'] ~= nil")
        .eval()
        .unwrap();
    assert!(seeded, "activation did not seed package.loaded");

    loader.disable_plugin("x");
    let cached: bool = loader
        .lua()
        .load("return package.loaded['x'] ~= nil")
        .eval()
        .unwrap();
    assert!(
        !cached,
        "a disabled plugin is still served from package.loaded"
    );
}

#[tokio::test]
async fn an_entry_config_replaces_the_default_setup_call() {
    let (mut loader, _dirs) = loader_with_plugin(
        "cfg",
        r#"
        local M = {}
        function M.setup() _G.default_ran = true end
        return M"#,
    )
    .await;
    loader
        .eval_user_init(
            r#"cru.plugin.setup({ { "cfg", config = function(m, opts) _G.custom_ran = true end } })"#,
        )
        .await
        .unwrap();
    loader.load_plugins_from_spec().await.unwrap();
    let g = loader.lua().globals();
    assert!(g.get::<bool>("custom_ran").unwrap());
    assert!(g.get::<Option<bool>>("default_ran").unwrap().is_none());
}

#[tokio::test]
async fn a_discovered_plugin_with_no_entry_stays_inactive() {
    let (mut loader, _dirs) = loader_with_plugin("orphan", r#"return {}"#).await;
    loader.load_plugins_from_spec().await.unwrap();
    assert_eq!(loader.plugin_state("orphan"), Some(PluginState::Discovered));
}

/// The operator asked twice and said two things: `require("x")` in init.lua
/// and `enabled = false` in the spec. The require was explicit, so the
/// plugin activates, and the daemon log names both sites.
#[tokio::test]
async fn a_boot_required_plugin_the_spec_disables_still_activates() {
    let (mut loader, _dirs) = loader_with_plugin(
        "both",
        r#"return { setup = function() _G.both_setup = true end }"#,
    )
    .await;
    loader
        .eval_user_init(
            r#"
            require("both")
            cru.plugin.setup({ { "both", enabled = false } })"#,
        )
        .await
        .unwrap();
    loader.load_plugins_from_spec().await.unwrap();
    assert_eq!(loader.plugin_state("both"), Some(PluginState::Active));
    assert!(loader.lua().globals().get::<bool>("both_setup").unwrap());
}

/// The entry's `opts` reach `setup`, merged as `resolve_opts` states.
#[tokio::test]
async fn setup_receives_the_resolved_opts() {
    let (mut loader, _dirs) = loader_with_plugin(
        "opted",
        r#"return { setup = function(opts) _G.seen = opts.greeting end }"#,
    )
    .await;
    loader
        .eval_user_init(r#"cru.plugin.setup({ { "opted", opts = { greeting = "hi" } } })"#)
        .await
        .unwrap();
    loader.load_plugins_from_spec().await.unwrap();
    let seen: String = loader.lua().globals().get("seen").unwrap();
    assert_eq!(seen, "hi");
}

/// A module that returns something other than a table declares nothing the
/// daemon can bind, and the refusal says what came back.
#[tokio::test]
async fn a_module_that_returns_no_table_is_refused_with_the_reason() {
    let (mut loader, _dirs) = loader_with_plugin("scalar", r#"return 42"#).await;
    loader
        .eval_user_init(r#"cru.plugin.setup({ "scalar" })"#)
        .await
        .unwrap();
    loader.load_plugins_from_spec().await.unwrap();
    assert_eq!(loader.plugin_state("scalar"), Some(PluginState::Error));
    let error = last_error(&loader, "scalar");
    assert!(
        error.contains("not a table") && error.contains("integer"),
        "{error}"
    );
}

// --- the lifecycle hooks a module table may carry ---------------------------

fn hooked_plugin_body() -> &'static str {
    r#"
    _G.trace = _G.trace or {}
    return {
        setup = function() table.insert(_G.trace, "setup") end,
        on_load = function() table.insert(_G.trace, "load") end,
        on_unload = function() table.insert(_G.trace, "unload") end,
    }"#
}

fn trace(loader: &DaemonPluginLoader) -> Vec<String> {
    loader
        .lua()
        .load("return _G.trace or {}")
        .eval::<Vec<String>>()
        .unwrap()
}

#[tokio::test]
async fn on_load_fires_after_config() {
    let (mut loader, _dirs) = loader_with_plugin("hooked", hooked_plugin_body()).await;
    loader
        .eval_user_init(r#"cru.plugin.setup({ "hooked" })"#)
        .await
        .unwrap();
    loader.load_plugins_from_spec().await.unwrap();
    assert_eq!(trace(&loader), ["setup", "load"]);
}

#[tokio::test]
async fn on_unload_fires_on_unload_disable_and_reload_once_each() {
    let (mut loader, _dirs) = loader_with_plugin("hooked", hooked_plugin_body()).await;
    loader
        .eval_user_init(r#"cru.plugin.setup({ "hooked" })"#)
        .await
        .unwrap();
    loader.load_plugins_from_spec().await.unwrap();

    // A reload runs on_unload for the old generation, then on_load for the new.
    loader.reload_plugin("hooked").await.expect("reload");
    assert_eq!(trace(&loader), ["setup", "load", "unload", "setup", "load"]);

    // Disable runs on_unload once; the plugin is not active afterwards.
    loader.disable_plugin("hooked");
    assert_eq!(loader.plugin_state("hooked"), Some(PluginState::Disabled));
    assert_eq!(
        trace(&loader),
        ["setup", "load", "unload", "setup", "load", "unload"]
    );

    // Unload of a plugin that is not active fires nothing.
    loader
        .deactivate_and_forget_plugin("hooked")
        .await
        .expect("forget");
    assert_eq!(
        trace(&loader),
        ["setup", "load", "unload", "setup", "load", "unload"]
    );
}

#[tokio::test]
async fn an_on_load_failure_is_not_fatal() {
    let (mut loader, _dirs) = loader_with_plugin(
        "fragile",
        r#"return { on_load = function() error("on_load boom") end }"#,
    )
    .await;
    loader
        .eval_user_init(r#"cru.plugin.setup({ "fragile" })"#)
        .await
        .unwrap();
    loader.load_plugins_from_spec().await.unwrap();
    assert_eq!(loader.plugin_state("fragile"), Some(PluginState::Active));
    assert_eq!(last_error(&loader, "fragile"), "");
}

#[tokio::test]
async fn a_module_without_hooks_activates() {
    let (mut loader, _dirs) = loader_with_plugin("plain", r#"return {}"#).await;
    loader
        .eval_user_init(r#"cru.plugin.setup({ "plain" })"#)
        .await
        .unwrap();
    loader.load_plugins_from_spec().await.unwrap();
    assert_eq!(loader.plugin_state("plain"), Some(PluginState::Active));
}

/// Activation follows discovery order: the search-path rank first, then
/// the file name inside one path. `zeta` sits in the higher root and
/// `alpha` in the lower one, so `zeta` runs first although its name sorts
/// last. A pass that sorted every discovered name would run `alpha` first.
#[tokio::test]
async fn activation_follows_discovery_order_rank_then_file_name() {
    let high = tempfile::TempDir::new().unwrap();
    let low = tempfile::TempDir::new().unwrap();
    for (root, name) in [(&high, "zeta"), (&low, "alpha")] {
        let dir = root.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("init.luau"),
            format!(
                r#"_G.order = _G.order or {{}}; table.insert(_G.order, "{name}"); return {{}}"#
            ),
        )
        .unwrap();
    }
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .add_plugin_paths(&[
            (high.path().to_path_buf(), PluginSource::Runtime),
            (low.path().to_path_buf(), PluginSource::Runtime),
        ])
        .expect("paths");
    boot::install_boot_require_hook(&loader);
    loader
        .eval_user_init(r#"cru.plugin.setup({ "zeta", "alpha" })"#)
        .await
        .unwrap();
    loader.load_plugins_from_spec().await.unwrap();
    let order: Vec<String> = loader.lua().load("return _G.order").eval().unwrap();
    assert_eq!(
        order,
        ["zeta", "alpha"],
        "the search-path rank must outrank the file name"
    );
    assert_eq!(loader.plugin_state("zeta"), Some(PluginState::Active));
    assert_eq!(loader.plugin_state("alpha"), Some(PluginState::Active));
}
