use crate::handlers::{interpret_handler_result, ScriptHandlerResult};
use crucible_core::events::SessionEvent;
use mlua::{Lua, Value};

use super::create_test_handler;

#[test]
fn test_interpret_handler_result_cancel() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    table.set("cancel", true).unwrap();
    table.set("reason", "test cancel").unwrap();

    let result = interpret_handler_result(&Value::Table(table)).unwrap();
    match result {
        ScriptHandlerResult::Cancel { reason } => {
            assert_eq!(reason, "test cancel");
        }
        _ => panic!("Expected Cancel result"),
    }
}

#[test]
fn test_interpret_handler_result_transform() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    table.set("key", "value").unwrap();

    let result = interpret_handler_result(&Value::Table(table)).unwrap();
    assert!(matches!(result, ScriptHandlerResult::Transform(_)));
}

#[test]
fn test_interpret_handler_result_inject_with_default_position() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    let inject_table = lua.create_table().unwrap();
    inject_table.set("content", "Continue with task").unwrap();
    table.set("inject", inject_table).unwrap();

    let result = interpret_handler_result(&Value::Table(table)).unwrap();
    match result {
        ScriptHandlerResult::Inject { content, position } => {
            assert_eq!(content, "Continue with task");
            assert_eq!(position, "user_prefix");
        }
        _ => panic!("Expected Inject variant"),
    }
}

#[test]
fn test_interpret_handler_result_inject_with_custom_position() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    let inject_table = lua.create_table().unwrap();
    inject_table.set("content", "Follow-up message").unwrap();
    inject_table.set("position", "user_suffix").unwrap();
    table.set("inject", inject_table).unwrap();

    let result = interpret_handler_result(&Value::Table(table)).unwrap();
    match result {
        ScriptHandlerResult::Inject { content, position } => {
            assert_eq!(content, "Follow-up message");
            assert_eq!(position, "user_suffix");
        }
        _ => panic!("Expected Inject variant"),
    }
}

#[test]
fn test_inject_takes_precedence_over_transform() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    let inject_table = lua.create_table().unwrap();
    inject_table.set("content", "injected").unwrap();
    table.set("inject", inject_table).unwrap();
    table.set("other_field", "should_be_ignored").unwrap();

    let result = interpret_handler_result(&Value::Table(table)).unwrap();
    match result {
        ScriptHandlerResult::Inject { content, position } => {
            assert_eq!(content, "injected");
            assert_eq!(position, "user_prefix");
        }
        _ => panic!("Expected Inject variant, not Transform"),
    }
}

#[test]
fn test_inject_checked_before_cancel() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    let inject_table = lua.create_table().unwrap();
    inject_table.set("content", "injected message").unwrap();
    table.set("inject", inject_table).unwrap();
    table.set("cancel", false).unwrap();

    let result = interpret_handler_result(&Value::Table(table)).unwrap();
    match result {
        ScriptHandlerResult::Inject { content, position } => {
            assert_eq!(content, "injected message");
            assert_eq!(position, "user_prefix");
        }
        _ => panic!("Expected Inject variant, not Cancel"),
    }
}

#[test]
fn test_handler_returns_inject_with_default_position() {
    let source = r#"
        function test_handler(ctx, event)
            return {inject={content="Continue with task"}}
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
    assert!(result.is_ok(), "handler should execute successfully");
    assert!(
        result.unwrap().is_none(),
        "Inject result returns None (processed by daemon)"
    );
}

#[test]
fn test_handler_returns_inject_with_custom_position() {
    let source = r#"
        function test_handler(ctx, event)
            return {inject={content="Follow-up", position="user_suffix"}}
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
    assert!(result.is_ok(), "handler should execute successfully");
    assert!(
        result.unwrap().is_none(),
        "Inject result returns None (processed by daemon)"
    );
}

#[test]
fn test_inject_without_content_field_errors() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    let inject_table = lua.create_table().unwrap();
    inject_table.set("position", "user_prefix").unwrap();
    table.set("inject", inject_table).unwrap();

    let result = interpret_handler_result(&Value::Table(table));
    assert!(
        result.is_err(),
        "Missing content field should error gracefully, got: {:?}",
        result
    );
}

#[test]
fn test_inject_with_empty_content_is_valid() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    let inject_table = lua.create_table().unwrap();
    inject_table.set("content", "").unwrap();
    table.set("inject", inject_table).unwrap();

    let result = interpret_handler_result(&Value::Table(table)).unwrap();
    match result {
        ScriptHandlerResult::Inject { content, position } => {
            assert_eq!(content, "");
            assert_eq!(position, "user_prefix");
        }
        _ => panic!("Expected Inject variant"),
    }
}

#[test]
fn test_interpret_nil_returns_passthrough() {
    let result = interpret_handler_result(&Value::Nil).unwrap();
    assert!(matches!(result, ScriptHandlerResult::PassThrough));
}

#[test]
fn test_interpret_handled_with_result() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    table.set("handled", true).unwrap();
    let result_table = lua.create_table().unwrap();
    result_table.set("answer", 42).unwrap();
    table.set("result", result_table).unwrap();

    let result = interpret_handler_result(&Value::Table(table)).unwrap();
    match result {
        ScriptHandlerResult::Handled { result } => {
            assert_eq!(result["answer"], 42);
        }
        other => panic!("Expected Handled, got: {:?}", other),
    }
}

#[test]
fn test_interpret_handled_without_result_gives_null() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    table.set("handled", true).unwrap();

    let result = interpret_handler_result(&Value::Table(table)).unwrap();
    match result {
        ScriptHandlerResult::Handled { result } => {
            assert!(result.is_null());
        }
        other => panic!("Expected Handled, got: {:?}", other),
    }
}

#[test]
fn test_interpret_cancel_still_works() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    table.set("cancel", true).unwrap();
    table.set("reason", "blocked").unwrap();

    let result = interpret_handler_result(&Value::Table(table)).unwrap();
    match result {
        ScriptHandlerResult::Cancel { reason } => {
            assert_eq!(reason, "blocked");
        }
        other => panic!("Expected Cancel, got: {:?}", other),
    }
}

#[test]
fn test_interpret_handled_takes_priority_over_cancel() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    table.set("handled", true).unwrap();
    table.set("cancel", true).unwrap();
    table.set("reason", "should not see this").unwrap();

    let result = interpret_handler_result(&Value::Table(table)).unwrap();
    // handled is checked before cancel
    assert!(matches!(result, ScriptHandlerResult::Handled { .. }));
}

#[test]
fn test_interpret_handled_false_is_not_handled() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    table.set("handled", false).unwrap();
    table.set("foo", "bar").unwrap();

    let result = interpret_handler_result(&Value::Table(table)).unwrap();
    // handled=false → falls through to Transform
    assert!(matches!(result, ScriptHandlerResult::Transform(_)));
}
