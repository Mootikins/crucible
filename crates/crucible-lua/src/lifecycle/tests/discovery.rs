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

#[test]
fn test_empty_directory_not_discovered() {
    let temp = TempDir::new().unwrap();
    let empty_dir = temp.path().join("empty-dir");
    std::fs::create_dir_all(&empty_dir).unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover().unwrap();

    assert!(discovered.is_empty());
}
