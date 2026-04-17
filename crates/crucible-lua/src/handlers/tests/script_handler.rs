use crate::annotations::DiscoveredHandler;
use crate::handlers::{HandlerExecutionResult, LuaScriptHandler};
use crucible_core::events::SessionEvent;
use crucible_core::utils::glob_match;
use mlua::Lua;

use super::create_test_handler;

#[test]
fn test_match_glob_star() {
    assert!(glob_match("*", "anything"));
    assert!(glob_match("just_*", "just_test"));
    assert!(glob_match("just_*", "just_build"));
    assert!(glob_match("*_test", "unit_test"));
    assert!(glob_match("*_test_*", "unit_test_foo"));
    assert!(!glob_match("just_*", "other_test"));
}

#[test]
fn test_match_glob_exact() {
    assert!(glob_match("test", "test"));
    assert!(!glob_match("test", "testing"));
}

#[test]
fn test_match_glob_question() {
    assert!(glob_match("test?", "tests"));
    assert!(glob_match("t?st", "test"));
    assert!(!glob_match("test?", "test"));
}

#[test]
fn test_handler_creation() {
    let hook = DiscoveredHandler {
        name: "filter_handler".to_string(),
        event_type: "ToolCalled".to_string(),
        pattern: "search_*".to_string(),
        priority: 50,
        description: "Filter search results".to_string(),
        source_path: "test.lua".to_string(),
        handler_fn: "filter_results".to_string(),
        is_fennel: false,
    };

    let handler = LuaScriptHandler::with_source(hook, "-- test source".to_string());
    assert_eq!(handler.metadata.name, "filter_handler");
    assert_eq!(handler.metadata.priority, 50);
    assert_eq!(handler.source(), "-- test source");
}

#[test]
fn test_handler_matches_event_type() {
    let handler = create_test_handler("");

    let event = SessionEvent::ToolCalled {
        name: "search".to_string(),
        args: serde_json::json!({}),
        description: None,
        source: None,
    };

    assert!(handler.matches(&event));

    let other_event = SessionEvent::ToolCompleted {
        name: "search".to_string(),
        result: "done".to_string(),
        error: None,
    };
    assert!(!handler.matches(&other_event));
}

#[test]
fn test_handler_matches_with_pattern() {
    let hook = DiscoveredHandler {
        name: "test".to_string(),
        event_type: "tool_called".to_string(),
        pattern: "search_*".to_string(),
        priority: 100,
        description: "".to_string(),
        source_path: "".to_string(),
        handler_fn: "handler".to_string(),
        is_fennel: false,
    };

    let handler = LuaScriptHandler::with_source(hook, String::new());

    assert!(handler.matches_with_identifier("tool_called", "search_notes"));
    assert!(handler.matches_with_identifier("tool_called", "search_files"));
    assert!(!handler.matches_with_identifier("tool_called", "fetch_data"));
    assert!(!handler.matches_with_identifier("other_event", "search_notes"));
}

#[test]
fn test_execute_simple_handler() {
    let source = r#"
        function test_handler(ctx, event)
            event.modified = true
            return event
        end
    "#;

    let handler = create_test_handler(source);
    let lua = Lua::new();

    let event = SessionEvent::ToolCalled {
        name: "test".to_string(),
        args: serde_json::json!({"key": "value"}),
        description: None,
        source: None,
    };

    let result = handler.execute(&lua, &event);
    assert!(result.is_ok());

    let modified = result.unwrap();
    assert!(modified.is_some());
}

#[test]
fn test_execute_handler_returns_nil() {
    let source = r#"
        function test_handler(ctx, event)
            -- Do nothing, return nil for pass-through
            return nil
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
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_execute_handler_returns_cancel() {
    let source = r#"
        function test_handler(ctx, event)
            return { cancel = true, reason = "blocked by policy" }
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
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("blocked by policy"));
}

#[test]
fn test_execute_json_roundtrip() {
    let source = r#"
        function test_handler(ctx, event)
            event.extra = "added"
            return event
        end
    "#;

    let handler = create_test_handler(source);
    let lua = Lua::new();

    let ctx = serde_json::json!({"handler_name": "test"});
    let event = serde_json::json!({"type": "ToolCalled", "name": "search"});

    let result = handler.execute_json(&lua, ctx, event);
    assert!(result.is_ok());

    let json = result.unwrap();
    assert_eq!(json["extra"], "added");
    assert_eq!(json["name"], "search");
}

#[test]
fn test_handler_result_constructors() {
    let event = SessionEvent::ToolCalled {
        name: "test".to_string(),
        args: serde_json::json!({}),
        description: None,
        source: None,
    };

    let ok_result = HandlerExecutionResult::ok("my_handler", event.clone());
    assert!(ok_result.success);
    assert!(ok_result.event.is_some());
    assert!(ok_result.error.is_none());

    let pass_result = HandlerExecutionResult::pass_through("my_handler");
    assert!(pass_result.success);
    assert!(pass_result.event.is_none());
    assert!(pass_result.error.is_none());

    let err_result = HandlerExecutionResult::err("my_handler", "something went wrong");
    assert!(!err_result.success);
    assert!(err_result.event.is_none());
    assert!(err_result.error.is_some());
}
