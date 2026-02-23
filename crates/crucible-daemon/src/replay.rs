use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use crucible_core::session::{Session, SessionType};
use crucible_core::protocol::SessionEventMessage;
use crate::recording::{RecordedEvent, RecordingFooter, RecordingHeader};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{debug, warn};

pub struct ReplaySession {
    replay_source: PathBuf,
    speed: f64,
    event_tx: broadcast::Sender<SessionEventMessage>,
    replay_session: Session,
    header: RecordingHeader,
    events: Vec<RecordedEvent>,
    footer: Option<RecordingFooter>,
}

impl ReplaySession {
    pub fn new(
        recording_path: PathBuf,
        speed: f64,
        event_tx: broadcast::Sender<SessionEventMessage>,
        replay_session_id: String,
    ) -> Result<Self> {
        let file = std::fs::File::open(&recording_path)
            .with_context(|| format!("recording file not found: {}", recording_path.display()))?;

        let mut header: Option<RecordingHeader> = None;
        let mut events = Vec::new();
        let mut footer: Option<RecordingFooter> = None;

        use std::io::BufRead;
        let reader = std::io::BufReader::new(file);
        for (idx, line_result) in reader.lines().enumerate() {
            let line_no = idx + 1;
            let line = match line_result {
                Ok(line) => line,
                Err(err) => {
                    warn!(line = line_no, error = %err, "Failed to read replay line, skipping");
                    continue;
                }
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if header.is_none() {
                match serde_json::from_str::<RecordingHeader>(trimmed) {
                    Ok(parsed) => {
                        header = Some(parsed);
                        continue;
                    }
                    Err(err) => {
                        return Err(anyhow!(
                            "invalid recording header at line {}: {}",
                            line_no,
                            err
                        ));
                    }
                }
            }

            if let Ok(event) = serde_json::from_str::<RecordedEvent>(trimmed) {
                events.push(event);
                continue;
            }

            if let Ok(parsed_footer) = serde_json::from_str::<RecordingFooter>(trimmed) {
                footer = Some(parsed_footer);
                continue;
            }

            warn!(line = line_no, content = %trimmed, "Skipping malformed replay line");
        }

        let header = header.ok_or_else(|| anyhow!("recording header missing"))?;

        let replay_kiln = recording_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let mut replay_session = Session::new(SessionType::Chat, replay_kiln);
        replay_session.id = replay_session_id.clone();
        replay_session.title = Some(format!("replay:{}", recording_path.display()));

        Ok(Self {
            replay_source: recording_path,
            speed,
            event_tx,
            replay_session,
            header,
            events,
            footer,
        })
    }

    pub fn session(&self) -> &Session {
        &self.replay_session
    }

    pub fn start(self) -> JoinHandle<Result<()>> {
        tokio::spawn(async move {
            let mut previous_ts: Option<DateTime<Utc>> = None;
            let total_events = self.events.len();

            debug!(
                session_id = %self.header.session_id,
                recording_mode = %self.header.recording_mode,
                started_at = %self.header.started_at,
                "Starting replay"
            );

            if self.footer.is_none() {
                warn!(
                    source = %self.replay_source.display(),
                    "Replay recording missing footer"
                );
            }

            for recorded in self.events {
                if is_keypress_event(&recorded.event) {
                    continue;
                }

                if let Some(prev) = previous_ts {
                    let delay = scaled_delay(prev, recorded.ts, self.speed);
                    if !delay.is_zero() {
                        sleep(delay).await;
                    }
                }
                previous_ts = Some(recorded.ts);

                let mut event = SessionEventMessage::new(
                    self.replay_session.id.clone(),
                    recorded.event,
                    recorded.data,
                );
                event.msg_type = "replay_event".to_string();
                event.timestamp = Some(recorded.ts);
                event.seq = Some(recorded.seq);

                if let Err(err) = self.event_tx.send(event) {
                    warn!(
                        source = %self.replay_source.display(),
                        error = %err,
                        "Replay broadcast send failed, continuing"
                    );
                }
            }

            let mut complete = SessionEventMessage::new(
                self.replay_session.id.clone(),
                "replay_complete".to_string(),
                serde_json::json!({"status": "complete", "total_events": total_events}),
            );
            complete.msg_type = "replay_event".to_string();
            let _ = self.event_tx.send(complete);

            Ok(())
        })
    }
}

fn scaled_delay(previous: DateTime<Utc>, current: DateTime<Utc>, speed: f64) -> Duration {
    if speed <= 0.0 {
        return Duration::ZERO;
    }

    let delay_ms = current.signed_duration_since(previous).num_milliseconds();
    if delay_ms <= 0 {
        return Duration::ZERO;
    }

    Duration::from_secs_f64((delay_ms as f64 / speed) / 1000.0)
}

fn is_keypress_event(event_name: &str) -> bool {
    matches!(
        event_name,
        "key_press" | "keypress" | "key_press_event" | "KeyPress"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use serde_json::json;
    use tempfile::TempDir;
    use tokio::time::Instant;

    fn write_recording(
        path: &PathBuf,
        events: &[RecordedEvent],
        footer: Option<RecordingFooter>,
        malformed_line: Option<&str>,
    ) {
        let started_at = events
            .first()
            .map(|e| e.ts)
            .unwrap_or_else(Utc::now)
            .checked_sub_signed(Duration::milliseconds(1))
            .expect("valid ts");
        let ended_at = events.last().map(|e| e.ts).unwrap_or_else(Utc::now);

        let header = RecordingHeader {
            version: 1,
            session_id: "original-session".to_string(),
            recording_mode: "granular".to_string(),
            started_at,
            terminal_size: None,
        };

        let mut lines = vec![serde_json::to_string(&header).expect("serialize header")];
        for (idx, event) in events.iter().enumerate() {
            lines.push(serde_json::to_string(event).expect("serialize event"));
            if idx == 0 {
                if let Some(malformed) = malformed_line {
                    lines.push(malformed.to_string());
                }
            }
        }
        let footer = footer.unwrap_or(RecordingFooter {
            ended_at,
            total_events: events.len() as u64,
            duration_ms: 0,
        });
        lines.push(serde_json::to_string(&footer).expect("serialize footer"));
        std::fs::write(path, format!("{}\n", lines.join("\n"))).expect("write recording");
    }

    fn sample_events(base: chrono::DateTime<Utc>, first_gap_ms: i64) -> Vec<RecordedEvent> {
        vec![
            RecordedEvent {
                ts: base,
                seq: 1,
                event: "text_delta".to_string(),
                session_id: "orig".to_string(),
                data: json!({"content":"a"}),
            },
            RecordedEvent {
                ts: base + Duration::milliseconds(first_gap_ms),
                seq: 2,
                event: "text_delta".to_string(),
                session_id: "orig".to_string(),
                data: json!({"content":"b"}),
            },
        ]
    }

    #[tokio::test]
    async fn replay_session_invalid_path_returns_error() {
        let (tx, _rx) = broadcast::channel(16);
        let replay = ReplaySession::new(
            PathBuf::from("/definitely/not/a/real/recording.jsonl"),
            1.0,
            tx,
            "replay-1".to_string(),
        );
        assert!(replay.is_err());
    }

    #[tokio::test]
    async fn replay_session_rewrites_session_id_and_emits_to_subscribers() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("recording.jsonl");
        let events = sample_events(Utc::now(), 5);
        write_recording(&path, &events, None, None);

        let (tx, mut rx) = broadcast::channel(16);
        let replay =
            ReplaySession::new(path, 0.0, tx, "replay-session".to_string()).expect("create replay");

        let handle = replay.start();
        let first = rx.recv().await.expect("first event");
        let second = rx.recv().await.expect("second event");
        handle.await.expect("join").expect("replay ok");

        assert_eq!(first.session_id, "replay-session");
        assert_eq!(second.session_id, "replay-session");
        assert_eq!(first.msg_type, "replay_event");
    }

    #[tokio::test]
    async fn replay_session_creates_chat_session_and_stores_source() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("recording.jsonl");
        let events = sample_events(Utc::now(), 1);
        write_recording(&path, &events, None, None);

        let (tx, _rx) = broadcast::channel(16);
        let replay = ReplaySession::new(path.clone(), 1.0, tx, "replay-session".to_string())
            .expect("create replay");

        assert_eq!(replay.replay_session.session_type, SessionType::Chat);
        assert_eq!(replay.replay_source, path);
    }

    #[tokio::test]
    async fn replay_session_1x_preserves_delay() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("recording.jsonl");
        let events = sample_events(Utc::now(), 120);
        write_recording(&path, &events, None, None);

        let (tx, mut rx) = broadcast::channel(16);
        let replay = ReplaySession::new(path, 1.0, tx, "replay-1x".to_string()).expect("create");

        let start = Instant::now();
        let handle = replay.start();
        let _ = rx.recv().await.expect("first");
        let _ = rx.recv().await.expect("second");
        handle.await.expect("join").expect("ok");

        let elapsed = start.elapsed().as_millis() as i64;
        assert!((elapsed - 120).abs() <= 50, "elapsed={}ms", elapsed);
    }

    #[tokio::test]
    async fn replay_session_2x_halves_delay() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("recording.jsonl");
        let events = sample_events(Utc::now(), 120);
        write_recording(&path, &events, None, None);

        let (tx, mut rx) = broadcast::channel(16);
        let replay = ReplaySession::new(path, 2.0, tx, "replay-2x".to_string()).expect("create");

        let start = Instant::now();
        let handle = replay.start();
        let _ = rx.recv().await.expect("first");
        let _ = rx.recv().await.expect("second");
        handle.await.expect("join").expect("ok");

        let elapsed = start.elapsed().as_millis() as i64;
        assert!((elapsed - 60).abs() <= 50, "elapsed={}ms", elapsed);
    }

    #[tokio::test]
    async fn replay_session_zero_speed_is_instant() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("recording.jsonl");
        let events = sample_events(Utc::now(), 200);
        write_recording(&path, &events, None, None);

        let (tx, mut rx) = broadcast::channel(16);
        let replay =
            ReplaySession::new(path, 0.0, tx, "replay-instant".to_string()).expect("create");

        let start = Instant::now();
        let handle = replay.start();
        let _ = rx.recv().await.expect("first");
        let _ = rx.recv().await.expect("second");
        handle.await.expect("join").expect("ok");

        let elapsed = start.elapsed().as_millis() as i64;
        assert!(elapsed <= 50, "elapsed={}ms", elapsed);
    }

    #[tokio::test]
    async fn replay_session_skips_malformed_lines() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("recording.jsonl");
        let events = sample_events(Utc::now(), 1);
        write_recording(&path, &events, None, Some("not-json"));

        let (tx, mut rx) = broadcast::channel(16);
        let replay =
            ReplaySession::new(path, 0.0, tx, "replay-malformed".to_string()).expect("create");
        let handle = replay.start();

        let first = rx.recv().await.expect("first");
        let second = rx.recv().await.expect("second");
        handle.await.expect("join").expect("ok");

        assert_eq!(first.event, "text_delta");
        assert_eq!(second.event, "text_delta");
    }

    #[tokio::test]
    async fn replay_session_skips_keypress_events() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("recording.jsonl");
        let base = Utc::now();
        let events = vec![
            RecordedEvent {
                ts: base,
                seq: 1,
                event: "key_press".to_string(),
                session_id: "orig".to_string(),
                data: json!({"key":"a"}),
            },
            RecordedEvent {
                ts: base + Duration::milliseconds(1),
                seq: 2,
                event: "text_delta".to_string(),
                session_id: "orig".to_string(),
                data: json!({"content":"ok"}),
            },
        ];
        write_recording(&path, &events, None, None);

        let (tx, mut rx) = broadcast::channel(16);
        let replay =
            ReplaySession::new(path, 0.0, tx, "replay-no-key".to_string()).expect("create");
        let handle = replay.start();

        let received = rx.recv().await.expect("text event");
        handle.await.expect("join").expect("ok");

        assert_eq!(received.event, "text_delta");
    }
}
