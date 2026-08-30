use super::create_test_plugin;
use crate::lifecycle::PluginManager;
use tempfile::TempDir;

#[test]
fn test_discover_plugins() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "plugin-a", "1.0.0");
    create_test_plugin(temp.path(), "plugin-b", "2.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);

    let discovered = manager.discover().unwrap();
    assert_eq!(discovered.len(), 2);
    assert!(discovered.contains(&"plugin-a".to_string()));
    assert!(discovered.contains(&"plugin-b".to_string()));
}

#[test]
fn test_discover_directory_without_manifest() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("my-plugin");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(plugin_dir.join("init.lua"), "-- code").unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover().unwrap();

    assert_eq!(discovered.len(), 1);
    assert!(discovered.contains(&"my-plugin".to_string()));

    let plugin = manager.get("my-plugin").unwrap();
    assert_eq!(plugin.version(), "0.0.0");
}

#[test]
fn test_discover_manifestless_with_spec_override() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("my-plugin");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    // Plugin returns a spec with custom name/version
    std::fs::write(
        plugin_dir.join("init.lua"),
        r#"return { name = "custom-name", version = "1.2.0" }"#,
    )
    .unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("my-plugin").unwrap();

    let plugin = manager.get("my-plugin").unwrap();
    // Name updated from spec (since version was 0.0.0 = directory defaults)
    assert_eq!(plugin.manifest.name, "custom-name");
    assert_eq!(plugin.version(), "1.2.0");
}

#[test]
fn test_manifest_takes_precedence_over_lua_table() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("my-plugin");
    std::fs::create_dir_all(&plugin_dir).unwrap();

    // Manifest with explicit version
    std::fs::write(
        plugin_dir.join("plugin.yaml"),
        "name: my-plugin\nversion: \"2.0.0\"\nmain: init.lua\n",
    )
    .unwrap();

    // Lua spec with different version
    std::fs::write(
        plugin_dir.join("init.lua"),
        r#"return { name = "other-name", version = "9.9.9" }"#,
    )
    .unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("my-plugin").unwrap();

    let plugin = manager.get("my-plugin").unwrap();
    // Manifest values should win (version != "0.0.0", so spec doesn't override)
    assert_eq!(plugin.manifest.name, "my-plugin");
    assert_eq!(plugin.version(), "2.0.0");
}

/// Pure-Lua vendoring is THE supported dependency mechanism: native rocks
/// cannot load into the statically vendored interpreter. A vendored module
/// sits in the plugin's own directory and is required under the plugin's
/// name (`my-plugin/vendored.lua`, required as `"my-plugin.vendored"`), so
/// two plugins vendoring the same library cannot collide. The manager and
/// the daemon resolve it identically, through the plugin root the plugin
/// directory sits in — the parity the two loaders used to lack. The
/// assertion pins the VALUE the vendored module returned, read back out of
/// the VM, so a require that silently resolved elsewhere fails.
#[test]
fn a_vendored_module_resolves_under_the_plugin_namespace() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("my-plugin");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join("vendored.lua"),
        r#"return { version = "3.1.4-vendored" }"#,
    )
    .unwrap();
    std::fs::write(
        plugin_dir.join("init.lua"),
        r#"
local dep = require("my-plugin.vendored")
rawset(_G, "vendored_probe", dep.version)
return { name = "my-plugin", version = "1.0.0" }
"#,
    )
    .unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("my-plugin").unwrap();

    let probe: String = manager.eval_runtime("return vendored_probe").unwrap();
    assert_eq!(probe, "3.1.4-vendored");
}

#[test]
fn test_empty_directory_not_discovered() {
    let temp = TempDir::new().unwrap();
    let empty_dir = temp.path().join("empty-dir");
    std::fs::create_dir_all(&empty_dir).unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover().unwrap();

    assert!(discovered.is_empty());
}

/// `plugins.declare` in the config holds plugin declarations, so a plugin
/// actually named `declare` could never be configured through the store
/// form. Discovery must refuse it by name — a silent discovery would drop
/// that plugin's configuration with no visible reason.
#[test]
fn a_plugin_named_declare_is_a_named_discovery_error_not_a_plugin() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join(crucible_core::config::PLUGINS_DECLARE_KEY);
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(plugin_dir.join("init.lua"), "return { name = 'declare' }").unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover().unwrap();

    assert!(
        discovered.is_empty(),
        "the reserved name must not be discovered: {discovered:?}"
    );
    let errors = manager.discovery_errors();
    assert_eq!(errors.len(), 1, "one named refusal: {errors:?}");
    assert_eq!(errors[0].path, plugin_dir, "the error names the path");
    assert!(
        errors[0].error.contains("reserved")
            && errors[0]
                .error
                .contains(crucible_core::config::PLUGINS_DECLARE_KEY),
        "the error names the reserved name and why: {}",
        errors[0].error
    );
}
