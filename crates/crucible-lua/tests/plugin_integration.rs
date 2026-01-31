//! Integration tests for the plugin system
//!
//! Tests the full plugin lifecycle:
//! 1. Plugin discovery from directories
//! 2. Manifest loading and validation
//! 3. Plugin loading with dependency resolution
//! 4. Tool/Command/View/Handler registration
//! 5. Programmatic registration API
//! 6. Plugin unloading and reloading

use crucible_lua::{
    CommandBuilder, HandlerBuilder, PluginManager, PluginState, ToolBuilder, ViewBuilder,
};
use std::fs;
use tempfile::TempDir;

fn create_plugin_structure(
    base: &std::path::Path,
    name: &str,
    version: &str,
) -> std::path::PathBuf {
    let plugin_dir = base.join(name);
    fs::create_dir_all(&plugin_dir).unwrap();

    let manifest = format!(
        "name: \"{}\"\nversion: \"{}\"\nmain: init.lua\n",
        name, version
    );

    fs::write(plugin_dir.join("plugin.yaml"), manifest).unwrap();

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

fn create_plugin_with_dependency(
    base: &std::path::Path,
    name: &str,
    version: &str,
    deps: &[(&str, &str)],
) -> std::path::PathBuf {
    let plugin_dir = base.join(name);
    fs::create_dir_all(&plugin_dir).unwrap();

    let deps_yaml: Vec<String> = deps
        .iter()
        .map(|(n, v)| format!("  - name: \"{}\"\n    version: \"{}\"", n, v))
        .collect();

    let manifest = format!(
        r#"name: "{}"
version: "{}"
main: init.lua
dependencies:
{}
"#,
        name,
        version,
        deps_yaml.join("\n")
    );

    fs::write(plugin_dir.join("plugin.yaml"), manifest).unwrap();
    fs::write(plugin_dir.join("init.lua"), "-- empty").unwrap();

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
    let discovered = manager.discover().unwrap();

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
    let discovered = manager.discover().unwrap();

    assert_eq!(discovered.len(), 3);
    assert!(discovered.contains(&"plugin-a".to_string()));
    assert!(discovered.contains(&"plugin-b".to_string()));
    assert!(discovered.contains(&"plugin-c".to_string()));
}

#[test]
fn test_discover_ignores_invalid_plugins() {
    let temp = TempDir::new().unwrap();

    // Valid plugin with manifest
    create_plugin_structure(temp.path(), "valid-plugin", "1.0.0");

    // Manifest-less plugin: has init.lua but no manifest — now valid via manifest-less discovery
    let manifestless_dir = temp.path().join("manifestless-plugin");
    fs::create_dir_all(&manifestless_dir).unwrap();
    fs::write(manifestless_dir.join("init.lua"), "-- code").unwrap();

    // Invalid: directory with unrecognized manifest file (not plugin.yaml)
    let bad_json_dir = temp.path().join("invalid-bad-json");
    fs::create_dir_all(&bad_json_dir).unwrap();
    fs::write(bad_json_dir.join("plugin.json"), "{ broken json").unwrap();

    // Invalid: empty directory (no init.lua, no manifest)
    let empty_dir = temp.path().join("empty-dir");
    fs::create_dir_all(&empty_dir).unwrap();

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover().unwrap();

    // valid-plugin (manifest) + manifestless-plugin (init.lua) = 2
    assert_eq!(discovered.len(), 2);
    assert!(discovered.contains(&"valid-plugin".to_string()));
    assert!(discovered.contains(&"manifestless-plugin".to_string()));
}

// ============================================================================
// PLUGIN LOADING
// ============================================================================

#[test]
fn test_load_plugin() {
    let temp = TempDir::new().unwrap();
    create_plugin_structure(temp.path(), "loadable", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();

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
    manager.discover().unwrap();

    let loaded = manager.load_all().unwrap();

    assert_eq!(loaded.len(), 2);
    assert!(loaded.contains(&"plugin-1".to_string()));
    assert!(loaded.contains(&"plugin-2".to_string()));
}

#[test]
fn test_load_with_dependencies() {
    let temp = TempDir::new().unwrap();

    // Base plugin
    create_plugin_structure(temp.path(), "base-plugin", "1.0.0");

    // Plugin that depends on base
    create_plugin_with_dependency(
        temp.path(),
        "dependent-plugin",
        "1.0.0",
        &[("base-plugin", ">=1.0.0")],
    );

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();

    // Should automatically load base first
    let loaded = manager.load_all().unwrap();

    assert_eq!(loaded.len(), 2);
    // Base should be loaded before dependent
    let base_idx = loaded.iter().position(|n| n == "base-plugin").unwrap();
    let dep_idx = loaded.iter().position(|n| n == "dependent-plugin").unwrap();
    assert!(base_idx < dep_idx);
}

#[test]
fn test_load_fails_for_missing_dependency() {
    let temp = TempDir::new().unwrap();

    // Plugin that depends on non-existent plugin
    create_plugin_with_dependency(
        temp.path(),
        "orphan-plugin",
        "1.0.0",
        &[("non-existent", ">=1.0.0")],
    );

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();

    let result = manager.load("orphan-plugin");
    assert!(result.is_err());
}

// ============================================================================
// PLUGIN UNLOADING
// ============================================================================

#[test]
fn test_unload_plugin() {
    let temp = TempDir::new().unwrap();
    create_plugin_structure(temp.path(), "unloadable", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("unloadable").unwrap();

    manager.unload("unloadable").unwrap();

    let plugin = manager.get("unloadable").unwrap();
    assert_eq!(plugin.state, PluginState::Discovered);
}

#[test]
fn test_unload_removes_plugin_tools() {
    let temp = TempDir::new().unwrap();
    create_plugin_structure(temp.path(), "tool-plugin", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("tool-plugin").unwrap();

    let tools_before = manager.tools().len();
    assert!(tools_before > 0, "Plugin should have tools");

    manager.unload("tool-plugin").unwrap();

    let tools_after = manager.tools().len();
    assert_eq!(tools_after, 0, "Tools should be removed on unload");
}

#[test]
fn test_cannot_unload_if_depended_upon() {
    let temp = TempDir::new().unwrap();
    create_plugin_structure(temp.path(), "base-plugin", "1.0.0");
    create_plugin_with_dependency(
        temp.path(),
        "dependent",
        "1.0.0",
        &[("base-plugin", ">=1.0.0")],
    );

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load_all().unwrap();

    // Should fail because dependent relies on base
    let result = manager.unload("base-plugin");
    assert!(result.is_err());
}

// ============================================================================
// PROGRAMMATIC REGISTRATION
// ============================================================================

#[test]
fn test_register_tool_programmatically() {
    let mut manager = PluginManager::new();

    let tool = ToolBuilder::new("custom_search")
        .description("A custom search tool")
        .param("query", "string")
        .param_optional("limit", "number")
        .returns("SearchResult[]")
        .build();

    let handle = manager.register_tool(tool, None);

    assert_eq!(manager.tools().len(), 1);
    assert_eq!(manager.tools()[0].name, "custom_search");
    assert_eq!(manager.tools()[0].params.len(), 2);

    assert!(manager.unregister(handle));
    assert_eq!(manager.tools().len(), 0);
}

#[test]
fn test_register_command_programmatically() {
    let mut manager = PluginManager::new();

    let cmd = CommandBuilder::new("tasks")
        .description("Manage tasks")
        .hint("[add|list|done] <args>")
        .param("action", "string")
        .build();

    let handle = manager.register_command(cmd, None);

    assert_eq!(manager.commands().len(), 1);
    assert_eq!(manager.commands()[0].name, "tasks");
    assert!(manager.commands()[0].input_hint.is_some());

    assert!(manager.unregister(handle));
    assert_eq!(manager.commands().len(), 0);
}

#[test]
fn test_register_handler_programmatically() {
    let mut manager = PluginManager::new();

    let handler = HandlerBuilder::new("log_calls", "tool:before")
        .pattern("*")
        .priority(100)
        .build();

    let handle = manager.register_handler(handler, None);

    assert_eq!(manager.handlers().len(), 1);
    assert_eq!(manager.handlers()[0].event_type, "tool:before");
    assert_eq!(manager.handlers()[0].priority, 100);

    assert!(manager.unregister(handle));
    assert_eq!(manager.handlers().len(), 0);
}

#[test]
fn test_register_view_programmatically() {
    let mut manager = PluginManager::new();

    let view = ViewBuilder::new("graph")
        .description("Interactive graph view")
        .handler_fn("render_graph")
        .build();

    let handle = manager.register_view(view, None);

    assert_eq!(manager.views().len(), 1);
    assert_eq!(manager.views()[0].name, "graph");
    assert_eq!(
        manager.views()[0].handler_fn,
        Some("render_graph".to_string())
    );

    assert!(manager.unregister(handle));
    assert_eq!(manager.views().len(), 0);
}

#[test]
fn test_register_with_owner() {
    let mut manager = PluginManager::new();

    let tool1 = ToolBuilder::new("owned_tool").build();
    let tool2 = ToolBuilder::new("orphan_tool").build();
    let cmd = CommandBuilder::new("owned_cmd").build();

    manager.register_tool(tool1, Some("my_workflow"));
    manager.register_tool(tool2, None);
    manager.register_command(cmd, Some("my_workflow"));

    assert_eq!(manager.tools().len(), 2);
    assert_eq!(manager.commands().len(), 1);

    let removed = manager.unregister_by_owner("my_workflow");

    assert_eq!(removed, 2);
    assert_eq!(manager.tools().len(), 1);
    assert_eq!(manager.tools()[0].name, "orphan_tool");
    assert_eq!(manager.commands().len(), 0);
}

// ============================================================================
// SPEC-BASED PLUGIN DISCOVERY
// ============================================================================

fn create_spec_plugin_structure(base: &std::path::Path, name: &str) -> std::path::PathBuf {
    let plugin_dir = base.join(name);
    fs::create_dir_all(&plugin_dir).unwrap();

    let lua = format!(
        r#"
local M = {{}}
function M.my_tool(args) return {{ result = "ok" }} end
function M.my_cmd(args, ctx) end

return {{
    name = "{}",
    version = "1.0.0",
    description = "Spec-based plugin",
    tools = {{
        my_tool = {{
            desc = "Do something",
            params = {{
                {{ name = "query", type = "string", desc = "Search query" }},
            }},
            fn = M.my_tool,
        }},
    }},
    commands = {{
        my_cmd = {{ desc = "A command", hint = "[args]", fn = M.my_cmd }},
    }},
}}
"#,
        name
    );

    fs::write(plugin_dir.join("init.lua"), lua).unwrap();
    plugin_dir
}

#[test]
fn test_spec_plugin_discover_and_load() {
    let temp = TempDir::new().unwrap();
    create_spec_plugin_structure(temp.path(), "spec-plugin");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("spec-plugin").unwrap();

    let plugin = manager.get("spec-plugin").unwrap();
    assert_eq!(plugin.state, PluginState::Active);
    assert_eq!(plugin.version(), "1.0.0");

    // Tools from spec
    assert_eq!(manager.tools().len(), 1);
    assert_eq!(manager.tools()[0].name, "my_tool");

    // Commands from spec
    assert_eq!(manager.commands().len(), 1);
    assert_eq!(manager.commands()[0].name, "my_cmd");
}

#[test]
fn test_manifestless_plugin_discover_and_load() {
    let temp = TempDir::new().unwrap();
    // No plugin.yaml, just init.lua with spec
    create_spec_plugin_structure(temp.path(), "no-yaml-plugin");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    let discovered = manager.discover().unwrap();

    assert_eq!(discovered.len(), 1);
    assert!(discovered.contains(&"no-yaml-plugin".to_string()));

    manager.load("no-yaml-plugin").unwrap();
    assert_eq!(manager.tools().len(), 1);
}

#[test]
fn test_spec_plugin_unload_cleans_exports() {
    let temp = TempDir::new().unwrap();
    create_spec_plugin_structure(temp.path(), "cleanup-test");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("cleanup-test").unwrap();

    assert_eq!(manager.tools().len(), 1);
    assert_eq!(manager.commands().len(), 1);

    manager.unload("cleanup-test").unwrap();

    assert_eq!(manager.tools().len(), 0);
    assert_eq!(manager.commands().len(), 0);
}

#[test]
fn test_multiple_spec_plugins() {
    let temp = TempDir::new().unwrap();

    // Manifest + spec plugin
    create_plugin_structure(temp.path(), "manifest-plugin", "1.0.0");

    // Manifest-less spec plugin
    create_spec_plugin_structure(temp.path(), "spec-plugin");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load_all().unwrap();

    let tool_names: Vec<_> = manager.tools().iter().map(|t| t.name.clone()).collect();
    assert!(tool_names.contains(&"test_tool".to_string())); // from manifest plugin
    assert!(tool_names.contains(&"my_tool".to_string())); // from spec-only plugin
}

// ============================================================================
// PLUGIN MANAGER DEBUG
// ============================================================================

#[test]
fn test_plugin_manager_debug() {
    let mut manager = PluginManager::new();
    manager.register_tool(ToolBuilder::new("test").build(), None);

    let debug_str = format!("{:?}", manager);
    assert!(debug_str.contains("PluginManager"));
    assert!(debug_str.contains("tools_count"));
}
