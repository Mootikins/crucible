use super::{create_test_plugin, create_test_plugin_with_source, setup_emitter_manager_with_paths};
use crate::lifecycle::PluginManager;
use crate::manifest::PluginState;
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_load_plugin() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "test-plugin", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("test-plugin").unwrap();

    let plugin = manager.get("test-plugin").unwrap();
    assert_eq!(plugin.state, PluginState::Active);
}

/// `enabled: false` is the only user-facing kill switch for a plugin, and
/// the documented remediation for a misbehaving one. `load` marked such a
/// plugin Disabled and then returned `Ok(())`, so `load_all` counted it as
/// loaded and the daemon went on to execute it — registering its tools and
/// services while `plugin.list` reported it Disabled.
#[test]
fn load_all_omits_a_disabled_plugin_from_the_loaded_list() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = create_test_plugin(temp.path(), "disabled-plugin", "1.0.0");
    std::fs::write(
        plugin_dir.join("plugin.yaml"),
        "name: disabled-plugin\nversion: \"1.0.0\"\nmain: init.lua\nenabled: false\n",
    )
    .unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    let loaded = manager.load_all().unwrap();

    assert!(
        !loaded.contains(&"disabled-plugin".to_string()),
        "a disabled plugin must not be reported as loaded: {loaded:?}"
    );
    assert_eq!(
        manager.get("disabled-plugin").unwrap().state,
        PluginState::Disabled
    );
}

/// The state flag is not enough on its own — what matters is that nothing
/// the plugin declares becomes reachable.
#[test]
fn a_disabled_plugin_registers_no_tools() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = create_test_plugin(temp.path(), "disabled-tools", "1.0.0");
    std::fs::write(
        plugin_dir.join("plugin.yaml"),
        "name: disabled-tools\nversion: \"1.0.0\"\nmain: init.lua\nenabled: false\n",
    )
    .unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load_all().unwrap();

    assert_eq!(
        manager.tools().len(),
        0,
        "a disabled plugin must register nothing"
    );
}

#[test]
fn test_load_discovers_tools() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "tool-plugin", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("tool-plugin").unwrap();

    assert_eq!(manager.tools().len(), 1);
    assert_eq!(manager.tools()[0].name, "test_tool");
}

#[test]
fn test_unload_plugin() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "unload-test", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("unload-test").unwrap();
    assert_eq!(manager.tools().len(), 1);

    manager.unload("unload-test").unwrap();
    let plugin = manager.get("unload-test").unwrap();
    assert_eq!(plugin.state, PluginState::Discovered);
    assert_eq!(manager.tools().len(), 0);
}

#[test]
fn test_reload_plugin() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "reload-test", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("reload-test").unwrap();

    manager.reload("reload-test").unwrap();
    let plugin = manager.get("reload-test").unwrap();
    assert_eq!(plugin.state, PluginState::Active);
}

#[test]
fn test_enable_disable() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "toggle-test", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("toggle-test").unwrap();

    manager.disable("toggle-test").unwrap();
    let plugin = manager.get("toggle-test").unwrap();
    assert_eq!(plugin.state, PluginState::Disabled);

    manager.enable("toggle-test").unwrap();
    let plugin = manager.get("toggle-test").unwrap();
    assert_eq!(plugin.state, PluginState::Active);
}

#[test]
fn test_cannot_unload_if_depended_upon() {
    use super::create_plugin_with_deps;
    use crate::lifecycle::LifecycleError;
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "core", "1.0.0");
    create_plugin_with_deps(temp.path(), "extension", &["core"]);

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load_all().unwrap();

    let result = manager.unload("core");
    assert!(matches!(result, Err(LifecycleError::LoadError(_))));
}

#[test]
fn test_disabled_plugin_skipped() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("disabled-plugin");
    std::fs::create_dir_all(&plugin_dir).unwrap();

    let manifest = r#"
name: disabled-plugin
version: "1.0.0"
enabled: false
"#;
    std::fs::write(plugin_dir.join("plugin.yaml"), manifest).unwrap();
    std::fs::write(plugin_dir.join("init.lua"), "-- empty").unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("disabled-plugin").unwrap();

    let plugin = manager.get("disabled-plugin").unwrap();
    assert_eq!(plugin.state, PluginState::Disabled);
}

#[test]
fn test_active_plugins_iterator() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "active", "1.0.0");

    let plugin_dir = temp.path().join("inactive");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join("plugin.yaml"),
        "name: inactive\nversion: \"1.0.0\"\nenabled: false",
    )
    .unwrap();
    std::fs::write(plugin_dir.join("init.lua"), "").unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load_all().unwrap();

    let active: Vec<_> = manager.active_plugins().collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].name(), "active");
}

/// `PluginManager` loads the shipped tree: Lua and Fennel, tools, commands and
/// views.
///
/// These three plugins used to live under `docs/plugins/` as "documentation
/// examples" that CI did not run, which is how `graph-view` reached the tree
/// with a `.fnl` main that had never executed in the daemon at all. They ship
/// now, so this walks `runtime/plugins/` — the same directory
/// `every_shipped_plugin_executes` drives through the real loader.
#[test]
fn test_load_shipped_plugins() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let plugins_dir = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("runtime")
        .join("plugins");

    if !plugins_dir.exists() {
        panic!(
            "Shipped plugins directory not found: {}",
            plugins_dir.display()
        );
    }

    let mut manager = PluginManager::new().with_search_paths(vec![plugins_dir.clone()]);

    let discovered = manager.discover().unwrap();
    assert!(
        discovered.len() >= 3,
        "Expected at least 3 shipped plugins, found {}: {:?}",
        discovered.len(),
        discovered
    );

    assert!(
        discovered.contains(&"todo-list".to_string()),
        "todo-list plugin not discovered"
    );
    assert!(
        discovered.contains(&"daily-notes".to_string()),
        "daily-notes plugin not discovered"
    );
    assert!(
        discovered.contains(&"graph-view".to_string()),
        "graph-view plugin not discovered"
    );

    let loaded = manager.load_all().unwrap();
    assert!(
        loaded.len() >= 3,
        "Expected at least 3 plugins loaded, got {}: {:?}",
        loaded.len(),
        loaded
    );

    for name in &["todo-list", "daily-notes", "graph-view"] {
        let plugin = manager
            .get(name)
            .unwrap_or_else(|| panic!("{} should be loaded", name));
        assert_eq!(
            plugin.state,
            PluginState::Active,
            "{} should be active",
            name
        );
    }

    assert!(
        !manager.tools().is_empty(),
        "Should have discovered tools from plugins"
    );
    assert!(
        !manager.commands().is_empty(),
        "Should have discovered commands from plugins"
    );

    let tool_names: Vec<_> = manager.tools().iter().map(|t| &t.name).collect();
    assert!(
        tool_names.contains(&&"tasks_list".to_string()),
        "tasks_list tool not found"
    );
    assert!(
        tool_names.contains(&&"daily_create".to_string()),
        "daily_create tool not found"
    );
    assert!(
        tool_names.contains(&&"graph_stats".to_string()),
        "graph_stats tool not found"
    );
}

#[test]
fn test_full_lifecycle_with_hooks_and_cleanup() {
    let temp = TempDir::new().unwrap();
    create_test_plugin_with_source(
        temp.path(),
        "full-plugin",
        "1.0.0",
        r#"
        return {
            on_load = function()
                _G.on_load_fired = true
                cru.emitter.global():on("test_event", function() end, "full-plugin")
            end,
            on_unload = function()
                _G.on_unload_fired = true
            end,
        }
    "#,
    );

    let mut manager = setup_emitter_manager_with_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("full-plugin").unwrap();

    let on_load_fired = manager
        .eval_runtime::<bool>("return _G.on_load_fired == true")
        .unwrap();
    assert!(on_load_fired, "on_load should have fired");

    let count = manager
        .eval_runtime::<i64>("return cru.emitter.global():count('test_event')")
        .unwrap();
    assert_eq!(count, 1, "emitter listener should be registered");

    manager.unload("full-plugin").unwrap();

    let on_unload_fired = manager
        .eval_runtime::<bool>("return _G.on_unload_fired == true")
        .unwrap();
    assert!(on_unload_fired, "on_unload should have fired");

    let count_after = manager
        .eval_runtime::<i64>("return cru.emitter.global():count('test_event')")
        .unwrap();
    assert_eq!(
        count_after, 0,
        "emitter listener should be cleaned up after unload"
    );

    let plugin = manager.get("full-plugin").unwrap();
    assert_eq!(plugin.state, PluginState::Discovered);
}

#[test]
fn test_reload_full_cycle() {
    let temp = TempDir::new().unwrap();
    create_test_plugin_with_source(
        temp.path(),
        "reload-plugin",
        "1.0.0",
        r#"
        _G.load_count = (_G.load_count or 0)
        _G.unload_count = (_G.unload_count or 0)
        return {
            on_load = function()
                _G.load_count = _G.load_count + 1
                cru.emitter.global():on("reload_event", function() end, "reload-plugin")
            end,
            on_unload = function()
                _G.unload_count = _G.unload_count + 1
            end,
        }
    "#,
    );

    let mut manager = setup_emitter_manager_with_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("reload-plugin").unwrap();

    let count = manager
        .eval_runtime::<i64>("return cru.emitter.global():count('reload_event')")
        .unwrap();
    assert_eq!(count, 1);

    manager.reload("reload-plugin").unwrap();

    let unload_count = manager
        .eval_runtime::<i64>("return _G.unload_count")
        .unwrap();
    assert_eq!(
        unload_count, 1,
        "on_unload should fire exactly once during reload"
    );

    let load_count = manager.eval_runtime::<i64>("return _G.load_count").unwrap();
    assert_eq!(
        load_count, 2,
        "on_load should fire once per successful load"
    );

    let count_after = manager
        .eval_runtime::<i64>("return cru.emitter.global():count('reload_event')")
        .unwrap();
    assert_eq!(
        count_after, 1,
        "emitter should have exactly 1 listener after reload"
    );
}

#[test]
fn test_multiple_plugins_isolated() {
    let temp = TempDir::new().unwrap();
    create_test_plugin_with_source(
        temp.path(),
        "plugin-a",
        "1.0.0",
        r#"
        return {
            on_load = function()
                cru.emitter.global():on("shared_event", function() end, "plugin-a")
            end,
        }
    "#,
    );
    create_test_plugin_with_source(
        temp.path(),
        "plugin-b",
        "1.0.0",
        r#"
        return {
            on_load = function()
                cru.emitter.global():on("shared_event", function() end, "plugin-b")
            end,
        }
    "#,
    );

    let mut manager = setup_emitter_manager_with_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("plugin-a").unwrap();
    manager.load("plugin-b").unwrap();

    let count = manager
        .eval_runtime::<i64>("return cru.emitter.global():count('shared_event')")
        .unwrap();
    assert_eq!(count, 2, "both plugins should have listeners");

    manager.unload("plugin-a").unwrap();

    let count_after = manager
        .eval_runtime::<i64>("return cru.emitter.global():count('shared_event')")
        .unwrap();
    assert_eq!(
        count_after, 1,
        "only plugin-b's listener should remain after plugin-a unload"
    );
}

#[test]
fn test_backward_compat_no_hooks() {
    let temp = TempDir::new().unwrap();
    create_test_plugin_with_source(
        temp.path(),
        "legacy-plugin",
        "1.0.0",
        r#"
        return {}
    "#,
    );

    let mut manager = setup_emitter_manager_with_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();

    manager.load("legacy-plugin").unwrap();

    let plugin = manager.get("legacy-plugin").unwrap();
    assert_eq!(plugin.state, PluginState::Active, "plugin should be Active");

    manager.unload("legacy-plugin").unwrap();

    assert!(
        manager.error_log().is_empty(),
        "no errors should be logged for clean plugin"
    );
}
