//! The state the manager records. Activation itself is the daemon's act,
//! proved in `crucible-daemon/src/daemon_plugins/tests/activate.rs`; these
//! tests cover the transitions the daemon asks the registry to record.

use super::create_test_plugin;
use crate::lifecycle::PluginManager;
use crate::manifest::PluginState;
use mlua::Lua;
use tempfile::TempDir;

#[test]
fn mark_active_then_unload_records_active_then_discovered() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "unload-test", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover(&Lua::new()).unwrap();
    assert_eq!(
        manager.get("unload-test").unwrap().state,
        PluginState::Discovered
    );

    manager.mark_active("unload-test");
    assert_eq!(
        manager.get("unload-test").unwrap().state,
        PluginState::Active
    );

    manager.unload("unload-test").unwrap();
    assert_eq!(
        manager.get("unload-test").unwrap().state,
        PluginState::Discovered
    );
}

#[test]
fn mark_error_records_the_reason_and_mark_active_clears_it() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "flaky", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover(&Lua::new()).unwrap();

    manager.mark_error("flaky", "setup raised");
    let plugin = manager.get("flaky").unwrap();
    assert_eq!(plugin.state, PluginState::Error);
    assert_eq!(plugin.last_error.as_deref(), Some("setup raised"));

    manager.mark_active("flaky");
    let plugin = manager.get("flaky").unwrap();
    assert_eq!(plugin.state, PluginState::Active);
    assert_eq!(plugin.last_error, None);
}

#[test]
fn disable_then_enable_records_disabled_then_discovered() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "toggle-test", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover(&Lua::new()).unwrap();
    manager.mark_active("toggle-test");

    manager.disable("toggle-test").unwrap();
    assert_eq!(
        manager.get("toggle-test").unwrap().state,
        PluginState::Disabled
    );

    // `enable` clears the disabled state; activation is a separate act,
    // and the daemon's.
    manager.enable("toggle-test").unwrap();
    assert_eq!(
        manager.get("toggle-test").unwrap().state,
        PluginState::Discovered
    );
}

#[test]
fn forget_drops_the_entry_so_a_reinstall_is_discovered_again() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "comeback", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover(&Lua::new()).unwrap();
    manager.mark_active("comeback");

    manager.forget("comeback");
    assert!(manager.get("comeback").is_none());

    let discovered = manager.discover(&Lua::new()).unwrap();
    assert_eq!(discovered, vec!["comeback"]);
}

/// A plugin written with the extension Luau's own tooling expects is
/// discovered exactly like a `.lua` one, and its entry point is the file
/// that is really there.
///
/// Manifest-LESS on purpose: with no `main` field to read, the loader has to
/// find the entry point itself, and that is the path that used to guess
/// `init.lua` and then fail to open it.
#[test]
fn a_luau_plugin_is_discovered_with_its_real_entry_point() {
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

    let plugin = manager.get("luau-plugin").unwrap();
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
