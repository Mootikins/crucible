//! Recording writer for session-level granular event capture
//!
//! Handles writing session events to `recording.jsonl` files with header/footer metadata.

use anyhow::Result;
use chrono::{DateTime, Utc};
use crucible_core::protocol::SessionEventMessage;
use crucible_core::session::RecordingMode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::fs::{create_dir_all, OpenOptions};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

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

const FLUSH_EVENT_INTERVAL: u64 = 100;
const FLUSH_TIME_INTERVAL: Duration = Duration::from_millis(500);

pub struct RecordingWriter {
    path: PathBuf,
    session_id: String,
    recording_mode: RecordingMode,
    terminal_size: Option<(u16, u16)>,
    rx: mpsc::Receiver<SessionEventMessage>,
}

impl RecordingWriter {
    pub fn new(
        path: PathBuf,
        session_id: String,
        recording_mode: RecordingMode,
        terminal_size: Option<(u16, u16)>,
    ) -> (Self, mpsc::Sender<SessionEventMessage>) {
        let (tx, rx) = mpsc::channel(4096);
        (
            Self {
                path,
                session_id,
                recording_mode,
                terminal_size,
                rx,
            },
            tx,
        )
    }

    pub fn start(self) -> JoinHandle<Result<()>> {
        tokio::spawn(async move { self.run().await })
    }

    async fn run(mut self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            create_dir_all(parent).await?;
        }

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.path)
            .await?;
        let mut writer = BufWriter::new(file);

        let started_at = Utc::now();
        let start_time = Instant::now();
        let header = RecordingHeader {
            version: 1,
            session_id: self.session_id.clone(),
            recording_mode: self.recording_mode.to_string(),
            started_at,
            terminal_size: self.terminal_size,
        };
        write_json_line(&mut writer, &header).await?;

        let mut total_events = 0_u64;
        let mut events_since_flush = 0_u64;
        let mut flush_interval = tokio::time::interval(FLUSH_TIME_INTERVAL);

        loop {
            tokio::select! {
                maybe_event = self.rx.recv() => {
                    let Some(event) = maybe_event else {
                        break;
                    };

                    total_events += 1;
                    events_since_flush += 1;

                    let recorded = RecordedEvent::from_session_event(&event, total_events);
                    write_json_line(&mut writer, &recorded).await?;

                    if events_since_flush >= FLUSH_EVENT_INTERVAL {
                        writer.flush().await?;
                        events_since_flush = 0;
                    }
                }
                _ = flush_interval.tick(), if events_since_flush > 0 => {
                    writer.flush().await?;
                    events_since_flush = 0;
                }
            }
        }

        let footer = RecordingFooter {
            ended_at: Utc::now(),
            total_events,
            duration_ms: start_time.elapsed().as_millis() as u64,
        };
        write_json_line(&mut writer, &footer).await?;
        writer.flush().await?;
        Ok(())
    }
}

async fn write_json_line<T: serde::Serialize>(
    writer: &mut BufWriter<tokio::fs::File>,
    value: &T,
) -> Result<()> {
    writer
        .write_all(serde_json::to_string(value)?.as_bytes())
        .await?;
    writer.write_all(b"\n").await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    #[tokio::test]
    async fn recording_writer_records_all_event_types_and_monotonic_seq() {
        let dir = TempDir::new().expect("create tempdir");
        let path = dir.path().join("recording.jsonl");
        let (writer, tx) = RecordingWriter::new(
            path.clone(),
            "session-1".to_string(),
            RecordingMode::Granular,
            Some((120, 40)),
        );

        let handle = writer.start();

        let events = vec![
            SessionEventMessage::text_delta("session-1", "part-1"),
            SessionEventMessage::thinking("session-1", "reasoning"),
            SessionEventMessage::tool_call("session-1", "call-1", "search", json!({ "q": "x" })),
            SessionEventMessage::tool_result(
                "session-1",
                "call-1",
                "search",
                json!({ "ok": true }),
            ),
            SessionEventMessage::user_message("session-1", "msg-1", "hello"),
            SessionEventMessage::message_complete("session-1", "msg-2", "done", None),
            SessionEventMessage::model_switched("session-1", "gpt-5", "openai"),
            SessionEventMessage::ended("session-1", "complete"),
            SessionEventMessage::new("session-1", "custom_event", json!({ "x": 1 })),
        ];

        for event in events {
            tx.send(event).await.expect("send event");
        }
        drop(tx);

        handle.await.expect("join writer").expect("writer success");

        let content = std::fs::read_to_string(&path).expect("read recording");
        let lines: Vec<&str> = content.lines().collect();
        assert!(lines.len() >= 3, "header + events + footer expected");

        let header: RecordingHeader = serde_json::from_str(lines[0]).expect("parse header");
        assert_eq!(header.session_id, "session-1");
        assert_eq!(header.recording_mode, "granular");
        assert_eq!(header.terminal_size, Some((120, 40)));

        let recorded_events: Vec<RecordedEvent> = lines[1..lines.len() - 1]
            .iter()
            .map(|line| serde_json::from_str(line).expect("parse event"))
            .collect();

        let names: Vec<&str> = recorded_events.iter().map(|ev| ev.event.as_str()).collect();
        assert!(names.contains(&"text_delta"));
        assert!(names.contains(&"thinking"));
        assert!(names.contains(&"tool_call"));
        assert!(names.contains(&"tool_result"));
        assert!(names.contains(&"user_message"));
        assert!(names.contains(&"message_complete"));
        assert!(names.contains(&"model_switched"));
        assert!(names.contains(&"ended"));
        assert!(names.contains(&"custom_event"));

        let mut prev_seq = 0;
        for event in &recorded_events {
            assert!(event.seq > prev_seq, "seq must be monotonic");
            prev_seq = event.seq;
        }
    }

    #[tokio::test]
    async fn recording_writer_writes_footer_on_graceful_shutdown() {
        let dir = TempDir::new().expect("create tempdir");
        let path = dir.path().join("recording.jsonl");
        let (writer, tx) = RecordingWriter::new(
            path.clone(),
            "session-2".to_string(),
            RecordingMode::Granular,
            None,
        );

        let handle = writer.start();

        for idx in 0..10 {
            tx.send(SessionEventMessage::text_delta(
                "session-2",
                format!("chunk-{idx}"),
            ))
            .await
            .expect("send event");
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
        drop(tx);

        handle.await.expect("join writer").expect("writer success");

        let content = std::fs::read_to_string(&path).expect("read recording");
        let lines: Vec<&str> = content.lines().collect();
        let footer: RecordingFooter =
            serde_json::from_str(lines.last().expect("footer line")).expect("parse footer");

        assert_eq!(footer.total_events, 10);
        assert!(footer.duration_ms > 0);
    }
}
