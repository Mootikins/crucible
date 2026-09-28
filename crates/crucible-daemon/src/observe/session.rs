//! Reading persisted sessions.
//!
//! Sessions are stored as append-only JSONL files in
//! `.crucible/sessions/<id>/session.jsonl`. The appending is `persist_event`'s
//! (`server/core.rs`), off the daemon's broadcast channel; this module only
//! reads. See [`crate::observe`] for the two line shapes a log can hold.

use crate::protocol::SessionEventMessage;
use crucible_core::transcript::{ItemBody, Transcript, TranscriptFold};
use std::path::Path;
use tokio::fs;

/// Errors that can occur during session operations.
///
/// Not to be confused with `session_manager::SessionError`, a different type
/// with its own `NotFound(String)`.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// The transcript of a stored log: each line in its current wire form
/// ([`crate::observe::stored_events`]), then the one fold of core.
///
/// `SessionManager::load_transcript` reads the same fold through its storage.
/// This reads text, for a caller that has the file: the observe RPCs and the
/// CLI when no daemon can start. A line that is not JSON is skipped with a
/// warning.
pub fn transcript_of_log(session_id: &str, jsonl: &str) -> Transcript {
    let lines = jsonl
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .filter_map(|(index, line)| match serde_json::from_str(line.trim()) {
            Ok(value) => Some(value),
            Err(error) => {
                tracing::warn!(line = index + 1, %error, "skipping unparseable session log line");
                None
            }
        })
        .collect();
    TranscriptFold::of_events(&crate::observe::stored_events(session_id, lines))
}

/// The transcript of the session in `session_dir`. A session with no log
/// has an empty transcript.
pub async fn load_transcript(session_dir: impl AsRef<Path>) -> Result<Transcript, SessionError> {
    let session_dir = session_dir.as_ref();
    let jsonl_path = session_dir.join("session.jsonl");
    if !jsonl_path.exists() {
        return Ok(Transcript::default());
    }
    let session_id = session_dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(transcript_of_log(
        &session_id,
        &fs::read_to_string(&jsonl_path).await?,
    ))
}

/// What a session list shows of a transcript: the count of user turns and
/// answer segments with text, and the start of the first user turn.
pub fn transcript_summary(transcript: &Transcript) -> (usize, String) {
    let count = transcript
        .items
        .iter()
        .filter(|item| match &item.body {
            ItemBody::UserTurn { .. } => true,
            ItemBody::AssistantSegment { text, .. } => !text.is_empty(),
            _ => false,
        })
        .count();
    let title = transcript
        .items
        .iter()
        .find_map(|item| match &item.body {
            ItemBody::UserTurn { content, .. } => {
                Some(crucible_core::text::truncate_chars(content, 50, true))
            }
            _ => None,
        })
        .unwrap_or_else(|| "(empty)".to_string());
    (count, title)
}

/// The wire envelopes a session log holds past a seq cursor, in order.
///
/// The replay path of a reconnecting chat stream (`session.events_after`):
/// unlike [`load_transcript`], this reads the RAW envelopes — `seq` is stamped at
/// emit and the transcript does not keep it, so the cursor can only be
/// compared against the wire form a live subscriber receives.
///
/// A line joins the tail only when it is a wire event carrying a seq: view
/// lines (`init`, `user`, …) have no `event`/`data` fields to deserialize and
/// fail on the envelope, and an unstamped wire line (only an older daemon
/// wrote one; `EventBus` stamps each event now) has no position to filter
/// on — dropping it is the only answer that cannot misorder the tail.
pub async fn events_after(
    session_dir: impl AsRef<Path>,
    after: u64,
) -> Result<Vec<SessionEventMessage>, SessionError> {
    let jsonl_path = session_dir.as_ref().join("session.jsonl");
    if !jsonl_path.exists() {
        return Ok(Vec::new());
    }

    let raw = fs::read_to_string(&jsonl_path).await?;
    Ok(raw
        .lines()
        .filter_map(|line| serde_json::from_str::<SessionEventMessage>(line.trim()).ok())
        .filter(|event| event.seq.is_some_and(|seq| seq > after))
        .collect())
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

    fn kinds(transcript: &Transcript) -> Vec<&'static str> {
        transcript
            .items
            .iter()
            .map(|item| match item.body {
                ItemBody::UserTurn { .. } => "user",
                ItemBody::AssistantSegment { .. } => "segment",
                ItemBody::ToolCard { .. } => "tool",
                ItemBody::Delegation { .. } => "delegation",
                ItemBody::InjectedContext { .. } => "context",
                ItemBody::Notice { .. } => "notice",
            })
            .collect()
    }

    #[tokio::test]
    async fn load_transcript_folds_a_real_session_log() {
        let dir = TempDir::new().unwrap();
        let id = SessionId::generate(SessionType::Chat);
        let session_dir = write_log(dir.path(), &id, WIRE_LOG).await;

        let transcript = load_transcript(&session_dir).await.unwrap();

        assert_eq!(kinds(&transcript), ["user", "segment", "tool", "segment"]);
        let ItemBody::ToolCard { result, .. } = &transcript.items[2].body else {
            panic!("not a tool card");
        };
        assert_eq!(result.as_deref(), Some("[package]"));
        let ItemBody::AssistantSegment { text, usage, .. } = &transcript.items[3].body else {
            panic!("not a segment");
        };
        assert_eq!(text, "Use std::fs::read_to_string.");
        assert_eq!(
            usage.unwrap().cache_read_tokens,
            Some(12),
            "cache accounting must not be dropped on the way in"
        );
        assert_eq!(
            transcript.items[3].timestamp,
            Some("2026-08-11T12:00:05Z".parse().unwrap()),
            "a segment takes the time of the event that ended it"
        );
    }

    /// An older daemon wrote view lines into the same file as the wire
    /// lines. Each view line is part of the transcript.
    #[tokio::test]
    async fn load_transcript_reads_the_view_lines_of_an_older_daemon() {
        let dir = TempDir::new().unwrap();
        let id = SessionId::generate(SessionType::Chat);
        let log = [
            r#"{"type":"system","ts":"2026-08-11T12:00:00Z","content":"System"}"#,
            r#"{"type":"user","ts":"2026-08-11T12:00:01Z","content":"Hello"}"#,
            r#"{"type":"assistant","ts":"2026-08-11T12:00:02Z","content":"Hi!"}"#,
            r#"{"type":"clear","ts":"2026-08-11T12:00:03Z"}"#,
        ]
        .join("\n");
        let session_dir = write_log(dir.path(), &id, &format!("{log}\n{WIRE_LOG}")).await;

        let transcript = load_transcript(&session_dir).await.unwrap();

        assert_eq!(
            kinds(&transcript),
            ["context", "user", "segment", "notice", "user", "segment", "tool", "segment"]
        );
        assert!(matches!(
            &transcript.items[1].body,
            ItemBody::UserTurn { content, .. } if content == "Hello"
        ));
        assert!(matches!(
            &transcript.items[2].body,
            ItemBody::AssistantSegment { text, .. } if text == "Hi!"
        ));
    }

    #[tokio::test]
    async fn load_transcript_of_a_missing_log_is_empty_not_an_error() {
        let dir = TempDir::new().unwrap();
        let session_dir = dir.path().join("chat-20260811-1200-abcd");
        fs::create_dir_all(&session_dir).await.unwrap();

        assert!(load_transcript(&session_dir)
            .await
            .unwrap()
            .items
            .is_empty());
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
}
