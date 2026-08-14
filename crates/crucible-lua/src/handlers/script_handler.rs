use mlua::{Result as LuaResult, Table, Value};
use serde_json::Value as JsonValue;
use tracing::warn;

use super::conversion::lua_table_to_json;
/// Result of script handler execution
///
/// Represents the possible outcomes from a Lua handler function:
/// - Transform: Handler returned a modified event table (as JSON for cross-boundary safety)
/// - PassThrough: Handler returned nil (no changes)
/// - Cancel: Handler returned `{cancel=true, reason="..."}` to abort
/// - Inject: Handler wants to inject a follow-up message
/// - Handled: Handler fully handled the event and provides the result directly
#[derive(Debug, Clone)]
pub enum ScriptHandlerResult {
    /// Handler returned modified event - continue with changes
    /// Stored as JSON to avoid Lua value lifetime issues
    Transform(JsonValue),
    /// Handler returned nil - pass through unchanged
    PassThrough,
    /// Handler returned cancel object - abort pipeline
    Cancel { reason: String },
    /// Handler wants to inject a follow-up message
    Inject {
        /// Content to inject
        content: String,
        /// Where to inject: "user_prefix" (default), "user_suffix"
        position: String,
    },
    /// Handler fully handled the event — use this result instead of default execution.
    /// Returned when Lua handler returns `{ handled = true, result = ..., terminate = bool }`.
    ///
    /// `terminate=true` signals the agent loop should end the turn after this tool
    /// batch (conjunctive across batch: only stops if every tool result in the batch
    /// sets terminate=true).
    Handled { result: JsonValue, terminate: bool },
}

/// Interpret the return value from a Lua handler function
///
/// Implements the neovim-style return conventions:
/// - nil → PassThrough
/// - table with `inject={content="...", position="..."}` → Inject
/// - table with `cancel=true` → Cancel
/// - table without `cancel` or `inject` → Transform
/// - other → Transform (treat as modified value)
pub fn interpret_handler_result(result: &Value) -> LuaResult<ScriptHandlerResult> {
    match result {
        Value::Nil => Ok(ScriptHandlerResult::PassThrough),
        Value::Table(t) => {
            // Directives only apply to directive-SHAPED returns. Flat events
            // carry the envelope `type` key, so a handler returning the event
            // table (a documented transform pattern) must stay a transform
            // even when the event's payload contains `cancel`/`handled`/
            // `inject` fields — otherwise a payload key silently becomes a
            // cancellation.
            let is_directive_shape = !t.contains_key("type").unwrap_or(false);
            if is_directive_shape {
                // {inject={content="...", position="..."}}
                if let Ok(inject_table) = t.get::<Table>("inject") {
                    let content = inject_table.get::<String>("content")?;
                    let position = inject_table
                        .get::<String>("position")
                        .unwrap_or_else(|_| "user_prefix".to_string());
                    return Ok(ScriptHandlerResult::Inject { content, position });
                }
                // {handled=true, result=..., terminate=bool}
                if let Ok(true) = t.get::<bool>("handled") {
                    let result = match lua_table_to_json(t) {
                        Ok(json) => json.get("result").cloned().unwrap_or(JsonValue::Null),
                        Err(_) => JsonValue::Null,
                    };
                    let terminate = t.get::<bool>("terminate").unwrap_or(false);
                    return Ok(ScriptHandlerResult::Handled { result, terminate });
                }
                // {cancel=true, reason="..."}
                if t.get::<bool>("cancel").unwrap_or(false) {
                    let reason = t
                        .get::<String>("reason")
                        .unwrap_or_else(|_| "cancelled".to_string());
                    return Ok(ScriptHandlerResult::Cancel { reason });
                }
            }
            // Anything else is a transform
            let json = lua_table_to_json(t)?;
            Ok(ScriptHandlerResult::Transform(json))
        }
        _ => {
            // Other values treated as transform - convert to JSON
            warn!("Handler returned unexpected type, treating as transform");
            let json = serde_json::to_value(result).map_err(mlua::Error::external)?;
            Ok(ScriptHandlerResult::Transform(json))
        }
    }
}
