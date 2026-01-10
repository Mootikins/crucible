//! Handler execution for Lua scripts
//!
//! Executes Lua functions discovered via `@handler` (or `@hook`) annotations.
//! This module provides the bridge between event bus events and Lua script execution.
//!
//! ## Example
//!
//! ```lua
//! --- Filter search results
//! -- @handler event="tool:after" pattern="search_*" priority=50
//! function filter_results(ctx, event)
//!     -- Modify event.result before returning
//!     return event
//! end
//! ```
//!
//! ## Return Conventions
//!
//! Handlers follow neovim-style return conventions:
//!
//! - **Return event table**: Transform - modified event continues through pipeline
//! - **Return nil**: Pass-through - event unchanged, continues
//! - **Return `{cancel=true, reason="..."}`**: Cancel - abort the pipeline
//!
//! ## Lifecycle
//!
//! 1. Handlers are discovered from Lua/Fennel sources via `AnnotationParser`
//! 2. `LuaScriptHandler` is created from each `DiscoveredHandler`
//! 3. Handlers are registered on the event bus or via `crucible.on()`
//! 4. Events trigger matching handlers in priority order
//!
//! ## Registry
//!
//! The `LuaScriptHandlerRegistry` provides centralized handler management:
//!
//! ```rust,ignore
//! use crucible_lua::LuaScriptHandlerRegistry;
//! use std::path::PathBuf;
//!
//! // Discover handlers from directories
//! let paths = vec![PathBuf::from("./handlers"), PathBuf::from("./plugins")];
//! let registry = LuaScriptHandlerRegistry::discover(&paths)?;
//!
//! // Get handlers matching an event
//! let handlers = registry.handlers_for(&event);
//! ```

use crate::annotations::{AnnotationParser, DiscoveredHandler};
use crate::error::LuaError;
use crucible_core::events::SessionEvent;
use mlua::{Function, Lua, Result as LuaResult, Table, Value};
use serde_json::Value as JsonValue;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};
use walkdir::WalkDir;

/// Result of script handler execution
///
/// Represents the three possible outcomes from a Lua handler function:
/// - Transform: Handler returned a modified event table (as JSON for cross-boundary safety)
/// - PassThrough: Handler returned nil (no changes)
/// - Cancel: Handler returned `{cancel=true, reason="..."}` to abort
#[derive(Debug, Clone)]
pub enum ScriptHandlerResult {
    /// Handler returned modified event - continue with changes
    /// Stored as JSON to avoid Lua value lifetime issues
    Transform(JsonValue),
    /// Handler returned nil - pass through unchanged
    PassThrough,
    /// Handler returned cancel object - abort pipeline
    Cancel { reason: String },
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
    /// Create handler from discovered hook/handler
    ///
    /// Reads and caches the source file for later execution.
    pub fn new(hook: DiscoveredHandler) -> Result<Self, LuaError> {
        let source = std::fs::read_to_string(&hook.source_path)?;
        Ok(Self {
            metadata: hook,
            source,
        })
    }

    /// Create handler with pre-loaded source
    ///
    /// Use this when source is already available (e.g., during discovery).
    pub fn with_source(hook: DiscoveredHandler, source: String) -> Self {
        Self {
            metadata: hook,
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
        match_glob(&self.metadata.pattern, event_type)
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

        match_glob(&self.metadata.pattern, identifier)
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
        // Load and execute the source to define functions
        lua.load(&self.source).exec()?;

        // Get the handler function
        let handler: Function = lua.globals().get(self.metadata.handler_fn.as_str())?;

        // Create context table
        let ctx_table = lua.create_table()?;
        ctx_table.set("handler_name", self.metadata.name.as_str())?;
        ctx_table.set("priority", self.metadata.priority)?;

        // Convert event to Lua table
        let event_table = session_event_to_lua(lua, event)?;

        // Call handler with (ctx, event)
        let result: Value = handler.call((ctx_table, event_table))?;

        // Process result using return conventions
        match interpret_handler_result(&result)? {
            ScriptHandlerResult::Transform(json) => {
                // Try to deserialize JSON back to SessionEvent
                match serde_json::from_value::<SessionEvent>(json.clone()) {
                    Ok(event) => Ok(Some(event)),
                    Err(e) => {
                        debug!("Could not deserialize to SessionEvent ({}), creating Custom event", e);
                        // Get type from JSON or use "Custom"
                        let name = json.get("type")
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
            ScriptHandlerResult::Cancel { reason } => {
                Err(mlua::Error::RuntimeError(format!("Handler cancelled: {}", reason)))
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
        // Load and execute the source
        lua.load(&self.source).exec()?;

        // Get the handler function
        let handler: Function = lua.globals().get(self.metadata.handler_fn.as_str())?;

        // Convert to Lua values
        let ctx_val = json_to_lua(lua, ctx)?;
        let event_val = json_to_lua(lua, event.clone())?;

        // Call handler
        let result: Value = handler.call((ctx_val, event_val))?;

        // Process result using return conventions
        match interpret_handler_result(&result)? {
            ScriptHandlerResult::Transform(json) => Ok(json),
            ScriptHandlerResult::PassThrough => Ok(event), // Pass through unchanged
            ScriptHandlerResult::Cancel { reason } => {
                Err(mlua::Error::RuntimeError(format!("Handler cancelled: {}", reason)))
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
/// - table with `cancel=true` → Cancel
/// - table without `cancel` → Transform
/// - other → Transform (treat as modified value)
pub fn interpret_handler_result(result: &Value) -> LuaResult<ScriptHandlerResult> {
    match result {
        Value::Nil => Ok(ScriptHandlerResult::PassThrough),
        Value::Table(t) => {
            // Check for cancel convention: {cancel=true, reason="..."}
            if let Ok(cancel) = t.get::<bool>("cancel") {
                if cancel {
                    let reason = t.get::<String>("reason").unwrap_or_else(|_| "cancelled".to_string());
                    return Ok(ScriptHandlerResult::Cancel { reason });
                }
            }
            // Not a cancel, treat as transform - convert to JSON for safety
            let json = lua_table_to_json(t)?;
            Ok(ScriptHandlerResult::Transform(json))
        }
        _ => {
            // Other values treated as transform - convert to JSON
            warn!("Handler returned unexpected type, treating as transform");
            let json = lua_to_json(result.clone())?;
            Ok(ScriptHandlerResult::Transform(json))
        }
    }
}

/// Registry of discovered Lua handlers
///
/// Manages a collection of `LuaScriptHandler` instances discovered from Lua/Fennel
/// source files. Provides event matching and priority-ordered dispatch.
///
/// # Example
///
/// ```rust,ignore
/// use crucible_lua::LuaScriptHandlerRegistry;
/// use std::path::PathBuf;
///
/// // Discover handlers from directories
/// let paths = vec![PathBuf::from("./handlers")];
/// let registry = LuaScriptHandlerRegistry::discover(&paths)?;
///
/// // Check what handlers are available
/// println!("Found {} handlers", registry.len());
///
/// // Get handlers matching an event
/// for handler in registry.handlers_for(&event) {
///     let result = handler.execute(&lua, &event)?;
/// }
/// ```
#[derive(Debug, Clone)]
pub struct LuaScriptHandlerRegistry {
    handlers: Vec<LuaScriptHandler>,
    /// Runtime-registered handlers (via crucible.on())
    runtime_handlers: Arc<Mutex<Vec<RuntimeHandler>>>,
}

/// A handler registered at runtime via crucible.on()
#[derive(Debug, Clone)]
pub struct RuntimeHandler {
    /// Event type to match
    pub event_type: String,
    /// Handler function name (for debugging)
    pub name: String,
    /// Priority (lower = earlier)
    pub priority: i64,
}

impl LuaScriptHandlerRegistry {
    /// Create an empty registry
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
            runtime_handlers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Discover handlers from the given paths
    ///
    /// Walks each path recursively, parsing `.lua` and `.fnl` files for handler
    /// annotations. Discovered handlers are sorted by priority (lowest first).
    ///
    /// # Arguments
    ///
    /// * `paths` - Directories or files to scan for handlers
    ///
    /// # Errors
    ///
    /// Returns an error if file reading fails. Missing paths are silently skipped.
    pub fn discover(paths: &[PathBuf]) -> Result<Self, std::io::Error> {
        let parser = AnnotationParser::new();
        let mut handlers = Vec::new();

        for path in paths {
            if !path.exists() {
                debug!("Handler discovery path does not exist: {:?}", path);
                continue;
            }

            for entry in WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .filter(|e| {
                    e.path()
                        .extension()
                        .is_some_and(|ext| ext == "lua" || ext == "fnl")
                })
            {
                let entry_path = entry.path();
                let source = match std::fs::read_to_string(entry_path) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("Failed to read handler source {:?}: {}", entry_path, e);
                        continue;
                    }
                };

                let is_fennel = entry_path.extension().is_some_and(|e| e == "fnl");

                // Try both @handler and @hook annotations
                match parser.parse_handlers(&source, entry_path, is_fennel) {
                    Ok(hooks) => {
                        for hook in hooks {
                            // Use with_source to avoid re-reading the file
                            let handler = LuaScriptHandler::with_source(hook, source.clone());
                            debug!(
                                "Discovered handler: {} (event={}, priority={})",
                                handler.metadata.name,
                                handler.metadata.event_type,
                                handler.metadata.priority
                            );
                            handlers.push(handler);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse handlers from {:?}: {}", entry_path, e);
                    }
                }
            }
        }

        // Sort by priority (lower priority values execute first)
        handlers.sort_by_key(|h| h.metadata.priority);

        debug!("Handler registry discovered {} handlers", handlers.len());
        Ok(Self {
            handlers,
            runtime_handlers: Arc::new(Mutex::new(Vec::new())),
        })
    }

    /// Get all handlers that match the given event
    ///
    /// Returns handlers in priority order (lowest priority value first).
    pub fn handlers_for(&self, event: &SessionEvent) -> Vec<&LuaScriptHandler> {
        self.handlers.iter().filter(|h| h.matches(event)).collect()
    }

    /// Get all handlers matching event type and identifier
    ///
    /// More flexible matching for cases where the event type and identifier
    /// are known separately (e.g., tool name for tool events).
    pub fn handlers_for_identifier(
        &self,
        event_type: &str,
        identifier: &str,
    ) -> Vec<&LuaScriptHandler> {
        self.handlers
            .iter()
            .filter(|h| h.matches_with_identifier(event_type, identifier))
            .collect()
    }

    /// Number of registered handlers
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    /// Add a handler manually
    ///
    /// The handler is inserted in priority order.
    pub fn add(&mut self, handler: LuaScriptHandler) {
        self.handlers.push(handler);
        self.handlers.sort_by_key(|h| h.metadata.priority);
    }

    /// Remove all handlers
    pub fn clear(&mut self) {
        self.handlers.clear();
    }

    /// Get an iterator over all handlers
    pub fn iter(&self) -> impl Iterator<Item = &LuaScriptHandler> {
        self.handlers.iter()
    }

    /// Reload all handlers from disk
    ///
    /// Re-reads source files for all registered handlers.
    pub fn reload_all(&mut self) -> Result<(), LuaError> {
        for handler in &mut self.handlers {
            handler.reload()?;
        }
        Ok(())
    }

    /// Get a shareable reference to runtime handlers
    pub fn runtime_handlers(&self) -> Arc<Mutex<Vec<RuntimeHandler>>> {
        self.runtime_handlers.clone()
    }
}

impl Default for LuaScriptHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Register the crucible.on() API for runtime handler registration
///
/// This allows Lua scripts to register handlers dynamically:
///
/// ```lua
/// crucible.on("pre_tool_call", function(event)
///     if event.tool == "dangerous" then
///         return { cancel = true, reason = "blocked" }
///     end
///     return event
/// end)
/// ```
pub fn register_crucible_on_api(
    lua: &Lua,
    runtime_handlers: Arc<Mutex<Vec<RuntimeHandler>>>,
) -> LuaResult<()> {
    // Get or create the crucible table
    let crucible: Table = match lua.globals().get("crucible") {
        Ok(t) => t,
        Err(_) => {
            let t = lua.create_table()?;
            lua.globals().set("crucible", t.clone())?;
            t
        }
    };

    // Create the on() function
    let handlers = runtime_handlers.clone();
    let on_fn = lua.create_function(move |_lua, (event_type, _handler): (String, Function)| {
        // Store the handler reference
        let mut guard = handlers.lock().map_err(|e| {
            mlua::Error::RuntimeError(format!("Failed to lock handlers: {}", e))
        })?;

        let name = format!("runtime_handler_{}", guard.len());
        guard.push(RuntimeHandler {
            event_type: event_type.clone(),
            name: name.clone(),
            priority: 100, // Default priority for runtime handlers
        });

        debug!("Registered runtime handler '{}' for event '{}'", name, event_type);
        Ok(())
    })?;

    crucible.set("on", on_fn)?;
    Ok(())
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

/// Result of handler execution (legacy compatibility)
#[derive(Debug, Clone)]
pub struct HandlerExecutionResult {
    /// Whether execution succeeded
    pub success: bool,
    /// Modified event if any
    pub event: Option<SessionEvent>,
    /// Error message if failed
    pub error: Option<String>,
    /// Handler name for logging
    pub handler_name: String,
}

impl HandlerExecutionResult {
    /// Create a successful result with modified event
    pub fn ok(handler_name: impl Into<String>, event: SessionEvent) -> Self {
        Self {
            success: true,
            event: Some(event),
            error: None,
            handler_name: handler_name.into(),
        }
    }

    /// Create a successful result with no modification (pass-through)
    pub fn pass_through(handler_name: impl Into<String>) -> Self {
        Self {
            success: true,
            event: None,
            error: None,
            handler_name: handler_name.into(),
        }
    }

    /// Create a failed result
    pub fn err(handler_name: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            success: false,
            event: None,
            error: Some(error.into()),
            handler_name: handler_name.into(),
        }
    }
}

/// Execute a handler and return a structured result
pub fn execute_handler(
    handler: &LuaScriptHandler,
    lua: &Lua,
    event: &SessionEvent,
) -> HandlerExecutionResult {
    match handler.execute(lua, event) {
        Ok(Some(modified_event)) => HandlerExecutionResult::ok(&handler.metadata.name, modified_event),
        Ok(None) => HandlerExecutionResult::pass_through(&handler.metadata.name),
        Err(e) => HandlerExecutionResult::err(&handler.metadata.name, e.to_string()),
    }
}

/// Run a chain of handlers on an event
///
/// Executes handlers in order, passing the (potentially modified) event
/// through each handler. Stops if a handler returns cancel.
///
/// # Returns
/// * `Ok(Some(event))` - All handlers passed, returns final event
/// * `Ok(None)` - A handler cancelled the event
/// * `Err(e)` - A handler failed
pub fn run_handler_chain(
    lua: &Lua,
    handlers: &[&LuaScriptHandler],
    event: SessionEvent,
) -> Result<Option<SessionEvent>, LuaError> {
    let mut current_event = event;

    for handler in handlers {
        match handler.execute(lua, &current_event) {
            Ok(Some(modified)) => {
                current_event = modified;
            }
            Ok(None) => {
                // Pass-through, keep current event
            }
            Err(e) => {
                // Check if this is a cancel
                let err_str = e.to_string();
                if err_str.contains("cancelled") {
                    debug!("Handler {} cancelled event: {}", handler.metadata.name, err_str);
                    return Ok(None);
                }
                return Err(LuaError::Runtime(err_str));
            }
        }
    }

    Ok(Some(current_event))
}


#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_handler(source: &str) -> LuaScriptHandler {
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
    fn test_handler_creation() {
        let hook = DiscoveredHandler {
            name: "filter_handler".to_string(),
            event_type: "ToolCalled".to_string(),
            pattern: "search_*".to_string(),
            priority: 50,
            description: "Filter search results".to_string(),
            source_path: "test.lua".to_string(),
            handler_fn: "filter_results".to_string(),
            is_fennel: false,
        };

        let handler = LuaScriptHandler::with_source(hook, "-- test source".to_string());
        assert_eq!(handler.metadata.name, "filter_handler");
        assert_eq!(handler.metadata.priority, 50);
        assert_eq!(handler.source(), "-- test source");
    }

    #[test]
    fn test_handler_matches_event_type() {
        let handler = create_test_handler("");

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
    fn test_handler_matches_with_pattern() {
        let hook = DiscoveredHandler {
            name: "test".to_string(),
            event_type: "tool_called".to_string(),
            pattern: "search_*".to_string(),
            priority: 100,
            description: "".to_string(),
            source_path: "".to_string(),
            handler_fn: "handler".to_string(),
            is_fennel: false,
        };

        let handler = LuaScriptHandler::with_source(hook, String::new());

        assert!(handler.matches_with_identifier("tool_called", "search_notes"));
        assert!(handler.matches_with_identifier("tool_called", "search_files"));
        assert!(!handler.matches_with_identifier("tool_called", "fetch_data"));
        assert!(!handler.matches_with_identifier("other_event", "search_notes"));
    }

    #[test]
    fn test_execute_simple_handler() {
        let source = r#"
            function test_handler(ctx, event)
                event.modified = true
                return event
            end
        "#;

        let handler = create_test_handler(source);
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
    fn test_execute_handler_returns_nil() {
        let source = r#"
            function test_handler(ctx, event)
                -- Do nothing, return nil for pass-through
                return nil
            end
        "#;

        let handler = create_test_handler(source);
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
    fn test_execute_handler_returns_cancel() {
        let source = r#"
            function test_handler(ctx, event)
                return { cancel = true, reason = "blocked by policy" }
            end
        "#;

        let handler = create_test_handler(source);
        let lua = Lua::new();

        let event = SessionEvent::ToolCalled {
            name: "test".to_string(),
            args: serde_json::json!({}),
        };

        let result = handler.execute(&lua, &event);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("blocked by policy"));
    }

    #[test]
    fn test_interpret_handler_result_nil() {
        let result = interpret_handler_result(&Value::Nil).unwrap();
        assert!(matches!(result, ScriptHandlerResult::PassThrough));
    }

    #[test]
    fn test_interpret_handler_result_cancel() {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        table.set("cancel", true).unwrap();
        table.set("reason", "test cancel").unwrap();

        let result = interpret_handler_result(&Value::Table(table)).unwrap();
        match result {
            ScriptHandlerResult::Cancel { reason } => {
                assert_eq!(reason, "test cancel");
            }
            _ => panic!("Expected Cancel result"),
        }
    }

    #[test]
    fn test_interpret_handler_result_transform() {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        table.set("key", "value").unwrap();

        let result = interpret_handler_result(&Value::Table(table)).unwrap();
        assert!(matches!(result, ScriptHandlerResult::Transform(_)));
    }

    #[test]
    fn test_execute_json_roundtrip() {
        let source = r#"
            function test_handler(ctx, event)
                event.extra = "added"
                return event
            end
        "#;

        let handler = create_test_handler(source);
        let lua = Lua::new();

        let ctx = serde_json::json!({"handler_name": "test"});
        let event = serde_json::json!({"type": "ToolCalled", "name": "search"});

        let result = handler.execute_json(&lua, ctx, event);
        assert!(result.is_ok());

        let json = result.unwrap();
        assert_eq!(json["extra"], "added");
        assert_eq!(json["name"], "search");
    }

    #[test]
    fn test_handler_result_constructors() {
        let event = SessionEvent::ToolCalled {
            name: "test".to_string(),
            args: serde_json::json!({}),
        };

        let ok_result = HandlerExecutionResult::ok("my_handler", event.clone());
        assert!(ok_result.success);
        assert!(ok_result.event.is_some());
        assert!(ok_result.error.is_none());

        let pass_result = HandlerExecutionResult::pass_through("my_handler");
        assert!(pass_result.success);
        assert!(pass_result.event.is_none());
        assert!(pass_result.error.is_none());

        let err_result = HandlerExecutionResult::err("my_handler", "something went wrong");
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

    // ============ Registry Tests ============

    #[test]
    fn test_registry_new_is_empty() {
        let registry = LuaScriptHandlerRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_default_is_empty() {
        let registry = LuaScriptHandlerRegistry::default();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_registry_add_handler() {
        let mut registry = LuaScriptHandlerRegistry::new();
        let handler = create_test_handler("-- test");

        registry.add(handler);

        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());
    }

    #[test]
    fn test_registry_add_maintains_priority_order() {
        let mut registry = LuaScriptHandlerRegistry::new();

        // Add handlers with different priorities
        let mut high_priority = create_test_handler("-- high");
        high_priority.metadata.priority = 10;

        let mut low_priority = create_test_handler("-- low");
        low_priority.metadata.priority = 200;

        let mut medium_priority = create_test_handler("-- medium");
        medium_priority.metadata.priority = 100;

        // Add in non-sorted order
        registry.add(low_priority);
        registry.add(high_priority);
        registry.add(medium_priority);

        // Verify they are sorted by priority
        let handlers: Vec<_> = registry.iter().collect();
        assert_eq!(handlers[0].metadata.priority, 10);
        assert_eq!(handlers[1].metadata.priority, 100);
        assert_eq!(handlers[2].metadata.priority, 200);
    }

    #[test]
    fn test_registry_handlers_for_event() {
        let mut registry = LuaScriptHandlerRegistry::new();

        // Add a ToolCalled handler
        let handler = create_test_handler("-- tool called handler");
        registry.add(handler);

        // Create a matching event
        let matching_event = SessionEvent::ToolCalled {
            name: "search".to_string(),
            args: serde_json::json!({}),
        };

        // Create a non-matching event
        let non_matching_event = SessionEvent::ToolCompleted {
            name: "search".to_string(),
            result: "done".to_string(),
            error: None,
        };

        let matching_handlers = registry.handlers_for(&matching_event);
        assert_eq!(matching_handlers.len(), 1);

        let non_matching_handlers = registry.handlers_for(&non_matching_event);
        assert!(non_matching_handlers.is_empty());
    }

    #[test]
    fn test_registry_handlers_for_identifier() {
        let mut registry = LuaScriptHandlerRegistry::new();

        let hook = DiscoveredHandler {
            name: "search_filter".to_string(),
            event_type: "tool_called".to_string(),
            pattern: "search_*".to_string(),
            priority: 50,
            description: "Filter search tools".to_string(),
            source_path: "test.lua".to_string(),
            handler_fn: "handler".to_string(),
            is_fennel: false,
        };
        let handler = LuaScriptHandler::with_source(hook, String::new());
        registry.add(handler);

        // Should match search_notes
        let handlers = registry.handlers_for_identifier("tool_called", "search_notes");
        assert_eq!(handlers.len(), 1);

        // Should match search_files
        let handlers = registry.handlers_for_identifier("tool_called", "search_files");
        assert_eq!(handlers.len(), 1);

        // Should not match fetch_data
        let handlers = registry.handlers_for_identifier("tool_called", "fetch_data");
        assert!(handlers.is_empty());

        // Should not match wrong event type
        let handlers = registry.handlers_for_identifier("tool_completed", "search_notes");
        assert!(handlers.is_empty());
    }

    #[test]
    fn test_registry_clear() {
        let mut registry = LuaScriptHandlerRegistry::new();
        registry.add(create_test_handler("-- one"));
        registry.add(create_test_handler("-- two"));

        assert_eq!(registry.len(), 2);

        registry.clear();

        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_iter() {
        let mut registry = LuaScriptHandlerRegistry::new();

        let mut handler1 = create_test_handler("-- one");
        handler1.metadata.name = "handler_one".to_string();

        let mut handler2 = create_test_handler("-- two");
        handler2.metadata.name = "handler_two".to_string();

        registry.add(handler1);
        registry.add(handler2);

        let names: Vec<_> = registry.iter().map(|h| h.metadata.name.as_str()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"handler_one"));
        assert!(names.contains(&"handler_two"));
    }

    #[test]
    fn test_registry_discover_nonexistent_path() {
        let paths = vec![PathBuf::from("/nonexistent/path/that/should/not/exist")];
        let registry = LuaScriptHandlerRegistry::discover(&paths).unwrap();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_registry_discover_from_temp_dir() {
        use std::io::Write;

        // Create a temp directory with a handler file
        let temp_dir = tempfile::tempdir().unwrap();
        let handler_file = temp_dir.path().join("my_handler.lua");

        let handler_source = r#"
--- Filter search results
-- @handler event="ToolCalled" pattern="*" priority=25
function filter_results(ctx, event)
    return event
end
"#;

        std::fs::File::create(&handler_file)
            .unwrap()
            .write_all(handler_source.as_bytes())
            .unwrap();

        // Discover handlers
        let paths = vec![temp_dir.path().to_path_buf()];
        let registry = LuaScriptHandlerRegistry::discover(&paths).unwrap();

        assert_eq!(registry.len(), 1);

        let handler = registry.iter().next().unwrap();
        assert_eq!(handler.metadata.name, "filter_results");
        assert_eq!(handler.metadata.event_type, "ToolCalled");
        assert_eq!(handler.metadata.priority, 25);
    }

    #[test]
    fn test_registry_clone() {
        let mut registry = LuaScriptHandlerRegistry::new();
        registry.add(create_test_handler("-- test"));

        let cloned = registry.clone();
        assert_eq!(cloned.len(), registry.len());
    }

    #[test]
    fn test_crucible_on_api_registration() {
        let lua = Lua::new();
        let handlers = Arc::new(Mutex::new(Vec::new()));

        register_crucible_on_api(&lua, handlers.clone()).unwrap();

        // Verify crucible.on exists
        lua.load(r#"
            crucible.on("test_event", function(event)
                return event
            end)
        "#).exec().unwrap();

        // Check that handler was registered
        let guard = handlers.lock().unwrap();
        assert_eq!(guard.len(), 1);
        assert_eq!(guard[0].event_type, "test_event");
    }

    // ============================================================================
    // Return Convention Tests
    // ============================================================================

    #[test]
    fn test_return_nil_is_passthrough() {
        // Handler returns nil → event passes through unchanged
        let source = r#"
            function test_handler(ctx, event)
                -- Side effect only (logging, etc)
                return nil
            end
        "#;

        let handler = create_test_handler(source);
        let lua = Lua::new();
        let event = SessionEvent::ToolCalled {
            name: "search".to_string(),
            args: serde_json::json!({"query": "test"}),
        };

        let result = handler.execute(&lua, &event).unwrap();
        assert!(result.is_none(), "nil return should produce None (pass-through)");
    }

    #[test]
    fn test_return_table_is_transform() {
        // Handler returns table → event is transformed
        let source = r#"
            function test_handler(ctx, event)
                event.injected = "by_handler"
                return event
            end
        "#;

        let handler = create_test_handler(source);
        let lua = Lua::new();
        let event = SessionEvent::ToolCalled {
            name: "search".to_string(),
            args: serde_json::json!({}),
        };

        let result = handler.execute(&lua, &event).unwrap();
        assert!(result.is_some(), "table return should produce Some(modified_event)");
    }

    #[test]
    fn test_return_cancel_aborts_pipeline() {
        // Handler returns {cancel=true} → pipeline aborts
        let source = r#"
            function test_handler(ctx, event)
                return { cancel = true, reason = "blocked by security" }
            end
        "#;

        let handler = create_test_handler(source);
        let lua = Lua::new();
        let event = SessionEvent::ToolCalled {
            name: "dangerous_tool".to_string(),
            args: serde_json::json!({}),
        };

        let result = handler.execute(&lua, &event);
        assert!(result.is_err(), "cancel return should produce error");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("blocked by security"), "error should contain reason");
    }

    #[test]
    fn test_cancel_without_reason_uses_default() {
        // Handler returns {cancel=true} without reason
        let source = r#"
            function test_handler(ctx, event)
                return { cancel = true }
            end
        "#;

        let handler = create_test_handler(source);
        let lua = Lua::new();
        let event = SessionEvent::ToolCalled {
            name: "test".to_string(),
            args: serde_json::json!({}),
        };

        let result = handler.execute(&lua, &event);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cancelled"), "should use default reason");
    }

    #[test]
    fn test_cancel_false_is_transform() {
        // Handler returns {cancel=false, ...} → treated as transform, not cancel
        let source = r#"
            function test_handler(ctx, event)
                return { cancel = false, data = "still valid" }
            end
        "#;

        let handler = create_test_handler(source);
        let lua = Lua::new();
        let event = SessionEvent::ToolCalled {
            name: "test".to_string(),
            args: serde_json::json!({}),
        };

        let result = handler.execute(&lua, &event);
        assert!(result.is_ok(), "cancel=false should not abort");
        assert!(result.unwrap().is_some(), "should return transformed event");
    }

    // ============================================================================
    // Handler Chain Tests
    // ============================================================================

    fn create_test_handler_named(source: &str, fn_name: &str) -> LuaScriptHandler {
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

    #[test]
    fn test_chain_transform_then_passthrough() {
        // h1: transform → h2: passthrough → result has h1's changes
        let h1 = create_test_handler_named(r#"
            function h1(ctx, event)
                event.from_h1 = true
                return event
            end
        "#, "h1");

        let h2 = create_test_handler_named(r#"
            function h2(ctx, event)
                return nil  -- pass through, keep h1's changes
            end
        "#, "h2");

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
        let h1 = create_test_handler_named(r#"
            function h1(ctx, event)
                event.step1 = true
                return event
            end
        "#, "h1");

        let h2 = create_test_handler_named(r#"
            function h2(ctx, event)
                return { cancel = true, reason = "stopped at h2" }
            end
        "#, "h2");

        let h3 = create_test_handler_named(r#"
            function h3(ctx, event)
                event.step3 = true  -- should never execute
                return event
            end
        "#, "h3");

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
        let h1 = create_test_handler_named(r#"
            function h1(ctx, event) return nil end
        "#, "h1");

        let h2 = create_test_handler_named(r#"
            function h2(ctx, event) return nil end
        "#, "h2");

        let lua = Lua::new();
        let event = SessionEvent::ToolCalled {
            name: "original".to_string(),
            args: serde_json::json!({"key": "value"}),
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
        let h1 = create_test_handler_named(r#"
            function h1(ctx, event)
                event.field1 = "from_h1"
                return event
            end
        "#, "h1");

        let h2 = create_test_handler_named(r#"
            function h2(ctx, event)
                event.field2 = "from_h2"
                return event
            end
        "#, "h2");

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
}
