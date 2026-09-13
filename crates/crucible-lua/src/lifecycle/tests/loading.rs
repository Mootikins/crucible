use super::{create_test_plugin, create_test_plugin_with_source, setup_emitter_manager_with_paths};
use crate::lifecycle::PluginManager;
use crate::manifest::PluginState;
use mlua::Lua;
use tempfile::TempDir;

#[test]
fn test_load_plugin() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "test-plugin", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover(&Lua::new()).unwrap();
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
        plugin_dir.join("init.lua"),
        "return { name = 'disabled-plugin', version = '1.0.0' }\n",
    )
    .unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover(&Lua::new()).unwrap();
    // Disabling is the operator's act. A plugin is enabled by BEING on the
    // runtimepath; there is no field it declares about itself.
    manager.disable("disabled-plugin").unwrap();
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

#[test]
fn test_unload_plugin() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "unload-test", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover(&Lua::new()).unwrap();
    manager.load("unload-test").unwrap();

    manager.unload("unload-test").unwrap();
    let plugin = manager.get("unload-test").unwrap();
    assert_eq!(plugin.state, PluginState::Discovered);
}

#[test]
fn test_reload_plugin() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "reload-test", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover(&Lua::new()).unwrap();
    manager.load("reload-test").unwrap();

    manager.reload_plugin("reload-test").unwrap();
    let plugin = manager.get("reload-test").unwrap();
    assert_eq!(plugin.state, PluginState::Active);
}

#[test]
fn test_enable_disable() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "toggle-test", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover(&Lua::new()).unwrap();
    manager.load("toggle-test").unwrap();

    manager.disable("toggle-test").unwrap();
    let plugin = manager.get("toggle-test").unwrap();
    assert_eq!(plugin.state, PluginState::Disabled);

    // `enable` clears the disabled state; loading is a separate act. It used
    // to flip a manifest field that `load` consulted, so the two were one
    // step. There is no such field now — being on the runtimepath is what
    // enables a plugin.
    manager.enable("toggle-test").unwrap();
    manager.load("toggle-test").unwrap();
    let plugin = manager.get("toggle-test").unwrap();
    assert_eq!(plugin.state, PluginState::Active);
}

#[test]
fn test_disabled_plugin_skipped() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("disabled-plugin");
    std::fs::create_dir_all(&plugin_dir).unwrap();

    std::fs::write(
        plugin_dir.join("init.lua"),
        "return { name = 'disabled-plugin', version = '1.0.0' }\n",
    )
    .unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover(&Lua::new()).unwrap();
    // Disabling is the OPERATOR\'s call, so it goes through the API a
    // config drives, not a field the plugin declares about itself.
    manager.disable("disabled-plugin").unwrap();
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
        plugin_dir.join("init.lua"),
        "return { name = 'inactive', version = '1.0.0' }\n",
    )
    .unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover(&Lua::new()).unwrap();
    manager.disable("inactive").unwrap();
    manager.load_all().unwrap();

    let active: Vec<_> = manager.active_plugins().collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].name(), "active");
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
    manager.discover(&Lua::new()).unwrap();

    manager.load("legacy-plugin").unwrap();

    let plugin = manager.get("legacy-plugin").unwrap();
    assert_eq!(plugin.state, PluginState::Active, "plugin should be Active");

    manager.unload("legacy-plugin").unwrap();

    assert!(
        manager.error_log().is_empty(),
        "no errors should be logged for clean plugin"
    );
}

/// A plugin written with the extension Luau's own tooling expects must be
/// discovered, loaded and executed exactly like a `.lua` one.
///
/// Manifest-LESS on purpose: with no `main` field to read, the loader has to
/// find the entry point itself, and that is the path that used to guess
/// `init.lua` and then fail to open it.
#[test]
fn a_luau_plugin_is_discovered_and_loads() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("luau-plugin");
    std::fs::create_dir_all(plugin_dir.join("lua")).unwrap();
    std::fs::write(
        plugin_dir.join("init.luau"),
        "return { name = 'luau-plugin', version = '1.0.0', description = 'a .luau plugin' }\n",
    )
    .unwrap();
    // A `.luau` submodule beside it. `require` resolution for both extensions
    // is proved in `modules::extension_tests`; what matters here is that a
    // directory containing one is still discovered as a plugin.
    std::fs::write(
        plugin_dir.join("lua/helper.luau"),
        "return { text = 'resolved through .luau' }\n",
    )
    .unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover(&Lua::new()).unwrap();
    assert!(
        discovered.iter().any(|name| name == "luau-plugin"),
        "a directory with an init.luau is a plugin: {discovered:?}"
    );

    manager.load("luau-plugin").unwrap();
    let plugin = manager.get("luau-plugin").unwrap();
    assert_eq!(plugin.state, PluginState::Active);
    assert!(
        plugin.main_path().ends_with("init.luau"),
        "the entry point must be the file that is really there, got {}",
        plugin.main_path().display()
    );
}

/// A directory with two entry points is REPORTED, never silently dropped.
///
/// `init_file(&path).ok().flatten()` turned the collision into "not a
/// plugin", so a directory that had loaded from `init.lua` for months
/// vanished the moment someone added `init.luau` beside it — discovered by
/// nothing, logged by nothing.
#[test]
fn a_plugin_with_both_entry_points_is_a_discovery_error() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("ambiguous");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join("init.lua"),
        "return { name = 'ambiguous' }\n",
    )
    .unwrap();
    std::fs::write(
        plugin_dir.join("init.luau"),
        "return { name = 'ambiguous' }\n",
    )
    .unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover(&Lua::new()).unwrap();
    assert!(
        !discovered.iter().any(|name| name == "ambiguous"),
        "a directory with two entry points must not load one of them at random"
    );

    let reported = manager
        .discovery_errors()
        .iter()
        .find(|e| e.path.ends_with("ambiguous"))
        .expect("the collision must be RECORDED, not dropped");
    assert!(
        reported.error.contains("init.luau") && reported.error.contains("init.lua"),
        "the report must name both files: {}",
        reported.error
    );
}

/// Two entry points answering to one name is refused, not resolved. Picking
/// one silently means an edit to the other appears to do nothing.
#[test]
fn a_plugin_with_both_entry_points_is_reported() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("ambiguous");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join("init.lua"),
        "return { name = 'ambiguous' }\n",
    )
    .unwrap();
    std::fs::write(
        plugin_dir.join("init.luau"),
        "return { name = 'ambiguous' }\n",
    )
    .unwrap();

    // No checker: this test is about the COLLISION, and what a typecheck adds
    // depends on what the machine has installed.
    let report = crate::check_plugin_using(&plugin_dir, None, false, &crate::CheckerChoice::None)
        .expect("check runs");
    assert!(
        report.findings.iter().any(|f| {
            let text = f.to_string();
            text.contains("init.luau") && text.contains("init.lua")
        }),
        "the collision must be reported and must name both files: {:?}",
        report.findings
    );
}
