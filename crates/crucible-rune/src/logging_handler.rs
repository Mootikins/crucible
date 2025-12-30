//! Logging handler for the event ring buffer system.
//!
//! This module provides a `LoggingHandler` that logs all events passing through
//! the ring buffer pipeline. It's useful for debugging, auditing, and tracing
//! event flow through the system.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_rune::logging_handler::{LoggingHandler, LogLevel};
//! use crucible_rune::handler_chain::HandlerChain;
//!
//! // Create a logging handler with info level
//! let logger = LoggingHandler::new("event_logger", LogLevel::Info);
//!
//! // Add to handler chain (usually first to capture all events)
//! let mut chain = HandlerChain::new();
//! chain.register(Box::new(logger));
//! ```
//!
//! ## Log Output
//!
//! Events are logged with structured fields:
//! - `handler`: Handler name ("event_logger")
//! - `seq`: Sequence number in the ring buffer
//! - `event_type`: Type of session event
//! - `event_details`: Summary of event content
//!
//! ## Configuration
//!
//! The handler can be configured with:
//! - **Log level**: Debug, Info, Warn, or Error
//! - **Event filtering**: Optional filter to log only specific event types
//! - **Dependencies**: Can depend on other handlers

use async_trait::async_trait;
use std::sync::Arc;

use crate::handler::{RingHandler, RingHandlerContext, RingHandlerResult};
use crate::reactor::SessionEvent;

/// Log level for the logging handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogLevel {
    /// Debug level - most verbose
    Debug,
    /// Info level - standard logging (default)
    #[default]
    Info,
    /// Warn level - only warnings and errors
    Warn,
    /// Error level - only errors
    Error,
    /// Trace level - most detailed, includes internal state
    Trace,
}

impl LogLevel {
    /// Parse log level from string.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "debug" => Some(Self::Debug),
            "info" => Some(Self::Info),
            "warn" | "warning" => Some(Self::Warn),
            "error" => Some(Self::Error),
            "trace" => Some(Self::Trace),
            _ => None,
        }
    }

    /// Get string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
            Self::Trace => "trace",
        }
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Filter for which events to log.
#[derive(Debug, Clone, Default)]
pub enum EventFilter {
    /// Log all events
    #[default]
    All,
    /// Log only specific event types
    Only(Vec<String>),
    /// Log all except specific event types
    Except(Vec<String>),
}

impl EventFilter {
    /// Check if an event type should be logged.
    pub fn should_log(&self, event_type: &str) -> bool {
        match self {
            Self::All => true,
            Self::Only(types) => types.iter().any(|t| t == event_type),
            Self::Except(types) => !types.iter().any(|t| t == event_type),
        }
    }
}

/// Configuration for the logging handler.
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    /// Log level
    pub level: LogLevel,
    /// Event filter
    pub filter: EventFilter,
    /// Whether to include payload/content in logs (may be large)
    pub include_payload: bool,
    /// Maximum payload length to log (truncated if longer)
    pub max_payload_length: usize,
    /// Prefix for log messages
    pub prefix: Option<String>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            filter: EventFilter::All,
            include_payload: false,
            max_payload_length: 200,
            prefix: None,
        }
    }
}

impl LoggingConfig {
    /// Create a new config with the given log level.
    pub fn with_level(level: LogLevel) -> Self {
        Self {
            level,
            ..Default::default()
        }
    }

    /// Set the event filter.
    pub fn filter(mut self, filter: EventFilter) -> Self {
        self.filter = filter;
        self
    }

    /// Enable payload logging.
    pub fn with_payload(mut self) -> Self {
        self.include_payload = true;
        self
    }

    /// Set maximum payload length.
    pub fn max_payload(mut self, len: usize) -> Self {
        self.max_payload_length = len;
        self
    }

    /// Set log prefix.
    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }
}

/// A handler that logs all events passing through the ring buffer.
///
/// This is typically registered first in the handler chain to capture
/// all events before any transformation occurs.
pub struct LoggingHandler {
    /// Handler name
    name: String,
    /// Handler dependencies
    dependencies: Vec<&'static str>,
    /// Logging configuration
    config: LoggingConfig,
}

impl LoggingHandler {
    /// Create a new logging handler with the given name and log level.
    pub fn new(name: impl Into<String>, level: LogLevel) -> Self {
        Self {
            name: name.into(),
            dependencies: Vec::new(),
            config: LoggingConfig::with_level(level),
        }
    }

    /// Create a logging handler with full configuration.
    pub fn with_config(name: impl Into<String>, config: LoggingConfig) -> Self {
        Self {
            name: name.into(),
            dependencies: Vec::new(),
            config,
        }
    }

    /// Add a dependency on another handler.
    ///
    /// The logging handler will run after the specified handler.
    pub fn with_dependency(mut self, handler_name: &'static str) -> Self {
        self.dependencies.push(handler_name);
        self
    }

    /// Get the event type name from a SessionEvent.
    fn event_type_name(event: &SessionEvent) -> &'static str {
        match event {
            SessionEvent::MessageReceived { .. } => "MessageReceived",
            SessionEvent::AgentResponded { .. } => "AgentResponded",
            SessionEvent::AgentThinking { .. } => "AgentThinking",
            SessionEvent::ToolCalled { .. } => "ToolCalled",
            SessionEvent::ToolCompleted { .. } => "ToolCompleted",
            SessionEvent::SessionStarted { .. } => "SessionStarted",
            SessionEvent::SessionCompacted { .. } => "SessionCompacted",
            SessionEvent::SessionEnded { .. } => "SessionEnded",
            SessionEvent::SubagentSpawned { .. } => "SubagentSpawned",
            SessionEvent::SubagentCompleted { .. } => "SubagentCompleted",
            SessionEvent::SubagentFailed { .. } => "SubagentFailed",
            SessionEvent::TextDelta { .. } => "TextDelta",
            SessionEvent::NoteParsed { .. } => "NoteParsed",
            SessionEvent::NoteCreated { .. } => "NoteCreated",
            SessionEvent::NoteModified { .. } => "NoteModified",
            SessionEvent::McpAttached { .. } => "McpAttached",
            SessionEvent::ToolDiscovered { .. } => "ToolDiscovered",
            SessionEvent::Custom { .. } => "Custom",
            // File events
            SessionEvent::FileChanged { .. } => "FileChanged",
            SessionEvent::FileDeleted { .. } => "FileDeleted",
            SessionEvent::FileMoved { .. } => "FileMoved",
            // Storage events
            SessionEvent::EntityStored { .. } => "EntityStored",
            SessionEvent::EntityDeleted { .. } => "EntityDeleted",
            SessionEvent::BlocksUpdated { .. } => "BlocksUpdated",
            SessionEvent::RelationStored { .. } => "RelationStored",
            SessionEvent::RelationDeleted { .. } => "RelationDeleted",
            SessionEvent::TagAssociated { .. } => "TagAssociated",
            // Embedding events
            SessionEvent::EmbeddingRequested { .. } => "EmbeddingRequested",
            SessionEvent::EmbeddingStored { .. } => "EmbeddingStored",
            SessionEvent::EmbeddingFailed { .. } => "EmbeddingFailed",
            SessionEvent::EmbeddingBatchComplete { .. } => "EmbeddingBatchComplete",
            // Pre-events
            SessionEvent::PreToolCall { .. } => "PreToolCall",
            SessionEvent::PreParse { .. } => "PreParse",
            SessionEvent::PreLlmCall { .. } => "PreLlmCall",
            SessionEvent::AwaitingInput { .. } => "AwaitingInput",
        }
    }

    /// Get a summary of the event content.
    fn event_summary(event: &SessionEvent, max_len: usize) -> String {
        let summary = match event {
            SessionEvent::MessageReceived {
                content,
                participant_id,
            } => {
                format!("from={}, content_len={}", participant_id, content.len())
            }
            SessionEvent::AgentResponded {
                content,
                tool_calls,
            } => {
                format!(
                    "content_len={}, tool_calls={}",
                    content.len(),
                    tool_calls.len()
                )
            }
            SessionEvent::AgentThinking { thought } => {
                format!("thought_len={}", thought.len())
            }
            SessionEvent::ToolCalled { name, args } => {
                format!("tool={}, args_size={}", name, args.to_string().len())
            }
            SessionEvent::ToolCompleted {
                name,
                result,
                error,
            } => {
                format!(
                    "tool={}, result_len={}, error={}",
                    name,
                    result.len(),
                    error.is_some()
                )
            }
            SessionEvent::SessionStarted { config } => {
                format!("session_id={}", config.session_id)
            }
            SessionEvent::SessionCompacted { summary, new_file } => {
                format!(
                    "summary_len={}, new_file={}",
                    summary.len(),
                    new_file.display()
                )
            }
            SessionEvent::SessionEnded { reason } => {
                format!("reason={}", truncate(reason, max_len))
            }
            SessionEvent::SubagentSpawned { id, prompt } => {
                format!("id={}, prompt_len={}", id, prompt.len())
            }
            SessionEvent::SubagentCompleted { id, result } => {
                format!("id={}, result_len={}", id, result.len())
            }
            SessionEvent::SubagentFailed { id, error } => {
                format!("id={}, error={}", id, truncate(error, max_len))
            }
            SessionEvent::TextDelta { delta, seq } => {
                format!("seq={}, delta_len={}", seq, delta.len())
            }
            SessionEvent::NoteParsed {
                path,
                block_count,
                payload,
            } => {
                let payload_str = if payload.is_some() {
                    ", has_payload"
                } else {
                    ""
                };
                format!(
                    "path={}, blocks={}{}",
                    path.display(),
                    block_count,
                    payload_str
                )
            }
            SessionEvent::NoteCreated { path, title } => {
                let title_str = title.as_deref().unwrap_or("(none)");
                format!(
                    "path={}, title={}",
                    path.display(),
                    truncate(title_str, max_len)
                )
            }
            SessionEvent::NoteModified { path, change_type } => {
                format!("path={}, change={:?}", path.display(), change_type)
            }
            SessionEvent::McpAttached { server, tool_count } => {
                format!("server={}, tools={}", server, tool_count)
            }
            SessionEvent::ToolDiscovered { name, source, .. } => {
                format!("name={}, source={:?}", name, source)
            }
            SessionEvent::Custom { name, payload } => {
                format!("name={}, payload_size={}", name, payload.to_string().len())
            }
            // File events
            SessionEvent::FileChanged { path, kind } => {
                format!("path={}, kind={:?}", path.display(), kind)
            }
            SessionEvent::FileDeleted { path } => {
                format!("path={}", path.display())
            }
            SessionEvent::FileMoved { from, to } => {
                format!("from={}, to={}", from.display(), to.display())
            }
            // Storage events
            SessionEvent::EntityStored {
                entity_id,
                entity_type,
            } => {
                format!("entity_id={}, type={:?}", entity_id, entity_type)
            }
            SessionEvent::EntityDeleted {
                entity_id,
                entity_type,
            } => {
                format!("entity_id={}, type={:?}", entity_id, entity_type)
            }
            SessionEvent::BlocksUpdated {
                entity_id,
                block_count,
            } => {
                format!("entity_id={}, blocks={}", entity_id, block_count)
            }
            SessionEvent::RelationStored {
                from_id,
                to_id,
                relation_type,
            } => {
                format!("from={}, to={}, type={}", from_id, to_id, relation_type)
            }
            SessionEvent::RelationDeleted {
                from_id,
                to_id,
                relation_type,
            } => {
                format!("from={}, to={}, type={}", from_id, to_id, relation_type)
            }
            SessionEvent::TagAssociated { entity_id, tag } => {
                format!("entity_id={}, tag={}", entity_id, tag)
            }
            // Embedding events
            SessionEvent::EmbeddingRequested {
                entity_id,
                priority,
                ..
            } => {
                format!("entity_id={}, priority={:?}", entity_id, priority)
            }
            SessionEvent::EmbeddingStored {
                entity_id,
                dimensions,
                ..
            } => {
                format!("entity_id={}, dims={}", entity_id, dimensions)
            }
            SessionEvent::EmbeddingFailed {
                entity_id, error, ..
            } => {
                format!(
                    "entity_id={}, error={}",
                    entity_id,
                    truncate(error, max_len)
                )
            }
            SessionEvent::EmbeddingBatchComplete {
                entity_id,
                count,
                duration_ms,
            } => {
                format!(
                    "entity_id={}, count={}, duration={}ms",
                    entity_id, count, duration_ms
                )
            }
            // Pre-events
            SessionEvent::PreToolCall { name, args } => {
                format!("tool={}, args_size={}", name, args.to_string().len())
            }
            SessionEvent::PreParse { path } => {
                format!("path={}", path.display())
            }
            SessionEvent::PreLlmCall { prompt, model } => {
                format!("model={}, prompt_len={}", model, prompt.len())
            }
            SessionEvent::AwaitingInput { input_type, context } => {
                format!(
                    "type={}, context={}",
                    input_type,
                    context.as_deref().unwrap_or("(none)")
                )
            }
        };

        summary
    }

    /// Get the payload content for detailed logging.
    fn event_payload(event: &SessionEvent, max_len: usize) -> Option<String> {
        let payload = match event {
            SessionEvent::MessageReceived { content, .. } => Some(content.clone()),
            SessionEvent::AgentResponded { content, .. } => Some(content.clone()),
            SessionEvent::AgentThinking { thought } => Some(thought.clone()),
            SessionEvent::ToolCalled { args, .. } => Some(args.to_string()),
            SessionEvent::ToolCompleted { result, .. } => Some(result.clone()),
            SessionEvent::SessionCompacted { summary, .. } => Some(summary.clone()),
            SessionEvent::SessionEnded { reason } => Some(reason.clone()),
            SessionEvent::SubagentSpawned { prompt, .. } => Some(prompt.clone()),
            SessionEvent::SubagentCompleted { result, .. } => Some(result.clone()),
            SessionEvent::SubagentFailed { error, .. } => Some(error.clone()),
            SessionEvent::Custom { payload, .. } => Some(payload.to_string()),
            SessionEvent::SessionStarted { .. } => None,
            SessionEvent::TextDelta { delta, .. } => Some(delta.clone()),
            SessionEvent::NoteParsed { path, .. } => Some(path.display().to_string()),
            SessionEvent::NoteCreated { path, title } => Some(format!(
                "{}: {}",
                path.display(),
                title.as_deref().unwrap_or("(none)")
            )),
            SessionEvent::NoteModified { path, change_type } => {
                Some(format!("{}: {:?}", path.display(), change_type))
            }
            SessionEvent::McpAttached { server, tool_count } => {
                Some(format!("{}: {} tools", server, tool_count))
            }
            SessionEvent::ToolDiscovered {
                name,
                source,
                schema,
            } => {
                let schema_len = schema.as_ref().map(|s| s.to_string().len()).unwrap_or(0);
                Some(format!("{}: {:?}, schema_len={}", name, source, schema_len))
            }
            // File events
            SessionEvent::FileChanged { path, kind } => {
                Some(format!("{}: {:?}", path.display(), kind))
            }
            SessionEvent::FileDeleted { path } => Some(path.display().to_string()),
            SessionEvent::FileMoved { from, to } => {
                Some(format!("{} -> {}", from.display(), to.display()))
            }
            // Storage events
            SessionEvent::EntityStored {
                entity_id,
                entity_type,
            } => Some(format!("{}: {:?}", entity_id, entity_type)),
            SessionEvent::EntityDeleted {
                entity_id,
                entity_type,
            } => Some(format!("{}: {:?}", entity_id, entity_type)),
            SessionEvent::BlocksUpdated {
                entity_id,
                block_count,
            } => Some(format!("{}: {} blocks", entity_id, block_count)),
            SessionEvent::RelationStored {
                from_id,
                to_id,
                relation_type,
            } => Some(format!("{} -> {} ({})", from_id, to_id, relation_type)),
            SessionEvent::RelationDeleted {
                from_id,
                to_id,
                relation_type,
            } => Some(format!("{} -> {} ({})", from_id, to_id, relation_type)),
            SessionEvent::TagAssociated { entity_id, tag } => {
                Some(format!("{}#{}", entity_id, tag))
            }
            // Embedding events
            SessionEvent::EmbeddingRequested {
                entity_id,
                priority,
                ..
            } => Some(format!("{}: {:?}", entity_id, priority)),
            SessionEvent::EmbeddingStored {
                entity_id,
                dimensions,
                model,
                ..
            } => Some(format!(
                "{}: {} dims, model={}",
                entity_id, dimensions, model
            )),
            SessionEvent::EmbeddingFailed {
                entity_id, error, ..
            } => Some(format!("{}: {}", entity_id, error)),
            SessionEvent::EmbeddingBatchComplete {
                entity_id,
                count,
                duration_ms,
            } => Some(format!(
                "{}: {} embeddings in {}ms",
                entity_id, count, duration_ms
            )),
            // Pre-events
            SessionEvent::PreToolCall { args, .. } => Some(args.to_string()),
            SessionEvent::PreParse { path } => Some(path.display().to_string()),
            SessionEvent::PreLlmCall { prompt, .. } => Some(prompt.clone()),
            SessionEvent::AwaitingInput { context, .. } => context.clone(),
        };

        payload.map(|p| truncate(&p, max_len).to_string())
    }

    /// Log the event using the configured level.
    fn log_event(&self, seq: u64, event: &SessionEvent) {
        let event_type = Self::event_type_name(event);

        // Check filter
        if !self.config.filter.should_log(event_type) {
            return;
        }

        let summary = Self::event_summary(event, self.config.max_payload_length);
        let prefix = self
            .config
            .prefix
            .as_deref()
            .map(|p| format!("[{}] ", p))
            .unwrap_or_default();

        let payload_str = if self.config.include_payload {
            Self::event_payload(event, self.config.max_payload_length)
                .map(|p| format!(" | payload={}", p))
                .unwrap_or_default()
        } else {
            String::new()
        };

        match self.config.level {
            LogLevel::Trace => {
                tracing::trace!(
                    handler = %self.name,
                    seq = seq,
                    event_type = event_type,
                    "{}Event: {} | {}{}",
                    prefix,
                    event_type,
                    summary,
                    payload_str
                );
            }
            LogLevel::Debug => {
                tracing::debug!(
                    handler = %self.name,
                    seq = seq,
                    event_type = event_type,
                    "{}Event: {} | {}{}",
                    prefix,
                    event_type,
                    summary,
                    payload_str
                );
            }
            LogLevel::Info => {
                tracing::info!(
                    handler = %self.name,
                    seq = seq,
                    event_type = event_type,
                    "{}Event: {} | {}",
                    prefix,
                    event_type,
                    summary
                );
            }
            LogLevel::Warn => {
                tracing::warn!(
                    handler = %self.name,
                    seq = seq,
                    event_type = event_type,
                    "{}Event: {} | {}",
                    prefix,
                    event_type,
                    summary
                );
            }
            LogLevel::Error => {
                tracing::error!(
                    handler = %self.name,
                    seq = seq,
                    event_type = event_type,
                    "{}Event: {} | {}",
                    prefix,
                    event_type,
                    summary
                );
            }
        }
    }
}

#[async_trait]
impl RingHandler<SessionEvent> for LoggingHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn depends_on(&self) -> &[&str] {
        &self.dependencies
    }

    async fn handle(
        &self,
        _ctx: &mut RingHandlerContext<SessionEvent>,
        event: Arc<SessionEvent>,
        seq: u64,
    ) -> RingHandlerResult<()> {
        self.log_event(seq, &event);
        Ok(())
    }

    async fn on_register(&self) -> RingHandlerResult<()> {
        tracing::debug!(
            handler = %self.name,
            level = %self.config.level,
            "LoggingHandler registered"
        );
        Ok(())
    }

    async fn on_unregister(&self) -> RingHandlerResult<()> {
        tracing::debug!(
            handler = %self.name,
            "LoggingHandler unregistered"
        );
        Ok(())
    }
}

impl std::fmt::Debug for LoggingHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoggingHandler")
            .field("name", &self.name)
            .field("dependencies", &self.dependencies)
            .field("config", &self.config)
            .finish()
    }
}

/// Truncate a string to max_len, adding "..." if truncated.
fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        // Find a char boundary near max_len
        let mut end = max_len;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &s[..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reactor::SessionEventConfig;
    use serde_json::json;
    use std::path::PathBuf;

    /// Cross-platform test path helper
    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("crucible_test_{}", name))
    }

    #[test]
    fn test_log_level_from_str() {
        assert_eq!(LogLevel::from_str("debug"), Some(LogLevel::Debug));
        assert_eq!(LogLevel::from_str("DEBUG"), Some(LogLevel::Debug));
        assert_eq!(LogLevel::from_str("info"), Some(LogLevel::Info));
        assert_eq!(LogLevel::from_str("warn"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::from_str("warning"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::from_str("error"), Some(LogLevel::Error));
        assert_eq!(LogLevel::from_str("trace"), Some(LogLevel::Trace));
        assert_eq!(LogLevel::from_str("invalid"), None);
    }

    #[test]
    fn test_log_level_as_str() {
        assert_eq!(LogLevel::Debug.as_str(), "debug");
        assert_eq!(LogLevel::Info.as_str(), "info");
        assert_eq!(LogLevel::Warn.as_str(), "warn");
        assert_eq!(LogLevel::Error.as_str(), "error");
        assert_eq!(LogLevel::Trace.as_str(), "trace");
    }

    #[test]
    fn test_log_level_display() {
        assert_eq!(format!("{}", LogLevel::Info), "info");
        assert_eq!(format!("{}", LogLevel::Debug), "debug");
    }

    #[test]
    fn test_log_level_default() {
        assert_eq!(LogLevel::default(), LogLevel::Info);
    }

    #[test]
    fn test_event_filter_all() {
        let filter = EventFilter::All;
        assert!(filter.should_log("MessageReceived"));
        assert!(filter.should_log("ToolCalled"));
        assert!(filter.should_log("Anything"));
    }

    #[test]
    fn test_event_filter_only() {
        let filter = EventFilter::Only(vec![
            "MessageReceived".to_string(),
            "AgentResponded".to_string(),
        ]);
        assert!(filter.should_log("MessageReceived"));
        assert!(filter.should_log("AgentResponded"));
        assert!(!filter.should_log("ToolCalled"));
        assert!(!filter.should_log("SessionStarted"));
    }

    #[test]
    fn test_event_filter_except() {
        let filter = EventFilter::Except(vec!["AgentThinking".to_string()]);
        assert!(filter.should_log("MessageReceived"));
        assert!(filter.should_log("ToolCalled"));
        assert!(!filter.should_log("AgentThinking"));
    }

    #[test]
    fn test_logging_config_default() {
        let config = LoggingConfig::default();
        assert_eq!(config.level, LogLevel::Info);
        assert!(!config.include_payload);
        assert_eq!(config.max_payload_length, 200);
        assert!(config.prefix.is_none());
    }

    #[test]
    fn test_logging_config_builder() {
        let config = LoggingConfig::with_level(LogLevel::Debug)
            .filter(EventFilter::Only(vec!["ToolCalled".to_string()]))
            .with_payload()
            .max_payload(500)
            .prefix("SESSION");

        assert_eq!(config.level, LogLevel::Debug);
        assert!(config.include_payload);
        assert_eq!(config.max_payload_length, 500);
        assert_eq!(config.prefix, Some("SESSION".to_string()));
    }

    #[test]
    fn test_logging_handler_new() {
        let handler = LoggingHandler::new("test_logger", LogLevel::Debug);
        assert_eq!(handler.name(), "test_logger");
        assert!(handler.dependencies.is_empty());
        assert_eq!(handler.config.level, LogLevel::Debug);
    }

    #[test]
    fn test_logging_handler_with_config() {
        let config = LoggingConfig::with_level(LogLevel::Warn).prefix("TEST");
        let handler = LoggingHandler::with_config("custom_logger", config);
        assert_eq!(handler.name(), "custom_logger");
        assert_eq!(handler.config.level, LogLevel::Warn);
        assert_eq!(handler.config.prefix, Some("TEST".to_string()));
    }

    #[test]
    fn test_logging_handler_with_dependency() {
        let handler = LoggingHandler::new("logger", LogLevel::Info)
            .with_dependency("persistence")
            .with_dependency("validation");
        assert_eq!(handler.dependencies, vec!["persistence", "validation"]);
    }

    #[test]
    fn test_event_type_name() {
        assert_eq!(
            LoggingHandler::event_type_name(&SessionEvent::MessageReceived {
                content: "test".into(),
                participant_id: "user".into(),
            }),
            "MessageReceived"
        );
        assert_eq!(
            LoggingHandler::event_type_name(&SessionEvent::ToolCalled {
                name: "tool".into(),
                args: json!({}),
            }),
            "ToolCalled"
        );
        assert_eq!(
            LoggingHandler::event_type_name(&SessionEvent::SessionEnded {
                reason: "done".into(),
            }),
            "SessionEnded"
        );
    }

    #[test]
    fn test_event_summary() {
        let event = SessionEvent::MessageReceived {
            content: "Hello world".into(),
            participant_id: "user".into(),
        };
        let summary = LoggingHandler::event_summary(&event, 100);
        assert!(summary.contains("from=user"));
        assert!(summary.contains("content_len=11"));

        let path = test_path("test.txt");
        let event = SessionEvent::ToolCalled {
            name: "read_file".into(),
            args: json!({"path": path.to_string_lossy()}),
        };
        let summary = LoggingHandler::event_summary(&event, 100);
        assert!(summary.contains("tool=read_file"));
        assert!(summary.contains("args_size="));

        let event = SessionEvent::ToolCompleted {
            name: "read_file".into(),
            result: "file contents".into(),
            error: Some("permission denied".into()),
        };
        let summary = LoggingHandler::event_summary(&event, 100);
        assert!(summary.contains("tool=read_file"));
        assert!(summary.contains("error=true"));
    }

    #[test]
    fn test_event_payload() {
        let event = SessionEvent::MessageReceived {
            content: "Hello world".into(),
            participant_id: "user".into(),
        };
        let payload = LoggingHandler::event_payload(&event, 100);
        assert_eq!(payload, Some("Hello world".to_string()));

        let event = SessionEvent::SessionStarted {
            config: SessionEventConfig::new("test"),
        };
        let payload = LoggingHandler::event_payload(&event, 100);
        assert!(payload.is_none());
    }

    #[test]
    fn test_event_payload_truncation() {
        let long_content = "x".repeat(500);
        let event = SessionEvent::MessageReceived {
            content: long_content.clone(),
            participant_id: "user".into(),
        };
        let payload = LoggingHandler::event_payload(&event, 100);
        assert!(payload.is_some());
        let p = payload.unwrap();
        assert!(p.len() <= 100);
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hello");
        assert_eq!(truncate("", 10), "");

        // Test with unicode - should not panic
        let unicode = "héllo wörld";
        let truncated = truncate(unicode, 5);
        assert!(truncated.len() <= 5);
    }

    #[test]
    fn test_logging_handler_debug() {
        let handler = LoggingHandler::new("debug_test", LogLevel::Info);
        let debug_str = format!("{:?}", handler);
        assert!(debug_str.contains("LoggingHandler"));
        assert!(debug_str.contains("debug_test"));
    }

    #[tokio::test]
    async fn test_logging_handler_handle() {
        let handler = LoggingHandler::new("test", LogLevel::Debug);
        let mut ctx = RingHandlerContext::new();
        let event = Arc::new(SessionEvent::MessageReceived {
            content: "test message".into(),
            participant_id: "user".into(),
        });

        // Should not error
        let result = handler.handle(&mut ctx, event, 42).await;
        assert!(result.is_ok());

        // Context should be unchanged (logging is side-effect only)
        assert_eq!(ctx.emitted_count(), 0);
        assert!(!ctx.is_cancelled());
    }

    #[tokio::test]
    async fn test_logging_handler_lifecycle() {
        let handler = LoggingHandler::new("lifecycle_test", LogLevel::Info);

        // on_register should succeed
        let result = handler.on_register().await;
        assert!(result.is_ok());

        // on_unregister should succeed
        let result = handler.on_unregister().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_logging_handler_with_filter() {
        let config = LoggingConfig::with_level(LogLevel::Info)
            .filter(EventFilter::Only(vec!["ToolCalled".to_string()]));
        let handler = LoggingHandler::with_config("filtered", config);

        let mut ctx = RingHandlerContext::new();

        // ToolCalled should be logged (passes filter)
        let event = Arc::new(SessionEvent::ToolCalled {
            name: "test".into(),
            args: json!({}),
        });
        let result = handler.handle(&mut ctx, event, 1).await;
        assert!(result.is_ok());

        // MessageReceived should be skipped (doesn't pass filter, but handle still succeeds)
        let event = Arc::new(SessionEvent::MessageReceived {
            content: "test".into(),
            participant_id: "user".into(),
        });
        let result = handler.handle(&mut ctx, event, 2).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_logging_handler_all_event_types() {
        let handler = LoggingHandler::new("all_events", LogLevel::Trace);
        let mut ctx = RingHandlerContext::new();

        // Test all event variants
        let events: Vec<SessionEvent> = vec![
            SessionEvent::MessageReceived {
                content: "test".into(),
                participant_id: "user".into(),
            },
            SessionEvent::AgentResponded {
                content: "response".into(),
                tool_calls: vec![],
            },
            SessionEvent::AgentThinking {
                thought: "thinking...".into(),
            },
            SessionEvent::ToolCalled {
                name: "tool".into(),
                args: json!({"key": "value"}),
            },
            SessionEvent::ToolCompleted {
                name: "tool".into(),
                result: "done".into(),
                error: None,
            },
            SessionEvent::SessionStarted {
                config: SessionEventConfig::new("test"),
            },
            SessionEvent::SessionCompacted {
                summary: "summary".into(),
                new_file: test_path("new"),
            },
            SessionEvent::SessionEnded {
                reason: "test done".into(),
            },
            SessionEvent::SubagentSpawned {
                id: "sub1".into(),
                prompt: "do something".into(),
            },
            SessionEvent::SubagentCompleted {
                id: "sub1".into(),
                result: "completed".into(),
            },
            SessionEvent::SubagentFailed {
                id: "sub1".into(),
                error: "failed".into(),
            },
            SessionEvent::Custom {
                name: "custom".into(),
                payload: json!({"custom": true}),
            },
        ];

        for (seq, event) in events.into_iter().enumerate() {
            let result = handler.handle(&mut ctx, Arc::new(event), seq as u64).await;
            assert!(result.is_ok());
        }
    }
}
