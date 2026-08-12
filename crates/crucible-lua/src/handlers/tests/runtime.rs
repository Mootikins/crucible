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
        crucible.on("pre_tool_call", test_handler)
    "#;
    lua.load(handler_code).eval::<()>().unwrap();

    let runtime_handlers = registry.runtime_handlers.lock().unwrap();
    assert_eq!(runtime_handlers.len(), 1);
    assert_eq!(runtime_handlers[0].event_type, "pre_tool_call");
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
        .execute_runtime_handler(&lua, "test_handler", &event, None)
        .await;
    assert!(result.is_ok());
}

/// A handler registered once at plugin load serves every session; the only
/// way it can tell sessions apart is `ctx.session_id`. This was silently
/// absent — `oci` keyed containers by it and every lookup returned nil, so
/// interception no-opped for all sessions while looking registered.
#[tokio::test]
async fn execute_runtime_handler_delivers_session_id_in_ctx() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    let handler_fn = lua
        .create_function(|_, (ctx, _event): (mlua::Table, mlua::Table)| {
            let session_id: String = ctx.get("session_id")?;
            assert_eq!(session_id, "s-ctx");
            Ok(mlua::Value::Nil)
        })
        .unwrap();
    let key = lua.create_registry_value(handler_fn).unwrap();
    registry
        .handler_functions
        .lock()
        .unwrap()
        .insert("ctx_handler".to_string(), key);

    let event = SessionEvent::Custom {
        name: "pre_tool_call".to_string(),
        payload: serde_json::json!({}),
    };

    let result = registry
        .execute_runtime_handler(&lua, "ctx_handler", &event, Some("s-ctx"))
        .await;
    assert!(
        result.is_ok(),
        "handler must see ctx.session_id when the dispatch site provides it: {result:?}"
    );

    // Without a session id the field is absent, not empty — a handler can
    // distinguish "no session context" from a session named "".
    let handler_fn = lua
        .create_function(|_, (ctx, _event): (mlua::Table, mlua::Table)| {
            let session_id: Option<String> = ctx.get("session_id")?;
            assert!(session_id.is_none());
            Ok(mlua::Value::Nil)
        })
        .unwrap();
    let key = lua.create_registry_value(handler_fn).unwrap();
    registry
        .handler_functions
        .lock()
        .unwrap()
        .insert("no_ctx_handler".to_string(), key);
    let result = registry
        .execute_runtime_handler(&lua, "no_ctx_handler", &event, None)
        .await;
    assert!(result.is_ok(), "{result:?}");
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
        .execute_runtime_handler(&lua, "cancel_handler", &event, None)
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
        .execute_runtime_handler(&lua, "handled_handler", &event, None)
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
        .execute_runtime_handler(&lua, "nonexistent", &event, None)
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
            plugin: None,
        });
        handlers.push(RuntimeHandler {
            event_type: "pre_tool_call".to_string(),
            name: "handler_b".to_string(),
            priority: 50,
            pattern: None,
            plugin: None,
        });
        handlers.push(RuntimeHandler {
            event_type: "turn:complete".to_string(),
            name: "handler_c".to_string(),
            priority: 200,
            pattern: None,
            plugin: None,
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
            plugin: None,
        });
        handlers.push(RuntimeHandler {
            event_type: "turn:complete".to_string(),
            name: "high_priority".to_string(),
            priority: 10,
            pattern: None,
            plugin: None,
        });
        handlers.push(RuntimeHandler {
            event_type: "turn:complete".to_string(),
            name: "medium_priority".to_string(),
            priority: 100,
            pattern: None,
            plugin: None,
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
            plugin: None,
        });
        handlers.push(RuntimeHandler {
            event_type: "pre_tool_call".to_string(),
            name: "all_handler".to_string(),
            priority: 100,
            pattern: None,
            plugin: None,
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
            plugin: None,
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
            local response = event.response or ""
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
        .execute_runtime_handler(&lua, "runtime_handler_0", &event_with_todo, None)
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
        .execute_runtime_handler(&lua, "runtime_handler_0", &event_complete, None)
        .await
        .unwrap();

    // Verify result is PassThrough (no injection)
    assert!(
        matches!(result, ScriptHandlerResult::PassThrough),
        "Expected PassThrough for complete todos, got {:?}",
        result
    );
}

/// A handler name must never be reused. `clear_plugin_handlers` shrinks the
/// `runtime_handlers` Vec, so a name derived from that Vec's length lands on a
/// name another registrant still holds in `handler_functions` — and dispatch is
/// by name, so the survivor's entry silently starts running the reloaded
/// plugin's function.
#[test]
fn a_cleared_plugins_names_are_not_reused_by_the_next_registration() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    register_crucible_on_api(
        &lua,
        registry.runtime_handlers(),
        registry.handler_functions(),
    )
    .unwrap();

    // Two plugins, loaded in order, exactly as the loader does it.
    lua.globals()
        .set("__crucible_loading_plugin__", "alpha")
        .unwrap();
    lua.load(
        r#"
        crucible.on("turn:complete", function() end)
        crucible.on("turn:complete", function() end)
    "#,
    )
    .exec()
    .unwrap();

    lua.globals()
        .set("__crucible_loading_plugin__", "beta")
        .unwrap();
    lua.load(r#"crucible.on("pre_tool_call", function() end)"#)
        .exec()
        .unwrap();

    let beta_name = registry
        .runtime_handlers_for("pre_tool_call", None)
        .first()
        .expect("beta registered one handler")
        .name
        .clone();

    // Reload alpha: drop its handlers, then let it register again.
    registry.clear_plugin_handlers("alpha");
    lua.globals()
        .set("__crucible_loading_plugin__", "alpha")
        .unwrap();
    lua.load(
        r#"
        crucible.on("turn:complete", function() end)
        crucible.on("turn:complete", function() end)
    "#,
    )
    .exec()
    .unwrap();

    let after_reload = registry.runtime_handlers_for("turn:complete", None);
    let reused: Vec<&String> = after_reload
        .iter()
        .map(|h| &h.name)
        .filter(|n| **n == beta_name)
        .collect();
    assert!(
        reused.is_empty(),
        "reload reused '{beta_name}', which beta still holds in handler_functions"
    );

    // Attribution is orthogonal to name allocation and must still hold: the
    // reload replaced alpha's two handlers rather than appending to them.
    assert_eq!(
        after_reload.len(),
        2,
        "alpha's reload should replace its own handlers, not accumulate them"
    );
    assert_eq!(registry.plugin_handler_count("alpha"), 2);
    assert_eq!(registry.plugin_handler_count("beta"), 1);
}

/// The name allocator makes this unreachable; assert it is nonetheless loud,
/// because the silent version of this was a live misbinding bug — an overwrite
/// orphans the live body and leaves its owner's handler pointing at the new one.
#[test]
fn registering_over_a_live_handler_name_is_an_error_not_an_overwrite() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_crucible_on_api(
        &lua,
        registry.runtime_handlers(),
        registry.handler_functions(),
    )
    .unwrap();

    // Occupy the first name the allocator will hand out.
    let squatter = lua.create_function(|_, (): ()| Ok(())).unwrap();
    let key = lua.create_registry_value(squatter).unwrap();
    registry
        .handler_functions()
        .lock()
        .unwrap()
        .insert("runtime_handler_0".to_string(), key);

    let err = lua
        .load(r#"crucible.on("turn:complete", function() end)"#)
        .exec()
        .expect_err("a name collision must fail the registration");
    assert!(
        err.to_string().contains("handler name collision"),
        "got: {err}"
    );

    // A refused registration must leave nothing behind: a `RuntimeHandler`
    // whose function is missing dispatches to "Handler not found", which
    // `pre_tool_call` turns into a denied tool call.
    assert!(
        registry
            .runtime_handlers_for("turn:complete", None)
            .is_empty(),
        "the refused registration left a handler with no function behind"
    );
}

/// A handler returning the (flat) event table is a TRANSFORM, even when the
/// event's payload contains a `cancel`/`handled` field — flat events carry
/// the envelope `type` key, and only directive-shaped returns (no `type`)
/// may cancel. Without this, a payload key silently cancelled the event.
#[tokio::test]
async fn returning_the_event_with_a_cancel_payload_key_is_not_a_cancellation() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    let handler_fn = lua
        .create_function(|_, (_ctx, event): (mlua::Table, mlua::Table)| Ok(event))
        .unwrap();
    let key = lua.create_registry_value(handler_fn).unwrap();
    registry
        .handler_functions
        .lock()
        .unwrap()
        .insert("echo_handler".to_string(), key);

    let event = SessionEvent::Custom {
        name: "weird_event".to_string(),
        payload: serde_json::json!({ "cancel": true, "handled": true, "tool": "bash" }),
    };
    let result = registry
        .execute_runtime_handler(&lua, "echo_handler", &event, None)
        .await
        .unwrap();
    assert!(
        matches!(result, ScriptHandlerResult::Transform(_)),
        "an echoed event must stay a transform, got {result:?}"
    );
}
