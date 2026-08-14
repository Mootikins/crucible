use crate::handlers::{interpret_handler_result, ScriptHandlerResult};
use mlua::{Lua, Value};

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
        ScriptHandlerResult::Handled { result, .. } => {
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
        ScriptHandlerResult::Handled { result, .. } => {
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

#[test]
fn test_interpret_handled_with_terminate() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    table.set("handled", true).unwrap();
    table.set("result", "done").unwrap();
    table.set("terminate", true).unwrap();

    let result = interpret_handler_result(&Value::Table(table)).unwrap();
    match result {
        ScriptHandlerResult::Handled { result, terminate } => {
            assert_eq!(result, serde_json::json!("done"));
            assert!(terminate, "terminate flag should propagate from Lua");
        }
        other => panic!("Expected Handled with terminate=true, got: {:?}", other),
    }
}

#[test]
fn test_interpret_handled_without_terminate_defaults_false() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    table.set("handled", true).unwrap();
    table.set("result", "keep going").unwrap();

    let result = interpret_handler_result(&Value::Table(table)).unwrap();
    match result {
        ScriptHandlerResult::Handled { result, terminate } => {
            assert_eq!(result, serde_json::json!("keep going"));
            assert!(!terminate, "terminate should default to false");
        }
        other => panic!("Expected Handled, got: {:?}", other),
    }
}

/// `{cancel = true}` with no reason still cancels, with a default message.
///
/// Carried over from a registry test that drove this through
/// `LuaScriptHandler::execute`. The handler chain is gone; the interpretation
/// it was really asserting is still live, so the case moved rather than died.
#[test]
fn test_interpret_cancel_without_reason_uses_a_default() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    table.set("cancel", true).unwrap();

    match interpret_handler_result(&Value::Table(table)).unwrap() {
        ScriptHandlerResult::Cancel { reason } => {
            assert!(
                !reason.is_empty(),
                "a cancel with no reason needs a default"
            );
        }
        other => panic!("Expected Cancel, got: {other:?}"),
    }
}

/// `{cancel = false, ...}` is a transform, not a cancellation.
///
/// The distinction that matters: a handler returning a table with a falsy
/// `cancel` key is returning data, and reading the key's presence rather than
/// its value would abort the pipeline on it.
#[test]
fn test_interpret_cancel_false_is_a_transform() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    table.set("cancel", false).unwrap();
    table.set("data", "still valid").unwrap();

    assert!(matches!(
        interpret_handler_result(&Value::Table(table)).unwrap(),
        ScriptHandlerResult::Transform(_)
    ));
}
