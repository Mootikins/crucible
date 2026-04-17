//! Recording types for session-level granular event capture
//!
//! Shared types used by the daemon's recording writer and the CLI's local
//! replay driver. The types live in core so that the CLI can read recordings
//! without pulling in the daemon.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::protocol::SessionEventMessage;

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
