use super::super::*;
use crucible_core::InteractionRegistry;
use mlua::{Lua, Table};
use std::sync::{Arc, Mutex};

#[test]
fn test_register_ask_module() {
    let lua = Lua::new();
    register_ask_module(&lua).expect("Should register ask module");

    // Verify ask table exists
    let ask: Table = lua.globals().get("ask").expect("ask should exist");
    assert!(ask.contains_key("question").unwrap());
    assert!(ask.contains_key("batch").unwrap());
}

#[test]
fn test_notify_function() {
    let lua = Lua::new();
    register_ask_module(&lua).expect("Should register ask module");

    // notify should not error
    let script = r#"
            ask.notify("Test notification")
            return true
        "#;

    let result: bool = lua.load(script).eval().expect("Should execute");
    assert!(result);
}

#[test]
fn test_module_has_new_functions() {
    let lua = Lua::new();
    register_ask_module(&lua).expect("Should register ask module");

    let ask: Table = lua.globals().get("ask").expect("ask should exist");
    assert!(ask.contains_key("notify").unwrap());
    assert!(ask.contains_key("answer").unwrap());
    assert!(ask.contains_key("answer_other").unwrap());
}

#[test]
fn test_register_ask_module_with_context() {
    let lua = Lua::new();
    let registry = Arc::new(Mutex::new(InteractionRegistry::new()));
    let push_fn: EventPushCallback = Arc::new(|_event| {});
    let context = Arc::new(LuaAskContext::new(registry, push_fn));

    register_ask_module_with_context(&lua, context).expect("Should register");

    // Verify ask_user function exists
    let ask: Table = lua.globals().get("ask").expect("ask should exist");
    assert!(ask.contains_key("ask_user").unwrap());
}
