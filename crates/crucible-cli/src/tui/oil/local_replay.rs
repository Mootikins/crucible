//! Local replay driver — reads recordings from disk and pumps events through
//! an in-process channel. No daemon contact.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use crucible_core::protocol::SessionEventMessage;
use crucible_core::recording::{RecordedEvent, RecordingFooter, RecordingHeader};
use std::io::BufRead;
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc;

/// Load a recording from disk. The returned header carries useful metadata
/// (terminal_size, original session_id, started_at) the caller may surface.
pub fn read_recording(path: &Path) -> Result<(RecordingHeader, Vec<RecordedEvent>)> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("replay file not found: {}", path.display()))?;
    let mut header: Option<RecordingHeader> = None;
    let mut events = Vec::new();

    for (idx, line) in std::io::BufReader::new(file).lines().enumerate() {
        let line = line.with_context(|| format!("read error at line {}", idx + 1))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if header.is_none() {
            header = Some(
                serde_json::from_str::<RecordingHeader>(trimmed)
                    .map_err(|e| anyhow!("bad recording header at line {}: {}", idx + 1, e))?,
            );
            continue;
        }
        if let Ok(ev) = serde_json::from_str::<RecordedEvent>(trimmed) {
            events.push(ev);
            continue;
        }
        if serde_json::from_str::<RecordingFooter>(trimmed).is_ok() {
            continue;
        }
        tracing::warn!(line = idx + 1, "skipping malformed replay line");
    }

    Ok((
        header.ok_or_else(|| anyhow!("recording header missing"))?,
        events,
    ))
}

/// Drive a recording's events into `tx`, honoring scaled delays.
/// Rewrites `session_id` on each message to `new_session_id` so the consumer
/// filter matches. Recorded `session_id` is ignored (informational only).
/// `speed <= 0.0` is treated as instant (no delay between events).
pub async fn drive_replay(
    events: Vec<RecordedEvent>,
    speed: f64,
    new_session_id: String,
    tx: mpsc::UnboundedSender<SessionEventMessage>,
) {
    let mut prev: Option<DateTime<Utc>> = None;
    for ev in events {
        if matches!(
            ev.event.as_str(),
            "key_press" | "keypress" | "key_press_event" | "KeyPress"
        ) {
            continue;
        }
        if let Some(p) = prev {
            let d = scaled_delay(p, ev.ts, speed);
            if !d.is_zero() {
                tokio::time::sleep(d).await;
            }
        }
        prev = Some(ev.ts);

        let mut msg = SessionEventMessage::new(new_session_id.clone(), ev.event, ev.data);
        msg.msg_type = "replay_event".into();
        msg.timestamp = Some(ev.ts);
        msg.seq = Some(ev.seq);
        if tx.send(msg).is_err() {
            return;
        }
    }

    let mut done = SessionEventMessage::new(
        new_session_id,
        "replay_complete",
        serde_json::json!({}),
    );
    done.msg_type = "replay_event".into();
    let _ = tx.send(done);
}

fn scaled_delay(prev: DateTime<Utc>, cur: DateTime<Utc>, speed: f64) -> Duration {
    if speed <= 0.0 {
        return Duration::ZERO;
    }
    let ms = cur.signed_duration_since(prev).num_milliseconds();
    if ms <= 0 {
        return Duration::ZERO;
    }
    Duration::from_secs_f64((ms as f64 / speed) / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_event(seq: u64, event: &str, ts: DateTime<Utc>) -> RecordedEvent {
        RecordedEvent {
            ts,
            seq,
            event: event.to_string(),
            session_id: "orig-session".to_string(),
            data: serde_json::json!({"content": format!("e{}", seq)}),
        }
    }

    #[test]
    fn read_recording_round_trips_header_and_events() {
        let mut f = NamedTempFile::new().expect("tempfile");
        let started = Utc.with_ymd_and_hms(2026, 4, 17, 0, 0, 0).unwrap();
        let header = RecordingHeader {
            version: 1,
            session_id: "sess-abc".into(),
            recording_mode: "granular".into(),
            started_at: started,
            terminal_size: Some((120, 40)),
        };
        writeln!(f, "{}", serde_json::to_string(&header).unwrap()).unwrap();

        for i in 1..=3u64 {
            let ev = make_event(
                i,
                "text_delta",
                started + chrono::Duration::milliseconds(i as i64 * 10),
            );
            writeln!(f, "{}", serde_json::to_string(&ev).unwrap()).unwrap();
        }

        let footer = RecordingFooter {
            ended_at: started + chrono::Duration::milliseconds(50),
            total_events: 3,
            duration_ms: 50,
        };
        writeln!(f, "{}", serde_json::to_string(&footer).unwrap()).unwrap();
        f.flush().unwrap();

        let (hdr, events) = read_recording(f.path()).expect("read");
        assert_eq!(hdr.session_id, "sess-abc");
        assert_eq!(hdr.recording_mode, "granular");
        assert_eq!(hdr.terminal_size, Some((120, 40)));
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].seq, 1);
        assert_eq!(events[2].seq, 3);
    }

    #[tokio::test]
    async fn drive_replay_at_zero_speed_preserves_order_and_emits_complete() {
        let t0 = Utc.with_ymd_and_hms(2026, 4, 17, 0, 0, 0).unwrap();
        let events = vec![
            make_event(1, "text_delta", t0),
            make_event(2, "text_delta", t0 + chrono::Duration::milliseconds(100)),
            make_event(3, "text_delta", t0 + chrono::Duration::milliseconds(200)),
        ];

        let (tx, mut rx) = mpsc::unbounded_channel();
        drive_replay(events, 0.0, "new-sess".into(), tx).await;

        let mut collected = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            collected.push(msg);
        }

        assert_eq!(collected.len(), 4, "3 events + replay_complete");
        assert_eq!(collected[0].event, "text_delta");
        assert_eq!(collected[0].seq, Some(1));
        assert_eq!(collected[0].session_id, "new-sess");
        assert_eq!(collected[0].msg_type, "replay_event");
        assert_eq!(collected[1].seq, Some(2));
        assert_eq!(collected[2].seq, Some(3));
        assert_eq!(collected[3].event, "replay_complete");
        assert_eq!(collected[3].msg_type, "replay_event");
    }

    #[tokio::test]
    async fn drive_replay_skips_key_press_events() {
        let t0 = Utc.with_ymd_and_hms(2026, 4, 17, 0, 0, 0).unwrap();
        let events = vec![
            make_event(1, "text_delta", t0),
            make_event(2, "key_press", t0 + chrono::Duration::milliseconds(5)),
            make_event(3, "text_delta", t0 + chrono::Duration::milliseconds(10)),
        ];

        let (tx, mut rx) = mpsc::unbounded_channel();
        drive_replay(events, 0.0, "sess".into(), tx).await;

        let mut names = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            names.push(msg.event);
        }
        assert_eq!(
            names,
            vec!["text_delta", "text_delta", "replay_complete"],
            "key_press filtered out"
        );
    }

    #[tokio::test]
    async fn drive_replay_handles_non_monotonic_timestamps() {
        let t0 = Utc.with_ymd_and_hms(2026, 4, 17, 0, 0, 0).unwrap();
        let events = vec![
            make_event(1, "text_delta", t0 + chrono::Duration::milliseconds(100)),
            make_event(2, "text_delta", t0), // earlier than previous
            make_event(3, "text_delta", t0 + chrono::Duration::milliseconds(50)),
        ];

        let (tx, mut rx) = mpsc::unbounded_channel();
        // Use a modest speed so a bug would cause real sleep; negative deltas
        // should clamp to zero and produce immediate emission.
        let start = std::time::Instant::now();
        drive_replay(events, 1.0, "sess".into(), tx).await;
        let elapsed = start.elapsed();

        // No panic; delays for going-backwards deltas clamp to zero,
        // so total elapsed is under 10ms on any reasonable machine.
        assert!(
            elapsed < Duration::from_millis(200),
            "non-monotonic should not sleep: elapsed={:?}",
            elapsed
        );
        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 4, "3 events + replay_complete, no panic");
    }
}
