//! Tool Event Integration
//!
//! This module provides utilities for emitting tool events and processing
//! them through the event bus. It's designed to be used by MCP servers
//! to integrate with the unified hook system.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_rune::tool_events::{ToolEventEmitter, ToolSource};
//!
//! let mut emitter = ToolEventEmitter::new();
//! emitter.register_builtin_hooks(&BuiltinHooksConfig::default());
//!
//! // Before tool execution
//! let (event, ctx) = emitter.emit_before("just_test", args)?;
//! if event.is_cancelled() {
//!     return Err("Tool execution cancelled by hook");
//! }
//! let modified_args = event.payload; // Hooks may have modified args
//!
//! // Execute tool...
//!
//! // After tool execution
//! let (result_event, _) = emitter.emit_after("just_test", result, ToolSource::Just)?;
//! let final_result = result_event.payload; // Hooks may have transformed result
//! ```

use crate::builtin_hooks::{register_builtin_hooks, BuiltinHooksConfig};
use crate::event_bus::{Event, EventBus, EventContext, EventType, HandlerError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::time::Instant;
use tracing::{debug, warn};

/// Source of a tool (for metadata)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolSource {
    /// Kiln native tools (note operations, search, etc.)
    Kiln,
    /// Just recipe tools
    Just,
    /// Rune script tools
    Rune,
    /// Upstream MCP server tools
    Upstream,
}

impl ToolSource {
    /// Convert to string for event source field
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Kiln => "kiln",
            Self::Just => "just",
            Self::Rune => "rune",
            Self::Upstream => "upstream",
        }
    }
}

/// Tool event emitter for MCP servers
///
/// Wraps an EventBus and provides convenient methods for emitting
/// tool:before, tool:after, and tool:error events.
pub struct ToolEventEmitter {
    bus: EventBus,
    builtin_config: BuiltinHooksConfig,
}

impl Default for ToolEventEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolEventEmitter {
    /// Create a new emitter with default configuration
    pub fn new() -> Self {
        Self {
            bus: EventBus::new(),
            builtin_config: BuiltinHooksConfig::default(),
        }
    }

    /// Create with custom built-in hooks configuration
    pub fn with_config(config: BuiltinHooksConfig) -> Self {
        let mut emitter = Self::new();
        emitter.builtin_config = config;
        emitter
    }

    /// Register built-in hooks (test filter, toon transform, event emit)
    pub fn register_builtin_hooks(&mut self) {
        register_builtin_hooks(&mut self.bus, &self.builtin_config);
    }

    /// Get mutable reference to the underlying EventBus
    ///
    /// Use this to register custom hooks.
    pub fn bus_mut(&mut self) -> &mut EventBus {
        &mut self.bus
    }

    /// Get reference to the underlying EventBus
    pub fn bus(&self) -> &EventBus {
        &self.bus
    }

    /// Emit a tool:before event
    ///
    /// Returns the (possibly modified) event and context.
    /// Check `event.is_cancelled()` to see if a hook cancelled the execution.
    pub fn emit_before(
        &self,
        tool_name: &str,
        arguments: JsonValue,
        source: ToolSource,
    ) -> (Event, EventContext, Vec<HandlerError>) {
        let event = Event::tool_before(tool_name, arguments)
            .with_source(source.as_str());

        debug!("Emitting tool:before for {} (source: {})", tool_name, source.as_str());
        self.bus.emit(event)
    }

    /// Emit a tool:after event
    ///
    /// The result payload should include the tool output in a structure like:
    /// ```json
    /// {
    ///   "content": [{"type": "text", "text": "..."}],
    ///   "is_error": false
    /// }
    /// ```
    pub fn emit_after(
        &self,
        tool_name: &str,
        result: JsonValue,
        source: ToolSource,
        duration_ms: u64,
    ) -> (Event, EventContext, Vec<HandlerError>) {
        let payload = json!({
            "result": result,
            "duration_ms": duration_ms,
        });

        let event = Event::tool_after(tool_name, payload)
            .with_source(source.as_str());

        debug!("Emitting tool:after for {} (duration: {}ms)", tool_name, duration_ms);
        self.bus.emit(event)
    }

    /// Emit a tool:after event with content blocks (MCP format)
    ///
    /// Convenience method that formats content for hooks to process.
    pub fn emit_after_with_content(
        &self,
        tool_name: &str,
        content: Vec<ContentBlock>,
        is_error: bool,
        source: ToolSource,
        duration_ms: u64,
    ) -> (Event, EventContext, Vec<HandlerError>) {
        let payload = json!({
            "content": content,
            "is_error": is_error,
            "duration_ms": duration_ms,
        });

        let event = Event::tool_after(tool_name, payload)
            .with_source(source.as_str());

        debug!("Emitting tool:after for {} (is_error: {}, duration: {}ms)",
               tool_name, is_error, duration_ms);
        self.bus.emit(event)
    }

    /// Emit a tool:error event
    pub fn emit_error(
        &self,
        tool_name: &str,
        error: &str,
        source: ToolSource,
        duration_ms: u64,
    ) -> (Event, EventContext, Vec<HandlerError>) {
        let payload = json!({
            "error": error,
            "duration_ms": duration_ms,
        });

        let event = Event::tool_error(tool_name, payload)
            .with_source(source.as_str());

        debug!("Emitting tool:error for {}: {}", tool_name, error);
        self.bus.emit(event)
    }

    /// Execute a tool with full event lifecycle
    ///
    /// This is a helper that:
    /// 1. Emits tool:before (allowing cancellation or arg modification)
    /// 2. Calls the executor function
    /// 3. Emits tool:after or tool:error
    /// 4. Returns the (possibly transformed) result
    pub async fn execute_with_events<F, Fut, R, E>(
        &self,
        tool_name: &str,
        arguments: JsonValue,
        source: ToolSource,
        executor: F,
    ) -> Result<JsonValue, String>
    where
        F: FnOnce(JsonValue) -> Fut,
        Fut: std::future::Future<Output = Result<R, E>>,
        R: Into<JsonValue>,
        E: std::fmt::Display,
    {
        let start = Instant::now();

        // 1. Emit tool:before
        let (before_event, _ctx, errors) = self.emit_before(tool_name, arguments.clone(), source);

        if !errors.is_empty() {
            for e in &errors {
                warn!("Hook error during tool:before: {}", e);
            }
        }

        // Check if cancelled
        if before_event.is_cancelled() {
            return Err("Tool execution cancelled by hook".to_string());
        }

        // Get potentially modified arguments
        let modified_args = before_event.payload;

        // 2. Execute the tool
        let duration_ms = start.elapsed().as_millis() as u64;

        match executor(modified_args).await {
            Ok(result) => {
                let result_json: JsonValue = result.into();
                let duration_ms = start.elapsed().as_millis() as u64;

                // 3. Emit tool:after
                let (after_event, _ctx, errors) = self.emit_after(
                    tool_name,
                    result_json,
                    source,
                    duration_ms,
                );

                if !errors.is_empty() {
                    for e in &errors {
                        warn!("Hook error during tool:after: {}", e);
                    }
                }

                // Extract result from event (hooks may have transformed it)
                Ok(after_event.payload.get("result").cloned().unwrap_or(JsonValue::Null))
            }
            Err(e) => {
                let error_msg = e.to_string();
                let duration_ms = start.elapsed().as_millis() as u64;

                // 3. Emit tool:error
                let (_error_event, _ctx, _errors) = self.emit_error(
                    tool_name,
                    &error_msg,
                    source,
                    duration_ms,
                );

                Err(error_msg)
            }
        }
    }

    /// Get count of registered handlers
    pub fn handler_count(&self) -> usize {
        self.bus.count_handlers(EventType::ToolBefore)
            + self.bus.count_handlers(EventType::ToolAfter)
            + self.bus.count_handlers(EventType::ToolError)
    }
}

/// Content block types (matching MCP specification)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// Text content
    #[serde(rename = "text")]
    Text { text: String },
    /// Image content (base64 encoded)
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    /// Resource reference
    #[serde(rename = "resource")]
    Resource { uri: String, text: Option<String> },
}

impl ContentBlock {
    /// Create a text content block
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Get text content if this is a text block
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::Handler;

    #[test]
    fn test_tool_source_as_str() {
        assert_eq!(ToolSource::Kiln.as_str(), "kiln");
        assert_eq!(ToolSource::Just.as_str(), "just");
        assert_eq!(ToolSource::Rune.as_str(), "rune");
        assert_eq!(ToolSource::Upstream.as_str(), "upstream");
    }

    #[test]
    fn test_emitter_creation() {
        let emitter = ToolEventEmitter::new();
        assert_eq!(emitter.handler_count(), 0);
    }

    #[test]
    fn test_emitter_with_builtin_hooks() {
        let mut emitter = ToolEventEmitter::new();
        emitter.register_builtin_hooks();
        // Should have at least test_filter registered
        assert!(emitter.handler_count() >= 1);
    }

    #[test]
    fn test_emit_before() {
        let emitter = ToolEventEmitter::new();

        let (event, _ctx, errors) = emitter.emit_before(
            "test_tool",
            json!({"arg": "value"}),
            ToolSource::Just,
        );

        assert!(errors.is_empty());
        assert_eq!(event.event_type, EventType::ToolBefore);
        assert_eq!(event.identifier, "test_tool");
        assert_eq!(event.source, Some("just".to_string()));
        assert!(!event.is_cancelled());
    }

    #[test]
    fn test_emit_after() {
        let emitter = ToolEventEmitter::new();

        let (event, _ctx, errors) = emitter.emit_after(
            "test_tool",
            json!({"output": "success"}),
            ToolSource::Rune,
            100,
        );

        assert!(errors.is_empty());
        assert_eq!(event.event_type, EventType::ToolAfter);
        assert_eq!(event.identifier, "test_tool");
        assert_eq!(event.payload["duration_ms"], json!(100));
    }

    #[test]
    fn test_emit_error() {
        let emitter = ToolEventEmitter::new();

        let (event, _ctx, errors) = emitter.emit_error(
            "test_tool",
            "Something went wrong",
            ToolSource::Kiln,
            50,
        );

        assert!(errors.is_empty());
        assert_eq!(event.event_type, EventType::ToolError);
        assert_eq!(event.payload["error"], json!("Something went wrong"));
    }

    #[test]
    fn test_hook_can_cancel_execution() {
        let mut emitter = ToolEventEmitter::new();

        // Register a hook that cancels just_* tools
        emitter.bus_mut().register(
            Handler::new(
                "canceller",
                EventType::ToolBefore,
                "just_*",
                |_ctx, mut event| {
                    event.cancel();
                    Ok(event)
                },
            ),
        );

        // just_test should be cancelled
        let (event, _ctx, _) = emitter.emit_before("just_test", json!({}), ToolSource::Just);
        assert!(event.is_cancelled());

        // rune_test should not be cancelled
        let (event, _ctx, _) = emitter.emit_before("rune_test", json!({}), ToolSource::Rune);
        assert!(!event.is_cancelled());
    }

    #[test]
    fn test_hook_can_modify_arguments() {
        let mut emitter = ToolEventEmitter::new();

        // Register a hook that adds a default value
        emitter.bus_mut().register(
            Handler::new(
                "arg_modifier",
                EventType::ToolBefore,
                "*",
                |_ctx, mut event| {
                    if let Some(obj) = event.payload.as_object_mut() {
                        obj.insert("added_by_hook".to_string(), json!(true));
                    }
                    Ok(event)
                },
            ),
        );

        let (event, _ctx, _) = emitter.emit_before("test", json!({"original": true}), ToolSource::Just);

        assert_eq!(event.payload["original"], json!(true));
        assert_eq!(event.payload["added_by_hook"], json!(true));
    }

    #[test]
    fn test_hook_can_transform_result() {
        let mut emitter = ToolEventEmitter::new();

        // Register a hook that adds processing info
        emitter.bus_mut().register(
            Handler::new(
                "result_transformer",
                EventType::ToolAfter,
                "*",
                |_ctx, mut event| {
                    if let Some(obj) = event.payload.as_object_mut() {
                        obj.insert("processed_by_hook".to_string(), json!(true));
                    }
                    Ok(event)
                },
            ),
        );

        let (event, _ctx, _) = emitter.emit_after("test", json!({"data": 42}), ToolSource::Just, 100);

        assert_eq!(event.payload["processed_by_hook"], json!(true));
    }

    #[test]
    fn test_content_block_text() {
        let block = ContentBlock::text("Hello");
        assert_eq!(block.as_text(), Some("Hello"));

        let image = ContentBlock::Image {
            data: "base64".to_string(),
            mime_type: "image/png".to_string(),
        };
        assert_eq!(image.as_text(), None);
    }

    #[tokio::test]
    async fn test_execute_with_events() {
        let emitter = ToolEventEmitter::new();

        let result = emitter.execute_with_events(
            "test_tool",
            json!({"input": 21}),
            ToolSource::Just,
            |args| async move {
                let input = args["input"].as_i64().unwrap_or(0);
                Ok::<_, String>(json!({"output": input * 2}))
            },
        ).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap()["output"], json!(42));
    }

    #[tokio::test]
    async fn test_execute_with_events_cancellation() {
        let mut emitter = ToolEventEmitter::new();

        // Register a cancelling hook
        emitter.bus_mut().register(
            Handler::new(
                "canceller",
                EventType::ToolBefore,
                "*",
                |_ctx, mut event| {
                    event.cancel();
                    Ok(event)
                },
            ),
        );

        let result = emitter.execute_with_events(
            "test_tool",
            json!({}),
            ToolSource::Just,
            |_args| async { Ok::<_, String>(json!({})) },
        ).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cancelled"));
    }
}
