use super::{create_test_plugin, setup_emitter_manager};
use crate::lifecycle::{
    CommandBuilder, HandlerBuilder, PluginManager, RegistrationHandle, ToolBuilder, ViewBuilder,
};
use tempfile::TempDir;

#[test]
fn test_register_tool_programmatic() {
    let mut manager = PluginManager::new();

    let tool = ToolBuilder::new("my_tool")
        .description("A programmatic tool")
        .param("query", "string")
        .param_optional("limit", "number")
        .build();

    let handle = manager.register_tool(tool, None);

    assert_eq!(manager.tools().len(), 1);
    assert_eq!(manager.tools()[0].name, "my_tool");
    assert_eq!(manager.tools()[0].params.len(), 2);
    assert!(!manager.tools()[0].params[0].optional);
    assert!(manager.tools()[0].params[1].optional);

    assert!(manager.unregister(handle));
    assert_eq!(manager.tools().len(), 0);
}

#[test]
fn test_register_command_programmatic() {
    let mut manager = PluginManager::new();

    let cmd = CommandBuilder::new("mycmd")
        .description("A programmatic command")
        .hint("args...")
        .build();

    let handle = manager.register_command(cmd, None);

    assert_eq!(manager.commands().len(), 1);
    assert_eq!(manager.commands()[0].name, "mycmd");
    assert_eq!(
        manager.commands()[0].input_hint,
        Some("args...".to_string())
    );

    assert!(manager.unregister(handle));
    assert_eq!(manager.commands().len(), 0);
}

#[test]
fn test_register_handler_programmatic() {
    let mut manager = PluginManager::new();

    let handler = HandlerBuilder::new("on_search", "tool:after")
        .pattern("search_*")
        .priority(50)
        .build();

    let handle = manager.register_handler(handler, None);

    assert_eq!(manager.handlers().len(), 1);
    assert_eq!(manager.handlers()[0].event_type, "tool:after");
    assert_eq!(manager.handlers()[0].pattern, "search_*");
    assert_eq!(manager.handlers()[0].priority, 50);

    assert!(manager.unregister(handle));
    assert_eq!(manager.handlers().len(), 0);
}

#[test]
fn test_register_view_programmatic() {
    let mut manager = PluginManager::new();

    let view = ViewBuilder::new("custom_view")
        .description("A custom view")
        .handler_fn("custom_handler")
        .build();

    let handle = manager.register_view(view, None);

    assert_eq!(manager.views().len(), 1);
    assert_eq!(manager.views()[0].name, "custom_view");
    assert_eq!(
        manager.views()[0].handler_fn,
        Some("custom_handler".to_string())
    );

    assert!(manager.unregister(handle));
    assert_eq!(manager.views().len(), 0);
}

#[test]
fn test_unregister_by_owner() {
    let mut manager = PluginManager::new();

    let tool1 = ToolBuilder::new("owned_tool").build();
    let tool2 = ToolBuilder::new("other_tool").build();
    let cmd = CommandBuilder::new("owned_cmd").build();

    manager.register_tool(tool1, Some("my_plugin"));
    manager.register_tool(tool2, None);
    manager.register_command(cmd, Some("my_plugin"));

    assert_eq!(manager.tools().len(), 2);
    assert_eq!(manager.commands().len(), 1);

    let removed = manager.unregister_by_owner("my_plugin");
    assert_eq!(removed, 2);
    assert_eq!(manager.tools().len(), 1);
    assert_eq!(manager.tools()[0].name, "other_tool");
    assert_eq!(manager.commands().len(), 0);
}

#[test]
fn test_unregister_invalid_handle_returns_false() {
    let mut manager = PluginManager::new();
    let fake_handle = RegistrationHandle(999999);
    assert!(!manager.unregister(fake_handle));
}

#[test]
fn test_mixed_spec_and_programmatic() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "spec-plugin", "1.0.0");

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();
    manager.load("spec-plugin").unwrap();

    assert_eq!(manager.tools().len(), 1);
    assert_eq!(manager.tools()[0].name, "test_tool");

    let prog_tool = ToolBuilder::new("programmatic_tool").build();
    let handle = manager.register_tool(prog_tool, None);

    assert_eq!(manager.tools().len(), 2);

    let names: Vec<_> = manager.tools().iter().map(|t| &t.name).collect();
    assert!(names.contains(&&"test_tool".to_string()));
    assert!(names.contains(&&"programmatic_tool".to_string()));

    manager.unregister(handle);
    assert_eq!(manager.tools().len(), 1);
    assert_eq!(manager.tools()[0].name, "test_tool");
}

#[test]
fn test_emitter_owner_registration() {
    let manager = setup_emitter_manager();
    let count = manager
        .eval_runtime::<i64>(
            r#"
        local e = cru.emitter.global()
        local fired = 0
        e:on("test_event", function() fired = fired + 1 end, "plugin-a")
        e:emit("test_event")
        return fired
    "#,
        )
        .unwrap();
    assert_eq!(count, 1, "owned listener should fire");
}

#[test]
fn test_emitter_unregister_owner() {
    let manager = setup_emitter_manager();
    let result = manager
        .eval_runtime::<i64>(
            r#"
        local e = cru.emitter.new()
        local fired_a = 0
        local fired_b = 0
        e:on("ev", function() fired_a = fired_a + 1 end, "owner-a")
        e:on("ev", function() fired_a = fired_a + 1 end, "owner-a")
        e:on("ev", function() fired_b = fired_b + 1 end, "owner-b")
        e:unregister_owner("owner-a")
        e:emit("ev")
        return fired_b
    "#,
        )
        .unwrap();
    assert_eq!(
        result, 1,
        "only owner-b listener should fire after owner-a unregistered"
    );
}

#[test]
fn test_emitter_backward_compat_no_owner() {
    let manager = setup_emitter_manager();
    let count = manager
        .eval_runtime::<i64>(
            r#"
        local e = cru.emitter.new()
        local fired = 0
        e:on("ev", function() fired = fired + 1 end)
        e:on("ev", function() fired = fired + 1 end)
        e:emit("ev")
        return fired
    "#,
        )
        .unwrap();
    assert_eq!(
        count, 2,
        "backward compat: listeners without owner should still fire"
    );
}

#[test]
fn test_unload_cleans_global_emitter() {
    let mut manager = setup_emitter_manager();
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("emitter-cleanup-test");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join("plugin.yaml"),
        "name: emitter-cleanup-test\nversion: \"1.0.0\"\nmain: init.lua\n",
    )
    .unwrap();
    std::fs::write(
        plugin_dir.join("init.lua"),
        r#"
return {
    name = "emitter-cleanup-test",
    version = "1.0.0",
    on_load = function()
        cru.emitter.global():on("test_cleanup_event", function() end, "emitter-cleanup-test")
    end,
}
"#,
    )
    .unwrap();

    manager.add_search_path(temp.path().to_path_buf());
    manager.discover().unwrap();
    manager.load("emitter-cleanup-test").unwrap();

    // Verify listener was registered
    let count_before = manager
        .eval_runtime::<i64>("return cru.emitter.global():count('test_cleanup_event')")
        .unwrap_or(0);
    assert_eq!(count_before, 1, "listener should be registered after load");

    manager.unload("emitter-cleanup-test").unwrap();

    let count_after = manager
        .eval_runtime::<i64>("return cru.emitter.global():count('test_cleanup_event')")
        .unwrap_or(-1);
    assert_eq!(count_after, 0, "listener should be removed after unload");
}

#[test]
fn test_unload_preserves_other_plugin_listeners() {
    let mut manager = setup_emitter_manager();
    let temp = TempDir::new().unwrap();

    // Plugin A
    let plugin_a_dir = temp.path().join("plugin-a");
    std::fs::create_dir_all(&plugin_a_dir).unwrap();
    std::fs::write(
        plugin_a_dir.join("plugin.yaml"),
        "name: plugin-a\nversion: \"1.0.0\"\nmain: init.lua\n",
    )
    .unwrap();
    std::fs::write(
        plugin_a_dir.join("init.lua"),
        r#"
return {
    name = "plugin-a",
    version = "1.0.0",
    on_load = function()
        cru.emitter.global():on("shared_event", function() end, "plugin-a")
    end,
}
"#,
    )
    .unwrap();

    // Plugin B
    let plugin_b_dir = temp.path().join("plugin-b");
    std::fs::create_dir_all(&plugin_b_dir).unwrap();
    std::fs::write(
        plugin_b_dir.join("plugin.yaml"),
        "name: plugin-b\nversion: \"1.0.0\"\nmain: init.lua\n",
    )
    .unwrap();
    std::fs::write(
        plugin_b_dir.join("init.lua"),
        r#"
return {
    name = "plugin-b",
    version = "1.0.0",
    on_load = function()
        cru.emitter.global():on("shared_event", function() end, "plugin-b")
    end,
}
"#,
    )
    .unwrap();

    manager.add_search_path(temp.path().to_path_buf());
    manager.discover().unwrap();
    manager.load("plugin-a").unwrap();
    manager.load("plugin-b").unwrap();

    // Both registered
    let count_both = manager
        .eval_runtime::<i64>("return cru.emitter.global():count('shared_event')")
        .unwrap_or(0);
    assert_eq!(
        count_both, 2,
        "both plugins should have registered listeners"
    );

    // Unload plugin-a
    manager.unload("plugin-a").unwrap();

    // Only plugin-b's listener remains
    let count_after = manager
        .eval_runtime::<i64>("return cru.emitter.global():count('shared_event')")
        .unwrap_or(0);
    assert_eq!(count_after, 1, "only plugin-b's listener should survive");
}
