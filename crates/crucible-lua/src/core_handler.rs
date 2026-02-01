//! Lua Handler implementing core's unified Handler trait
//!
//! This module bridges Lua scripts to the core event system, allowing Lua
//! handlers to interleave with Rust and Rune handlers in the unified Reactor.
//!
//! ## Design
//!
//! Each `LuaHandler` represents a single handler function in a Lua script:
//!
//! ```lua
//! --- Check permissions before tool execution
//! -- @handler event="tool:before" priority=10
//! function check_permissions(ctx, event)
//!     if event.tool_name == "dangerous" then
//!         return { cancel = true }
//!     end
//!     return event
//! end
//!
//! --- Log tool results
//! -- @handler event="tool:after" depends="persist"
//! function log_result(ctx, event)
//!     crucible.log("info", "Tool completed: " .. event.tool_name)
//!     return event
//! end
//! ```
//!
//! A single script file can define multiple handlers, each becoming a separate
//! `LuaHandler` that registers with the core Reactor.
//!
//! ## Async Execution
//!
//! Lua execution is synchronous. The `handle()` method uses `spawn_blocking`
//! to run Lua on a thread pool, preventing reactor blocking.

use crate::error::LuaError;
use async_trait::async_trait;
use crucible_core::events::{Handler, HandlerContext, HandlerResult, SessionEvent};
use mlua::{Function, Lua, LuaSerdeExt, Value};
use serde_json::Value as JsonValue;
use std::path::PathBuf;
use tokio::task::spawn_blocking;

/// Metadata for a discovered Lua handler function.
///
/// Extracted from `-- @handler(...)` annotations during script discovery.
#[derive(Debug, Clone)]
pub struct LuaHandlerMeta {
    /// Path to the script file
    pub script_path: PathBuf,

    /// Name of the handler function in the script
    pub function_name: String,

    /// Event pattern to match (e.g., "tool:*", "note:modified")
    pub event_pattern: String,

    /// Execution priority (lower = earlier, default 50)
    pub priority: i32,

    /// Handler dependencies (names of handlers that must complete first)
    pub dependencies: Vec<String>,

    /// Whether this handler is enabled
    pub enabled: bool,
}

impl LuaHandlerMeta {
    /// Create new handler metadata.
    pub fn new(script_path: impl Into<PathBuf>, function_name: impl Into<String>) -> Self {
        Self {
            script_path: script_path.into(),
            function_name: function_name.into(),
            event_pattern: "*".into(),
            priority: 50,
            dependencies: vec![],
            enabled: true,
        }
    }

    /// Set the event pattern.
    pub fn with_event_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.event_pattern = pattern.into();
        self
    }

    /// Set the priority.
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Set dependencies.
    pub fn with_dependencies(mut self, deps: Vec<String>) -> Self {
        self.dependencies = deps;
        self
    }

    /// Set enabled state.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Generate the unique handler name.
    ///
    /// Format: `lua:<script_path>:<function_name>`
    pub fn handler_name(&self) -> String {
        format!("lua:{}:{}", self.script_path.display(), self.function_name)
    }
}

/// A Lua script handler implementing core's `Handler` trait.
///
/// Each instance represents a single handler function within a Lua script.
/// Since mlua's `Lua` state is not `Send + Sync` by default, this handler
/// creates a new Lua state for each invocation inside `spawn_blocking`.
///
/// The script source is cached to avoid repeated file reads.
pub struct LuaHandler {
    /// Handler metadata (name, event pattern, priority, deps)
    meta: LuaHandlerMeta,

    /// Cached script source (to avoid repeated file reads)
    script_source: String,

    /// Cached handler name
    name: String,

    /// Cached dependencies as static strings (for trait method return)
    deps_static: Vec<&'static str>,
}

// Safety: LuaHandler doesn't contain Lua state directly (it's created per-call),
// so it's safe to share across threads.
unsafe impl Send for LuaHandler {}
unsafe impl Sync for LuaHandler {}

impl LuaHandler {
    /// Create a new Lua handler from metadata.
    ///
    /// Reads and caches the script source.
    pub fn new(meta: LuaHandlerMeta) -> Result<Self, LuaError> {
        // Read and cache the script source
        let script_source = std::fs::read_to_string(&meta.script_path).map_err(|e| {
            LuaError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Failed to read script {:?}: {}", meta.script_path, e),
            ))
        })?;

        Self::with_source(meta, script_source)
    }

    /// Create a handler with a pre-loaded script source.
    ///
    /// Use this when you've already read the script.
    pub fn with_source(meta: LuaHandlerMeta, script_source: String) -> Result<Self, LuaError> {
        let name = meta.handler_name();

        // Convert dependencies to static strings (leaked for trait compatibility)
        // This is acceptable because handlers are long-lived
        let deps_static: Vec<&'static str> = meta
            .dependencies
            .iter()
            .map(|s| -> &'static str { Box::leak(s.clone().into_boxed_str()) })
            .collect();

        Ok(Self {
            meta,
            script_source,
            name,
            deps_static,
        })
    }

    /// Get the handler metadata.
    pub fn metadata(&self) -> &LuaHandlerMeta {
        &self.meta
    }

    /// Check if this handler is enabled.
    pub fn is_enabled(&self) -> bool {
        self.meta.enabled
    }

    /// Get the script path.
    pub fn script_path(&self) -> &PathBuf {
        &self.meta.script_path
    }

    /// Get the function name.
    pub fn function_name(&self) -> &str {
        &self.meta.function_name
    }
}

#[async_trait]
impl Handler for LuaHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn dependencies(&self) -> &[&str] {
        &self.deps_static
    }

    fn priority(&self) -> i32 {
        self.meta.priority
    }

    fn event_pattern(&self) -> &str {
        &self.meta.event_pattern
    }

    async fn handle(
        &self,
        ctx: &mut HandlerContext,
        event: SessionEvent,
    ) -> HandlerResult<SessionEvent> {
        if !self.meta.enabled {
            return HandlerResult::ok(event);
        }

        let script_source = self.script_source.clone();
        let function_name = self.meta.function_name.clone();
        let handler_name = self.name.clone();

        // Serialize context metadata and event to JSON
        let ctx_json =
            serde_json::to_value(ctx.metadata()).unwrap_or(JsonValue::Object(Default::default()));
        let event_json = match serde_json::to_value(&event) {
            Ok(j) => j,
            Err(e) => {
                return HandlerResult::soft_error(
                    event,
                    format!("Failed to serialize event: {}", e),
                );
            }
        };

        // Run Lua execution on blocking thread pool
        // Create Lua state inside spawn_blocking to avoid Send/Sync issues
        let result = spawn_blocking(move || {
            execute_lua_handler_from_source(&script_source, &function_name, ctx_json, event_json)
        })
        .await;

        // Handle spawn result
        let result = match result {
            Ok(Ok(result_json)) => result_json,
            Ok(Err(e)) => {
                return HandlerResult::soft_error(
                    event,
                    format!("Handler {} execution failed: {}", handler_name, e),
                );
            }
            Err(e) => {
                return HandlerResult::soft_error(
                    event,
                    format!("Handler {} spawn failed: {}", handler_name, e),
                );
            }
        };

        // Parse the result
        parse_handler_result(result, event, &handler_name)
    }
}

/// Execute a Lua handler function from source code.
///
/// Creates a new Lua state, loads the script, and calls the handler function.
/// This is called inside `spawn_blocking` to avoid Send/Sync issues.
fn execute_lua_handler_from_source(
    script_source: &str,
    function_name: &str,
    ctx_json: JsonValue,
    event_json: JsonValue,
) -> Result<JsonValue, LuaError> {
    // Create a new Lua state for this call
    let lua = Lua::new();

    // Set up crucible globals
    setup_globals(&lua)?;

    // Load the script
    lua.load(script_source).exec()?;

    // Get the handler function
    let globals = lua.globals();
    let handler: Function = globals.get(function_name).map_err(|_| {
        LuaError::InvalidTool(format!("Handler function '{}' not found", function_name))
    })?;

    // Convert JSON to Lua values
    let ctx_val = lua.to_value(&ctx_json)?;
    let event_val = lua.to_value(&event_json)?;

    // Call the handler
    let result: Value = handler.call((ctx_val, event_val))?;

    // Convert result back to JSON
    Ok(serde_json::to_value(&result)?)
}

/// Set up global functions available to Lua scripts.
fn setup_globals(lua: &Lua) -> Result<(), LuaError> {
    let globals = lua.globals();

    // Create crucible namespace
    let crucible = lua.create_table()?;

    // crucible.log(level, message)
    let log_fn = lua.create_function(|_, (level, msg): (String, String)| {
        match level.as_str() {
            "debug" => tracing::debug!("{}", msg),
            "info" => tracing::info!("{}", msg),
            "warn" => tracing::warn!("{}", msg),
            "error" => tracing::error!("{}", msg),
            _ => tracing::info!("{}", msg),
        }
        Ok(())
    })?;
    crucible.set("log", log_fn)?;

    // crucible.json_encode(value) -> string
    let json_encode = lua.create_function(|_lua, value: Value| {
        serde_json::to_string(&value).map_err(mlua::Error::external)
    })?;
    crucible.set("json_encode", json_encode)?;

    // crucible.json_decode(string) -> value
    let json_decode = lua.create_function(|lua, s: String| {
        let json: JsonValue = serde_json::from_str(&s).map_err(mlua::Error::external)?;
        lua.to_value(&json)
    })?;
    crucible.set("json_decode", json_decode)?;

    globals.set("crucible", crucible)?;

    Ok(())
}

/// Parse the JSON result from a Lua handler function.
///
/// Lua handlers can return:
/// - `nil` - Pass through unchanged
/// - `{ cancel = true }` - Cancel the event
/// - `{ emit = {...} }` - Emit additional events (then continue)
/// - Modified event table - Continue with modified event
fn parse_handler_result(
    result: JsonValue,
    original_event: SessionEvent,
    handler_name: &str,
) -> HandlerResult<SessionEvent> {
    if result.is_null() {
        // Handler returned nil - pass through unchanged
        return HandlerResult::ok(original_event);
    }

    // Check for cancel directive
    if let Some(obj) = result.as_object() {
        if obj.get("cancel") == Some(&JsonValue::Bool(true)) {
            return HandlerResult::cancel();
        }

        // Check for emit directive
        if let Some(events) = obj.get("emit") {
            if events.is_array() {
                tracing::debug!(
                    "Handler {} wants to emit {} events (not yet implemented in unified handler)",
                    handler_name,
                    events.as_array().map(|a| a.len()).unwrap_or(0)
                );
            }
        }
    }

    // Try to deserialize back to SessionEvent
    match serde_json::from_value::<SessionEvent>(result.clone()) {
        Ok(modified_event) => HandlerResult::ok(modified_event),
        Err(e) => {
            tracing::warn!(
                "Handler {} returned invalid event structure: {}",
                handler_name,
                e
            );
            // Return original event on parse error (fail-open)
            HandlerResult::ok(original_event)
        }
    }
}

impl std::fmt::Debug for LuaHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LuaHandler")
            .field("name", &self.name)
            .field("function", &self.meta.function_name)
            .field("script", &self.meta.script_path)
            .field("event_pattern", &self.meta.event_pattern)
            .field("priority", &self.meta.priority)
            .field("dependencies", &self.meta.dependencies)
            .field("enabled", &self.meta.enabled)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_lua_handler_meta_default() {
        let meta = LuaHandlerMeta::new("/path/to/script.lua", "my_handler");

        assert_eq!(meta.script_path, PathBuf::from("/path/to/script.lua"));
        assert_eq!(meta.function_name, "my_handler");
        assert_eq!(meta.event_pattern, "*");
        assert_eq!(meta.priority, 50);
        assert!(meta.dependencies.is_empty());
        assert!(meta.enabled);
    }

    #[test]
    fn test_lua_handler_meta_builder() {
        let meta = LuaHandlerMeta::new("/path/to/script.lua", "my_handler")
            .with_event_pattern("tool:*")
            .with_priority(10)
            .with_dependencies(vec!["persist".into(), "validate".into()])
            .with_enabled(false);

        assert_eq!(meta.event_pattern, "tool:*");
        assert_eq!(meta.priority, 10);
        assert_eq!(meta.dependencies, vec!["persist", "validate"]);
        assert!(!meta.enabled);
    }

    #[test]
    fn test_lua_handler_meta_name() {
        let meta = LuaHandlerMeta::new("plugins/auth.lua", "check_permissions");
        assert_eq!(
            meta.handler_name(),
            "lua:plugins/auth.lua:check_permissions"
        );
    }

    #[tokio::test]
    async fn test_lua_handler_creation() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("test_handler.lua");

        fs::write(
            &script_path,
            r#"
function test_handler(ctx, event)
    return event
end
"#,
        )
        .unwrap();

        let meta = LuaHandlerMeta::new(&script_path, "test_handler");
        let handler = LuaHandler::new(meta).expect("Failed to create handler");

        assert!(handler.name().contains("test_handler"));
        assert_eq!(handler.priority(), 50);
        assert_eq!(handler.event_pattern(), "*");
        assert!(handler.dependencies().is_empty());
    }

    #[tokio::test]
    async fn test_lua_handler_passthrough() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("passthrough.lua");

        // Handler that just returns the event unchanged
        fs::write(
            &script_path,
            r#"
function passthrough(ctx, event)
    return event
end
"#,
        )
        .unwrap();

        let meta = LuaHandlerMeta::new(&script_path, "passthrough");
        let handler = LuaHandler::new(meta).unwrap();

        let mut ctx = HandlerContext::new();
        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({"value": 42}),
        };

        let result = handler.handle(&mut ctx, event.clone()).await;

        assert!(result.is_continue());
        let result_event = result.event().unwrap();
        if let SessionEvent::Custom { name, payload } = result_event {
            assert_eq!(name, "test");
            assert_eq!(payload["value"], 42);
        } else {
            panic!("Expected Custom event");
        }
    }

    #[tokio::test]
    async fn test_lua_handler_cancel() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("canceller.lua");

        // Handler that cancels events
        fs::write(
            &script_path,
            r#"
function canceller(ctx, event)
    return { cancel = true }
end
"#,
        )
        .unwrap();

        let meta = LuaHandlerMeta::new(&script_path, "canceller");
        let handler = LuaHandler::new(meta).unwrap();

        let mut ctx = HandlerContext::new();
        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({}),
        };

        let result = handler.handle(&mut ctx, event).await;

        assert!(result.is_cancel());
    }

    #[tokio::test]
    async fn test_lua_handler_disabled() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("disabled.lua");

        fs::write(
            &script_path,
            r#"
function disabled_handler(ctx, event)
    return { cancel = true }
end
"#,
        )
        .unwrap();

        let meta = LuaHandlerMeta::new(&script_path, "disabled_handler").with_enabled(false);
        let handler = LuaHandler::new(meta).unwrap();

        let mut ctx = HandlerContext::new();
        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({}),
        };

        // Disabled handler should pass through
        let result = handler.handle(&mut ctx, event).await;
        assert!(result.is_continue());
    }

    #[tokio::test]
    async fn test_lua_handler_with_priority() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("priority.lua");

        fs::write(
            &script_path,
            r#"
function priority_handler(ctx, event)
    return event
end
"#,
        )
        .unwrap();

        let meta = LuaHandlerMeta::new(&script_path, "priority_handler").with_priority(10);
        let handler = LuaHandler::new(meta).unwrap();

        assert_eq!(handler.priority(), 10);
    }

    #[tokio::test]
    async fn test_lua_handler_with_dependencies() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("deps.lua");

        fs::write(
            &script_path,
            r#"
function dependent_handler(ctx, event)
    return event
end
"#,
        )
        .unwrap();

        let meta = LuaHandlerMeta::new(&script_path, "dependent_handler")
            .with_dependencies(vec!["persist".into(), "validate".into()]);
        let handler = LuaHandler::new(meta).unwrap();

        let deps = handler.dependencies();
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&"persist"));
        assert!(deps.contains(&"validate"));
    }

    #[tokio::test]
    async fn test_lua_handler_with_event_pattern() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("pattern.lua");

        fs::write(
            &script_path,
            r#"
function tool_handler(ctx, event)
    return event
end
"#,
        )
        .unwrap();

        let meta = LuaHandlerMeta::new(&script_path, "tool_handler").with_event_pattern("tool:*");
        let handler = LuaHandler::new(meta).unwrap();

        assert_eq!(handler.event_pattern(), "tool:*");
    }

    #[tokio::test]
    async fn test_lua_handler_modify_event() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("modifier.lua");

        // Handler that modifies the event
        fs::write(
            &script_path,
            r#"
function modifier(ctx, event)
    event.payload.modified = true
    return event
end
"#,
        )
        .unwrap();

        let meta = LuaHandlerMeta::new(&script_path, "modifier");
        let handler = LuaHandler::new(meta).unwrap();

        let mut ctx = HandlerContext::new();
        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({"original": true}),
        };

        let result = handler.handle(&mut ctx, event).await;

        assert!(result.is_continue());
        let result_event = result.event().unwrap();
        if let SessionEvent::Custom { payload, .. } = result_event {
            assert_eq!(payload["original"], true);
            assert_eq!(payload["modified"], true);
        } else {
            panic!("Expected Custom event");
        }
    }

    #[tokio::test]
    async fn test_lua_handler_nil_passthrough() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("nil_return.lua");

        // Handler that returns nil (should pass through unchanged)
        fs::write(
            &script_path,
            r#"
function nil_handler(ctx, event)
    -- Do some processing but return nil to pass through
    return nil
end
"#,
        )
        .unwrap();

        let meta = LuaHandlerMeta::new(&script_path, "nil_handler");
        let handler = LuaHandler::new(meta).unwrap();

        let mut ctx = HandlerContext::new();
        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({"value": 123}),
        };

        let result = handler.handle(&mut ctx, event).await;

        assert!(result.is_continue());
        let result_event = result.event().unwrap();
        if let SessionEvent::Custom { payload, .. } = result_event {
            assert_eq!(payload["value"], 123);
        } else {
            panic!("Expected Custom event");
        }
    }

    #[test]
    fn test_lua_handler_debug() {
        let meta = LuaHandlerMeta::new("/path/script.lua", "handler");
        let debug = format!("{:?}", meta);
        assert!(debug.contains("script.lua"));
    }
}
