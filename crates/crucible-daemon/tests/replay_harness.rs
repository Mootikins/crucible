//! Compact test fixture helper for creating synthetic JSONL recording files
//!
//! Provides `create_test_recording()` and convenience event constructors
//! for building replay test fixtures without manual JSON serialization.

use chrono::{Duration, Utc};
use crucible_daemon::{RecordedEvent, RecordingFooter, RecordingHeader};
use serde_json::{json, Value};
use tempfile::NamedTempFile;

/// Create a test recording file with given events
///
/// Returns a `TempPath` that persists for the test lifetime.
/// Generates monotonic timestamps and sequence numbers automatically.
pub fn create_test_recording(session_id: &str, events: Vec<(String, Value)>) -> tempfile::TempPath {
    let temp = NamedTempFile::new().expect("create temp file");
    let path = temp.path().to_path_buf();

    let base_time = Utc::now();
    let mut lines = Vec::new();

    // Header
    let header = RecordingHeader {
        version: 1,
        session_id: session_id.to_string(),
        recording_mode: "granular".to_string(),
        started_at: base_time,
        terminal_size: None,
    };
    lines.push(serde_json::to_string(&header).expect("serialize header"));

    // Events with monotonic timestamps and sequence numbers
    for (idx, (event_type, data)) in events.iter().enumerate() {
        let recorded = RecordedEvent {
            ts: base_time + Duration::milliseconds((idx as i64) * 100),
            seq: (idx as u64) + 1,
            event: event_type.clone(),
            session_id: session_id.to_string(),
            data: data.clone(),
        };
        lines.push(serde_json::to_string(&recorded).expect("serialize event"));
    }

    // Footer
    let footer = RecordingFooter {
        ended_at: base_time + Duration::milliseconds((events.len() as i64) * 100),
        total_events: events.len() as u64,
        duration_ms: (events.len() as u64) * 100,
    };
    lines.push(serde_json::to_string(&footer).expect("serialize footer"));

    // Write to file
    let content = format!("{}\n", lines.join("\n"));
    std::fs::write(&path, content).expect("write recording file");

    temp.into_temp_path()
}

// Convenience constructors for common event types
pub fn text_delta(content: &str) -> (String, Value) {
    ("text_delta".to_string(), json!({"content": content}))
}

pub fn user_message(content: &str) -> (String, Value) {
    ("user_message".to_string(), json!({"content": content}))
}

pub fn message_complete() -> (String, Value) {
    ("message_complete".to_string(), json!({}))
}

pub fn tool_call(name: &str, args: Value) -> (String, Value) {
    (
        "tool_call".to_string(),
        json!({"name": name, "arguments": args}),
    )
}

pub fn tool_result(name: &str, result: Value) -> (String, Value) {
    (
        "tool_result".to_string(),
        json!({"name": name, "result": result}),
    )
}

pub fn thinking(content: &str) -> (String, Value) {
    ("thinking".to_string(), json!({"content": content}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_daemon::replay::ReplaySession;
    use tokio::sync::broadcast;

    #[test]
    fn test_create_recording_with_events() {
        let events = vec![
            text_delta("hello"),
            text_delta(" world"),
            message_complete(),
        ];
        let path = create_test_recording("test-session", events);

        // Verify file exists and has correct line count
        let content = std::fs::read_to_string(&path).expect("read file");
        let lines: Vec<&str> = content.lines().collect();

        // Should have: header + 3 events + footer = 5 lines
        assert_eq!(
            lines.len(),
            5,
            "Expected 5 lines (header + 3 events + footer)"
        );

        // Verify header is valid JSON
        let header: RecordingHeader = serde_json::from_str(lines[0]).expect("parse header");
        assert_eq!(header.session_id, "test-session");
        assert_eq!(header.version, 1);
    }

    #[test]
    fn test_replay_session_accepts_fixture() {
        let events = vec![text_delta("test"), message_complete()];
        let path = create_test_recording("replay-test", events);

        let (tx, _rx) = broadcast::channel(16);
        let result = ReplaySession::new(path.to_path_buf(), 0.0, tx, "replay-test".to_string());

        assert!(result.is_ok(), "ReplaySession should accept fixture");
    }

    #[test]
    fn test_create_recording_with_empty_events() {
        let path = create_test_recording("empty-session", vec![]);

        let content = std::fs::read_to_string(&path).expect("read file");
        let lines: Vec<&str> = content.lines().collect();

        // Should have: header + footer = 2 lines
        assert_eq!(lines.len(), 2, "Expected 2 lines (header + footer)");
    }
}
