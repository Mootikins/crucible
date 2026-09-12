use crate::handlers::{
    register_cru_on_api, LuaScriptHandlerRegistry, RegistrationSpec, ScriptHandlerResult, StageId,
};
use crate::plugin_context::LuaSource;
use crucible_core::events::SessionEvent;
use mlua::Lua;

/// Register `handler` for `name` and answer its dispatch id.
///
/// Tests used to push a row straight into the registry's `Vec` and insert the
/// body into a second map by hand. The row and the body live together now, so
/// a test registers the way production does.
fn register<F>(lua: &Lua, registry: &LuaScriptHandlerRegistry, name: StageId, handler: F) -> u64
where
    F: Fn(&Lua, (mlua::Table, mlua::Table)) -> mlua::Result<mlua::Value>
        + mlua::MaybeSend
        + 'static,
{
    let func = lua.create_function(handler).unwrap();
    registry
        .register(lua, RegistrationSpec::new(name.into()), func)
        .unwrap()
}

/// Register a no-op handler for `name` with `priority` and `pattern`.
fn register_stub(
    lua: &Lua,
    registry: &LuaScriptHandlerRegistry,
    name: StageId,
    priority: i64,
    pattern: Option<&str>,
) -> u64 {
    let func = lua.create_function(|_, ()| Ok(())).unwrap();
    registry
        .register(
            lua,
            RegistrationSpec {
                name: name.into(),
                priority,
                pattern: pattern.map(str::to_string),
                scope: crate::handlers::SessionScope::Global,
                key: None,
                once: false,
                timeout_ms: None,
                required: false,
            },
            func,
        )
        .unwrap()
}

fn custom_event(name: &str) -> SessionEvent {
    SessionEvent::Custom {
        name: name.to_string(),
        payload: serde_json::json!({}),
    }
}

#[test]
fn runtime_handler_stores_function_reference() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    register_cru_on_api(&lua, registry.clone()).unwrap();

    let handler_code = r#"
        function test_handler(event)
            return event
        end
        cru.on("pre_tool_call", test_handler)
    "#;
    lua.load(handler_code).eval::<()>().unwrap();

    let handlers =
        registry.runtime_handlers_for("pre_tool_call", None, crate::handlers::Firing::Sessionless);
    assert_eq!(handlers.len(), 1);
    assert_eq!(handlers[0].name, StageId::PreToolCall.into());
    assert_eq!(handlers[0].id, 0, "the first id the allocator hands out");
    let _func: mlua::Function = handlers[0].take_body(&lua).unwrap();
}

#[tokio::test]
async fn execute_runtime_handler_receives_event() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    let id = register(
        &lua,
        &registry,
        StageId::PreToolCall,
        |_, (ctx, event): (mlua::Table, mlua::Table)| {
            // Verify ctx is a table (may be empty)
            let _ctx_type = ctx.raw_len();
            // Verify event has expected fields
            let event_type: String = event.get("event_type").unwrap();
            assert_eq!(event_type, "custom");
            Ok(mlua::Value::Nil)
        },
    );

    let result = registry
        .execute_runtime_handler(&lua, id, &custom_event("test"), None)
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

    let with_session = register(
        &lua,
        &registry,
        StageId::PreToolCall,
        |_, (ctx, _event): (mlua::Table, mlua::Table)| {
            let session_id: String = ctx.get("session_id")?;
            assert_eq!(session_id, "s-ctx");
            Ok(mlua::Value::Nil)
        },
    );

    let event = custom_event("pre_tool_call");
    let result = registry
        .execute_runtime_handler(&lua, with_session, &event, Some("s-ctx"))
        .await;
    assert!(
        result.is_ok(),
        "handler must see ctx.session_id when the dispatch site provides it: {result:?}"
    );

    // Without a session id the field is absent, not empty — a handler can
    // distinguish "no session context" from a session named "".
    let without_session = register(
        &lua,
        &registry,
        StageId::PreToolCall,
        |_, (ctx, _event): (mlua::Table, mlua::Table)| {
            let session_id: Option<String> = ctx.get("session_id")?;
            assert!(session_id.is_none());
            Ok(mlua::Value::Nil)
        },
    );
    let result = registry
        .execute_runtime_handler(&lua, without_session, &event, None)
        .await;
    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test]
async fn execute_runtime_handler_returns_cancel() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    let id = register(&lua, &registry, StageId::PreToolCall, |lua, _| {
        let result = lua.create_table().unwrap();
        result.set("cancel", true).unwrap();
        result.set("reason", "test cancel").unwrap();
        Ok(mlua::Value::Table(result))
    });

    let result = registry
        .execute_runtime_handler(&lua, id, &custom_event("test"), None)
        .await;
    assert!(result.is_ok());
    match result.unwrap() {
        ScriptHandlerResult::Cancel { reason } => {
            assert_eq!(reason, "test cancel");
        }
        other => panic!("Expected Cancel result, got {other:?}"),
    }
}

#[tokio::test]
async fn execute_runtime_handler_returns_handled() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    let id = register(&lua, &registry, StageId::PreToolCall, |lua, _| {
        let result = lua.create_table().unwrap();
        result.set("handled", true).unwrap();
        let inner = lua.create_table().unwrap();
        inner.set("output", "from plugin").unwrap();
        result.set("result", inner).unwrap();
        Ok(mlua::Value::Table(result))
    });

    let result = registry
        .execute_runtime_handler(&lua, id, &custom_event("test"), None)
        .await;
    assert!(result.is_ok());
    match result.unwrap() {
        ScriptHandlerResult::Handled { result, .. } => {
            assert_eq!(result["output"], "from plugin");
        }
        other => panic!("Expected Handled, got: {other:?}"),
    }
}

#[tokio::test]
async fn execute_runtime_handler_passes_through_on_an_unknown_id() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    // An unknown id means the handler is gone (reload cleared it) or never
    // existed; either way it has no opinion. Erroring here used to land in
    // `pre_tool_call`'s fail-closed arm and deny the tool call.
    let result = registry
        .execute_runtime_handler(&lua, 9999, &custom_event("test"), None)
        .await
        .expect("an unknown handler id is not an error");
    assert!(matches!(result, ScriptHandlerResult::PassThrough));
}

#[test]
fn runtime_handlers_for_returns_matching_handlers() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    let a = register_stub(&lua, &registry, StageId::TurnComplete, 100, None);
    let b = register_stub(&lua, &registry, StageId::PreToolCall, 50, None);
    let c = register_stub(&lua, &registry, StageId::TurnComplete, 200, None);

    let matching =
        registry.runtime_handlers_for("turn:complete", None, crate::handlers::Firing::Sessionless);
    assert_eq!(matching.len(), 2);
    assert_eq!(matching[0].id, a);
    assert_eq!(matching[1].id, c);

    let other =
        registry.runtime_handlers_for("pre_tool_call", None, crate::handlers::Firing::Sessionless);
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].id, b);

    let none =
        registry.runtime_handlers_for("nonexistent", None, crate::handlers::Firing::Sessionless);
    assert!(none.is_empty());
}

#[test]
fn runtime_handlers_for_returns_sorted_by_priority() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    let low = register_stub(&lua, &registry, StageId::TurnComplete, 200, None);
    let high = register_stub(&lua, &registry, StageId::TurnComplete, 10, None);
    let medium = register_stub(&lua, &registry, StageId::TurnComplete, 100, None);

    let handlers =
        registry.runtime_handlers_for("turn:complete", None, crate::handlers::Firing::Sessionless);
    assert_eq!(handlers.len(), 3);
    assert_eq!(handlers[0].id, high);
    assert_eq!(handlers[0].priority, 10);
    assert_eq!(handlers[1].id, medium);
    assert_eq!(handlers[1].priority, 100);
    assert_eq!(handlers[2].id, low);
    assert_eq!(handlers[2].priority, 200);
}

#[test]
fn pattern_filtering_matches_exact_tool_name() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    let bash = register_stub(&lua, &registry, StageId::PreToolCall, 10, Some("bash"));
    let all = register_stub(&lua, &registry, StageId::PreToolCall, 100, None);

    // With identifier "bash" — both match
    let matching = registry.runtime_handlers_for(
        "pre_tool_call",
        Some("bash"),
        crate::handlers::Firing::Sessionless,
    );
    assert_eq!(matching.len(), 2);
    assert_eq!(matching[0].id, bash); // priority 10
    assert_eq!(matching[1].id, all); // priority 100

    // With identifier "read_file" — only the no-pattern handler matches
    let matching = registry.runtime_handlers_for(
        "pre_tool_call",
        Some("read_file"),
        crate::handlers::Firing::Sessionless,
    );
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].id, all);

    // With no identifier — only no-pattern handler matches (pattern handlers require identifier)
    let matching =
        registry.runtime_handlers_for("pre_tool_call", None, crate::handlers::Firing::Sessionless);
    assert_eq!(matching.len(), 1);
    assert_eq!(matching[0].id, all);
}

#[test]
fn pattern_filtering_supports_glob() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_stub(&lua, &registry, StageId::PreToolCall, 10, Some("read_*"));

    let matching = registry.runtime_handlers_for(
        "pre_tool_call",
        Some("read_file"),
        crate::handlers::Firing::Sessionless,
    );
    assert_eq!(matching.len(), 1);

    let matching = registry.runtime_handlers_for(
        "pre_tool_call",
        Some("write_file"),
        crate::handlers::Firing::Sessionless,
    );
    assert_eq!(matching.len(), 0);
}

#[tokio::test]
async fn todo_enforcer_pattern_integration() {
    // This test demonstrates the full FSM handler pattern:
    // 1. Register handler with cru.on("turn:complete", fn)
    // 2. Handler checks event for incomplete todos pattern
    // 3. Handler returns {inject={content="Continue..."}} if pattern found
    // 4. Verify result is ScriptHandlerResult::Inject

    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    // Step 1: Register the cru.on API
    register_cru_on_api(&lua, registry.clone()).unwrap();

    // Step 2: Register todo enforcer handler via cru.on
    lua.load(
        r#"
        cru.on("turn:complete", function(ctx, event)
            -- Check if response contains incomplete todos
            local response = event.response or ""
            if response:find("%[ %]") then  -- Finds "[ ]" pattern
                return {
                    inject = {
                        content = "You have incomplete tasks. Please continue working on them."
                    }
                }
            end
            return nil  -- PassThrough if no incomplete todos
        end)
    "#,
    )
    .exec()
    .unwrap();

    let id =
        registry.runtime_handlers_for("turn:complete", None, crate::handlers::Firing::Sessionless)
            [0]
        .id;

    // Step 3: Test with incomplete todo - should trigger injection
    let event_with_todo = SessionEvent::Custom {
        name: "turn:complete".to_string(),
        payload: serde_json::json!({
            "response": "Here are the tasks:\n- [x] Done task\n- [ ] Incomplete task"
        }),
    };

    let result = registry
        .execute_runtime_handler(&lua, id, &event_with_todo, None)
        .await
        .unwrap();

    // Verify result is Inject with expected content
    match result {
        ScriptHandlerResult::Inject { content } => {
            assert!(
                content.contains("incomplete tasks"),
                "Inject content should mention incomplete tasks"
            );
        }
        other => panic!("Expected ScriptHandlerResult::Inject, got {other:?}"),
    }

    // Step 4: Test without incomplete todo - should pass through
    let event_complete = SessionEvent::Custom {
        name: "turn:complete".to_string(),
        payload: serde_json::json!({
            "response": "All tasks done:\n- [x] Task 1\n- [x] Task 2"
        }),
    };

    let result = registry
        .execute_runtime_handler(&lua, id, &event_complete, None)
        .await
        .unwrap();

    // Verify result is PassThrough (no injection)
    assert!(
        matches!(result, ScriptHandlerResult::PassThrough),
        "Expected PassThrough for complete todos, got {result:?}"
    );
}

/// A dispatch id must never be reused. `clear_source` shrinks the list, so an
/// id derived from that list's length would land on one another registrant
/// still holds — and dispatch is by id, so the survivor's row would silently
/// start running the reloaded plugin's function.
#[test]
fn a_cleared_owners_ids_are_not_reused_by_the_next_registration() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    register_cru_on_api(&lua, registry.clone()).unwrap();

    // Two plugins, loaded in order, exactly as the loader does it.
    crate::plugin_context::enter_plugin(&lua, "alpha", false);
    lua.load(
        r#"
        cru.on("turn:complete", function() end)
        cru.on("turn:complete", function() end)
    "#,
    )
    .exec()
    .unwrap();

    crate::plugin_context::enter_plugin(&lua, "beta", false);
    lua.load(r#"cru.on("pre_tool_call", function() end)"#)
        .exec()
        .unwrap();

    let beta_id = registry
        .runtime_handlers_for("pre_tool_call", None, crate::handlers::Firing::Sessionless)
        .first()
        .expect("beta registered one handler")
        .id;

    // Reload alpha: drop its handlers, then let it register again.
    registry.clear_source(&LuaSource::Plugin("alpha".into()));
    crate::plugin_context::enter_plugin(&lua, "alpha", false);
    lua.load(
        r#"
        cru.on("turn:complete", function() end)
        cru.on("turn:complete", function() end)
    "#,
    )
    .exec()
    .unwrap();

    let after_reload =
        registry.runtime_handlers_for("turn:complete", None, crate::handlers::Firing::Sessionless);
    assert!(
        !after_reload.iter().any(|h| h.id == beta_id),
        "reload reused id {beta_id}, which beta still holds"
    );

    // Attribution is orthogonal to id allocation and must still hold: the
    // reload replaced alpha's two handlers rather than appending to them.
    assert_eq!(
        after_reload.len(),
        2,
        "alpha's reload should replace its own handlers, not accumulate them"
    );
    assert_eq!(registry.plugin_handler_count("alpha"), 2);
    assert_eq!(registry.plugin_handler_count("beta"), 1);
}

/// A handler returning the (flat) event table is a TRANSFORM, even when the
/// event's payload contains a `cancel`/`handled` field — flat events carry
/// the envelope `type` key, and only directive-shaped returns (no `type`)
/// may cancel. Without this, a payload key silently cancelled the event.
#[tokio::test]
async fn returning_the_event_with_a_cancel_payload_key_is_not_a_cancellation() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    let id = register(
        &lua,
        &registry,
        StageId::PreToolCall,
        |_, (_ctx, event): (mlua::Table, mlua::Table)| Ok(mlua::Value::Table(event)),
    );

    let event = SessionEvent::Custom {
        name: "weird_event".to_string(),
        payload: serde_json::json!({ "cancel": true, "handled": true, "tool": "bash" }),
    };
    let result = registry
        .execute_runtime_handler(&lua, id, &event, None)
        .await
        .unwrap();
    assert!(
        matches!(result, ScriptHandlerResult::Transform(_)),
        "an echoed event must stay a transform, got {result:?}"
    );
}

/// A handler can be unregistered between the dispatch snapshot and execution
/// — a plugin reload (file watcher, no human in the loop) clears its rows
/// while a tool call is in flight. An absent handler has no opinion about
/// the event: erroring here landed in `pre_tool_call`'s fail-closed arm and
/// denied the tool call on behalf of a handler that no longer exists.
#[tokio::test]
async fn an_unregistered_handler_has_no_opinion_instead_of_failing_closed() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_cru_on_api(&lua, registry.clone()).unwrap();

    crate::plugin_context::enter_plugin(&lua, "alpha", false);
    lua.load(r#"cru.on("pre_tool_call", function() return { cancel = true } end)"#)
        .exec()
        .unwrap();
    let stale_id =
        registry.runtime_handlers_for("pre_tool_call", None, crate::handlers::Firing::Sessionless)
            [0]
        .id;

    // The reload's clear lands between snapshot and execution.
    registry.clear_source(&LuaSource::Plugin("alpha".into()));

    let event = SessionEvent::Custom {
        name: "pre_tool_call".to_string(),
        payload: serde_json::json!({ "tool": "bash" }),
    };
    let result = registry
        .execute_runtime_handler(&lua, stale_id, &event, None)
        .await
        .expect("an unregistered handler must not surface as an error");
    assert!(
        matches!(result, ScriptHandlerResult::PassThrough),
        "got: {result:?}"
    );
}

/// Same contract for the JSON-payload dispatch path (`tool:before_execute`,
/// display hooks).
#[tokio::test]
async fn an_unregistered_json_handler_has_no_opinion_instead_of_failing_closed() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_cru_on_api(&lua, registry.clone()).unwrap();

    let result = crate::handlers::before_execute::execute_runtime_json_handler(
        &lua,
        &registry,
        999,
        serde_json::json!({}),
        None,
    )
    .await
    .expect("an unregistered handler must not surface as an error");
    assert!(
        matches!(result, ScriptHandlerResult::PassThrough),
        "got: {result:?}"
    );
}

/// `search:rerank` is a registered stage, and a handler at it can return
/// the hits in a new order with a widened span.
#[tokio::test]
async fn a_search_rerank_handler_returns_the_hits_it_reordered() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_cru_on_api(&lua, registry.clone()).unwrap();

    lua.load(
        r#"
        cru.on("search:rerank", function(ctx, event)
            local out = {}
            for i = #event.hits, 1, -1 do
                local hit = event.hits[i]
                out[#out + 1] = { index = hit.index, span_end = hit.span_end + 5 }
            end
            return out
        end)
        "#,
    )
    .exec()
    .unwrap();

    let handlers =
        registry.runtime_handlers_for("search:rerank", None, crate::handlers::Firing::Sessionless);
    assert_eq!(handlers.len(), 1);

    let event = SessionEvent::Custom {
        name: "search:rerank".to_string(),
        payload: serde_json::json!({
            "query_vector": [1.0, 0.0],
            "kilns": ["lab"],
            "limit": 2,
            "hits": [
                { "index": 1, "path": "a.md", "span_start": 0, "span_end": 10, "kind": "paragraph", "score": 0.9 },
                { "index": 2, "path": "b.md", "span_start": 20, "span_end": 30, "kind": "paragraph", "score": 0.8 },
            ],
        }),
    };
    let result = registry
        .execute_runtime_handler(&lua, handlers[0].id, &event, None)
        .await
        .unwrap();

    let ScriptHandlerResult::Transform(value) = result else {
        panic!("expected a Transform, got {result:?}");
    };
    // A top-level Lua array crosses as an object keyed by position.
    assert_eq!(
        value,
        serde_json::json!({
            "1": { "index": 2, "span_end": 35 },
            "2": { "index": 1, "span_end": 15 },
        })
    );
}
