use crate::annotations::DiscoveredHandler;
use crate::handlers::{register_crucible_on_api, LuaScriptHandler, LuaScriptHandlerRegistry};
use crucible_core::events::SessionEvent;
use mlua::Lua;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::create_test_handler;

#[test]
fn test_registry_new_is_empty() {
    let registry = LuaScriptHandlerRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
}

#[test]
fn test_registry_default_is_empty() {
    let registry = LuaScriptHandlerRegistry::default();
    assert!(registry.is_empty());
}

#[test]
fn test_registry_add_handler() {
    let mut registry = LuaScriptHandlerRegistry::new();
    let handler = create_test_handler("-- test");

    registry.add(handler);

    assert_eq!(registry.len(), 1);
    assert!(!registry.is_empty());
}

#[test]
fn test_registry_add_maintains_priority_order() {
    let mut registry = LuaScriptHandlerRegistry::new();

    // Add handlers with different priorities
    let mut high_priority = create_test_handler("-- high");
    high_priority.metadata.priority = 10;

    let mut low_priority = create_test_handler("-- low");
    low_priority.metadata.priority = 200;

    let mut medium_priority = create_test_handler("-- medium");
    medium_priority.metadata.priority = 100;

    // Add in non-sorted order
    registry.add(low_priority);
    registry.add(high_priority);
    registry.add(medium_priority);

    // Verify they are sorted by priority
    let handlers: Vec<_> = registry.iter().collect();
    assert_eq!(handlers[0].metadata.priority, 10);
    assert_eq!(handlers[1].metadata.priority, 100);
    assert_eq!(handlers[2].metadata.priority, 200);
}

#[test]
fn test_registry_handlers_for_event() {
    let mut registry = LuaScriptHandlerRegistry::new();

    // Add a ToolCalled handler
    let handler = create_test_handler("-- tool called handler");
    registry.add(handler);

    // Create a matching event
    let matching_event = SessionEvent::ToolCalled {
        name: "search".to_string(),
        args: serde_json::json!({}),
        description: None,
        source: None,
    };

    // Create a non-matching event
    let non_matching_event = SessionEvent::ToolCompleted {
        name: "search".to_string(),
        result: "done".to_string(),
        error: None,
    };

    let matching_handlers = registry.handlers_for(&matching_event);
    assert_eq!(matching_handlers.len(), 1);

    let non_matching_handlers = registry.handlers_for(&non_matching_event);
    assert!(non_matching_handlers.is_empty());
}

#[test]
fn test_registry_handlers_for_identifier() {
    let mut registry = LuaScriptHandlerRegistry::new();

    let hook = DiscoveredHandler {
        name: "search_filter".to_string(),
        event_type: "tool_called".to_string(),
        pattern: "search_*".to_string(),
        priority: 50,
        description: "Filter search tools".to_string(),
        source_path: "test.lua".to_string(),
        handler_fn: "handler".to_string(),
        is_fennel: false,
    };
    let handler = LuaScriptHandler::with_source(hook, String::new());
    registry.add(handler);

    // Should match search_notes
    let handlers = registry.handlers_for_identifier("tool_called", "search_notes");
    assert_eq!(handlers.len(), 1);

    // Should match search_files
    let handlers = registry.handlers_for_identifier("tool_called", "search_files");
    assert_eq!(handlers.len(), 1);

    // Should not match fetch_data
    let handlers = registry.handlers_for_identifier("tool_called", "fetch_data");
    assert!(handlers.is_empty());

    // Should not match wrong event type
    let handlers = registry.handlers_for_identifier("tool_completed", "search_notes");
    assert!(handlers.is_empty());
}

#[test]
fn test_registry_clear() {
    let mut registry = LuaScriptHandlerRegistry::new();
    registry.add(create_test_handler("-- one"));
    registry.add(create_test_handler("-- two"));

    assert_eq!(registry.len(), 2);

    registry.clear();

    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
}

#[test]
fn test_registry_iter() {
    let mut registry = LuaScriptHandlerRegistry::new();

    let mut handler1 = create_test_handler("-- one");
    handler1.metadata.name = "handler_one".to_string();

    let mut handler2 = create_test_handler("-- two");
    handler2.metadata.name = "handler_two".to_string();

    registry.add(handler1);
    registry.add(handler2);

    let names: Vec<_> = registry.iter().map(|h| h.metadata.name.as_str()).collect();
    assert_eq!(names.len(), 2);
    assert!(names.contains(&"handler_one"));
    assert!(names.contains(&"handler_two"));
}

#[test]
fn test_registry_discover_nonexistent_path() {
    let paths = vec![PathBuf::from("/nonexistent/path/that/should/not/exist")];
    let registry = LuaScriptHandlerRegistry::discover(&paths).unwrap();
    assert!(registry.is_empty());
}

#[test]
fn test_registry_discover_from_temp_dir() {
    use std::io::Write;

    // Create a temp directory with a handler file
    let temp_dir = tempfile::tempdir().unwrap();
    let handler_file = temp_dir.path().join("my_handler.lua");

    let handler_source = r#"
--- Filter search results
-- @handler event="ToolCalled" pattern="*" priority=25
function filter_results(ctx, event)
    return event
end
"#;

    std::fs::File::create(&handler_file)
        .unwrap()
        .write_all(handler_source.as_bytes())
        .unwrap();

    // Discover handlers
    let paths = vec![temp_dir.path().to_path_buf()];
    let registry = LuaScriptHandlerRegistry::discover(&paths).unwrap();

    assert_eq!(registry.len(), 1);

    let handler = registry.iter().next().unwrap();
    assert_eq!(handler.metadata.name, "filter_results");
    assert_eq!(handler.metadata.event_type, "ToolCalled");
    assert_eq!(handler.metadata.priority, 25);
}

#[test]
fn test_registry_clone() {
    let mut registry = LuaScriptHandlerRegistry::new();
    registry.add(create_test_handler("-- test"));

    let cloned = registry.clone();
    assert_eq!(cloned.len(), registry.len());
}

#[test]
fn test_crucible_on_api_registration() {
    let lua = Lua::new();
    let handlers = Arc::new(Mutex::new(Vec::new()));
    let functions = Arc::new(Mutex::new(HashMap::new()));

    register_crucible_on_api(&lua, handlers.clone(), functions.clone()).unwrap();

    // Verify crucible.on exists
    lua.load(
        r#"
        crucible.on("test_event", function(event)
            return event
        end)
    "#,
    )
    .exec()
    .unwrap();

    // Check that handler was registered
    let guard = handlers.lock().unwrap();
    assert_eq!(guard.len(), 1);
    assert_eq!(guard[0].event_type, "test_event");
}

// ============================================================================
// Return Convention Tests
// ============================================================================

#[test]
fn test_return_nil_is_passthrough() {
    // Handler returns nil → event passes through unchanged
    let source = r#"
        function test_handler(ctx, event)
            -- Side effect only (logging, etc)
            return nil
        end
    "#;

    let handler = create_test_handler(source);
    let lua = Lua::new();
    let event = SessionEvent::ToolCalled {
        name: "search".to_string(),
        args: serde_json::json!({"query": "test"}),
        description: None,
        source: None,
    };

    let result = handler.execute(&lua, &event).unwrap();
    assert!(
        result.is_none(),
        "nil return should produce None (pass-through)"
    );
}

#[test]
fn test_return_table_is_transform() {
    // Handler returns table → event is transformed
    let source = r#"
        function test_handler(ctx, event)
            event.injected = "by_handler"
            return event
        end
    "#;

    let handler = create_test_handler(source);
    let lua = Lua::new();
    let event = SessionEvent::ToolCalled {
        name: "search".to_string(),
        args: serde_json::json!({}),
        description: None,
        source: None,
    };

    let result = handler.execute(&lua, &event).unwrap();
    assert!(
        result.is_some(),
        "table return should produce Some(modified_event)"
    );
}

#[test]
fn test_return_cancel_aborts_pipeline() {
    // Handler returns {cancel=true} → pipeline aborts
    let source = r#"
        function test_handler(ctx, event)
            return { cancel = true, reason = "blocked by security" }
        end
    "#;

    let handler = create_test_handler(source);
    let lua = Lua::new();
    let event = SessionEvent::ToolCalled {
        name: "dangerous_tool".to_string(),
        args: serde_json::json!({}),
        description: None,
        source: None,
    };

    let result = handler.execute(&lua, &event);
    assert!(result.is_err(), "cancel return should produce error");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("blocked by security"),
        "error should contain reason"
    );
}

#[test]
fn test_cancel_without_reason_uses_default() {
    // Handler returns {cancel=true} without reason
    let source = r#"
        function test_handler(ctx, event)
            return { cancel = true }
        end
    "#;

    let handler = create_test_handler(source);
    let lua = Lua::new();
    let event = SessionEvent::ToolCalled {
        name: "test".to_string(),
        args: serde_json::json!({}),
        description: None,
        source: None,
    };

    let result = handler.execute(&lua, &event);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("cancelled"), "should use default reason");
}

#[test]
fn test_cancel_false_is_transform() {
    // Handler returns {cancel=false, ...} → treated as transform, not cancel
    let source = r#"
        function test_handler(ctx, event)
            return { cancel = false, data = "still valid" }
        end
    "#;

    let handler = create_test_handler(source);
    let lua = Lua::new();
    let event = SessionEvent::ToolCalled {
        name: "test".to_string(),
        args: serde_json::json!({}),
        description: None,
        source: None,
    };

    let result = handler.execute(&lua, &event);
    assert!(result.is_ok(), "cancel=false should not abort");
    assert!(result.unwrap().is_some(), "should return transformed event");
}

#[test]
fn crucible_on_with_opts_table_sets_pattern_and_priority() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    register_crucible_on_api(
        &lua,
        registry.runtime_handlers(),
        registry.handler_functions(),
    )
    .unwrap();

    lua.load(
        r#"
        crucible.on("pre_tool_call", { pattern = "bash", priority = 10 }, function(ctx, event)
            return nil
        end)
    "#,
    )
    .exec()
    .unwrap();

    let handlers = registry.runtime_handlers_for("pre_tool_call", Some("bash"));
    assert_eq!(handlers.len(), 1);
    assert_eq!(handlers[0].priority, 10);
    assert_eq!(handlers[0].pattern, Some("bash".to_string()));

    // Doesn't match other tools
    let handlers = registry.runtime_handlers_for("pre_tool_call", Some("grep"));
    assert_eq!(handlers.len(), 0);
}

#[test]
fn crucible_on_backward_compat_no_opts() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    register_crucible_on_api(
        &lua,
        registry.runtime_handlers(),
        registry.handler_functions(),
    )
    .unwrap();

    lua.load(
        r#"
        crucible.on("turn:complete", function(ctx, event)
            return nil
        end)
    "#,
    )
    .exec()
    .unwrap();

    let handlers = registry.runtime_handlers_for("turn:complete", None);
    assert_eq!(handlers.len(), 1);
    assert_eq!(handlers[0].priority, 100); // default
    assert_eq!(handlers[0].pattern, None);
}
