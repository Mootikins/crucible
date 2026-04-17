use crate::annotations::DiscoveredHandler;
use crate::error::LuaError;
use crucible_core::events::SessionEvent;
use crucible_core::utils::glob_match;
use mlua::{Function, Lua, LuaSerdeExt, Result as LuaResult, Table, Value};
use serde_json::Value as JsonValue;
use tracing::{debug, warn};

use super::conversion::{lua_table_to_json, session_event_to_lua};

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
    /// Returned when Lua handler returns `{ handled = true, result = ... }`.
    Handled { result: JsonValue },
}

/// Handler for Lua script execution
///
/// Wraps a discovered handler and executes the Lua handler function
/// when events match the configured event type and pattern.
pub struct LuaScriptHandler {
    /// The discovered handler metadata
    pub metadata: DiscoveredHandler,
    /// Source code (cached for reloading)
    source: String,
}

impl LuaScriptHandler {
    /// Create handler from discovered handler metadata
    ///
    /// Reads and caches the source file for later execution.
    pub fn new(discovered: DiscoveredHandler) -> Result<Self, LuaError> {
        let source = std::fs::read_to_string(&discovered.source_path)?;
        Ok(Self {
            metadata: discovered,
            source,
        })
    }

    /// Create handler with pre-loaded source
    ///
    /// Use this when source is already available (e.g., during discovery).
    pub fn with_source(discovered: DiscoveredHandler, source: String) -> Self {
        Self {
            metadata: discovered,
            source,
        }
    }

    /// Reload the source from disk
    pub fn reload(&mut self) -> Result<(), LuaError> {
        self.source = std::fs::read_to_string(&self.metadata.source_path)?;
        debug!("Reloaded handler source: {}", self.metadata.name);
        Ok(())
    }

    /// Get the cached source code
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Check if this handler matches an event
    ///
    /// Matches based on event type name and optional pattern.
    pub fn matches(&self, event: &SessionEvent) -> bool {
        let event_type = event.type_name();
        if event_type != self.metadata.event_type {
            return false;
        }

        // Pattern matching - wildcard matches everything
        if self.metadata.pattern == "*" {
            return true;
        }

        // Use glob-style matching for patterns
        glob_match(&self.metadata.pattern, event_type)
    }

    /// Check if this handler matches an event type and identifier
    ///
    /// More flexible matching for event-type + identifier patterns.
    pub fn matches_with_identifier(&self, event_type: &str, identifier: &str) -> bool {
        if self.metadata.event_type != event_type {
            return false;
        }

        if self.metadata.pattern == "*" {
            return true;
        }

        glob_match(&self.metadata.pattern, identifier)
    }

    /// Execute the handler with an event
    ///
    /// Loads the script, converts the event to a Lua table, calls the handler
    /// function, and converts the result back to a `SessionEvent` if modified.
    ///
    /// # Arguments
    /// * `lua` - The Lua state to execute in
    /// * `event` - The event to process
    ///
    /// # Returns
    /// * `Ok(Some(event))` - Handler returned a modified event
    /// * `Ok(None)` - Handler returned nil (pass through unchanged)
    /// * `Err(e)` - Execution failed
    pub fn execute(&self, lua: &Lua, event: &SessionEvent) -> LuaResult<Option<SessionEvent>> {
        lua.load(&self.source).exec()?;
        let handler: Function = lua.globals().get(self.metadata.handler_fn.as_str())?;

        let ctx_table = lua.create_table()?;
        ctx_table.set("handler_name", self.metadata.name.as_str())?;
        ctx_table.set("priority", self.metadata.priority)?;

        let event_table = session_event_to_lua(lua, event)?;
        let result: Value = handler.call((ctx_table, event_table))?;

        match interpret_handler_result(&result)? {
            ScriptHandlerResult::Transform(json) => {
                // Try to deserialize JSON back to SessionEvent
                match serde_json::from_value::<SessionEvent>(json.clone()) {
                    Ok(event) => Ok(Some(event)),
                    Err(e) => {
                        debug!(
                            "Could not deserialize to SessionEvent ({}), creating Custom event",
                            e
                        );
                        // Get type from JSON or use "Custom"
                        let name = json
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Custom")
                            .to_string();
                        Ok(Some(SessionEvent::Custom {
                            name,
                            payload: json,
                        }))
                    }
                }
            }
            ScriptHandlerResult::PassThrough => Ok(None),
            ScriptHandlerResult::Cancel { reason } => Err(mlua::Error::RuntimeError(format!(
                "Handler cancelled: {}",
                reason
            ))),
            ScriptHandlerResult::Inject { .. } => {
                debug!("Handler returned Inject result (will be processed by daemon)");
                Ok(None)
            }
            ScriptHandlerResult::Handled { .. } => {
                debug!("Handler returned Handled result (will be processed by daemon)");
                Ok(None)
            }
        }
    }

    /// Execute the handler with a JSON context and event
    ///
    /// Lower-level interface that works with JSON values directly.
    pub fn execute_json(
        &self,
        lua: &Lua,
        ctx: JsonValue,
        event: JsonValue,
    ) -> LuaResult<JsonValue> {
        lua.load(&self.source).exec()?;
        let handler: Function = lua.globals().get(self.metadata.handler_fn.as_str())?;

        let ctx_val = lua.to_value(&ctx)?;
        let event_val = lua.to_value(&event)?;
        let result: Value = handler.call((ctx_val, event_val))?;

        match interpret_handler_result(&result)? {
            ScriptHandlerResult::Transform(json) => Ok(json),
            ScriptHandlerResult::PassThrough => Ok(event), // Pass through unchanged
            ScriptHandlerResult::Cancel { reason } => Err(mlua::Error::RuntimeError(format!(
                "Handler cancelled: {}",
                reason
            ))),
            ScriptHandlerResult::Inject { content, position } => {
                debug!(
                    "Handler returned Inject result: content={}, position={}",
                    content, position
                );
                Ok(event)
            }
            ScriptHandlerResult::Handled { result } => {
                debug!("Handler returned Handled result");
                Ok(result)
            }
        }
    }
}

impl std::fmt::Debug for LuaScriptHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LuaScriptHandler")
            .field("metadata", &self.metadata)
            .field("source_len", &self.source.len())
            .finish()
    }
}

impl Clone for LuaScriptHandler {
    fn clone(&self) -> Self {
        Self {
            metadata: self.metadata.clone(),
            source: self.source.clone(),
        }
    }
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
            // {inject={content="...", position="..."}}
            if let Ok(inject_table) = t.get::<Table>("inject") {
                let content = inject_table.get::<String>("content")?;
                let position = inject_table
                    .get::<String>("position")
                    .unwrap_or_else(|_| "user_prefix".to_string());
                return Ok(ScriptHandlerResult::Inject { content, position });
            }
            // {handled=true, result=...}
            if let Ok(true) = t.get::<bool>("handled") {
                let result = match lua_table_to_json(t) {
                    Ok(json) => json.get("result").cloned().unwrap_or(JsonValue::Null),
                    Err(_) => JsonValue::Null,
                };
                return Ok(ScriptHandlerResult::Handled { result });
            }
            // {cancel=true, reason="..."}
            if t.get::<bool>("cancel").unwrap_or(false) {
                let reason = t
                    .get::<String>("reason")
                    .unwrap_or_else(|_| "cancelled".to_string());
                return Ok(ScriptHandlerResult::Cancel { reason });
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
