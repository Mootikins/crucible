//! Reading persisted sessions.
//!
//! Sessions are stored as append-only JSONL files in
//! `.crucible/sessions/<id>/session.jsonl`. The appending is `persist_event`'s
//! (`server/core.rs`), off the daemon's broadcast channel; this module only
//! reads. See [`crate::observe`] for the two line shapes a log can hold.

use crate::observe::events::LogEvent;
use std::path::Path;
use tokio::fs;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observe::id::{SessionId, SessionType};
    use std::path::PathBuf;
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
        let id = SessionId::generate(SessionType::Chat);

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

    /// `LogEvent`'s `type` tags and the wire envelope's `type` values share one
    /// field in one file. Nothing enforces disjointness, so a `LogEvent` variant
    /// named `Event` would make every wire line decode as the wrong thing —
    /// silently, since both are valid JSON with a `type` string.
    #[test]
    fn log_event_tags_never_collide_with_wire_envelope_types() {
        const WIRE_TYPES: &[&str] = &["event", "replay_event"];

        let src = include_str!("events.rs");
        let needle = "pub enum LogEvent {\n";
        let start = src
            .find(needle)
            .expect("`pub enum LogEvent {` not found — fix this test")
            + needle.len();
        let body = &src[start..];
        let end = body.find("\n}\n").expect("unterminated LogEvent enum");

        let mut tags = Vec::new();
        let mut depth = 0i32;
        for line in body[..end].lines() {
            let trimmed = line.trim();
            if depth == 0
                && trimmed
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_uppercase())
            {
                let ident: String = trimmed
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                // `#[serde(rename_all = "snake_case")]` on the enum.
                let mut tag = String::new();
                for (i, c) in ident.chars().enumerate() {
                    if c.is_ascii_uppercase() {
                        if i > 0 {
                            tag.push('_');
                        }
                        tag.push(c.to_ascii_lowercase());
                    } else {
                        tag.push(c);
                    }
                }
                tags.push(tag);
            }
            depth += line.matches(['{', '(']).count() as i32;
            depth -= line.matches(['}', ')']).count() as i32;
        }

        assert!(
            tags.len() > 5,
            "extracted only {} LogEvent tags — the scan markers moved, fix this test",
            tags.len()
        );
        for tag in tags {
            assert!(
                !WIRE_TYPES.contains(&tag.as_str()),
                "LogEvent variant `{tag}` collides with a SessionEventMessage msg_type",
            );
        }
    }

    /// A real log is mixed. `inject_context` writes a `LogEvent` line to disk and
    /// broadcasts a `SessionEventMessage`; `fork` copies `LogEvent` lines into a
    /// file the turn loop then appends wire lines to. Reading one shape must not
    /// drop the other — and until now no fixture contained both, so the reader's
    /// tolerance for that was entirely untested.
    #[tokio::test]
    async fn a_log_holding_both_shapes_yields_both() {
        let dir = TempDir::new().unwrap();
        let sessions_dir = dir.path().join("sessions");
        let id = SessionId::generate(SessionType::Chat);

        // A LogEvent line (as `inject_context` and `fork` write it), then the
        // wire lines the broadcast path appends.
        let mixed = format!(
            "{}{}",
            as_jsonl(&[LogEvent::system("injected context")]),
            WIRE_LOG
        );
        let session_dir = write_log(&sessions_dir, &id, &mixed).await;

        let events = load_events(&session_dir).await.unwrap();

        assert!(
            matches!(&events[0], LogEvent::System { content, .. } if content == "injected context"),
            "the LogEvent line must survive, got {:?}",
            events.first()
        );
        assert!(
            events.len() > 1,
            "the wire lines must survive alongside it, got {events:?}"
        );
        assert!(
            events.iter().any(
                |e| matches!(e, LogEvent::User { content, .. } if content == "how do I read a file")
            ),
            "the wire `user_message` line must survive, got {events:?}"
        );
    }
}
