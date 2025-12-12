//! Tracing utilities for ACP observability
//!
//! This module provides structured logging and tracing infrastructure for
//! debugging, testing, and monitoring ACP operations.
//!
//! ## Design (from openspec)
//!
//! - **Trace IDs**: Unique ID per session/request for correlation
//! - **Span Context**: Automatic span creation for key operations
//! - **Test Utilities**: Log capture and assertion helpers
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_acp::tracing_utils::{TraceContext, init_test_subscriber};
//!
//! // Create a trace context for a session
//! let ctx = TraceContext::new_session();
//! let span = ctx.session_span();
//! let _guard = span.enter();
//!
//! // In tests, capture logs
//! let (subscriber, logs) = init_test_subscriber();
//! tracing::subscriber::with_default(subscriber, || {
//!     // ... run test code ...
//! });
//! assert!(logs.contains("expected message"));
//! ```

use std::sync::{Arc, Mutex};
use tracing::{span, Level, Span};
use uuid::Uuid;

// ============================================================================
// Trace Context
// ============================================================================

/// Trace context for correlating logs across operations
///
/// Provides unique trace IDs for sessions and requests, enabling log correlation
/// across async boundaries and tool calls.
///
/// # Example
///
/// ```rust
/// use crucible_acp::tracing_utils::TraceContext;
///
/// let ctx = TraceContext::new_session();
/// println!("Session trace: {}", ctx.session_id());
///
/// let request_ctx = ctx.new_request();
/// println!("Request trace: {}", request_ctx.request_id().unwrap());
/// ```
#[derive(Debug, Clone)]
pub struct TraceContext {
    /// Session-level trace ID (persists across requests)
    session_id: String,

    /// Request-level trace ID (unique per request)
    request_id: Option<String>,

    /// Parent trace chain for nested operations
    parent_chain: Vec<String>,
}

impl TraceContext {
    /// Create a new session-level trace context
    pub fn new_session() -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            request_id: None,
            parent_chain: Vec::new(),
        }
    }

    /// Create a new session-level trace context with a specific ID
    pub fn with_session_id(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            request_id: None,
            parent_chain: Vec::new(),
        }
    }

    /// Create a new request context within this session
    pub fn new_request(&self) -> Self {
        let mut parent_chain = self.parent_chain.clone();
        if let Some(ref req_id) = self.request_id {
            parent_chain.push(req_id.clone());
        }

        Self {
            session_id: self.session_id.clone(),
            request_id: Some(Uuid::new_v4().to_string()),
            parent_chain,
        }
    }

    /// Create a child context (for nested operations like subagents)
    pub fn child(&self) -> Self {
        let mut parent_chain = self.parent_chain.clone();
        if let Some(ref req_id) = self.request_id {
            parent_chain.push(req_id.clone());
        }

        Self {
            session_id: self.session_id.clone(),
            request_id: Some(Uuid::new_v4().to_string()),
            parent_chain,
        }
    }

    /// Get the session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Get the request ID (if set)
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    /// Get the parent chain
    pub fn parent_chain(&self) -> &[String] {
        &self.parent_chain
    }

    /// Create a tracing span for this session
    pub fn session_span(&self) -> Span {
        span!(
            Level::INFO,
            "acp_session",
            session_id = %self.session_id,
        )
    }

    /// Create a tracing span for a request
    pub fn request_span(&self, operation: &str) -> Span {
        if let Some(ref req_id) = self.request_id {
            span!(
                Level::INFO,
                "acp_request",
                session_id = %self.session_id,
                request_id = %req_id,
                operation = %operation,
            )
        } else {
            span!(
                Level::INFO,
                "acp_request",
                session_id = %self.session_id,
                operation = %operation,
            )
        }
    }

    /// Create a tracing span for a tool call
    pub fn tool_span(&self, tool_name: &str, tool_id: Option<&str>) -> Span {
        let req_id = self.request_id.as_deref().unwrap_or("none");
        let tool_id = tool_id.unwrap_or("none");

        span!(
            Level::DEBUG,
            "tool_call",
            session_id = %self.session_id,
            request_id = %req_id,
            tool_name = %tool_name,
            tool_id = %tool_id,
        )
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::new_session()
    }
}

// ============================================================================
// Test Utilities
// ============================================================================

/// Captured log storage for testing
///
/// Thread-safe storage for captured log messages during tests.
#[derive(Debug, Clone, Default)]
pub struct LogCapture {
    logs: Arc<Mutex<Vec<CapturedLog>>>,
}

/// A single captured log entry
#[derive(Debug, Clone)]
pub struct CapturedLog {
    /// Log level
    pub level: String,
    /// Log message
    pub message: String,
    /// Target (module path)
    pub target: String,
    /// Span fields
    pub fields: std::collections::HashMap<String, String>,
}

impl LogCapture {
    /// Create a new log capture
    pub fn new() -> Self {
        Self {
            logs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Add a log entry
    pub fn push(&self, log: CapturedLog) {
        if let Ok(mut logs) = self.logs.lock() {
            logs.push(log);
        }
    }

    /// Get all captured logs
    pub fn logs(&self) -> Vec<CapturedLog> {
        self.logs.lock().map(|l| l.clone()).unwrap_or_default()
    }

    /// Check if any log contains the given substring
    pub fn contains(&self, substring: &str) -> bool {
        self.logs()
            .iter()
            .any(|log| log.message.contains(substring))
    }

    /// Check if any log at the given level contains the substring
    pub fn contains_at_level(&self, level: &str, substring: &str) -> bool {
        self.logs()
            .iter()
            .any(|log| log.level == level && log.message.contains(substring))
    }

    /// Find logs matching a predicate
    pub fn find<F>(&self, predicate: F) -> Vec<CapturedLog>
    where
        F: Fn(&CapturedLog) -> bool,
    {
        self.logs().into_iter().filter(predicate).collect()
    }

    /// Get logs containing a specific field value
    pub fn with_field(&self, field: &str, value: &str) -> Vec<CapturedLog> {
        self.find(|log| log.fields.get(field).map(|v| v == value).unwrap_or(false))
    }

    /// Get logs for a specific session
    pub fn for_session(&self, session_id: &str) -> Vec<CapturedLog> {
        self.with_field("session_id", session_id)
    }

    /// Clear all captured logs
    pub fn clear(&self) {
        if let Ok(mut logs) = self.logs.lock() {
            logs.clear();
        }
    }

    /// Get the count of captured logs
    pub fn len(&self) -> usize {
        self.logs.lock().map(|l| l.len()).unwrap_or(0)
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Custom tracing layer for capturing logs in tests
#[cfg(any(test, feature = "test-utils"))]
pub mod test_subscriber {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{fmt, EnvFilter, Layer};

    /// A simple log capturing layer for tests
    pub struct LogCaptureLayer {
        capture: LogCapture,
    }

    impl LogCaptureLayer {
        /// Create a new capture layer
        pub fn new(capture: LogCapture) -> Self {
            Self { capture }
        }
    }

    impl<S> Layer<S> for LogCaptureLayer
    where
        S: tracing::Subscriber,
    {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let mut visitor = FieldVisitor::default();
            event.record(&mut visitor);

            let log = CapturedLog {
                level: event.metadata().level().to_string(),
                message: visitor.message,
                target: event.metadata().target().to_string(),
                fields: visitor.fields,
            };

            self.capture.push(log);
        }
    }

    /// Field visitor for extracting log data
    #[derive(Default)]
    struct FieldVisitor {
        message: String,
        fields: std::collections::HashMap<String, String>,
    }

    impl tracing::field::Visit for FieldVisitor {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            let value_str = format!("{:?}", value);
            if field.name() == "message" {
                self.message = value_str;
            } else {
                self.fields.insert(field.name().to_string(), value_str);
            }
        }

        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            if field.name() == "message" {
                self.message = value.to_string();
            } else {
                self.fields
                    .insert(field.name().to_string(), value.to_string());
            }
        }
    }

    /// Initialize a test subscriber that captures logs
    ///
    /// Returns a LogCapture that can be used to inspect captured logs.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let capture = init_test_subscriber();
    /// tracing::info!("test message");
    /// assert!(capture.contains("test message"));
    /// ```
    pub fn init_test_subscriber() -> LogCapture {
        let capture = LogCapture::new();
        let capture_layer = LogCaptureLayer::new(capture.clone());

        let subscriber = tracing_subscriber::registry()
            .with(capture_layer)
            .with(
                fmt::layer()
                    .with_test_writer()
                    .with_ansi(false)
                    .without_time(),
            )
            .with(EnvFilter::from_default_env().add_directive(tracing::Level::TRACE.into()));

        // Try to set the default subscriber (may fail if already set)
        let _ = subscriber.try_init();

        capture
    }

    /// Create a subscriber and capture for use with `with_default`
    ///
    /// This returns both the subscriber and capture, allowing scoped usage.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let (subscriber, capture) = create_test_subscriber();
    /// tracing::subscriber::with_default(subscriber, || {
    ///     tracing::info!("test message");
    /// });
    /// assert!(capture.contains("test message"));
    /// ```
    pub fn create_test_subscriber() -> (impl tracing::Subscriber, LogCapture) {
        let capture = LogCapture::new();
        let capture_layer = LogCaptureLayer::new(capture.clone());

        let subscriber = tracing_subscriber::registry()
            .with(capture_layer)
            .with(EnvFilter::from_default_env().add_directive(tracing::Level::TRACE.into()));

        (subscriber, capture)
    }
}

#[cfg(any(test, feature = "test-utils"))]
pub use test_subscriber::{create_test_subscriber, init_test_subscriber};

// ============================================================================
// Instrumentation Helpers
// ============================================================================

/// Macro for creating a span with trace context
///
/// # Example
///
/// ```rust,ignore
/// let ctx = TraceContext::new_session();
/// trace_span!(ctx, "operation_name", extra_field = "value");
/// ```
#[macro_export]
macro_rules! trace_span {
    ($ctx:expr, $name:expr) => {
        tracing::span!(
            tracing::Level::INFO,
            $name,
            session_id = %$ctx.session_id(),
            request_id = %$ctx.request_id().unwrap_or("none"),
        )
    };
    ($ctx:expr, $name:expr, $($field:tt)*) => {
        tracing::span!(
            tracing::Level::INFO,
            $name,
            session_id = %$ctx.session_id(),
            request_id = %$ctx.request_id().unwrap_or("none"),
            $($field)*
        )
    };
}

/// Log an event with trace context
#[macro_export]
macro_rules! trace_event {
    ($ctx:expr, $level:ident, $($arg:tt)*) => {
        tracing::$level!(
            session_id = %$ctx.session_id(),
            request_id = %$ctx.request_id().unwrap_or("none"),
            $($arg)*
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_context_creation() {
        let ctx = TraceContext::new_session();
        assert!(!ctx.session_id().is_empty());
        assert!(ctx.request_id().is_none());
        assert!(ctx.parent_chain().is_empty());
    }

    #[test]
    fn test_trace_context_with_session_id() {
        let ctx = TraceContext::with_session_id("test-session-123");
        assert_eq!(ctx.session_id(), "test-session-123");
    }

    #[test]
    fn test_trace_context_new_request() {
        let session_ctx = TraceContext::new_session();
        let request_ctx = session_ctx.new_request();

        assert_eq!(request_ctx.session_id(), session_ctx.session_id());
        assert!(request_ctx.request_id().is_some());
        assert!(request_ctx.parent_chain().is_empty());
    }

    #[test]
    fn test_trace_context_child() {
        let session_ctx = TraceContext::new_session();
        let request_ctx = session_ctx.new_request();
        let child_ctx = request_ctx.child();

        assert_eq!(child_ctx.session_id(), session_ctx.session_id());
        assert!(child_ctx.request_id().is_some());
        assert_ne!(child_ctx.request_id(), request_ctx.request_id());
        assert_eq!(child_ctx.parent_chain().len(), 1);
        assert_eq!(
            child_ctx.parent_chain()[0],
            request_ctx.request_id().unwrap()
        );
    }

    #[test]
    fn test_trace_context_spans() {
        // Test that span creation methods produce valid spans
        // Note: Spans may be disabled if no subscriber is set, so we just
        // verify the methods don't panic and return spans with expected metadata
        let ctx = TraceContext::new_session();

        // Create spans - they may be disabled without a subscriber, but should not panic
        let session_span = ctx.session_span();
        let _ = session_span.id(); // Access span to verify it's valid

        let request_ctx = ctx.new_request();
        let request_span = request_ctx.request_span("test_operation");
        let _ = request_span.id();

        let tool_span = request_ctx.tool_span("test_tool", Some("tool-123"));
        let _ = tool_span.id();

        // Verify the spans have the expected names via metadata
        assert_eq!(
            session_span.metadata().map(|m| m.name()),
            Some("acp_session")
        );
        assert_eq!(
            request_span.metadata().map(|m| m.name()),
            Some("acp_request")
        );
        assert_eq!(tool_span.metadata().map(|m| m.name()), Some("tool_call"));
    }

    #[test]
    fn test_log_capture() {
        let capture = LogCapture::new();
        assert!(capture.is_empty());

        capture.push(CapturedLog {
            level: "INFO".to_string(),
            message: "test message".to_string(),
            target: "test".to_string(),
            fields: std::collections::HashMap::new(),
        });

        assert_eq!(capture.len(), 1);
        assert!(capture.contains("test message"));
        assert!(!capture.contains("other message"));
    }

    #[test]
    fn test_log_capture_with_fields() {
        let capture = LogCapture::new();

        let mut fields = std::collections::HashMap::new();
        fields.insert("session_id".to_string(), "sess-123".to_string());

        capture.push(CapturedLog {
            level: "INFO".to_string(),
            message: "session event".to_string(),
            target: "test".to_string(),
            fields,
        });

        let session_logs = capture.for_session("sess-123");
        assert_eq!(session_logs.len(), 1);
        assert_eq!(session_logs[0].message, "session event");
    }

    #[test]
    fn test_log_capture_level_filter() {
        let capture = LogCapture::new();

        capture.push(CapturedLog {
            level: "INFO".to_string(),
            message: "info message".to_string(),
            target: "test".to_string(),
            fields: std::collections::HashMap::new(),
        });

        capture.push(CapturedLog {
            level: "ERROR".to_string(),
            message: "error message".to_string(),
            target: "test".to_string(),
            fields: std::collections::HashMap::new(),
        });

        assert!(capture.contains_at_level("INFO", "info"));
        assert!(capture.contains_at_level("ERROR", "error"));
        assert!(!capture.contains_at_level("INFO", "error"));
    }

    #[test]
    fn test_log_capture_clear() {
        let capture = LogCapture::new();

        capture.push(CapturedLog {
            level: "INFO".to_string(),
            message: "test".to_string(),
            target: "test".to_string(),
            fields: std::collections::HashMap::new(),
        });

        assert!(!capture.is_empty());
        capture.clear();
        assert!(capture.is_empty());
    }

    #[cfg(feature = "test-utils")]
    #[test]
    fn test_subscriber_captures_logs() {
        let (subscriber, capture) = create_test_subscriber();

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("captured log message");
        });

        assert!(capture.contains("captured log message"));
    }
}
