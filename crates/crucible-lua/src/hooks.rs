//! Hook execution for Lua scripts
//!
//! Executes Lua functions discovered via `@hook` annotations. This module provides
//! the bridge between event bus events and Lua script execution.
//!
//! ## Example
//!
//! ```lua
//! --- Filter search results
//! -- @hook event="tool:after" pattern="search_*" priority=50
//! function filter_results(ctx, event)
//!     -- Modify event.result before returning
//!     return event
//! end
//! ```
//!
//! ## Lifecycle
//!
//! 1. Hooks are discovered from Lua/Fennel sources via `AnnotationParser`
//! 2. `LuaHookHandler` is created from each `DiscoveredHook`
//! 3. Handlers are registered on the event bus
//! 4. Events trigger matching handlers in priority order

use crate::annotations::DiscoveredHook;
use crate::error::LuaError;
use crucible_core::events::SessionEvent;
use mlua::{Function, Lua, Result as LuaResult, Table, Value};
use serde_json::Value as JsonValue;
use tracing::{debug, warn};

/// Handler for Lua hook execution
///
/// Wraps a discovered `DiscoveredHook` and executes the Lua handler function
/// when events match the configured event type and pattern.
pub struct LuaHookHandler {
    /// The discovered hook metadata
    pub metadata: DiscoveredHook,
    /// Source code (cached for reloading)
    source: String,
}

impl LuaHookHandler {
    /// Create handler from discovered hook
    ///
    /// Reads and caches the source file for later execution.
    pub fn new(hook: DiscoveredHook) -> Result<Self, LuaError> {
        let source = std::fs::read_to_string(&hook.source_path)?;
        Ok(Self {
            metadata: hook,
            source,
        })
    }

    /// Create handler with pre-loaded source
    ///
    /// Use this when source is already available (e.g., during discovery).
    pub fn with_source(hook: DiscoveredHook, source: String) -> Self {
        Self {
            metadata: hook,
            source,
        }
    }

    /// Reload the source from disk
    pub fn reload(&mut self) -> Result<(), LuaError> {
        self.source = std::fs::read_to_string(&self.metadata.source_path)?;
        debug!("Reloaded hook source: {}", self.metadata.name);
        Ok(())
    }

    /// Get the cached source code
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Check if this hook matches an event
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
        match_glob(&self.metadata.pattern, event_type)
    }

    /// Check if this hook matches an event type and identifier
    ///
    /// More flexible matching for event-type + identifier patterns.
    pub fn matches_with_identifier(&self, event_type: &str, identifier: &str) -> bool {
        if self.metadata.event_type != event_type {
            return false;
        }

        if self.metadata.pattern == "*" {
            return true;
        }

        match_glob(&self.metadata.pattern, identifier)
    }

    /// Execute the hook with an event
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
        // Load and execute the source to define functions
        lua.load(&self.source).exec()?;

        // Get the handler function
        let handler: Function = lua.globals().get(self.metadata.handler_fn.as_str())?;

        // Create context table
        let ctx_table = lua.create_table()?;
        ctx_table.set("hook_name", self.metadata.name.as_str())?;
        ctx_table.set("priority", self.metadata.priority)?;

        // Convert event to Lua table
        let event_table = session_event_to_lua(lua, event)?;

        // Call handler with (ctx, event)
        let result: Value = handler.call((ctx_table, event_table))?;

        // Convert result back if not nil
        match result {
            Value::Table(t) => lua_table_to_session_event(&t).map(Some),
            Value::Nil => Ok(None),
            _ => {
                warn!(
                    "Hook {} returned unexpected type, expected table or nil",
                    self.metadata.name
                );
                Ok(None)
            }
        }
    }

    /// Execute the hook with a JSON context and event
    ///
    /// Lower-level interface that works with JSON values directly.
    pub fn execute_json(
        &self,
        lua: &Lua,
        ctx: JsonValue,
        event: JsonValue,
    ) -> LuaResult<JsonValue> {
        // Load and execute the source
        lua.load(&self.source).exec()?;

        // Get the handler function
        let handler: Function = lua.globals().get(self.metadata.handler_fn.as_str())?;

        // Convert to Lua values
        let ctx_val = json_to_lua(lua, ctx)?;
        let event_val = json_to_lua(lua, event.clone())?;

        // Call handler
        let result: Value = handler.call((ctx_val, event_val))?;

        // Convert result back to JSON
        match result {
            Value::Nil => Ok(event), // Pass through unchanged
            _ => lua_to_json(result),
        }
    }
}

impl std::fmt::Debug for LuaHookHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LuaHookHandler")
            .field("metadata", &self.metadata)
            .field("source_len", &self.source.len())
            .finish()
    }
}

impl Clone for LuaHookHandler {
    fn clone(&self) -> Self {
        Self {
            metadata: self.metadata.clone(),
            source: self.source.clone(),
        }
    }
}

/// Convert SessionEvent to Lua table
///
/// Creates a Lua table representation of the event suitable for script processing.
fn session_event_to_lua(lua: &Lua, event: &SessionEvent) -> LuaResult<Table> {
    let table = lua.create_table()?;

    // Common fields
    table.set("type", event.type_name())?;
    table.set("event_type", event.event_type())?;
    table.set("summary", event.summary(200))?;

    // Serialize event to JSON and then to Lua for full access to fields
    match serde_json::to_value(event) {
        Ok(json) => {
            // Flatten JSON fields into the table
            if let JsonValue::Object(map) = json {
                for (key, value) in map {
                    if key != "type" {
                        // Don't overwrite our type field
                        let lua_val = json_to_lua(lua, value)?;
                        table.set(key, lua_val)?;
                    }
                }
            }
        }
        Err(e) => {
            warn!("Failed to serialize event to JSON: {}", e);
        }
    }

    Ok(table)
}

/// Convert Lua table back to SessionEvent
///
/// Attempts to reconstruct a SessionEvent from a Lua table.
/// Falls back to a Custom event if the structure doesn't match a known variant.
fn lua_table_to_session_event(table: &Table) -> LuaResult<SessionEvent> {
    // Get the event type to determine variant
    let event_type: String = table.get("type").unwrap_or_else(|_| "Custom".to_string());

    // Convert table to JSON first
    let json = lua_table_to_json(table)?;

    // Try to deserialize to SessionEvent
    match serde_json::from_value::<SessionEvent>(json.clone()) {
        Ok(event) => Ok(event),
        Err(e) => {
            debug!(
                "Could not deserialize to SessionEvent ({}), creating Custom event",
                e
            );
            // Fall back to Custom event
            Ok(SessionEvent::Custom {
                name: event_type,
                payload: json,
            })
        }
    }
}

/// Convert Lua table to JSON value
fn lua_table_to_json(table: &Table) -> LuaResult<JsonValue> {
    let mut map = serde_json::Map::new();

    for pair in table.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;

        // Convert key to string
        let key_str = match key {
            Value::String(s) => s.to_str()?.to_string(),
            Value::Integer(i) => i.to_string(),
            _ => continue, // Skip non-string, non-integer keys
        };

        let json_val = lua_to_json(value)?;
        map.insert(key_str, json_val);
    }

    Ok(JsonValue::Object(map))
}

/// Convert JSON to Lua value
fn json_to_lua(lua: &Lua, value: JsonValue) -> LuaResult<Value> {
    match value {
        JsonValue::Null => Ok(Value::Nil),
        JsonValue::Bool(b) => Ok(Value::Boolean(b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(f))
            } else {
                Ok(Value::Nil)
            }
        }
        JsonValue::String(s) => Ok(Value::String(lua.create_string(&s)?)),
        JsonValue::Array(arr) => {
            let table = lua.create_table()?;
            for (i, v) in arr.into_iter().enumerate() {
                table.set(i + 1, json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
        JsonValue::Object(obj) => {
            let table = lua.create_table()?;
            for (k, v) in obj {
                table.set(k, json_to_lua(lua, v)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}

/// Convert Lua value to JSON
fn lua_to_json(value: Value) -> LuaResult<JsonValue> {
    match value {
        Value::Nil => Ok(JsonValue::Null),
        Value::Boolean(b) => Ok(JsonValue::Bool(b)),
        Value::Integer(i) => Ok(JsonValue::Number(i.into())),
        Value::Number(n) => Ok(serde_json::Number::from_f64(n)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null)),
        Value::String(s) => Ok(JsonValue::String(s.to_str()?.to_string())),
        Value::Table(t) => {
            // Check if it's an array (sequential integer keys starting at 1)
            let len = t.raw_len();
            let is_array = len > 0 && {
                let mut is_seq = true;
                for i in 1..=len {
                    if t.get::<Value>(i).is_err() {
                        is_seq = false;
                        break;
                    }
                }
                is_seq
            };

            if is_array {
                let mut arr = Vec::with_capacity(len);
                for i in 1..=len {
                    let v: Value = t.get(i)?;
                    arr.push(lua_to_json(v)?);
                }
                Ok(JsonValue::Array(arr))
            } else {
                let mut map = serde_json::Map::new();
                for pair in t.pairs::<String, Value>() {
                    let (k, v) = pair?;
                    map.insert(k, lua_to_json(v)?);
                }
                Ok(JsonValue::Object(map))
            }
        }
        // Functions, userdata, etc. become null
        _ => Ok(JsonValue::Null),
    }
}

/// Simple glob pattern matching
///
/// Supports `*` for any number of characters and `?` for single character.
fn match_glob(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let pattern_chars: Vec<char> = pattern.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();

    let mut pattern_idx = 0;
    let mut text_idx = 0;
    let mut star_idx: Option<usize> = None;
    let mut match_idx: Option<usize> = None;

    while text_idx < text_chars.len() {
        if pattern_idx < pattern_chars.len() && pattern_chars[pattern_idx] == '*' {
            star_idx = Some(pattern_idx);
            match_idx = Some(text_idx);
            pattern_idx += 1;
        } else if pattern_idx < pattern_chars.len()
            && (pattern_chars[pattern_idx] == text_chars[text_idx]
                || pattern_chars[pattern_idx] == '?')
        {
            pattern_idx += 1;
            text_idx += 1;
        } else if let Some(star) = star_idx {
            pattern_idx = star + 1;
            match_idx = Some(match_idx.unwrap() + 1);
            text_idx = match_idx.unwrap();
        } else {
            return false;
        }
    }

    // Check for remaining stars in pattern
    while pattern_idx < pattern_chars.len() && pattern_chars[pattern_idx] == '*' {
        pattern_idx += 1;
    }

    pattern_idx == pattern_chars.len()
}

/// Result of hook execution
#[derive(Debug, Clone)]
pub struct HookResult {
    /// Whether execution succeeded
    pub success: bool,
    /// Modified event if any
    pub event: Option<SessionEvent>,
    /// Error message if failed
    pub error: Option<String>,
    /// Hook name for logging
    pub hook_name: String,
}

impl HookResult {
    /// Create a successful result with modified event
    pub fn ok(hook_name: impl Into<String>, event: SessionEvent) -> Self {
        Self {
            success: true,
            event: Some(event),
            error: None,
            hook_name: hook_name.into(),
        }
    }

    /// Create a successful result with no modification (pass-through)
    pub fn pass_through(hook_name: impl Into<String>) -> Self {
        Self {
            success: true,
            event: None,
            error: None,
            hook_name: hook_name.into(),
        }
    }

    /// Create a failed result
    pub fn err(hook_name: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            success: false,
            event: None,
            error: Some(error.into()),
            hook_name: hook_name.into(),
        }
    }
}

/// Execute a hook and return a structured result
pub fn execute_hook(
    handler: &LuaHookHandler,
    lua: &Lua,
    event: &SessionEvent,
) -> HookResult {
    match handler.execute(lua, event) {
        Ok(Some(modified_event)) => HookResult::ok(&handler.metadata.name, modified_event),
        Ok(None) => HookResult::pass_through(&handler.metadata.name),
        Err(e) => HookResult::err(&handler.metadata.name, e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_hook(source: &str) -> LuaHookHandler {
        let hook = DiscoveredHook {
            name: "test_hook".to_string(),
            event_type: "ToolCalled".to_string(),
            pattern: "*".to_string(),
            priority: 100,
            description: "Test hook".to_string(),
            source_path: "test.lua".to_string(),
            handler_fn: "test_handler".to_string(),
            is_fennel: false,
        };
        LuaHookHandler::with_source(hook, source.to_string())
    }

    #[test]
    fn test_match_glob_star() {
        assert!(match_glob("*", "anything"));
        assert!(match_glob("just_*", "just_test"));
        assert!(match_glob("just_*", "just_build"));
        assert!(match_glob("*_test", "unit_test"));
        assert!(match_glob("*_test_*", "unit_test_foo"));
        assert!(!match_glob("just_*", "other_test"));
    }

    #[test]
    fn test_match_glob_exact() {
        assert!(match_glob("test", "test"));
        assert!(!match_glob("test", "testing"));
    }

    #[test]
    fn test_match_glob_question() {
        assert!(match_glob("test?", "tests"));
        assert!(match_glob("t?st", "test"));
        assert!(!match_glob("test?", "test"));
    }

    #[test]
    fn test_hook_handler_creation() {
        let hook = DiscoveredHook {
            name: "filter_hook".to_string(),
            event_type: "ToolCalled".to_string(),
            pattern: "search_*".to_string(),
            priority: 50,
            description: "Filter search results".to_string(),
            source_path: "test.lua".to_string(),
            handler_fn: "filter_results".to_string(),
            is_fennel: false,
        };

        let handler = LuaHookHandler::with_source(hook, "-- test source".to_string());
        assert_eq!(handler.metadata.name, "filter_hook");
        assert_eq!(handler.metadata.priority, 50);
        assert_eq!(handler.source(), "-- test source");
    }

    #[test]
    fn test_hook_matches_event_type() {
        let handler = create_test_hook("");

        let event = SessionEvent::ToolCalled {
            name: "search".to_string(),
            args: serde_json::json!({}),
        };

        assert!(handler.matches(&event));

        let other_event = SessionEvent::ToolCompleted {
            name: "search".to_string(),
            result: "done".to_string(),
            error: None,
        };
        assert!(!handler.matches(&other_event));
    }

    #[test]
    fn test_hook_matches_with_pattern() {
        let hook = DiscoveredHook {
            name: "test".to_string(),
            event_type: "tool_called".to_string(),
            pattern: "search_*".to_string(),
            priority: 100,
            description: "".to_string(),
            source_path: "".to_string(),
            handler_fn: "handler".to_string(),
            is_fennel: false,
        };

        let handler = LuaHookHandler::with_source(hook, String::new());

        assert!(handler.matches_with_identifier("tool_called", "search_notes"));
        assert!(handler.matches_with_identifier("tool_called", "search_files"));
        assert!(!handler.matches_with_identifier("tool_called", "fetch_data"));
        assert!(!handler.matches_with_identifier("other_event", "search_notes"));
    }

    #[test]
    fn test_execute_simple_hook() {
        let source = r#"
            function test_handler(ctx, event)
                event.modified = true
                return event
            end
        "#;

        let handler = create_test_hook(source);
        let lua = Lua::new();

        let event = SessionEvent::ToolCalled {
            name: "test".to_string(),
            args: serde_json::json!({"key": "value"}),
        };

        let result = handler.execute(&lua, &event);
        assert!(result.is_ok());

        let modified = result.unwrap();
        assert!(modified.is_some());
    }

    #[test]
    fn test_execute_hook_returns_nil() {
        let source = r#"
            function test_handler(ctx, event)
                -- Do nothing, return nil for pass-through
                return nil
            end
        "#;

        let handler = create_test_hook(source);
        let lua = Lua::new();

        let event = SessionEvent::ToolCalled {
            name: "test".to_string(),
            args: serde_json::json!({}),
        };

        let result = handler.execute(&lua, &event);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_execute_json_roundtrip() {
        let source = r#"
            function test_handler(ctx, event)
                event.extra = "added"
                return event
            end
        "#;

        let handler = create_test_hook(source);
        let lua = Lua::new();

        let ctx = serde_json::json!({"hook_name": "test"});
        let event = serde_json::json!({"type": "ToolCalled", "name": "search"});

        let result = handler.execute_json(&lua, ctx, event);
        assert!(result.is_ok());

        let json = result.unwrap();
        assert_eq!(json["extra"], "added");
        assert_eq!(json["name"], "search");
    }

    #[test]
    fn test_hook_result_constructors() {
        let event = SessionEvent::ToolCalled {
            name: "test".to_string(),
            args: serde_json::json!({}),
        };

        let ok_result = HookResult::ok("my_hook", event.clone());
        assert!(ok_result.success);
        assert!(ok_result.event.is_some());
        assert!(ok_result.error.is_none());

        let pass_result = HookResult::pass_through("my_hook");
        assert!(pass_result.success);
        assert!(pass_result.event.is_none());
        assert!(pass_result.error.is_none());

        let err_result = HookResult::err("my_hook", "something went wrong");
        assert!(!err_result.success);
        assert!(err_result.event.is_none());
        assert!(err_result.error.is_some());
    }

    #[test]
    fn test_json_to_lua_roundtrip() {
        let lua = Lua::new();
        let original = serde_json::json!({
            "string": "hello",
            "number": 42,
            "float": 3.14,
            "bool": true,
            "null": null,
            "array": [1, 2, 3],
            "nested": {"key": "value"}
        });

        let lua_val = json_to_lua(&lua, original.clone()).unwrap();
        let back = lua_to_json(lua_val).unwrap();

        assert_eq!(original["string"], back["string"]);
        assert_eq!(original["number"], back["number"]);
        assert_eq!(original["bool"], back["bool"]);
        assert_eq!(original["array"], back["array"]);
        assert_eq!(original["nested"], back["nested"]);
    }
}
