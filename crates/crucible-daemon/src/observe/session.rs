//! Reading persisted sessions.
//!
//! Sessions are stored as append-only JSONL files in
//! `.crucible/sessions/<id>/session.jsonl`. The appending is `persist_event`'s
//! (`server/core.rs`), off the daemon's broadcast channel; this module only
//! reads. See [`crate::observe`] for the two line shapes a log can hold.

use crate::observe::events::LogEvent;
use crate::observe::id::{SessionId, SessionType};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Session metadata.
///
/// On its way out: `SessionMetadata::new` has zero callers repo-wide, and the
/// type's only remaining constructor is `observe/session_index.rs`, which
/// [[2026-08-11-dead-code-and-schema-migrations]]'s T-E1 deletes — taking this
/// with it. Do not add new uses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: SessionId,
    pub session_type: SessionType,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub title: Option<String>,
    pub message_count: u32,
    pub kiln_path: PathBuf,
}

impl SessionMetadata {
    /// Create metadata for a new session
    pub fn new(id: SessionId, kiln_path: impl Into<PathBuf>) -> Self {
        Self {
            session_type: id.session_type(),
            id,
            started_at: Utc::now(),
            ended_at: None,
            title: None,
            message_count: 0,
            kiln_path: kiln_path.into(),
        }
    }
}

/// Errors that can occur during session operations.
///
/// Not to be confused with `session_manager::SessionError`, a different type
/// with its own `NotFound(String)`.
///
/// Only `Io` remains. `NotFound` and `AlreadyExists` were constructed solely by
/// the deleted `SessionWriter`, and `Json` solely by its `event.to_jsonl()?` —
/// `parse_session_log` handles its own parse failures with `warn!` and never
/// propagates a `serde_json::Error`, so nothing produces it any more. rustc
/// would not have told us: this enum is `pub` and re-exported from `lib.rs`.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Kept only for `observe/session_index.rs`, the sole `rusqlite` user in
/// `observe/`. It is dead the moment that file goes, which
/// [[2026-08-11-dead-code-and-schema-migrations]]'s T-E1 does — along with
/// `SessionMetadata` above.
impl From<rusqlite::Error> for SessionError {
    fn from(err: rusqlite::Error) -> Self {
        Self::Io(std::io::Error::other(err))
    }
}

/// Load all events from a session log.
///
/// Reads both shapes the file can hold; see [`crate::observe::SessionLogLine`].
pub async fn load_events(session_dir: impl AsRef<Path>) -> Result<Vec<LogEvent>, SessionError> {
    let jsonl_path = session_dir.as_ref().join("session.jsonl");

    if !jsonl_path.exists() {
        return Ok(Vec::new());
    }

    // Whole-file read, matching `FileSessionStorage::load_events`
    // (`session_storage.rs`) and its `count_events`. The streaming
    // reader this replaces bought nothing: every caller consumes the entire
    // `Vec` anyway (see the note at `session_bridge.rs`).
    Ok(crate::observe::events::parse_session_log(
        &fs::read_to_string(&jsonl_path).await?,
    ))
}

/// List all session IDs in a sessions directory
pub async fn list_sessions(sessions_dir: impl AsRef<Path>) -> Result<Vec<SessionId>, SessionError> {
    let sessions_dir = sessions_dir.as_ref();

    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = fs::read_dir(sessions_dir).await?;
    let mut ids = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            let name = entry.file_name();
            if let Some(name_str) = name.to_str() {
                if let Ok(id) = SessionId::parse(name_str) {
                    ids.push(id);
                }
            }
        }
    }

    // Sort by ID (which includes timestamp, so newest last)
    ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// The bytes `persist_event` (`server/core.rs`) actually appends: a
    /// serialized `SessionEventMessage`, not a `LogEvent`. Written literally
    /// rather than through a helper so the test still pins the wire shape if
    /// the helper changes.
    const WIRE_LOG: &str = concat!(
        r#"{"type":"event","session_id":"chat-20260811-1200-abcd","event":"user_message","data":{"message_id":"m1","content":"how do I read a file"},"timestamp":"2026-08-11T12:00:01Z","seq":1}"#,
        "\n",
        r#"{"type":"event","session_id":"chat-20260811-1200-abcd","event":"thinking","data":{"content":"consider std::fs"},"timestamp":"2026-08-11T12:00:02Z","seq":2}"#,
        "\n",
        r#"{"type":"event","session_id":"chat-20260811-1200-abcd","event":"tool_call","data":{"call_id":"c1","tool":"read_file","args":{"path":"Cargo.toml"}},"timestamp":"2026-08-11T12:00:03Z","seq":3}"#,
        "\n",
        r#"{"type":"event","session_id":"chat-20260811-1200-abcd","event":"tool_result","data":{"call_id":"c1","tool":"read_file","result":"[package]","terminate":false},"timestamp":"2026-08-11T12:00:04Z","seq":4}"#,
        "\n",
        r#"{"type":"event","session_id":"chat-20260811-1200-abcd","event":"message_complete","data":{"message_id":"m1","full_response":"Use std::fs::read_to_string.","prompt_tokens":25,"completion_tokens":75,"total_tokens":100,"cache_read_tokens":12},"timestamp":"2026-08-11T12:00:05Z","seq":5}"#,
        "\n",
    );

    /// Materialize a session directory holding `lines` as its log.
    async fn write_log(sessions_dir: &Path, id: &SessionId, lines: &str) -> PathBuf {
        let session_dir = sessions_dir.join(id.as_str());
        fs::create_dir_all(&session_dir).await.unwrap();
        fs::write(session_dir.join("session.jsonl"), lines)
            .await
            .unwrap();
        session_dir
    }

    /// The committed wire-format log, which
    /// `server::tests::session_log_capture` keeps equal to live daemon output.
    ///
    /// Read from the fixture rather than hand-written so this cannot drift from
    /// the real format — a hand-written line would, for one thing, have omitted
    /// the `display` object `tool_call_with_metadata` adds.
    fn sample_wire_log() -> String {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/fixtures/session_log_wire.jsonl");
        std::fs::read_to_string(&fixture)
            .unwrap_or_else(|e| panic!("read {}: {e}", fixture.display()))
    }

    fn as_jsonl(events: &[LogEvent]) -> String {
        events
            .iter()
            .map(|e| e.to_jsonl().unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    #[tokio::test]
    async fn load_events_returns_events_for_a_real_session_log() {
        let dir = TempDir::new().unwrap();
        let session_dir = dir.path().join("chat-20260811-1200-abcd");
        fs::create_dir_all(&session_dir).await.unwrap();
        fs::write(session_dir.join("session.jsonl"), WIRE_LOG)
            .await
            .unwrap();

        let events = load_events(&session_dir).await.unwrap();

        assert_eq!(
            events.len(),
            5,
            "every line of a real session log must survive the read"
        );
        assert!(matches!(
            &events[0],
            LogEvent::User { content, .. } if content == "how do I read a file"
        ));
        assert!(matches!(
            &events[1],
            LogEvent::Thinking { content, .. } if content == "consider std::fs"
        ));
        assert!(matches!(
            &events[2],
            LogEvent::ToolCall { id, name, .. } if id == "c1" && name == "read_file"
        ));
        assert!(matches!(
            &events[3],
            LogEvent::ToolResult { id, result, .. } if id == "c1" && result == "[package]"
        ));
        let LogEvent::Assistant {
            content, tokens, ..
        } = &events[4]
        else {
            panic!("expected Assistant, got {:?}", events[4]);
        };
        assert_eq!(content, "Use std::fs::read_to_string.");
        let tokens = tokens.as_ref().expect("message_complete carries usage");
        assert_eq!(tokens.prompt_tokens, 25);
        assert_eq!(tokens.completion_tokens, 75);
        assert_eq!(
            tokens.cache_read_tokens,
            Some(12),
            "cache accounting must not be dropped on the way in"
        );
    }

    /// `inject_context_impl` (`server/session/messaging.rs`) and both fork
    /// handlers append `LogEvent` to the same file. A log is mixed by
    /// construction, and both shapes must survive.
    #[tokio::test]
    async fn load_events_reads_a_log_holding_both_persisted_shapes() {
        let dir = TempDir::new().unwrap();
        let session_dir = dir.path().join("chat-20260811-1200-abcd");
        fs::create_dir_all(&session_dir).await.unwrap();
        let mixed = format!(
            "{}{}\n",
            WIRE_LOG, r#"{"type":"system","ts":"2026-08-11T12:00:06Z","content":"injected note"}"#
        );
        fs::write(session_dir.join("session.jsonl"), mixed)
            .await
            .unwrap();

        let events = load_events(&session_dir).await.unwrap();

        assert_eq!(events.len(), 6);
        assert!(matches!(
            events.last().unwrap(),
            LogEvent::System { content, .. } if content == "injected note"
        ));
    }

    /// The `View` branch on its own — the shape `inject_context` and fork write.
    #[tokio::test]
    async fn test_load_events_roundtrip() {
        let dir = TempDir::new().unwrap();
        let sessions_dir = dir.path().join("sessions");
        let id = SessionId::new(SessionType::Chat, Utc::now());

        let session_dir = write_log(
            &sessions_dir,
            &id,
            &as_jsonl(&[
                LogEvent::system("System"),
                LogEvent::user("Hello"),
                LogEvent::assistant("Hi!"),
            ]),
        )
        .await;

        let events = load_events(&session_dir).await.unwrap();

        assert_eq!(events.len(), 3);

        match &events[0] {
            LogEvent::System { content, .. } => assert_eq!(content, "System"),
            _ => panic!("wrong event type"),
        }

        match &events[1] {
            LogEvent::User { content, .. } => assert_eq!(content, "Hello"),
            _ => panic!("wrong event type"),
        }

        match &events[2] {
            LogEvent::Assistant { content, .. } => assert_eq!(content, "Hi!"),
            _ => panic!("wrong event type"),
        }
    }

    #[tokio::test]
    async fn load_events_on_a_missing_log_is_empty_not_an_error() {
        let dir = TempDir::new().unwrap();
        let session_dir = dir.path().join("chat-20260811-1200-abcd");
        fs::create_dir_all(&session_dir).await.unwrap();

        assert!(load_events(&session_dir).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let dir = TempDir::new().unwrap();
        let sessions_dir = dir.path().join("sessions");

        let mut ids = Vec::new();
        for _ in 0..3 {
            let id = SessionId::new(SessionType::Chat, Utc::now());
            write_log(&sessions_dir, &id, &as_jsonl(&[LogEvent::user("hi")])).await;
            ids.push(id);
        }

        let listed = list_sessions(&sessions_dir).await.unwrap();
        assert_eq!(listed.len(), 3);

        for id in &ids {
            assert!(listed.contains(id));
        }
    }

    #[tokio::test]
    async fn test_list_sessions_empty() {
        let dir = TempDir::new().unwrap();
        let sessions_dir = dir.path().join("nonexistent");

        let listed = list_sessions(&sessions_dir).await.unwrap();
        assert!(listed.is_empty());
    }

    /// Listing a kiln's sessions stays usable as the log count grows.
    ///
    /// Before the read-path fix this was free, because every line failed to
    /// parse. It is not free now, and no index is planned — see the plan's
    /// Risk 2. This asserts the order of magnitude, not a precise number: it
    /// exists so a future regression is visible and so anyone weighing a
    /// metadata cache has a figure to weigh it against.
    #[tokio::test]
    async fn listing_many_sessions_stays_within_budget() {
        let tmp = TempDir::new().unwrap();
        let log = sample_wire_log();

        // `type-YYYYMMDD-HHMM-hhhh` with a 4-hex-char hash — `list_sessions`
        // goes through `SessionId::parse`, which rejects anything else.
        for i in 0..500 {
            let dir = tmp.path().join(format!("chat-20260811-0000-{i:04x}"));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("session.jsonl"), &log).unwrap();
        }

        let started = std::time::Instant::now();
        let ids = list_sessions(tmp.path()).await.unwrap();
        let mut total = 0usize;
        for id in &ids {
            total += load_events(tmp.path().join(id.as_str()))
                .await
                .unwrap()
                .len();
        }
        let elapsed = started.elapsed();

        assert_eq!(ids.len(), 500);
        assert!(total > 0, "parsed no events — the fix regressed");
        println!(
            "parsed {total} events across {} logs in {elapsed:?}",
            ids.len()
        );

        // Deliberately loose. A debug-profile CI box is slow and this must not
        // flake; the point is to catch an order-of-magnitude regression, and to
        // record the real number in the failure message when it does trip.
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "parsing 500 session logs took {elapsed:?}; Risk 2 assumed this stayed \
             in the low seconds. If this is now the user-visible cost of `cru session \
             list`, the metadata cache described in Risk 2 is worth building."
        );
    }

    /// A directory whose name is not a session id is not a session.
    #[tokio::test]
    async fn list_sessions_skips_directories_that_are_not_session_ids() {
        let dir = TempDir::new().unwrap();
        let sessions_dir = dir.path().join("sessions");
        fs::create_dir_all(sessions_dir.join("not-a-session-id"))
            .await
            .unwrap();
        let id = SessionId::new(SessionType::Chat, Utc::now());
        write_log(&sessions_dir, &id, &as_jsonl(&[LogEvent::user("hi")])).await;

        let listed = list_sessions(&sessions_dir).await.unwrap();
        assert_eq!(listed, vec![id]);
    }
}
