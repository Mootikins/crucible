use super::{create_spec_plugin, create_test_plugin};
use crate::lifecycle::{load_plugin_spec_from_source, PluginManager};
use crate::manifest::{Capability, PluginState};
use std::path::Path;
use tempfile::TempDir;

#[test]
fn test_load_plugin_spec_basic() {
    let source = r#"
return {
    name = "test-plugin",
    version = "1.2.3",
    description = "A test plugin",
    tools = {
        my_tool = {
            desc = "Do something",
            params = {
                { name = "query", type = "string", desc = "Search query" },
            },
            fn = function(args) return { result = "ok" } end,
        },
    },
}
"#;
    let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
        .unwrap()
        .expect("Should return Some(spec)");

    assert_eq!(spec.name, Some("test-plugin".to_string()));
    assert_eq!(spec.version, Some("1.2.3".to_string()));
    assert_eq!(spec.description, Some("A test plugin".to_string()));
    assert_eq!(spec.tools.len(), 1);
    assert_eq!(spec.tools[0].name, "my_tool");
    assert_eq!(spec.tools[0].description, "Do something");
    assert_eq!(spec.tools[0].params.len(), 1);
    assert_eq!(spec.tools[0].params[0].name, "query");
    assert_eq!(spec.tools[0].params[0].param_type, "string");
    assert!(!spec.tools[0].params[0].optional);
}

#[test]
fn test_load_plugin_spec_all_export_types() {
    let source = r#"
local M = {}
function M.my_tool(args) return { result = "ok" } end
function M.my_command(args, ctx) end
function M.my_handler(ctx, event) return event end
function M.my_view(ctx) end

return {
    name = "full-plugin",
    version = "0.1.0",
    tools = {
        my_tool = { desc = "A tool", fn = M.my_tool },
    },
    commands = {
        my_command = { desc = "A command", hint = "[args]", fn = M.my_command },
    },
    handlers = {
        { event = "note:created", priority = 150, name = "on_note_created", fn = M.my_handler },
    },
    views = {
        ["my-view"] = { desc = "A view", fn = M.my_view },
    },
}
"#;
    let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
        .unwrap()
        .expect("Should return Some(spec)");

    assert_eq!(spec.tools.len(), 1);
    assert_eq!(spec.commands.len(), 1);
    assert_eq!(spec.handlers.len(), 1);
    assert_eq!(spec.views.len(), 1);
}

#[test]
fn test_load_plugin_spec_with_setup() {
    let source = r#"
return {
    name = "setup-plugin",
    version = "0.1.0",
    setup = function(config)
        -- Called after load with plugin config
    end,
}
"#;
    let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
        .unwrap()
        .expect("Should return Some(spec)");

    assert!(spec.has_setup);
    assert_eq!(spec.name, Some("setup-plugin".to_string()));
}

#[test]
fn test_load_plugin_spec_empty_table() {
    // Empty table with no recognized fields returns None (not a spec)
    let source = "return {}";
    let result = load_plugin_spec_from_source(source, Path::new("test/init.lua")).unwrap();
    assert!(
        result.is_none(),
        "Empty table should not be recognized as a spec"
    );
}

#[test]
fn test_load_plugin_spec_no_return() {
    // Script that doesn't return anything (returns nil)
    let source = "local x = 42";
    let result = load_plugin_spec_from_source(source, Path::new("test/init.lua")).unwrap();
    assert!(result.is_none(), "nil return should yield None");
}

#[test]
fn test_load_plugin_spec_lua_error() {
    // Syntax error in Lua
    let source = "this is not valid lua!!!";
    let result = load_plugin_spec_from_source(source, Path::new("test/init.lua"));
    assert!(result.is_err(), "Lua syntax error should return Err");
}

#[test]
fn test_load_plugin_spec_runtime_error() {
    // Runtime error
    let source = r#"error("boom")"#;
    let result = load_plugin_spec_from_source(source, Path::new("test/init.lua"));
    assert!(result.is_err(), "Runtime error should return Err");
}

#[test]
fn test_tool_params_required_and_optional() {
    let source = r#"
return {
    name = "params-test",
    version = "1.0.0",
    tools = {
        search = {
            desc = "Search",
            params = {
                { name = "query", type = "string", desc = "Search query" },
                { name = "limit", type = "number", desc = "Max results", optional = true },
            },
        },
    },
}
"#;
    let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
        .unwrap()
        .unwrap();

    let tool = &spec.tools[0];
    assert_eq!(tool.params.len(), 2);
    assert!(!tool.params[0].optional);
    assert!(tool.params[1].optional);
    assert_eq!(tool.params[1].param_type, "number");
}

#[test]
fn test_handler_spec_fields() {
    let source = r#"
return {
    name = "handler-test",
    version = "1.0.0",
    handlers = {
        { event = "note:created", priority = 50, pattern = "*.md", name = "on_md_created" },
        { event = "tool:after", name = "log_tool" },
    },
}
"#;
    let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
        .unwrap()
        .unwrap();

    assert_eq!(spec.handlers.len(), 2);

    let h1 = &spec.handlers[0];
    assert_eq!(h1.event_type, "note:created");
    assert_eq!(h1.priority, 50);
    assert_eq!(h1.pattern, "*.md");
    assert_eq!(h1.name, "on_md_created");

    let h2 = &spec.handlers[1];
    assert_eq!(h2.event_type, "tool:after");
    assert_eq!(h2.priority, 100); // default
    assert_eq!(h2.pattern, "*"); // default
}

#[test]
fn test_view_spec_with_handler() {
    let source = r#"
return {
    name = "view-test",
    version = "1.0.0",
    views = {
        ["my-view"] = {
            desc = "A custom view",
            fn = function(ctx) end,
            handler = function(key, ctx) end,
        },
    },
}
"#;
    let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
        .unwrap()
        .unwrap();

    assert_eq!(spec.views.len(), 1);
    assert_eq!(spec.views[0].name, "my-view");
    assert_eq!(spec.views[0].description, "A custom view");
    assert!(spec.views[0].handler_fn.is_some());
}

#[test]
fn test_view_spec_without_handler() {
    let source = r#"
return {
    name = "view-test",
    version = "1.0.0",
    views = {
        ["simple-view"] = {
            desc = "Simple",
            fn = function(ctx) end,
        },
    },
}
"#;
    let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
        .unwrap()
        .unwrap();

    assert_eq!(spec.views.len(), 1);
    assert!(spec.views[0].handler_fn.is_none());
}

#[test]
fn test_command_spec_with_hint() {
    let source = r#"
return {
    name = "cmd-test",
    version = "1.0.0",
    commands = {
        daily = { desc = "Create daily note", hint = "[title]" },
    },
}
"#;
    let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
        .unwrap()
        .unwrap();

    assert_eq!(spec.commands.len(), 1);
    assert_eq!(spec.commands[0].name, "daily");
    assert_eq!(spec.commands[0].description, "Create daily note");
    assert_eq!(spec.commands[0].input_hint, Some("[title]".to_string()));
}

#[test]
fn test_capabilities_from_spec() {
    let source = r#"
return {
    name = "cap-test",
    version = "1.0.0",
    capabilities = { "kiln", "ui", "config" },
}
"#;
    let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
        .unwrap()
        .unwrap();

    assert_eq!(spec.capabilities, vec!["kiln", "ui", "config"]);
}

#[test]
fn test_plain_module_table_not_spec() {
    // A plugin that returns a module table (not a spec) should be None
    let source = r#"
local M = {}
function M.my_tool(args) return { result = "ok" } end
function M.my_command(args, ctx) end
return M
"#;
    let result = load_plugin_spec_from_source(source, Path::new("test/init.lua")).unwrap();
    assert!(
        result.is_none(),
        "Module table with only function values should not be a spec"
    );
}

#[test]
fn test_spec_with_only_name() {
    // A table with just a name field is recognized as a spec
    let source = r#"return { name = "minimal" }"#;
    let spec = load_plugin_spec_from_source(source, Path::new("test/init.lua"))
        .unwrap()
        .expect("Table with name should be a spec");

    assert_eq!(spec.name, Some("minimal".to_string()));
    assert!(spec.tools.is_empty());
}

#[test]
fn test_spec_plugin_full_lifecycle() {
    let temp = TempDir::new().unwrap();
    create_spec_plugin(temp.path(), "spec-test");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("spec-test").unwrap();

    let plugin = manager.get("spec-test").unwrap();
    assert_eq!(plugin.state, PluginState::Active);
    assert_eq!(plugin.manifest.name, "spec-test");
    assert_eq!(plugin.version(), "1.0.0");

    assert_eq!(manager.tools().len(), 1);
    assert_eq!(manager.tools()[0].name, "search");
    assert_eq!(manager.tools()[0].params.len(), 2);

    assert_eq!(manager.commands().len(), 1);
    assert_eq!(manager.commands()[0].name, "search");
    assert_eq!(
        manager.commands()[0].input_hint,
        Some("[query]".to_string())
    );

    assert_eq!(manager.handlers().len(), 1);
    assert_eq!(manager.handlers()[0].event_type, "note:created");
    assert_eq!(manager.handlers()[0].priority, 50);

    assert_eq!(manager.views().len(), 1);
    assert_eq!(manager.views()[0].name, "graph");
    assert!(manager.views()[0].handler_fn.is_some());

    // Unload and verify cleanup
    manager.unload("spec-test").unwrap();
    assert_eq!(manager.tools().len(), 0);
    assert_eq!(manager.commands().len(), 0);
    assert_eq!(manager.handlers().len(), 0);
    assert_eq!(manager.views().len(), 0);
}

#[test]
fn test_spec_plugin_without_manifest() {
    let temp = TempDir::new().unwrap();
    // Create a manifest-less plugin that returns a spec
    create_spec_plugin(temp.path(), "no-manifest");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("no-manifest").unwrap();

    let plugin = manager.get("no-manifest").unwrap();
    assert_eq!(plugin.state, PluginState::Active);
    // Name/version updated from spec
    assert_eq!(plugin.manifest.name, "no-manifest");
    assert_eq!(plugin.version(), "1.0.0");

    assert_eq!(manager.tools().len(), 1);
    assert_eq!(manager.commands().len(), 1);
    assert_eq!(manager.handlers().len(), 1);
    assert_eq!(manager.views().len(), 1);
}

#[test]
fn test_spec_capabilities_merged_into_manifest() {
    let temp = TempDir::new().unwrap();
    create_spec_plugin(temp.path(), "cap-merge");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("cap-merge").unwrap();

    let plugin = manager.get("cap-merge").unwrap();
    assert!(plugin.manifest.has_capability(Capability::Kiln));
}

#[test]
fn test_multiple_spec_plugins_coexist() {
    let temp = TempDir::new().unwrap();

    // Manifest + spec plugin
    create_test_plugin(temp.path(), "manifest-plugin", "1.0.0");

    // Manifest-less spec plugin
    create_spec_plugin(temp.path(), "spec-plugin");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load_all().unwrap();

    // Both should be loaded
    assert_eq!(
        manager.get("manifest-plugin").unwrap().state,
        PluginState::Active
    );
    assert_eq!(
        manager.get("spec-plugin").unwrap().state,
        PluginState::Active
    );

    // manifest-plugin has 1 tool (test_tool), spec-plugin has 1 tool (search)
    let tool_names: Vec<_> = manager.tools().iter().map(|t| t.name.clone()).collect();
    assert!(tool_names.contains(&"test_tool".to_string()));
    assert!(tool_names.contains(&"search".to_string()));
}

#[test]
fn test_spec_services_parsed() {
    let source = r#"
        return {
            name = "service-plugin",
            services = {
                gateway = {
                    desc = "WebSocket gateway",
                    fn = function() end,
                },
                heartbeat = {
                    desc = "Keep-alive pinger",
                    fn = function() end,
                },
                no_fn_service = {
                    desc = "Missing fn field -- should be skipped",
                },
            },
        }
    "#;

    let spec = load_plugin_spec_from_source(source, Path::new("test.lua")).unwrap();
    let spec = spec.expect("should return Some");

    assert_eq!(spec.services.len(), 2);

    let names: Vec<&str> = spec.services.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"gateway"));
    assert!(names.contains(&"heartbeat"));

    let gw = spec.services.iter().find(|s| s.name == "gateway").unwrap();
    assert_eq!(gw.description, "WebSocket gateway");
    assert_eq!(gw.service_fn, "gateway");
}
