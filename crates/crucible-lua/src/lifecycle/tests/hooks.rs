use super::create_plugin_with_lua;
use crate::lifecycle::PluginManager;
use crate::manifest::PluginState;
use tempfile::TempDir;

#[test]
fn test_on_unload_fires_during_unload() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("unload-hook-test");
    std::fs::create_dir_all(&plugin_dir).unwrap();

    let manifest = r#"
name: unload-hook-test
version: "1.0.0"
main: init.lua
"#;
    std::fs::write(plugin_dir.join("plugin.yaml"), manifest).unwrap();

    let lua = r#"
return {
    name = "unload-hook-test",
    version = "1.0.0",
    on_unload = function()
        _G._unload_fired = true
    end,
}
"#;
    std::fs::write(plugin_dir.join("init.lua"), lua).unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("unload-hook-test").unwrap();

    manager.unload("unload-hook-test").unwrap();

    let fired = manager
        .eval_runtime::<bool>("return _G._unload_fired == true")
        .unwrap_or(false);
    assert!(fired, "on_unload hook should have fired during unload()");
}

#[test]
fn test_on_unload_fires_during_disable() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("disable-hook-test");
    std::fs::create_dir_all(&plugin_dir).unwrap();

    let manifest = r#"
name: disable-hook-test
version: "1.0.0"
main: init.lua
"#;
    std::fs::write(plugin_dir.join("plugin.yaml"), manifest).unwrap();

    let lua = r#"
return {
    name = "disable-hook-test",
    version = "1.0.0",
    on_unload = function()
        _G._unload_fired = true
    end,
}
"#;
    std::fs::write(plugin_dir.join("init.lua"), lua).unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("disable-hook-test").unwrap();

    manager.disable("disable-hook-test").unwrap();

    let fired = manager
        .eval_runtime::<bool>("return _G._unload_fired == true")
        .unwrap_or(false);
    assert!(fired, "on_unload hook should have fired during disable()");
}

#[test]
fn test_on_unload_fires_once_during_reload() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("reload-hook-test");
    std::fs::create_dir_all(&plugin_dir).unwrap();

    let manifest = r#"
name: reload-hook-test
version: "1.0.0"
main: init.lua
"#;
    std::fs::write(plugin_dir.join("plugin.yaml"), manifest).unwrap();

    let lua = r#"
return {
    name = "reload-hook-test",
    version = "1.0.0",
    on_unload = function()
        _G._unload_count = (_G._unload_count or 0) + 1
    end,
}
"#;
    std::fs::write(plugin_dir.join("init.lua"), lua).unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("reload-hook-test").unwrap();

    manager.reload("reload-hook-test").unwrap();

    let count = manager
        .eval_runtime::<i32>("return _G._unload_count or 0")
        .unwrap_or(0);
    assert_eq!(
        count, 1,
        "on_unload hook should fire exactly once during reload()"
    );
}

#[test]
fn test_on_load_fires_after_load() {
    let temp = TempDir::new().unwrap();
    create_plugin_with_lua(
        temp.path(),
        "load-hook-test",
        "1.0.0",
        r#"
return {
    name = "load-hook-test",
    version = "1.0.0",
    on_load = function()
        _G._load_fired = true
    end,
}
"#,
    );
    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("load-hook-test").unwrap();

    let fired = manager
        .eval_runtime::<bool>("return _G._load_fired == true")
        .unwrap_or(false);
    assert!(fired, "on_load hook should fire after load()");

    let plugin = manager.get("load-hook-test").unwrap();
    assert_eq!(
        plugin.state,
        PluginState::Active,
        "plugin should be Active even after on_load fires"
    );
}

#[test]
fn test_on_load_and_on_unload_order_on_reload() {
    let temp = TempDir::new().unwrap();
    create_plugin_with_lua(
        temp.path(),
        "order-test",
        "1.0.0",
        r#"
_G._events = _G._events or {}
return {
    name = "order-test",
    version = "1.0.0",
    on_load = function()
        table.insert(_G._events, "load")
    end,
    on_unload = function()
        table.insert(_G._events, "unload")
    end,
}
"#,
    );
    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("order-test").unwrap();
    manager.reload("order-test").unwrap();

    let events_str = manager
        .eval_runtime::<String>(
            r#"
        return table.concat(_G._events, ",")
    "#,
        )
        .unwrap_or_default();
    assert_eq!(
        events_str, "load,unload,load",
        "expected load → unload → load order"
    );
}

#[test]
fn test_on_load_failure_is_nonfatal() {
    let temp = TempDir::new().unwrap();
    create_plugin_with_lua(
        temp.path(),
        "failing-load-test",
        "1.0.0",
        r#"
return {
    name = "failing-load-test",
    version = "1.0.0",
    on_load = function()
        error("intentional on_load failure")
    end,
}
"#,
    );
    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    let result = manager.load("failing-load-test");
    assert!(
        result.is_ok(),
        "load() should succeed even if on_load fails"
    );

    let plugin = manager.get("failing-load-test").unwrap();
    assert_eq!(
        plugin.state,
        PluginState::Active,
        "plugin should be Active despite on_load error"
    );
}

#[test]
fn test_missing_on_load_still_loads() {
    let temp = TempDir::new().unwrap();
    create_plugin_with_lua(
        temp.path(),
        "no-hooks-test",
        "1.0.0",
        r#"
return {
    name = "no-hooks-test",
    version = "1.0.0",
}
"#,
    );
    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    let result = manager.load("no-hooks-test");
    assert!(
        result.is_ok(),
        "load() should succeed without on_load defined"
    );

    let plugin = manager.get("no-hooks-test").unwrap();
    assert_eq!(plugin.state, PluginState::Active);
}
