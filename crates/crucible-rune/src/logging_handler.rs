//! Logging handler for the event system.
//!
//! This module provides a `LoggingHandler` that logs all events passing through
//! the handler pipeline. It's useful for debugging, auditing, and tracing
//! event flow through the system.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_rune::logging_handler::{LoggingHandler, LogLevel};
//! use crucible_rune::handler_chain::SessionHandlerChain;
//!
//! let logger = LoggingHandler::new("event_logger", LogLevel::Info);
//! let mut chain = SessionHandlerChain::new();
//! chain.add_handler(Box::new(logger)).unwrap();
//! ```

use async_trait::async_trait;

use crate::handler::{Handler, HandlerContext, HandlerResult};
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

    fn log_event(&self, event: &SessionEvent) {
        let event_type = event.type_name();

        if !self.config.filter.should_log(event_type) {
            return;
        }

        let summary = event.summary(self.config.max_payload_length);
        let prefix = self
            .config
            .prefix
            .as_deref()
            .map(|p| format!("[{}] ", p))
            .unwrap_or_default();

        let payload_str = if self.config.include_payload {
            event
                .payload(self.config.max_payload_length)
                .map(|p| format!(" | payload={}", p))
                .unwrap_or_default()
        } else {
            String::new()
        };

        match self.config.level {
            LogLevel::Trace => {
                tracing::trace!(
                    handler = %self.name,
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
impl Handler for LoggingHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn dependencies(&self) -> &[&str] {
        &self.dependencies
    }

    fn priority(&self) -> i32 {
        10
    }

    fn event_pattern(&self) -> &str {
        "*"
    }

    async fn handle(
        &self,
        _ctx: &mut HandlerContext,
        event: SessionEvent,
    ) -> HandlerResult<SessionEvent> {
        self.log_event(&event);
        HandlerResult::ok(event)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::Handler;
    use crate::reactor::SessionEventConfig;
    use serde_json::json;
    use std::path::PathBuf;

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
        assert_eq!(Handler::name(&handler), "test_logger");
        assert!(handler.dependencies.is_empty());
        assert_eq!(handler.config.level, LogLevel::Debug);
    }

    #[test]
    fn test_logging_handler_with_config() {
        let config = LoggingConfig::with_level(LogLevel::Warn).prefix("TEST");
        let handler = LoggingHandler::with_config("custom_logger", config);
        assert_eq!(Handler::name(&handler), "custom_logger");
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
        // These tests now use the SessionEvent::type_name method from crucible-core
        let event = SessionEvent::MessageReceived {
            content: "test".into(),
            participant_id: "user".into(),
        };
        assert_eq!(event.type_name(), "MessageReceived");

        let event = SessionEvent::ToolCalled {
            name: "tool".into(),
            args: json!({}),
        };
        assert_eq!(event.type_name(), "ToolCalled");

        let event = SessionEvent::SessionEnded {
            reason: "done".into(),
        };
        assert_eq!(event.type_name(), "SessionEnded");
    }

    #[test]
    fn test_event_summary() {
        // These tests now use the SessionEvent::summary method from crucible-core
        let event = SessionEvent::MessageReceived {
            content: "Hello world".into(),
            participant_id: "user".into(),
        };
        let summary = event.summary(100);
        assert!(summary.contains("from=user"));
        assert!(summary.contains("content_len=11"));

        let path = test_path("test.txt");
        let event = SessionEvent::ToolCalled {
            name: "read_file".into(),
            args: json!({"path": path.to_string_lossy()}),
        };
        let summary = event.summary(100);
        assert!(summary.contains("tool=read_file"));
        assert!(summary.contains("args_size="));

        let event = SessionEvent::ToolCompleted {
            name: "read_file".into(),
            result: "file contents".into(),
            error: Some("permission denied".into()),
        };
        let summary = event.summary(100);
        assert!(summary.contains("tool=read_file"));
        assert!(summary.contains("error=true"));
    }

    #[test]
    fn test_event_payload() {
        // These tests now use the SessionEvent::payload method from crucible-core
        let event = SessionEvent::MessageReceived {
            content: "Hello world".into(),
            participant_id: "user".into(),
        };
        let payload = event.payload(100);
        assert_eq!(payload, Some("Hello world".to_string()));

        let event = SessionEvent::SessionStarted {
            config: SessionEventConfig::new("test"),
        };
        let payload = event.payload(100);
        assert!(payload.is_none());
    }

    #[test]
    fn test_event_payload_truncation() {
        // These tests now use the SessionEvent::payload method from crucible-core
        let long_content = "x".repeat(500);
        let event = SessionEvent::MessageReceived {
            content: long_content.clone(),
            participant_id: "user".into(),
        };
        let payload = event.payload(100);
        assert!(payload.is_some());
        let p = payload.unwrap();
        assert!(p.len() <= 100);
    }

    // Note: test_truncate removed - truncate function now lives in crucible-core

    #[test]
    fn test_logging_handler_debug() {
        let handler = LoggingHandler::new("debug_test", LogLevel::Info);
        let debug_str = format!("{:?}", handler);
        assert!(debug_str.contains("LoggingHandler"));
        assert!(debug_str.contains("debug_test"));
    }

    #[tokio::test]
    async fn test_handler_handle() {
        let handler = LoggingHandler::new("test", LogLevel::Debug);
        let mut ctx = HandlerContext::new();
        let event = SessionEvent::MessageReceived {
            content: "test message".into(),
            participant_id: "user".into(),
        };

        let result = Handler::handle(&handler, &mut ctx, event).await;
        assert!(result.is_continue());
    }

    #[tokio::test]
    async fn test_handler_with_filter() {
        let config = LoggingConfig::with_level(LogLevel::Info)
            .filter(EventFilter::Only(vec!["ToolCalled".to_string()]));
        let handler = LoggingHandler::with_config("filtered", config);

        let mut ctx = HandlerContext::new();

        let event = SessionEvent::ToolCalled {
            name: "test".into(),
            args: json!({}),
        };
        let result = Handler::handle(&handler, &mut ctx, event).await;
        assert!(result.is_continue());

        let event = SessionEvent::MessageReceived {
            content: "test".into(),
            participant_id: "user".into(),
        };
        let result = Handler::handle(&handler, &mut ctx, event).await;
        assert!(result.is_continue());
    }

    #[tokio::test]
    async fn test_logging_handler_all_event_types() {
        let handler = LoggingHandler::new("all_events", LogLevel::Trace);
        let mut ctx = HandlerContext::new();

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

        for event in events {
            let result = Handler::handle(&handler, &mut ctx, event).await;
            assert!(result.is_continue());
        }
    }
}
