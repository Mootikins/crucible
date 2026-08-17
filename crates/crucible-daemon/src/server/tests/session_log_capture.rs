//! Provenance for `assets/fixtures/session_log_wire.jsonl`.
//!
//! The fixture is a real `session.jsonl`: it was produced by `persist_event`
//! (`server/core.rs`) writing events off a live `TestServer`'s broadcast
//! channel, not hand-written. That matters because the read-path bug this
//! guards against was invisible for as long as it was — every reader test
//! built its input with `SessionWriter`, which no production path ever
//! called, so the tests agreed with each other about a format the daemon
//! did not write.
//!
//! Deliberately not `#[ignore]`d: it needs no daemon binary, no network and
//! no model — an in-process `Server` over a `TempDir` socket. Gating it
//! would defeat the point, which is that CI notices drift.

use super::*;
use crate::event_emitter::emit_event;
use crucible_core::protocol::SessionEventMessage;

pub(super) fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fixtures/session_log_wire.jsonl")
}

/// The turn the fixture records: one user message, a thinking block, a tool
/// call and its result, and the completed response with usage.
fn scripted_turn(session_id: &str) -> Vec<SessionEventMessage> {
    vec![
        SessionEventMessage::user_message(session_id, "m1", "how do I read a file"),
        SessionEventMessage::thinking(session_id, "consider std::fs"),
        SessionEventMessage::tool_call(
            session_id,
            "c1",
            "read_file",
            json!({ "path": "Cargo.toml" }),
        ),
        SessionEventMessage::tool_result(
            session_id,
            "c1",
            "read_file",
            json!("[package]\nname = \"example\""),
        ),
        SessionEventMessage::message_complete(
            session_id,
            "m1",
            "Use std::fs::read_to_string.",
            Some(&crucible_core::traits::llm::TokenUsage {
                prompt_tokens: 25,
                completion_tokens: 75,
                total_tokens: 100,
                cache_read_tokens: Some(12),
                cache_creation_tokens: None,
            }),
        ),
    ]
}

/// Strip the fields that legitimately differ run to run: the generated
/// session id, wall-clock `timestamp`, and the process-global `seq`.
/// Everything else is the daemon's own work and must match byte for byte.
fn normalize(jsonl: &str) -> Vec<Value> {
    jsonl
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut v: Value = serde_json::from_str(l).expect("captured line must be JSON");
            let obj = v.as_object_mut().unwrap();
            obj.remove("session_id");
            obj.remove("timestamp");
            obj.remove("seq");
            v
        })
        .collect()
}

#[tokio::test]
async fn the_committed_session_log_is_what_the_daemon_writes() {
    let server = TestServer::start().await;
    let _kiln_path = server.kiln_path.clone();
    let event_tx = server.event_tx.clone();
    let mut client = server.connect().await;

    let session_id = create_chat_session(&mut client, TestServer::KILN, 1).await;

    // `emit_event`, not a bare `send`: the stamping it applies
    // (`event_emitter.rs`'s `stamp_event`) is part of what lands on disk.
    for event in scripted_turn(&session_id) {
        emit_event(&event_tx, event);
    }

    let jsonl_path = server
        .sessions_root()
        .join(&session_id)
        .join("session.jsonl");
    let captured = wait_for_lines(&jsonl_path, 5).await;
    server.shutdown().await;

    let expected = std::fs::read_to_string(fixture_path()).unwrap_or_default();
    if normalize(&captured) != normalize(&expected) {
        let new_path = fixture_path().with_extension("jsonl.new");
        std::fs::write(&new_path, &captured).unwrap();
        panic!(
            "the daemon no longer writes the committed session log.\n\
             Captured output written to {}.\n\
             Read it, decide whether the new shape is right, and only then\n\
             `mv` it over {}.",
            new_path.display(),
            fixture_path().display()
        );
    }
}
