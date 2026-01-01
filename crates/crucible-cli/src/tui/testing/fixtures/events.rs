//! Event sequence fixtures
//!
//! Pre-built event sequences for testing streaming, tool calls, etc.

use crate::tui::streaming_channel::StreamingEvent;

/// Streaming response broken into word chunks
pub fn streaming_chunks(text: &str) -> Vec<StreamingEvent> {
    let mut events: Vec<StreamingEvent> = text
        .split_inclusive(' ')
        .enumerate()
        .map(|(seq, word)| StreamingEvent::Delta {
            text: word.to_string(),
            seq: seq as u64,
        })
        .collect();

    events.push(StreamingEvent::Done {
        full_response: text.to_string(),
    });
    events
}

/// Character-by-character streaming (for detailed tests)
pub fn streaming_chars(text: &str) -> Vec<StreamingEvent> {
    let mut events: Vec<StreamingEvent> = text
        .chars()
        .enumerate()
        .map(|(seq, c)| StreamingEvent::Delta {
            text: c.to_string(),
            seq: seq as u64,
        })
        .collect();

    events.push(StreamingEvent::Done {
        full_response: text.to_string(),
    });
    events
}

/// Simple tool call event
pub fn tool_call_event(name: impl Into<String>) -> StreamingEvent {
    StreamingEvent::ToolCall {
        id: Some("test-id".to_string()),
        name: name.into(),
        args: serde_json::json!({}),
    }
}

/// Tool call event with arguments
pub fn tool_call_with_args(
    name: impl Into<String>,
    args: serde_json::Value,
) -> StreamingEvent {
    StreamingEvent::ToolCall {
        id: Some("test-id".to_string()),
        name: name.into(),
        args,
    }
}

/// Tool completion event (success)
pub fn tool_completed_event(name: impl Into<String>, result: impl Into<String>) -> StreamingEvent {
    StreamingEvent::ToolCompleted {
        name: name.into(),
        result: result.into(),
        error: None,
    }
}

/// Tool completion event (error)
pub fn tool_error_event(name: impl Into<String>, error: impl Into<String>) -> StreamingEvent {
    StreamingEvent::ToolCompleted {
        name: name.into(),
        result: String::new(),
        error: Some(error.into()),
    }
}

/// Full tool call lifecycle: call -> completion
pub fn tool_lifecycle(
    name: impl Into<String> + Clone,
    args: serde_json::Value,
    result: impl Into<String>,
) -> Vec<StreamingEvent> {
    let name_str: String = name.into();
    vec![
        StreamingEvent::ToolCall {
            id: Some("test-id".to_string()),
            name: name_str.clone(),
            args,
        },
        StreamingEvent::ToolCompleted {
            name: name_str,
            result: result.into(),
            error: None,
        },
    ]
}

/// Multi-tool sequence: simulates agent making multiple tool calls
pub fn multi_tool_sequence() -> Vec<StreamingEvent> {
    vec![
        // First tool: glob for files
        StreamingEvent::ToolCall {
            id: Some("call_1".to_string()),
            name: "glob".to_string(),
            args: serde_json::json!({"pattern": "**/*.rs"}),
        },
        StreamingEvent::ToolCompleted {
            name: "glob".to_string(),
            result: "Found 15 files".to_string(),
            error: None,
        },
        // Second tool: read a file
        StreamingEvent::ToolCall {
            id: Some("call_2".to_string()),
            name: "read_file".to_string(),
            args: serde_json::json!({"path": "src/main.rs", "limit": 50}),
        },
        StreamingEvent::ToolCompleted {
            name: "read_file".to_string(),
            result: "fn main() { ... }".to_string(),
            error: None,
        },
        // Third tool: grep with error
        StreamingEvent::ToolCall {
            id: Some("call_3".to_string()),
            name: "grep".to_string(),
            args: serde_json::json!({"pattern": "[invalid"}),
        },
        StreamingEvent::ToolCompleted {
            name: "grep".to_string(),
            result: String::new(),
            error: Some("Invalid regex pattern".to_string()),
        },
    ]
}

/// Error during streaming
pub fn streaming_error(partial: &str, error: &str) -> Vec<StreamingEvent> {
    let mut events: Vec<StreamingEvent> = partial
        .split_inclusive(' ')
        .enumerate()
        .map(|(seq, word)| StreamingEvent::Delta {
            text: word.to_string(),
            seq: seq as u64,
        })
        .collect();

    events.push(StreamingEvent::Error {
        message: error.to_string(),
    });
    events
}

/// Complete streaming sequence with thinking and response
pub fn full_response(text: &str) -> Vec<StreamingEvent> {
    streaming_chunks(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_chunks_ends_with_done() {
        let events = streaming_chunks("Hello world");
        assert!(matches!(events.last(), Some(StreamingEvent::Done { .. })));
    }

    #[test]
    fn streaming_error_ends_with_error() {
        let events = streaming_error("Partial", "Connection lost");
        assert!(matches!(events.last(), Some(StreamingEvent::Error { .. })));
    }

    #[test]
    fn streaming_chars_has_one_per_char() {
        let events = streaming_chars("Hi");
        // "H", "i", Done = 3 events
        assert_eq!(events.len(), 3);
    }
}
