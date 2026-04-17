use mlua::{Lua, Result as LuaResult};
use serde_json::Value as JsonValue;

use super::before_execute::execute_runtime_json_handler;
use super::registry::LuaScriptHandlerRegistry;
use super::script_handler::ScriptHandlerResult;

pub const TOOL_DISPLAY_START_EVENT: &str = "tool:display_start";
pub const TOOL_DISPLAY_COMPLETE_EVENT: &str = "tool:display_complete";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDisplayStartEvent {
    pub name: String,
    pub args: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolDisplayStartHints {
    pub label: Option<String>,
    pub detail: Option<String>,
    pub primary_arg: Option<String>,
    pub max_lines: Option<usize>,
}

impl ToolDisplayStartHints {
    fn is_empty(&self) -> bool {
        self.label.is_none()
            && self.detail.is_none()
            && self.primary_arg.is_none()
            && self.max_lines.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDisplayCompleteEvent {
    pub name: String,
    pub args: String,
    pub result: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolDisplayCompleteHints {
    pub summary: Option<String>,
    pub max_lines: Option<usize>,
}

impl ToolDisplayCompleteHints {
    fn is_empty(&self) -> bool {
        self.summary.is_none() && self.max_lines.is_none()
    }
}

pub async fn execute_tool_display_start_hooks(
    lua: &Lua,
    registry: &LuaScriptHandlerRegistry,
    event: &ToolDisplayStartEvent,
) -> LuaResult<Option<ToolDisplayStartHints>> {
    let handlers = registry.runtime_handlers_for(TOOL_DISPLAY_START_EVENT, None);
    if handlers.is_empty() {
        return Ok(None);
    }

    let payload = serde_json::json!({
        "name": event.name,
        "args": event.args,
    });

    for handler in handlers {
        match execute_runtime_json_handler(lua, registry, &handler.name, payload.clone()).await? {
            ScriptHandlerResult::Transform(payload) => {
                let hints = parse_display_start_hints(&payload);
                if !hints.is_empty() {
                    return Ok(Some(hints));
                }
            }
            ScriptHandlerResult::PassThrough
            | ScriptHandlerResult::Cancel { .. }
            | ScriptHandlerResult::Inject { .. }
            | ScriptHandlerResult::Handled { .. } => {}
        }
    }

    Ok(None)
}

pub async fn execute_tool_display_complete_hooks(
    lua: &Lua,
    registry: &LuaScriptHandlerRegistry,
    event: &ToolDisplayCompleteEvent,
) -> LuaResult<Option<ToolDisplayCompleteHints>> {
    let handlers = registry.runtime_handlers_for(TOOL_DISPLAY_COMPLETE_EVENT, None);
    if handlers.is_empty() {
        return Ok(None);
    }

    let payload = serde_json::json!({
        "name": event.name,
        "args": event.args,
        "result": event.result,
    });

    for handler in handlers {
        match execute_runtime_json_handler(lua, registry, &handler.name, payload.clone()).await? {
            ScriptHandlerResult::Transform(payload) => {
                let hints = parse_display_complete_hints(&payload);
                if !hints.is_empty() {
                    return Ok(Some(hints));
                }
            }
            ScriptHandlerResult::PassThrough
            | ScriptHandlerResult::Cancel { .. }
            | ScriptHandlerResult::Inject { .. }
            | ScriptHandlerResult::Handled { .. } => {}
        }
    }

    Ok(None)
}

fn parse_display_start_hints(payload: &JsonValue) -> ToolDisplayStartHints {
    ToolDisplayStartHints {
        label: payload
            .get("label")
            .and_then(JsonValue::as_str)
            .map(ToString::to_string),
        detail: payload
            .get("detail")
            .and_then(JsonValue::as_str)
            .map(ToString::to_string),
        primary_arg: payload
            .get("primary_arg")
            .and_then(JsonValue::as_str)
            .map(ToString::to_string),
        max_lines: payload
            .get("max_lines")
            .and_then(JsonValue::as_u64)
            .map(|n| n as usize),
    }
}

fn parse_display_complete_hints(payload: &JsonValue) -> ToolDisplayCompleteHints {
    ToolDisplayCompleteHints {
        summary: payload
            .get("summary")
            .and_then(JsonValue::as_str)
            .map(ToString::to_string),
        max_lines: payload
            .get("max_lines")
            .and_then(JsonValue::as_u64)
            .map(|n| n as usize),
    }
}
