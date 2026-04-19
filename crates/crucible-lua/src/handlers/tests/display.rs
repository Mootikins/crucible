use crate::handlers::{
    execute_tool_display_complete_hooks, execute_tool_display_start_hooks,
    register_crucible_on_api, LuaScriptHandlerRegistry, ToolDisplayCompleteEvent,
    ToolDisplayCompleteHints, ToolDisplayStartEvent, ToolDisplayStartHints,
};
use mlua::Lua;

#[tokio::test]
async fn lua_display_start_hook_returns_label_and_detail() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    register_crucible_on_api(
        &lua,
        registry.runtime_handlers.clone(),
        registry.handler_functions.clone(),
    )
    .unwrap();

    lua.load(
        r#"
        crucible.on("tool:display_start", function(ctx, event)
            return {
                label = "Custom " .. event.name,
                detail = "custom_detail"
            }
        end)
    "#,
    )
    .exec()
    .unwrap();

    let event = ToolDisplayStartEvent {
        name: "semantic_search".to_string(),
        args: "{}".to_string(),
    };

    let result = execute_tool_display_start_hooks(&lua, &registry, &event)
        .await
        .unwrap();
    assert_eq!(
        result,
        Some(ToolDisplayStartHints {
            label: Some("Custom semantic_search".to_string()),
            detail: Some("custom_detail".to_string()),
            primary_arg: None,
            max_lines: None,
        })
    );
}

#[tokio::test]
async fn lua_display_complete_hook_returns_summary() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    register_crucible_on_api(
        &lua,
        registry.runtime_handlers.clone(),
        registry.handler_functions.clone(),
    )
    .unwrap();

    lua.load(
        r#"
        crucible.on("tool:display_complete", function(ctx, event)
            return {
                summary = "Result for " .. event.name
            }
        end)
    "#,
    )
    .exec()
    .unwrap();

    let event = ToolDisplayCompleteEvent {
        name: "semantic_search".to_string(),
        args: "{}".to_string(),
        result: "Found 5 notes about authentication".to_string(),
    };

    let result = execute_tool_display_complete_hooks(&lua, &registry, &event)
        .await
        .unwrap();
    assert_eq!(
        result,
        Some(ToolDisplayCompleteHints {
            summary: Some("Result for semantic_search".to_string()),
            max_lines: None,
        })
    );
}
