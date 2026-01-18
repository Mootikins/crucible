//! Rune Handler implementing core's unified Handler trait
//!
//! This module bridges Rune scripts to the core event system, allowing Rune
//! handlers to interleave with Rust and Lua handlers in the unified Reactor.
//!
//! ## Design
//!
//! Each `RuneHandler` represents a single handler function in a Rune script:
//!
//! ```rune
//! /// Check permissions before tool execution
//! #[handler(event = "tool:before", priority = 10)]
//! pub fn check_permissions(ctx, event) {
//!     if event.tool_name == "dangerous" {
//!         #{ cancel: true }
//!     } else {
//!         event
//!     }
//! }
//!
//! /// Log tool results
//! #[handler(event = "tool:after", depends = "persist")]
//! pub fn log_result(ctx, event) {
//!     log::info(`Tool completed: ${event.tool_name}`);
//!     event
//! }
//! ```
//!
//! A single script file can define multiple handlers, each becoming a separate
//! `RuneHandler` that registers with the core Reactor.
//!
//! ## Async Execution
//!
//! Rune execution is synchronous. The `handle()` method uses `spawn_blocking`
//! to run Rune on a thread pool, preventing reactor blocking.

use crate::executor::RuneExecutor;
use crate::RuneError;
use async_trait::async_trait;
use crucible_core::events::{Handler, HandlerContext, HandlerResult, SessionEvent};
use rune::Unit;
use serde_json::Value as JsonValue;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task::spawn_blocking;

/// Metadata for a discovered Rune handler function.
///
/// Extracted from `#[handler(...)]` attributes during script discovery.
#[derive(Debug, Clone)]
pub struct RuneHandlerMeta {
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

impl RuneHandlerMeta {
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
    /// Format: `rune:<script_path>:<function_name>`
    pub fn handler_name(&self) -> String {
        format!("rune:{}:{}", self.script_path.display(), self.function_name)
    }
}

/// A Rune script handler implementing core's `Handler` trait.
///
/// Each instance represents a single handler function within a Rune script.
/// The script is compiled once and the compiled unit is shared across
/// multiple handlers from the same file.
pub struct RuneHandler {
    /// Handler metadata (name, event pattern, priority, deps)
    meta: RuneHandlerMeta,

    /// Compiled Rune unit (shared across handlers from same script)
    unit: Arc<Unit>,

    /// Executor for running Rune code
    executor: Arc<RuneExecutor>,

    /// Cached handler name
    name: String,

    /// Cached dependencies as static strings (for trait method return)
    deps_static: Vec<&'static str>,
}

impl RuneHandler {
    /// Create a new Rune handler from metadata.
    ///
    /// Compiles the script if not already compiled.
    pub fn new(meta: RuneHandlerMeta, executor: Arc<RuneExecutor>) -> Result<Self, RuneError> {
        // Read and compile the script
        let source = std::fs::read_to_string(&meta.script_path).map_err(|e| {
            RuneError::Io(format!(
                "Failed to read handler script {:?}: {}",
                meta.script_path, e
            ))
        })?;

        let script_name = meta
            .script_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let unit = executor.compile(&script_name, &source)?;

        Self::with_unit(meta, unit, executor)
    }

    /// Create a handler with a pre-compiled unit.
    ///
    /// Use this when multiple handlers come from the same script file
    /// to share the compiled unit.
    pub fn with_unit(
        meta: RuneHandlerMeta,
        unit: Arc<Unit>,
        executor: Arc<RuneExecutor>,
    ) -> Result<Self, RuneError> {
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
            unit,
            executor,
            name,
            deps_static,
        })
    }

    /// Get the handler metadata.
    pub fn metadata(&self) -> &RuneHandlerMeta {
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
impl Handler for RuneHandler {
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

        let executor = self.executor.clone();
        let unit = self.unit.clone();
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

        // Run Rune execution on blocking thread pool
        let result = spawn_blocking(move || {
            // Convert JSON to Rune values
            let ctx_val = executor.json_to_rune_value(ctx_json)?;
            let event_val = executor.json_to_rune_value(event_json)?;

            // We need to run the async call_function synchronously
            // Use tokio's block_in_place pattern
            let runtime = tokio::runtime::Handle::try_current()
                .map_err(|e| RuneError::Execution(format!("No runtime available: {}", e)))?;

            let result = runtime.block_on(async {
                executor
                    .call_function(&unit, &function_name, (ctx_val, event_val))
                    .await
            });

            result
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

        parse_handler_result(result, event, &handler_name, ctx)
    }
}

/// Parse the JSON result from a Rune handler function.
///
/// Rune handlers can return:
/// - `null` or `None` - Pass through unchanged
/// - `{ cancel: true }` - Cancel the event
/// - `{ emit: [...] }` - Emit additional events (queued in ctx, continue with original)
/// - `{ emit: [...], event: {...} }` - Emit events and continue with modified event
/// - Modified event object - Continue with modified event
fn parse_handler_result(
    result: JsonValue,
    original_event: SessionEvent,
    handler_name: &str,
    ctx: &mut HandlerContext,
) -> HandlerResult<SessionEvent> {
    if result.is_null() {
        return HandlerResult::ok(original_event);
    }

    if let Some(obj) = result.as_object() {
        if obj.get("cancel") == Some(&JsonValue::Bool(true)) {
            return HandlerResult::cancel();
        }

        if let Some(events) = obj.get("emit") {
            if let Some(events_arr) = events.as_array() {
                for event_json in events_arr {
                    match serde_json::from_value::<SessionEvent>(event_json.clone()) {
                        Ok(event) => {
                            ctx.emit(event);
                        }
                        Err(e) => {
                            tracing::warn!("Handler {} emitted invalid event: {}", handler_name, e);
                        }
                    }
                }
            }

            if let Some(event_json) = obj.get("event") {
                match serde_json::from_value::<SessionEvent>(event_json.clone()) {
                    Ok(modified_event) => return HandlerResult::ok(modified_event),
                    Err(e) => {
                        tracing::warn!(
                            "Handler {} returned invalid event in emit response: {}",
                            handler_name,
                            e
                        );
                        return HandlerResult::ok(original_event);
                    }
                }
            }

            return HandlerResult::ok(original_event);
        }
    }

    match serde_json::from_value::<SessionEvent>(result.clone()) {
        Ok(modified_event) => HandlerResult::ok(modified_event),
        Err(e) => {
            tracing::warn!(
                "Handler {} returned invalid event structure: {}",
                handler_name,
                e
            );
            HandlerResult::ok(original_event)
        }
    }
}

impl std::fmt::Debug for RuneHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuneHandler")
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

    fn create_test_executor() -> Arc<RuneExecutor> {
        Arc::new(RuneExecutor::new().expect("Failed to create executor"))
    }

    #[test]
    fn test_rune_handler_meta_default() {
        let meta = RuneHandlerMeta::new("/path/to/script.rn", "my_handler");

        assert_eq!(meta.script_path, PathBuf::from("/path/to/script.rn"));
        assert_eq!(meta.function_name, "my_handler");
        assert_eq!(meta.event_pattern, "*");
        assert_eq!(meta.priority, 50);
        assert!(meta.dependencies.is_empty());
        assert!(meta.enabled);
    }

    #[test]
    fn test_rune_handler_meta_builder() {
        let meta = RuneHandlerMeta::new("/path/to/script.rn", "my_handler")
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
    fn test_rune_handler_meta_name() {
        let meta = RuneHandlerMeta::new("plugins/auth.rn", "check_permissions");
        assert_eq!(
            meta.handler_name(),
            "rune:plugins/auth.rn:check_permissions"
        );
    }

    #[tokio::test]
    async fn test_rune_handler_creation() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("test_handler.rn");

        fs::write(
            &script_path,
            r#"
pub fn test_handler(ctx, event) {
    event
}
"#,
        )
        .unwrap();

        let executor = create_test_executor();
        let meta = RuneHandlerMeta::new(&script_path, "test_handler");

        let handler = RuneHandler::new(meta, executor).expect("Failed to create handler");

        assert!(handler.name().contains("test_handler"));
        assert_eq!(handler.priority(), 50);
        assert_eq!(handler.event_pattern(), "*");
        assert!(handler.dependencies().is_empty());
    }

    #[tokio::test]
    async fn test_rune_handler_passthrough() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("passthrough.rn");

        // Handler that just returns the event unchanged
        fs::write(
            &script_path,
            r#"
pub fn passthrough(ctx, event) {
    event
}
"#,
        )
        .unwrap();

        let executor = create_test_executor();
        let meta = RuneHandlerMeta::new(&script_path, "passthrough");
        let handler = RuneHandler::new(meta, executor).unwrap();

        let mut ctx = HandlerContext::new();
        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({"value": 42}),
        };

        let result = handler.handle(&mut ctx, event.clone()).await;

        assert!(result.is_continue());
        let result_event = result.event().unwrap();
        // The event should be essentially unchanged
        if let SessionEvent::Custom { name, payload } = result_event {
            assert_eq!(name, "test");
            assert_eq!(payload["value"], 42);
        } else {
            panic!("Expected Custom event");
        }
    }

    #[tokio::test]
    async fn test_rune_handler_cancel() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("canceller.rn");

        // Handler that cancels events
        fs::write(
            &script_path,
            r#"
pub fn canceller(ctx, event) {
    #{ cancel: true }
}
"#,
        )
        .unwrap();

        let executor = create_test_executor();
        let meta = RuneHandlerMeta::new(&script_path, "canceller");
        let handler = RuneHandler::new(meta, executor).unwrap();

        let mut ctx = HandlerContext::new();
        let event = SessionEvent::Custom {
            name: "test".into(),
            payload: serde_json::json!({}),
        };

        let result = handler.handle(&mut ctx, event).await;

        assert!(result.is_cancel());
    }

    #[tokio::test]
    async fn test_rune_handler_disabled() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("disabled.rn");

        fs::write(
            &script_path,
            r#"
pub fn disabled_handler(ctx, event) {
    #{ cancel: true }
}
"#,
        )
        .unwrap();

        let executor = create_test_executor();
        let meta = RuneHandlerMeta::new(&script_path, "disabled_handler").with_enabled(false);
        let handler = RuneHandler::new(meta, executor).unwrap();

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
    async fn test_rune_handler_with_priority() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("priority.rn");

        fs::write(
            &script_path,
            r#"
pub fn priority_handler(ctx, event) {
    event
}
"#,
        )
        .unwrap();

        let executor = create_test_executor();
        let meta = RuneHandlerMeta::new(&script_path, "priority_handler").with_priority(10);
        let handler = RuneHandler::new(meta, executor).unwrap();

        assert_eq!(handler.priority(), 10);
    }

    #[tokio::test]
    async fn test_rune_handler_with_dependencies() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("deps.rn");

        fs::write(
            &script_path,
            r#"
pub fn dependent_handler(ctx, event) {
    event
}
"#,
        )
        .unwrap();

        let executor = create_test_executor();
        let meta = RuneHandlerMeta::new(&script_path, "dependent_handler")
            .with_dependencies(vec!["persist".into(), "validate".into()]);
        let handler = RuneHandler::new(meta, executor).unwrap();

        let deps = handler.dependencies();
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&"persist"));
        assert!(deps.contains(&"validate"));
    }

    #[tokio::test]
    async fn test_rune_handler_with_event_pattern() {
        let temp = TempDir::new().unwrap();
        let script_path = temp.path().join("pattern.rn");

        fs::write(
            &script_path,
            r#"
pub fn tool_handler(ctx, event) {
    event
}
"#,
        )
        .unwrap();

        let executor = create_test_executor();
        let meta = RuneHandlerMeta::new(&script_path, "tool_handler").with_event_pattern("tool:*");
        let handler = RuneHandler::new(meta, executor).unwrap();

        assert_eq!(handler.event_pattern(), "tool:*");
    }

    #[test]
    fn test_rune_handler_debug() {
        // Just test that Debug doesn't panic
        let meta = RuneHandlerMeta::new("/path/script.rn", "handler");
        let debug = format!("{:?}", meta);
        assert!(debug.contains("script.rn"));
    }

    #[test]
    fn test_parse_handler_result_emit_events() {
        use crucible_core::events::{SessionEvent, SessionEventConfig};

        let original = SessionEvent::SessionStarted {
            config: SessionEventConfig::new("test-session"),
        };

        let mut ctx = HandlerContext::new();

        let result = serde_json::json!({
            "emit": [
                { "type": "session_paused", "session_id": "test-session" }
            ]
        });

        let handler_result =
            parse_handler_result(result, original.clone(), "test_handler", &mut ctx);

        assert!(
            matches!(handler_result, HandlerResult::Continue(ev) if matches!(ev, SessionEvent::SessionStarted { .. }))
        );

        let emitted = ctx.take_emitted();
        assert_eq!(emitted.len(), 1);
        assert!(matches!(emitted[0], SessionEvent::SessionPaused { .. }));
    }

    #[test]
    fn test_parse_handler_result_emit_with_modified_event() {
        use crucible_core::events::{SessionEvent, SessionEventConfig};

        let original = SessionEvent::SessionStarted {
            config: SessionEventConfig::new("original"),
        };

        let mut ctx = HandlerContext::new();

        let result = serde_json::json!({
            "emit": [
                { "type": "session_paused", "session_id": "emitted" }
            ],
            "event": {
                "type": "session_started",
                "config": { "session_id": "modified" }
            }
        });

        let handler_result = parse_handler_result(result, original, "test_handler", &mut ctx);

        match handler_result {
            HandlerResult::Continue(SessionEvent::SessionStarted { config }) => {
                assert_eq!(config.session_id, "modified");
            }
            _ => panic!("Expected Continue with modified SessionStarted"),
        }

        let emitted = ctx.take_emitted();
        assert_eq!(emitted.len(), 1);
        match &emitted[0] {
            SessionEvent::SessionPaused { session_id } => {
                assert_eq!(session_id, "emitted");
            }
            _ => panic!("Expected SessionPaused"),
        }
    }

    #[test]
    fn test_parse_handler_result_invalid_emit_events_are_skipped() {
        use crucible_core::events::{SessionEvent, SessionEventConfig};

        let original = SessionEvent::SessionStarted {
            config: SessionEventConfig::new("test"),
        };

        let mut ctx = HandlerContext::new();

        let result = serde_json::json!({
            "emit": [
                { "type": "session_paused", "session_id": "valid" },
                { "type": "invalid_event", "bad": "data" },
                { "type": "session_resumed", "session_id": "also-valid" }
            ]
        });

        let _ = parse_handler_result(result, original, "test_handler", &mut ctx);

        let emitted = ctx.take_emitted();
        assert_eq!(emitted.len(), 2);
        assert!(matches!(emitted[0], SessionEvent::SessionPaused { .. }));
        assert!(matches!(emitted[1], SessionEvent::SessionResumed { .. }));
    }
}
