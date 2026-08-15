//! Load/reload lifecycle bookkeeping: `loaded_specs` merging, the inert
//! contract for failed plugins, and `__crucible_loading_plugin__`
//! attribution across every exit path of `execute_plugin`.
use super::super::*;

/// A second `load_plugins` call must merge into `loaded_specs`, not replace
/// it. The assignment it used to do dropped every previously loaded plugin's
/// entry: an Active plugin is skipped by `load_all` (`AlreadyLoaded`), so its
/// spec is absent from the second call's result, and `plugin.list` reported
/// its tool/command counts as 0 while the tools stayed registered and
/// working. `plugin.install` loads at runtime via exactly this second call.
#[tokio::test]
async fn a_second_load_plugins_call_keeps_previously_loaded_specs() {
    let tmp = tempfile::TempDir::new().unwrap();
    let write_plugin = |name: &str| {
        let dir = tmp.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("init.lua"),
            format!(
                r#"return {{
                    name = "{name}",
                    version = "0.1.0",
                    tools = {{ {name}_probe = {{ description = "x", fn = function() return "t" end }} }},
                }}"#
            ),
        )
        .unwrap();
    };

    write_plugin("alpha");
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("first load");

    // A new plugin dir appears (the install flow), and load_plugins runs again
    // over the same search path.
    write_plugin("beta");
    loader
        .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("second load");

    let info = loader.loaded_plugin_info();
    let counts = |name: &str| {
        let entry = info
            .iter()
            .find(|p| p["name"] == name)
            .unwrap_or_else(|| panic!("'{name}' missing from plugin info: {info:#?}"));
        (entry["state"].clone(), entry["tools"].clone())
    };
    assert_eq!(
        counts("alpha"),
        ("Active".into(), 1.into()),
        "alpha's spec was dropped by the second load_plugins call"
    );
    assert_eq!(counts("beta"), ("Active".into(), 1.into()));
}

/// Two `name: None` specs must not merge with each other — `None == None`
/// would make the first anonymous spec swallow every later one.
#[test]
fn remember_specs_never_merges_anonymous_specs() {
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    let anon = PluginSpec::default();
    loader.remember_specs(std::slice::from_ref(&anon));
    loader.remember_specs(std::slice::from_ref(&anon));
    assert_eq!(loader.loaded_specs.len(), 2);

    let named = PluginSpec {
        name: Some("gamma".to_string()),
        ..Default::default()
    };
    loader.remember_specs(std::slice::from_ref(&named));
    loader.remember_specs(std::slice::from_ref(&named));
    assert_eq!(loader.loaded_specs.len(), 3, "named specs upsert in place");
}

/// A plugin that fails in the daemon VM must end up fully inert, not
/// half-alive: `setup()` runs *after* init.lua's top-level `crucible.on`
/// calls have already registered handlers, and `pre_tool_call` fails closed —
/// a stale handler from a dead plugin can deny every tool call in every
/// session while `plugin.reload` reports success.
#[tokio::test]
async fn a_plugin_whose_setup_raises_ends_inert_and_the_load_reports_failure() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("halfdead");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("init.lua"),
        r#"
        crucible.on("pre_tool_call", function() return { cancel = true, reason = "stale" } end)
        crucible.options{
            type = "group", name = "Halfdead",
            get = function() return true end,
            set = function() end,
            args = { probe_toggle = { type = "toggle", name = "Probe" } },
        }
        return {
            name = "halfdead",
            version = "0.1.0",
            tools = { probe = { description = "x", fn = function() return "t" end } },
            setup = function() error("boom inside setup") end,
        }
    "#,
    )
    .unwrap();

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("load_plugins is fail-open per plugin");

    let info = loader.loaded_plugin_info();
    let entry = info
        .iter()
        .find(|p| p["name"] == "halfdead")
        .expect("listed");
    assert_eq!(entry["state"], "Error", "got: {entry}");
    // Error must MEAN inert:
    assert_eq!(loader.plugin_handlers().plugin_handler_count("halfdead"), 0);
    assert!(!loader.plugin_registry().tool_names().contains("probe"));
    // A dead plugin's options are not settable, so its declarations must not
    // reach the settings pane either.
    assert!(
        !loader.options().plugins().contains(&"halfdead".to_string()),
        "a dead plugin's option tree must be released"
    );
    // …but the plugin still shows what it DECLARES:
    assert_eq!(
        entry["tools"], 1,
        "a broken plugin's declared counts stay visible"
    );
    // And an explicit reload of a broken plugin must say so:
    assert!(loader.reload_plugin("halfdead").await.is_err());
}

/// `__crucible_loading_plugin__` must be cleared on EVERY exit from
/// `execute_plugin`. A top-level raise used to leave it set, so everything
/// registered next — including the user's init.lua, which runs after all
/// plugins — was attributed to the dead plugin, and a later reload of that
/// plugin deleted the user's handlers.
#[tokio::test]
async fn a_top_level_raise_does_not_swallow_later_registrations() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("raiser");
    std::fs::create_dir_all(&dir).unwrap();
    // `crucible.no_such_api` is a permissive stub in the discovery sandbox but
    // nil in the daemon VM, so the raise happens exactly where the bug lives:
    // inside `execute_plugin`, after the loading marker is set.
    std::fs::write(
        dir.join("init.lua"),
        r#"
        crucible.no_such_api("dead")
        return { name = "raiser", version = "0.1.0" }
    "#,
    )
    .unwrap();

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("load_plugins is fail-open per plugin");

    // User-shaped registration: no loading marker, so it belongs to no plugin.
    let lua = loader.plugin_lua();
    lua.load(r#"crucible.on("pre_tool_call", function() _G.user_handler_ran = true end)"#)
        .exec()
        .expect("user registration");

    assert!(
        loader.reload_plugin("raiser").await.is_err(),
        "reloading a plugin that raises must report failure"
    );

    // The user's handler must still dispatch after the dead plugin's reload.
    let handlers = loader
        .plugin_handlers()
        .runtime_handlers_for("pre_tool_call", None);
    let event = crucible_core::events::SessionEvent::Custom {
        name: "pre_tool_call".to_string(),
        payload: serde_json::json!({}),
    };
    for handler in &handlers {
        loader
            .plugin_handlers()
            .execute_runtime_handler(&lua, &handler.name, &event, None)
            .await
            .expect("dispatch");
    }
    let ran = lua
        .globals()
        .get::<Option<bool>>("user_handler_ran")
        .expect("read flag")
        .unwrap_or(false);
    assert!(ran, "reloading the dead plugin deleted the user's handler");
}

/// Executing a plugin twice (= one reload) must leave exactly one copy of its
/// session hooks, measured by FIRING them, which exercises the executor sync
/// path (`fire_session_start` replaces the cached hook Vec from the Lua table
/// on every fire, so clearing the table is sufficient — this pins that).
/// oci's `on_session_start` owns a container isolation boundary and is
/// `required = true`; running it twice per session is not cosmetic.
#[tokio::test]
async fn re_executing_a_plugin_fires_its_session_hooks_exactly_once() {
    use crucible_lua::{Session, SessionConfigRpc};

    struct TestRpc;
    impl SessionConfigRpc for TestRpc {}

    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("hooker");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("init.lua"),
        r#"
        crucible.on_session_start(function(s)
            _G.start_count = (_G.start_count or 0) + 1
        end)
        return { name = "hooker", version = "0.1.0" }
    "#,
    )
    .unwrap();

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("load");
    loader.reload_plugin("hooker").await.expect("reload");

    let session = Session::new("hook-once".to_string());
    session.bind(Box::new(TestRpc));
    loader
        .fire_session_start(&session)
        .await
        .expect("fire start hooks");

    let count: i64 = loader
        .plugin_lua()
        .load("return _G.start_count or 0")
        .eval()
        .expect("read counter");
    assert_eq!(
        count, 1,
        "one reload must not leave a second copy of the session hook"
    );
}

/// The install flow is "write the plugin dir, then call `load_plugins` again":
/// the new plugin must come up without disturbing Active ones — `load_all`
/// skips Active plugins (`AlreadyLoaded`), so an installed neighbour must not
/// re-execute anyone's init.lua. NOTE: plugins in state Error ARE retried by
/// every load_all pass — deliberate (installing a plugin retries your broken
/// ones) — so this fixture contains no errored plugins.
#[tokio::test]
async fn a_second_load_plugins_call_picks_up_a_new_plugin_without_disturbing_active_ones() {
    let tmp = tempfile::TempDir::new().unwrap();
    let write_plugin = |name: &str| {
        let dir = tmp.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("init.lua"),
            format!(
                r#"
                _G.{name}_exec_count = (_G.{name}_exec_count or 0) + 1
                return {{
                    name = "{name}",
                    version = "0.1.0",
                    tools = {{ {name}_probe = {{ description = "x", fn = function() return "t" end }} }},
                }}"#
            ),
        )
        .unwrap();
    };

    write_plugin("alpha");
    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("first load");

    // The install flow: a new plugin dir appears, load_plugins runs again.
    write_plugin("beta");
    loader
        .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("second load");

    let info = loader.loaded_plugin_info();
    let state = |name: &str| {
        info.iter()
            .find(|p| p["name"] == name)
            .unwrap_or_else(|| panic!("'{name}' missing from plugin info: {info:#?}"))["state"]
            .clone()
    };
    assert_eq!(state("alpha"), "Active");
    assert_eq!(state("beta"), "Active");
    let tools = loader.plugin_registry().tool_names();
    assert!(tools.contains("alpha_probe"), "got: {tools:?}");
    assert!(tools.contains("beta_probe"), "got: {tools:?}");

    let exec_count = |name: &str| -> i64 {
        loader
            .plugin_lua()
            .load(format!("return _G.{name}_exec_count or 0"))
            .eval()
            .expect("read counter")
    };
    assert_eq!(
        exec_count("alpha"),
        1,
        "installing beta must not re-execute alpha's init.lua"
    );
    assert_eq!(exec_count("beta"), 1);
}

/// remove = deactivate + forget: nothing registered, nothing running, not in
/// `plugin.list` — and a REINSTALL works. `discover()` skips names still in
/// the manager map, so without `forget` a removed plugin stayed listed forever
/// and reinstalling it loaded nothing while reporting success.
#[tokio::test]
async fn removing_then_reinstalling_a_plugin_registers_its_tools_again() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("comeback");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("init.lua"),
        r#"
        crucible.on("pre_tool_call", function() return nil end)
        return {
            name = "comeback",
            version = "0.1.0",
            tools = { comeback_probe = { description = "x", fn = function() return "t" end } },
        }
    "#,
    )
    .unwrap();

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("load");
    assert!(loader
        .plugin_registry()
        .tool_names()
        .contains("comeback_probe"));

    loader
        .deactivate_and_forget_plugin("comeback")
        .await
        .expect("remove");

    assert!(
        !loader
            .plugin_registry()
            .tool_names()
            .contains("comeback_probe"),
        "a removed plugin's tools must be unregistered"
    );
    assert_eq!(loader.plugin_handlers().plugin_handler_count("comeback"), 0);
    assert!(
        !loader
            .loaded_plugin_info()
            .iter()
            .any(|p| p["name"] == "comeback"),
        "a removed plugin must leave plugin.list"
    );

    // Reinstall: the dir is still there (removal of files is plugin_ops'
    // business); a fresh load_plugins pass must bring the plugin back whole.
    loader
        .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("reinstall");
    let info = loader.loaded_plugin_info();
    let entry = info
        .iter()
        .find(|p| p["name"] == "comeback")
        .expect("reinstalled plugin listed");
    assert_eq!(entry["state"], "Active", "got: {entry}");
    assert!(
        loader
            .plugin_registry()
            .tool_names()
            .contains("comeback_probe"),
        "reinstall must register the plugin's tools again"
    );
}

/// The inverse: a handler registered inside `setup()` IS owned by the plugin.
/// `setup()` used to run after the loading marker was cleared, so its
/// registrations were unowned — reload could not remove them and appended
/// another copy per reload.
#[tokio::test]
async fn a_setup_registered_handler_is_owned_so_reload_does_not_duplicate_it() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("setupper");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("init.lua"),
        r#"
        return {
            name = "setupper",
            version = "0.1.0",
            setup = function()
                crucible.on("pre_tool_call", function() return nil end)
            end,
        }
    "#,
    )
    .unwrap();

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("load");
    loader.reload_plugin("setupper").await.expect("reload");

    assert_eq!(
        loader.plugin_handlers().plugin_handler_count("setupper"),
        1,
        "setup-registered handlers must be attributed to the plugin"
    );
    assert_eq!(
        loader
            .plugin_handlers()
            .runtime_handlers_for("pre_tool_call", None)
            .len(),
        1,
        "one reload must not leave a second copy of the handler"
    );
}

/// A reload that fails inside `PluginManager` — the everyday trigger is
/// saving init.lua with a syntax error while the watcher is on — must leave
/// the plugin inert and marked Error. `unload` succeeds (state Discovered),
/// then `load`'s sandbox eval fails; bailing there left the previous
/// generation's tools, handlers, hooks and services fully live while
/// plugin.list said `Discovered` with no error — "broken looks exactly like
/// working", the failure this branch exists to eliminate.
#[tokio::test]
async fn a_reload_that_fails_in_the_manager_leaves_the_plugin_inert_and_errored() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("brittle");
    std::fs::create_dir_all(&dir).unwrap();
    let good = r#"
        crucible.on("turn:complete", function() end)
        return {
            name = "brittle",
            version = "0.1.0",
            tools = { brittle_probe = { description = "x", fn = function() return "t" end } },
        }
    "#;
    std::fs::write(dir.join("init.lua"), good).unwrap();

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("initial load");
    assert!(loader
        .plugin_registry()
        .tool_names()
        .contains("brittle_probe"));

    // The operator saves a syntax error; the watcher reloads.
    std::fs::write(dir.join("init.lua"), "this is not lua").unwrap();
    let err = loader.reload_plugin("brittle").await;
    assert!(
        err.is_err(),
        "a reload that cannot parse must report failure"
    );

    let info = loader.loaded_plugin_info();
    let entry = info
        .iter()
        .find(|p| p["name"] == "brittle")
        .expect("still listed for diagnosis");
    assert_eq!(entry["state"], "Error", "got: {entry}");
    assert!(
        entry["last_error"].as_str().is_some_and(|e| !e.is_empty()),
        "the failure must be visible in plugin.list: {entry}"
    );
    assert_eq!(
        loader.plugin_handlers().plugin_handler_count("brittle"),
        0,
        "Error must mean inert: no live handlers from the previous generation"
    );
    assert!(
        !loader
            .plugin_registry()
            .tool_names()
            .contains("brittle_probe"),
        "Error must mean inert: no tools from the previous generation"
    );
}

/// Auth hooks ride the same owner-tag contract as session hooks: executing a
/// plugin twice (= one reload) leaves exactly one copy of its
/// `crucible.on_provider_auth` registration. They were the one hook family
/// left untagged — one copy accumulated per reload, forever.
#[tokio::test]
async fn re_executing_a_plugin_does_not_duplicate_its_provider_auth_hooks() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join("author");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("init.lua"),
        r#"
        crucible.on_provider_auth(function(ctx) return nil end)
        return { name = "author", version = "0.1.0" }
    "#,
    )
    .unwrap();

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .load_plugins(&[(tmp.path().to_path_buf(), PluginSource::Runtime)])
        .await
        .expect("load");
    loader.reload_plugin("author").await.expect("reload");

    let hooks = crucible_lua::get_provider_auth_hooks(&loader.plugin_lua()).expect("hooks");
    assert_eq!(hooks.len(), 1, "one reload must not mean two auth hooks");
}
