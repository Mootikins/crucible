use crucible_core::turn::TurnOrigin;
use crucible_core::types::{CanonicalToolCall, ToolRender};
use mlua::{Lua, Result as LuaResult};
use serde_json::Value as JsonValue;

use super::before_execute::execute_runtime_json_handler;
use super::registry::LuaScriptHandlerRegistry;
use super::script_handler::ScriptHandlerResult;

pub const TOOL_RENDER_EVENT: &str = super::StageId::ToolRender.as_str();

/// Run the render function of `call.kind`.
///
/// The payload is the call, its `args` and the `origin` of the turn. For a
/// finished call, `outcome` adds its `result` text and its `error`. The
/// renders run from the last registration to the first, and the first
/// answer wins, so a user file or a plugin replaces a shipped render.
/// `None`: no render answered, and the caller uses
/// [`ToolRender::fallback`]. An answer that is not a render is an error.
pub async fn execute_tool_render(
    lua: &Lua,
    registry: &LuaScriptHandlerRegistry,
    session_id: Option<&str>,
    call: &CanonicalToolCall,
    args: &JsonValue,
    origin: TurnOrigin,
    outcome: Option<(&str, Option<&str>)>,
) -> LuaResult<Option<ToolRender>> {
    let handlers = registry.runtime_handlers_for(
        TOOL_RENDER_EVENT,
        Some(&call.kind),
        super::Firing::of(session_id),
    );
    if handlers.is_empty() {
        return Ok(None);
    }
    let mut payload = serde_json::to_value(call).map_err(mlua::Error::external)?;
    payload["args"] = args.clone();
    payload["origin"] = serde_json::json!({ "kind": origin });
    // A JSON null is a true value in Lua, so an absent error stays absent.
    if let Some((result, error)) = outcome {
        payload["result"] = result.into();
        if let Some(error) = error {
            payload["error"] = error.into();
        }
    }
    for handler in handlers.into_iter().rev() {
        let result =
            execute_runtime_json_handler(lua, registry, handler.id, payload.clone(), session_id)
                .await?;
        if let ScriptHandlerResult::Transform(mut render) = result {
            // An empty Lua table has no array form: `fields = {}` comes as `{}`.
            if render["fields"].as_object().is_some_and(|m| m.is_empty()) {
                render["fields"] = serde_json::json!([]);
            }
            return serde_json::from_value(render)
                .map(Some)
                .map_err(mlua::Error::external);
        }
    }
    Ok(None)
}
