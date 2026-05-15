use crate::handlers::{
    register_crucible_on_api, LuaScriptHandlerRegistry, RuntimeHandler, ScriptHandlerResult,
};
use crucible_core::events::SessionEvent;
use mlua::{Function, Lua};

#[test]
fn runtime_handler_stores_function_reference() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    register_crucible_on_api(
        &lua,
        registry.runtime_handlers.clone(),
        registry.handler_functions.clone(),
    )
    .unwrap();

    let handler_code = r#"
        function test_handler(event)
            return event
        end
        crucible.on("test_event", test_handler)
    "#;
    lua.load(handler_code).eval::<()>().unwrap();

    let runtime_handlers = registry.runtime_handlers.lock().unwrap();
    assert_eq!(runtime_handlers.len(), 1);
    assert_eq!(runtime_handlers[0].event_type, "test_event");
    assert_eq!(runtime_handlers[0].name, "runtime_handler_0");

    let functions = registry.handler_functions.lock().unwrap();
    assert!(functions.contains_key("runtime_handler_0"));
    let key = functions.get("runtime_handler_0").unwrap();
    let _func: Function = lua.registry_value(key).unwrap();
}

#[tokio::test]
async fn execute_runtime_handler_receives_event() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    // Register a handler that captures the event
    let handler_fn = lua
        .create_function(|_, (ctx, event): (mlua::Table, mlua::Table)| {
            // Verify ctx is a table (may be empty)
            let _ctx_type = ctx.raw_len();
            // Verify event has expected fields
            let event_type: String = event.get("event_type").unwrap();
            assert_eq!(event_type, "custom");
            Ok(mlua::Value::Nil)
        })
        .unwrap();

    let key = lua.create_registry_value(handler_fn).unwrap();
    registry
        .handler_functions
        .lock()
        .unwrap()
        .insert("test_handler".to_string(), key);

    let event = SessionEvent::Custom {
        name: "test".to_string(),
        payload: serde_json::json!({}),
    };

    let result = registry
        .execute_runtime_handler(&lua, "test_handler", &event)
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn execute_runtime_handler_returns_cancel() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    // Register a handler that returns cancel
    let handler_fn = lua
        .create_function(|lua, _: (mlua::Table, mlua::Table)| {
            let result = lua.create_table().unwrap();
            result.set("cancel", true).unwrap();
            result.set("reason", "test cancel").unwrap();
            Ok(mlua::Value::Table(result))
        })
        .unwrap();

    let key = lua.create_registry_value(handler_fn).unwrap();
    registry
        .handler_functions
        .lock()
        .unwrap()
        .insert("cancel_handler".to_string(), key);

    let event = SessionEvent::Custom {
        name: "test".to_string(),
        payload: serde_json::json!({}),
    };

    let result = registry
        .execute_runtime_handler(&lua, "cancel_handler", &event)
        .await;
    assert!(result.is_ok());
    match result.unwrap() {
        ScriptHandlerResult::Cancel { reason } => {
            assert_eq!(reason, "test cancel");
        }
        _ => panic!("Expected Cancel result"),
    }
}

#[tokio::test]
async fn execute_runtime_handler_returns_handled() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    let handler_fn = lua
        .create_function(|lua, _: (mlua::Table, mlua::Table)| {
            let result = lua.create_table().unwrap();
            result.set("handled", true).unwrap();
            let inner = lua.create_table().unwrap();
            inner.set("output", "from plugin").unwrap();
            result.set("result", inner).unwrap();
            Ok(mlua::Value::Table(result))
        })
        .unwrap();

    let key = lua.create_registry_value(handler_fn).unwrap();
    registry
        .handler_functions
        .lock()
        .unwrap()
        .insert("handled_handler".to_string(), key);

    let event = SessionEvent::Custom {
        name: "test".to_string(),
        payload: serde_json::json!({}),
    };

    let result = registry
        .execute_runtime_handler(&lua, "handled_handler", &event)
        .await;
    assert!(result.is_ok());
    match result.unwrap() {
        ScriptHandlerResult::Handled { result, .. } => {
            assert_eq!(result["output"], "from plugin");
        }
        other => panic!("Expected Handled, got: {:?}", other),
    }
}

#[tokio::test]
async fn execute_runtime_handler_not_found() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    let event = SessionEvent::Custom {
        name: "test".to_string(),
        payload: serde_json::json!({}),
    };

    let result = registry
        .execute_runtime_handler(&lua, "nonexistent", &event)
        .await;
    assert!(result.is_err());
}

#[test]
fn runtime_handlers_for_returns_matching_handlers() {
    let registry = LuaScriptHandlerRegistry::new();

    {
        let mut handlers = registry.runtime_handlers.lock().unwrap();
        handlers.push(RuntimeHandler {
            event_type: "turn:complete".to_string(),
            name: "handler_a".to_string(),
            priority: 100,
            pattern: None,
        });
        handlers.push(RuntimeHandler {
            event_type: "pre_tool_call".to_string(),
            name: "handler_b".to_string(),
            priority: 50,
            pattern: None,
        });
        handlers.push(RuntimeHandler {
            event_type: "turn:complete".to_string(),
            name: "handler_c".to_string(),
            priority: 200,
            pattern: None,
        });
    }

    let matching = registry.runtime_handlers_for("turn:complete", None);
    assert_eq!(matching.len(), 2);
    assert_eq!(matching[0].name, "handler_a");
    assert_eq!(matching[1].name, "handler_c");

    let other = registry.runtime_handlers_for("pre_tool_call", None);
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].name, "handler_b");

    let none = registry.runtime_handlers_for("nonexistent", None);
    assert!(none.is_empty());
}

#[test]
fn runtime_handlers_for_returns_sorted_by_priority() {
    let registry = LuaScriptHandlerRegistry::new();

    {
        let mut handlers = registry.runtime_handlers.lock().unwrap();
        handlers.push(RuntimeHandler {
            event_type: "turn:complete".to_string(),
            name: "low_priority".to_string(),
            priority: 200,
            pattern: None,
        });
        handlers.push(RuntimeHandler {
            event_type: "turn:complete".to_string(),
            name: "high_priority".to_string(),
            priority: 10,
            pattern: None,
        });
        handlers.push(RuntimeHandler {
            event_type: "turn:complete".to_string(),
            name: "medium_priority".to_string(),
            priority: 100,
            pattern: None,
        });
    }

    let handlers = registry.runtime_handlers_for("turn:complete", None);
    assert_eq!(handlers.len(), 3);
    assert_eq!(handlers[0].name, "high_priority");
    assert_eq!(handlers[0].priority, 10);
    assert_eq!(handlers[1].name, "medium_priority");
    assert_eq!(handlers[1].priority, 100);
    assert_eq!(handlers[2].name, "low_priority");
    assert_eq!(handlers[2].priority, 200);
}

#[test]
fn pattern_filtering_matches_exact_tool_name() {
    let registry = LuaScriptHandlerRegistry::new();
    {
        let mut handlers = registry.runtime_handlers.lock().unwrap();
        handlers.push(RuntimeHandler {
            event_type: "pre_tool_call".to_string(),
            name: "bash_handler".to_string(),
            priority: 10,
            pattern: Some("bash".to_string()),
        });
        handlers.push(RuntimeHandler {
            event_type: "pre_tool_call".to_string(),
            name: "all_handler".to_string(),
            priority: 100,
            pattern: None,
        });
    }

    // With identifier "bash" — both match
    let matching = registry.runtime_handlers_for("pre_tool_call", Some("bash"));
    assert_eq!(matching.len(), 2);
    assert_eq!(matching[0].name, "bash_handler"); // priority 10
    assert_eq!(matching[1].name, "all_handler"); // priority 100

    // With identifier "read_file" — only the no-pattern handler matches
    let matching = registry.runtime_handlers_for("pre_tool_call", Some("read_file"));
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].name, "all_handler");

    // With no identifier — only no-pattern handler matches (pattern handlers require identifier)
    let matching = registry.runtime_handlers_for("pre_tool_call", None);
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].name, "all_handler");
}

#[test]
fn pattern_filtering_supports_glob() {
    let registry = LuaScriptHandlerRegistry::new();
    {
        let mut handlers = registry.runtime_handlers.lock().unwrap();
        handlers.push(RuntimeHandler {
            event_type: "pre_tool_call".to_string(),
            name: "read_handler".to_string(),
            priority: 10,
            pattern: Some("read_*".to_string()),
        });
    }

    let matching = registry.runtime_handlers_for("pre_tool_call", Some("read_file"));
    assert_eq!(matching.len(), 1);

    let matching = registry.runtime_handlers_for("pre_tool_call", Some("write_file"));
    assert_eq!(matching.len(), 0);
}

#[tokio::test]
async fn todo_enforcer_pattern_integration() {
    // This test demonstrates the full FSM handler pattern:
    // 1. Register handler with crucible.on("turn:complete", fn)
    // 2. Handler checks event for incomplete todos pattern
    // 3. Handler returns {inject={content="Continue..."}} if pattern found
    // 4. Verify result is ScriptHandlerResult::Inject

    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    // Step 1: Register the crucible.on API
    register_crucible_on_api(
        &lua,
        registry.runtime_handlers.clone(),
        registry.handler_functions.clone(),
    )
    .unwrap();

    // Step 2: Register todo enforcer handler via crucible.on
    lua.load(
        r#"
        crucible.on("turn:complete", function(ctx, event)
            -- Check if response contains incomplete todos
            local response = event.payload.response or ""
            if response:find("%[ %]") then  -- Finds "[ ]" pattern
                return {
                    inject = {
                        content = "You have incomplete tasks. Please continue working on them.",
                        position = "user_prefix"
                    }
                }
            end
            return nil  -- PassThrough if no incomplete todos
        end)
    "#,
    )
    .exec()
    .unwrap();

    // Step 3: Test with incomplete todo - should trigger injection
    let event_with_todo = SessionEvent::Custom {
        name: "turn:complete".to_string(),
        payload: serde_json::json!({
            "response": "Here are the tasks:\n- [x] Done task\n- [ ] Incomplete task"
        }),
    };

    let result = registry
        .execute_runtime_handler(&lua, "runtime_handler_0", &event_with_todo)
        .await
        .unwrap();

    // Verify result is Inject with expected content
    match result {
        ScriptHandlerResult::Inject { content, position } => {
            assert!(
                content.contains("incomplete tasks"),
                "Inject content should mention incomplete tasks"
            );
            assert_eq!(
                position, "user_prefix",
                "Position should be user_prefix by default"
            );
        }
        _ => panic!("Expected ScriptHandlerResult::Inject, got {:?}", result),
    }

    // Step 4: Test without incomplete todo - should pass through
    let event_complete = SessionEvent::Custom {
        name: "turn:complete".to_string(),
        payload: serde_json::json!({
            "response": "All tasks done:\n- [x] Task 1\n- [x] Task 2"
        }),
    };

    let result = registry
        .execute_runtime_handler(&lua, "runtime_handler_0", &event_complete)
        .await
        .unwrap();

    // Verify result is PassThrough (no injection)
    assert!(
        matches!(result, ScriptHandlerResult::PassThrough),
        "Expected PassThrough for complete todos, got {:?}",
        result
    );
}
