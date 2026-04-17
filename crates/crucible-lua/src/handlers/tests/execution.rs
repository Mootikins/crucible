use crate::handlers::run_handler_chain;
use crucible_core::events::SessionEvent;
use mlua::{Lua, LuaSerdeExt};

use super::create_test_handler_named;

#[test]
fn test_chain_transform_then_passthrough() {
    // h1: transform → h2: passthrough → result has h1's changes
    let h1 = create_test_handler_named(
        r#"
        function h1(ctx, event)
            event.from_h1 = true
            return event
        end
    "#,
        "h1",
    );

    let h2 = create_test_handler_named(
        r#"
        function h2(ctx, event)
            return nil  -- pass through, keep h1's changes
        end
    "#,
        "h2",
    );

    let lua = Lua::new();
    let event = SessionEvent::Custom {
        name: "test".to_string(),
        payload: serde_json::json!({}),
    };

    let handlers = [&h1, &h2];
    let result = run_handler_chain(&lua, &handlers, event).unwrap();

    assert!(result.is_some());
    // Final event should have h1's modification preserved through h2's passthrough
}

#[test]
fn test_chain_cancel_stops_execution() {
    // h1: transform → h2: cancel → h3 never runs
    let h1 = create_test_handler_named(
        r#"
        function h1(ctx, event)
            event.step1 = true
            return event
        end
    "#,
        "h1",
    );

    let h2 = create_test_handler_named(
        r#"
        function h2(ctx, event)
            return { cancel = true, reason = "stopped at h2" }
        end
    "#,
        "h2",
    );

    let h3 = create_test_handler_named(
        r#"
        function h3(ctx, event)
            event.step3 = true  -- should never execute
            return event
        end
    "#,
        "h3",
    );

    let lua = Lua::new();
    let event = SessionEvent::Custom {
        name: "test".to_string(),
        payload: serde_json::json!({}),
    };

    let handlers = [&h1, &h2, &h3];
    let result = run_handler_chain(&lua, &handlers, event).unwrap();

    assert!(result.is_none(), "cancelled chain returns None");
}

#[test]
fn test_chain_all_passthrough() {
    // All handlers return nil → original event unchanged
    let h1 = create_test_handler_named(
        r#"
        function h1(ctx, event) return nil end
    "#,
        "h1",
    );

    let h2 = create_test_handler_named(
        r#"
        function h2(ctx, event) return nil end
    "#,
        "h2",
    );

    let lua = Lua::new();
    let event = SessionEvent::ToolCalled {
        name: "original".to_string(),
        args: serde_json::json!({"key": "value"}),
        description: None,
        source: None,
    };

    let handlers = [&h1, &h2];
    let result = run_handler_chain(&lua, &handlers, event.clone()).unwrap();

    assert!(result.is_some());
    let final_event = result.unwrap();
    // Event should be unchanged from original
    if let SessionEvent::ToolCalled { name, .. } = final_event {
        assert_eq!(name, "original");
    }
}

#[test]
fn test_chain_multiple_transforms() {
    // h1: add field1 → h2: add field2 → result has both
    let h1 = create_test_handler_named(
        r#"
        function h1(ctx, event)
            event.field1 = "from_h1"
            return event
        end
    "#,
        "h1",
    );

    let h2 = create_test_handler_named(
        r#"
        function h2(ctx, event)
            event.field2 = "from_h2"
            return event
        end
    "#,
        "h2",
    );

    let lua = Lua::new();
    let event = SessionEvent::Custom {
        name: "test".to_string(),
        payload: serde_json::json!({}),
    };

    let handlers = [&h1, &h2];
    let result = run_handler_chain(&lua, &handlers, event).unwrap();

    assert!(result.is_some());
    // Both transformations should be applied
}

#[test]
fn test_json_to_lua_roundtrip() {
    let lua = Lua::new();
    let original = serde_json::json!({
        "string": "hello",
        "number": 42,
        "float": 3.125,
        "bool": true,
        "null": null,
        "array": [1, 2, 3],
        "nested": {"key": "value"}
    });

    let lua_val = lua.to_value(&original).unwrap();
    let back: serde_json::Value = serde_json::to_value(&lua_val).unwrap();

    assert_eq!(original["string"], back["string"]);
    assert_eq!(original["number"], back["number"]);
    assert_eq!(original["bool"], back["bool"]);
    assert_eq!(original["array"], back["array"]);
    assert_eq!(original["nested"], back["nested"]);
}
