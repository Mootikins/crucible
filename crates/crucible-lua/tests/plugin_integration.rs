//! Integration tests for the plugin system
//!
//! Tests the manager's registry of discovered plugins:
//! 1. Plugin discovery from directories
//! 2. The state a load and an unload record
//!
//! The manager runs no plugin code. What a plugin exports is read at
//! activation, in the daemon (`crucible-daemon/src/daemon_plugins/tests`).

use crucible_lua::{PluginManager, PluginState};
use mlua::Lua;
use std::fs;
use tempfile::TempDir;

fn create_plugin_structure(
    base: &std::path::Path,
    name: &str,
    version: &str,
) -> std::path::PathBuf {
    let plugin_dir = base.join(name);
    fs::create_dir_all(&plugin_dir).unwrap();

    let lua = format!(
        r#"
local M = {{}}
function M.test_tool(args)
    return {{ result = "ok" }}
end
return {{
    name = "{}",
    version = "{}",
    tools = {{
        test_tool = {{
            desc = "Test tool",
            params = {{
                {{ name = "query", type = "string", desc = "Search query" }},
            }},
            fn = M.test_tool,
        }},
    }},
}}
"#,
        name, version
    );

    fs::write(plugin_dir.join("init.lua"), lua).unwrap();

    plugin_dir
}

// ============================================================================
// PLUGIN DISCOVERY
// ============================================================================

#[test]
fn test_discover_single_plugin() {
    let temp = TempDir::new().unwrap();
    create_plugin_structure(temp.path(), "my-plugin", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover(&Lua::new()).unwrap();

    assert_eq!(discovered.len(), 1);
    assert!(discovered.contains(&"my-plugin".to_string()));
}

#[test]
fn test_discover_multiple_plugins() {
    let temp = TempDir::new().unwrap();
    create_plugin_structure(temp.path(), "plugin-a", "1.0.0");
    create_plugin_structure(temp.path(), "plugin-b", "2.0.0");
    create_plugin_structure(temp.path(), "plugin-c", "0.1.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover(&Lua::new()).unwrap();

    assert_eq!(discovered.len(), 3);
    assert!(discovered.contains(&"plugin-a".to_string()));
    assert!(discovered.contains(&"plugin-b".to_string()));
    assert!(discovered.contains(&"plugin-c".to_string()));
}

#[test]
fn test_discover_ignores_invalid_plugins() {
    let temp = TempDir::new().unwrap();

    // Valid: an entry file that returns a spec table.
    create_plugin_structure(temp.path(), "valid-plugin", "1.0.0");

    // Valid: an entry file that declares nothing. The directory name is the
    // identity, so a bare entry file is still a plugin.
    let bare_dir = temp.path().join("bare-plugin");
    fs::create_dir_all(&bare_dir).unwrap();
    fs::write(bare_dir.join("init.lua"), "-- code").unwrap();

    // Invalid: a directory whose only file is not an entry file. Nothing but
    // `init.luau` or `init.lua` identifies a plugin.
    let no_entry_dir = temp.path().join("invalid-no-entry");
    fs::create_dir_all(&no_entry_dir).unwrap();
    fs::write(no_entry_dir.join("README.md"), "not a plugin").unwrap();

    // Invalid: an empty directory.
    let empty_dir = temp.path().join("empty-dir");
    fs::create_dir_all(&empty_dir).unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover(&Lua::new()).unwrap();

    assert_eq!(discovered.len(), 2, "discovered: {discovered:?}");
    assert!(discovered.contains(&"valid-plugin".to_string()));
    assert!(discovered.contains(&"bare-plugin".to_string()));
}

// ============================================================================
// PLUGIN LOADING
// ============================================================================

#[test]
fn test_load_plugin() {
    let temp = TempDir::new().unwrap();
    create_plugin_structure(temp.path(), "loadable", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover(&Lua::new()).unwrap();

    manager.load("loadable").unwrap();

    let plugin = manager.get("loadable").unwrap();
    assert_eq!(plugin.state, PluginState::Active);
}

#[test]
fn test_load_all_plugins() {
    let temp = TempDir::new().unwrap();
    create_plugin_structure(temp.path(), "plugin-1", "1.0.0");
    create_plugin_structure(temp.path(), "plugin-2", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover(&Lua::new()).unwrap();

    let loaded = manager.load_all().unwrap();

    assert_eq!(loaded.len(), 2);
    assert!(loaded.contains(&"plugin-1".to_string()));
    assert!(loaded.contains(&"plugin-2".to_string()));
}

// ============================================================================
// PLUGIN UNLOADING
// ============================================================================

#[test]
fn test_unload_plugin() {
    let temp = TempDir::new().unwrap();
    create_plugin_structure(temp.path(), "unloadable", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover(&Lua::new()).unwrap();
    manager.load("unloadable").unwrap();

    manager.unload("unloadable").unwrap();

    let plugin = manager.get("unloadable").unwrap();
    assert_eq!(plugin.state, PluginState::Discovered);
}

// ============================================================================
// PLUGIN MANAGER DEBUG
// ============================================================================

#[test]
fn test_plugin_manager_debug() {
    let manager = PluginManager::new();

    let debug_str = format!("{:?}", manager);
    assert!(debug_str.contains("PluginManager"));
    assert!(debug_str.contains("search_paths"));
}
