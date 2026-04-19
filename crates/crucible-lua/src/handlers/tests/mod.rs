use crate::annotations::DiscoveredHandler;

use super::LuaScriptHandler;

mod display;
mod execution;
mod interpret;
mod permission;
mod registry;
mod runtime;
mod script_handler;

pub(super) fn create_test_handler(source: &str) -> LuaScriptHandler {
    let hook = DiscoveredHandler {
        name: "test_handler".to_string(),
        event_type: "ToolCalled".to_string(),
        pattern: "*".to_string(),
        priority: 100,
        description: "Test handler".to_string(),
        source_path: "test.lua".to_string(),
        handler_fn: "test_handler".to_string(),
        is_fennel: false,
    };
    LuaScriptHandler::with_source(hook, source.to_string())
}

pub(super) fn create_test_handler_named(source: &str, fn_name: &str) -> LuaScriptHandler {
    let handler = DiscoveredHandler {
        name: fn_name.to_string(),
        event_type: "Custom".to_string(),
        pattern: "*".to_string(),
        priority: 100,
        description: "Test handler".to_string(),
        source_path: "test.lua".to_string(),
        handler_fn: fn_name.to_string(),
        is_fennel: false,
    };
    LuaScriptHandler::with_source(handler, source.to_string())
}
