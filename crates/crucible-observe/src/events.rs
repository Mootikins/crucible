//! Session log events for JSONL persistence
//!
//! These events are for session persistence/resume, separate from
//! `crucible_core::events::SessionEvent` which is for Reactor dispatch.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    /// Input tokens consumed
    #[serde(rename = "in")]
    pub input: u32,
    /// Output tokens generated
    #[serde(rename = "out")]
    pub output: u32,
}

/// A single event in the session log
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LogEvent {
    /// System message (prompt, context injection)
    System { ts: DateTime<Utc>, content: String },

    /// User message
    User { ts: DateTime<Utc>, content: String },

    /// Assistant response (final, not streaming chunks)
    Assistant {
        ts: DateTime<Utc>,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tokens: Option<TokenUsage>,
    },

    /// Tool invocation
    ToolCall {
        ts: DateTime<Utc>,
        /// Correlation ID for matching with result
        id: String,
        name: String,
        /// Tool arguments as JSON
        args: Value,
    },

    /// Tool execution result
    ToolResult {
        ts: DateTime<Utc>,
        /// Correlation ID matching the ToolCall
        id: String,
        /// Result content (may be truncated for large outputs)
        result: String,
        /// Whether the result was truncated
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        truncated: bool,
        /// Error message if tool failed
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    /// Error during session
    Error {
        ts: DateTime<Utc>,
        message: String,
        /// Whether the error is recoverable
        #[serde(default)]
        recoverable: bool,
    },
}

impl LogEvent {
    /// Create a system event
    pub fn system(content: impl Into<String>) -> Self {
        LogEvent::System {
            ts: Utc::now(),
            content: content.into(),
        }
    }

    /// Create a user message event
    pub fn user(content: impl Into<String>) -> Self {
        LogEvent::User {
            ts: Utc::now(),
            content: content.into(),
        }
    }

    /// Create an assistant message event
    pub fn assistant(content: impl Into<String>) -> Self {
        LogEvent::Assistant {
            ts: Utc::now(),
            content: content.into(),
            model: None,
            tokens: None,
        }
    }

    /// Create an assistant message with model info
    pub fn assistant_with_model(
        content: impl Into<String>,
        model: impl Into<String>,
        tokens: Option<TokenUsage>,
    ) -> Self {
        LogEvent::Assistant {
            ts: Utc::now(),
            content: content.into(),
            model: Some(model.into()),
            tokens,
        }
    }

    /// Create a tool call event
    pub fn tool_call(id: impl Into<String>, name: impl Into<String>, args: Value) -> Self {
        LogEvent::ToolCall {
            ts: Utc::now(),
            id: id.into(),
            name: name.into(),
            args,
        }
    }

    /// Create a tool result event
    pub fn tool_result(id: impl Into<String>, result: impl Into<String>) -> Self {
        LogEvent::ToolResult {
            ts: Utc::now(),
            id: id.into(),
            result: result.into(),
            truncated: false,
            error: None,
        }
    }

    /// Create a tool result event with truncation
    pub fn tool_result_truncated(
        id: impl Into<String>,
        result: impl Into<String>,
        truncated: bool,
    ) -> Self {
        LogEvent::ToolResult {
            ts: Utc::now(),
            id: id.into(),
            result: result.into(),
            truncated,
            error: None,
        }
    }

    /// Create a tool error event
    pub fn tool_error(id: impl Into<String>, error: impl Into<String>) -> Self {
        LogEvent::ToolResult {
            ts: Utc::now(),
            id: id.into(),
            result: String::new(),
            truncated: false,
            error: Some(error.into()),
        }
    }

    /// Create an error event
    pub fn error(message: impl Into<String>, recoverable: bool) -> Self {
        LogEvent::Error {
            ts: Utc::now(),
            message: message.into(),
            recoverable,
        }
    }

    /// Get the timestamp of this event
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            LogEvent::System { ts, .. }
            | LogEvent::User { ts, .. }
            | LogEvent::Assistant { ts, .. }
            | LogEvent::ToolCall { ts, .. }
            | LogEvent::ToolResult { ts, .. }
            | LogEvent::Error { ts, .. } => *ts,
        }
    }

    /// Serialize to JSONL format (single line)
    pub fn to_jsonl(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Parse from JSONL line
    pub fn from_jsonl(line: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_system_event_json() {
        let event = LogEvent::system("You are a helpful assistant");
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"system\""));
        assert!(json.contains("\"content\":\"You are a helpful assistant\""));
        assert!(json.contains("\"ts\":"));

        // Round-trip
        let parsed = LogEvent::from_jsonl(&json).unwrap();
        if let LogEvent::System { content, .. } = parsed {
            assert_eq!(content, "You are a helpful assistant");
        } else {
            panic!("wrong event type");
        }
    }

    #[test]
    fn test_user_event_json() {
        let event = LogEvent::user("Hello");
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"user\""));
        assert!(json.contains("\"content\":\"Hello\""));
    }

    #[test]
    fn test_assistant_event_json() {
        let event = LogEvent::assistant_with_model(
            "Hi there!",
            "claude-3-haiku",
            Some(TokenUsage {
                input: 10,
                output: 5,
            }),
        );
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"assistant\""));
        assert!(json.contains("\"model\":\"claude-3-haiku\""));
        assert!(json.contains("\"in\":10"));
        assert!(json.contains("\"out\":5"));
    }

    #[test]
    fn test_assistant_minimal_json() {
        let event = LogEvent::assistant("Hi!");
        let json = event.to_jsonl().unwrap();

        // Should NOT contain model or tokens when None
        assert!(!json.contains("\"model\""));
        assert!(!json.contains("\"tokens\""));
    }

    #[test]
    fn test_tool_call_json() {
        let event =
            LogEvent::tool_call("tc_001", "read_file", serde_json::json!({"path": "foo.rs"}));
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"tool_call\""));
        assert!(json.contains("\"id\":\"tc_001\""));
        assert!(json.contains("\"name\":\"read_file\""));
        assert!(json.contains("\"path\":\"foo.rs\""));
    }

    #[test]
    fn test_tool_result_json() {
        let event = LogEvent::tool_result("tc_001", "fn main() {}");
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"tool_result\""));
        assert!(json.contains("\"id\":\"tc_001\""));
        assert!(json.contains("\"result\":\"fn main() {}\""));
        // truncated: false should be omitted
        assert!(!json.contains("\"truncated\""));
    }

    #[test]
    fn test_tool_result_truncated_json() {
        let event = LogEvent::tool_result_truncated("tc_001", "...", true);
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"truncated\":true"));
    }

    #[test]
    fn test_tool_error_json() {
        let event = LogEvent::tool_error("tc_001", "File not found");
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"error\":\"File not found\""));
    }

    #[test]
    fn test_error_event_json() {
        let event = LogEvent::error("Rate limited", true);
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("\"message\":\"Rate limited\""));
        assert!(json.contains("\"recoverable\":true"));
    }

    #[test]
    fn test_jsonl_roundtrip() {
        let events = vec![
            LogEvent::system("System prompt"),
            LogEvent::user("Hello"),
            LogEvent::assistant("Hi!"),
            LogEvent::tool_call("t1", "test", serde_json::json!({})),
            LogEvent::tool_result("t1", "result"),
            LogEvent::error("oops", false),
        ];

        for event in events {
            let json = event.to_jsonl().unwrap();
            let parsed = LogEvent::from_jsonl(&json).unwrap();
            let json2 = parsed.to_jsonl().unwrap();
            assert_eq!(json, json2);
        }
    }

    #[test]
    fn test_parse_example_jsonl() {
        // From the spec
        let lines = [
            r#"{"ts":"2026-01-04T15:30:00Z","type":"system","content":"You are a helpful assistant..."}"#,
            r#"{"ts":"2026-01-04T15:30:01Z","type":"user","content":"Hello"}"#,
            r#"{"ts":"2026-01-04T15:30:02Z","type":"assistant","content":"Hi!","model":"claude-3-haiku","tokens":{"in":10,"out":5}}"#,
            r#"{"ts":"2026-01-04T15:30:03Z","type":"tool_call","id":"tc_001","name":"read_file","args":{"path":"foo.rs"}}"#,
            r#"{"ts":"2026-01-04T15:30:04Z","type":"tool_result","id":"tc_001","result":"fn main()...","truncated":false}"#,
            r#"{"ts":"2026-01-04T15:30:05Z","type":"error","message":"Rate limited","recoverable":true}"#,
        ];

        for line in lines {
            let event = LogEvent::from_jsonl(line).unwrap();
            assert!(event.timestamp().year() == 2026);
        }
    }
}
