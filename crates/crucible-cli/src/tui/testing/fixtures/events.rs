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
