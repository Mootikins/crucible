//! Recording types for session-level granular event capture
//!
//! Defines the wrapper types for recording session events to `recording.jsonl`:
//! - `RecordingHeader`: metadata at start of file
//! - `RecordedEvent`: individual event wrapper with timestamp and sequence
//! - `RecordingFooter`: summary at end of file

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crucible_core::protocol::SessionEventMessage;

/// Header metadata for a recording file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingHeader {
    /// Recording format version
    pub version: u32,
    /// Session ID this recording belongs to
    pub session_id: String,
    /// Recording mode (e.g., "granular", "coarse")
    pub recording_mode: String,
    /// When recording started
    pub started_at: DateTime<Utc>,
    /// Terminal size at recording start, if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_size: Option<(u16, u16)>,
}

/// A recorded event with timestamp and sequence number
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedEvent {
    /// When the event occurred
    pub ts: DateTime<Utc>,
    /// Sequence number within this recording
    pub seq: u64,
    /// Event type name (e.g., "text_delta", "user_message")
    pub event: String,
    /// Session ID
    pub session_id: String,
    /// Event data payload
    pub data: Value,
}

impl RecordedEvent {
    /// Convert a SessionEventMessage to a RecordedEvent
    ///
    /// Maps:
    /// - `msg.event` → `event`
    /// - `msg.session_id` → `session_id`
    /// - `msg.data` → `data`
    /// - `ts` = `Utc::now()` (granular writer controls actual timing)
    /// - `seq` = provided sequence number
    pub fn from_session_event(msg: &SessionEventMessage, seq: u64) -> Self {
        Self {
            ts: Utc::now(),
            seq,
            event: msg.event.clone(),
            session_id: msg.session_id.clone(),
            data: msg.data.clone(),
        }
    }
}

/// Footer metadata for a recording file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingFooter {
    /// When recording ended
    pub ended_at: DateTime<Utc>,
    /// Total number of events recorded
    pub total_events: u64,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recording_header_roundtrip() {
        let header = RecordingHeader {
            version: 1,
            session_id: "session-123".to_string(),
            recording_mode: "granular".to_string(),
            started_at: Utc::now(),
            terminal_size: Some((80, 24)),
        };

        let json = serde_json::to_string(&header).expect("serialize");
        let deserialized: RecordingHeader = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.version, 1);
        assert_eq!(deserialized.session_id, "session-123");
        assert_eq!(deserialized.recording_mode, "granular");
        assert_eq!(deserialized.terminal_size, Some((80, 24)));
    }

    #[test]
    fn test_recorded_event_roundtrip() {
        let now = Utc::now();
        let event = RecordedEvent {
            ts: now,
            seq: 42,
            event: "text_delta".to_string(),
            session_id: "session-456".to_string(),
            data: serde_json::json!({ "content": "hello" }),
        };

        let json = serde_json::to_string(&event).expect("serialize");
        let deserialized: RecordedEvent = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.seq, 42);
        assert_eq!(deserialized.event, "text_delta");
        assert_eq!(deserialized.session_id, "session-456");
        assert_eq!(deserialized.data, serde_json::json!({ "content": "hello" }));
    }

    #[test]
    fn test_recording_footer_roundtrip() {
        let footer = RecordingFooter {
            ended_at: Utc::now(),
            total_events: 100,
            duration_ms: 5000,
        };

        let json = serde_json::to_string(&footer).expect("serialize");
        let deserialized: RecordingFooter = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.total_events, 100);
        assert_eq!(deserialized.duration_ms, 5000);
    }

    #[test]
    fn test_from_session_event() {
        let msg = SessionEventMessage::text_delta("session-1", "hello");
        let recorded = RecordedEvent::from_session_event(&msg, 5);

        assert_eq!(recorded.event, "text_delta");
        assert_eq!(recorded.seq, 5);
        assert_eq!(recorded.session_id, "session-1");
        assert_eq!(recorded.data, serde_json::json!({ "content": "hello" }));
    }

    #[test]
    fn test_terminal_size_optional() {
        // With terminal_size: None
        let header_none = RecordingHeader {
            version: 1,
            session_id: "session-789".to_string(),
            recording_mode: "granular".to_string(),
            started_at: Utc::now(),
            terminal_size: None,
        };

        let json_none = serde_json::to_string(&header_none).expect("serialize");
        assert!(!json_none.contains("terminal_size"));

        // With terminal_size: Some
        let header_some = RecordingHeader {
            version: 1,
            session_id: "session-789".to_string(),
            recording_mode: "granular".to_string(),
            started_at: Utc::now(),
            terminal_size: Some((120, 40)),
        };

        let json_some = serde_json::to_string(&header_some).expect("serialize");
        assert!(json_some.contains("terminal_size"));

        let deserialized: RecordingHeader = serde_json::from_str(&json_some).expect("deserialize");
        assert_eq!(deserialized.terminal_size, Some((120, 40)));
    }
}
