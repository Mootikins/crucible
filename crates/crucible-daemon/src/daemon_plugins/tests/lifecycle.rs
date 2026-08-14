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
