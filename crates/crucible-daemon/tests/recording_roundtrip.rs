//! Round-trip integration tests: RecordingWriter → JSONL → ReplaySession
//!
//! Verifies all event types survive serialization through the recording pipeline.
//! Uses speed=0.0 for instant replay (no timing delays).

mod replay_harness;

use crucible_core::protocol::SessionEventMessage;
use crucible_core::session::RecordingMode;
use crucible_daemon::recording::RecordingWriter;
use crucible_daemon::replay::ReplaySession;
use replay_harness::{
    create_test_recording, message_complete, text_delta, thinking, tool_call, tool_result,
    user_message,
};
use serde_json::json;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::broadcast;

/// Collect all replay events until `replay_complete`, with a timeout guard.
async fn collect_replay_events(
    rx: &mut broadcast::Receiver<SessionEventMessage>,
) -> Vec<SessionEventMessage> {
    let mut events = Vec::new();
    loop {
        let evt = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("replay timed out")
            .expect("broadcast recv failed");
        if evt.event == "replay_complete" {
            break;
        }
        events.push(evt);
    }
    events
}

/// Helper: write events through RecordingWriter and replay them back.
async fn roundtrip(events: Vec<SessionEventMessage>) -> Vec<SessionEventMessage> {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("recording.jsonl");

    let (writer, tx) = RecordingWriter::new(
        path.clone(),
        "record-session".to_string(),
        RecordingMode::Granular,
        None,
    );
    let handle = writer.start();

    for event in events {
        tx.send(event).await.expect("send event");
    }
    drop(tx);
    handle.await.expect("join writer").expect("writer ok");

    let (replay_tx, mut replay_rx) = broadcast::channel(64);
    let replay =
        ReplaySession::new(path, 0.0, replay_tx, "replay-session".to_string()).expect("replay");
    let replay_handle = replay.start();

    let collected = collect_replay_events(&mut replay_rx).await;
    replay_handle
        .await
        .expect("join replay")
        .expect("replay ok");
    collected
}

// ---------------------------------------------------------------------------
// Test 1: text events survive roundtrip
// ---------------------------------------------------------------------------
#[tokio::test]
async fn text_events_survive_roundtrip() {
    let events = vec![
        SessionEventMessage::text_delta("record-session", "content1"),
        SessionEventMessage::text_delta("record-session", "content2"),
        SessionEventMessage::message_complete("record-session", "msg-1", "content1content2", None),
    ];

    let replayed = roundtrip(events).await;

    let names: Vec<&str> = replayed.iter().map(|e| e.event.as_str()).collect();
    assert_eq!(names, vec!["text_delta", "text_delta", "message_complete"]);
    assert!(
        replayed.iter().all(|e| e.session_id == "replay-session"),
        "all events should have replay session id"
    );
    assert_eq!(replayed[0].data["content"], "content1");
    assert_eq!(replayed[1].data["content"], "content2");
}

// ---------------------------------------------------------------------------
// Test 2: tool_call and tool_result survive roundtrip
// ---------------------------------------------------------------------------
#[tokio::test]
async fn tool_call_and_result_survive_roundtrip() {
    let events = vec![
        SessionEventMessage::tool_call(
            "record-session",
            "call-42",
            "semantic_search",
            json!({"query": "rust async"}),
        ),
        SessionEventMessage::tool_result(
            "record-session",
            "call-42",
            "semantic_search",
            json!({"results": [{"title": "Async Rust"}]}),
        ),
    ];

    let replayed = roundtrip(events).await;

    let names: Vec<&str> = replayed.iter().map(|e| e.event.as_str()).collect();
    assert_eq!(names, vec!["tool_call", "tool_result"]);

    assert_eq!(replayed[0].data["tool"], "semantic_search");
    assert_eq!(replayed[0].data["call_id"], "call-42");
    assert_eq!(replayed[0].data["args"]["query"], "rust async");

    assert_eq!(replayed[1].data["tool"], "semantic_search");
    assert_eq!(
        replayed[1].data["result"]["results"][0]["title"],
        "Async Rust"
    );
}

// ---------------------------------------------------------------------------
// Test 3: thinking event survives roundtrip
// ---------------------------------------------------------------------------
#[tokio::test]
async fn thinking_event_survives_roundtrip() {
    let events = vec![
        SessionEventMessage::thinking("record-session", "I should search for this"),
        SessionEventMessage::text_delta("record-session", "Here is what I found"),
    ];

    let replayed = roundtrip(events).await;

    let names: Vec<&str> = replayed.iter().map(|e| e.event.as_str()).collect();
    assert_eq!(names, vec!["thinking", "text_delta"]);

    assert_eq!(replayed[0].data["content"], "I should search for this");
    assert_eq!(replayed[1].data["content"], "Here is what I found");
}

// ---------------------------------------------------------------------------
// Test 4: delegation events survive roundtrip (Optional field preservation)
// ---------------------------------------------------------------------------
#[tokio::test]
async fn delegation_events_survive_roundtrip() {
    let events = vec![
        SessionEventMessage::new(
            "record-session",
            "delegation_spawned",
            json!({
                "id": "subagent-1",
                "prompt": "Analyze this codebase",
                "target_agent": "cursor",
                "working_directory": "/home/user/project"
            }),
        ),
        SessionEventMessage::new(
            "record-session",
            "delegation_completed",
            json!({
                "id": "subagent-1",
                "summary": "Found 3 issues in the authentication module",
                "exit_code": 0
            }),
        ),
    ];

    let replayed = roundtrip(events).await;

    let names: Vec<&str> = replayed.iter().map(|e| e.event.as_str()).collect();
    assert_eq!(names, vec!["delegation_spawned", "delegation_completed"]);

    let spawned = &replayed[0].data;
    assert_eq!(spawned["id"], "subagent-1");
    assert_eq!(spawned["target_agent"], "cursor");
    assert_eq!(spawned["prompt"], "Analyze this codebase");
    assert_eq!(spawned["working_directory"], "/home/user/project");

    let completed = &replayed[1].data;
    assert_eq!(completed["id"], "subagent-1");
    assert_eq!(
        completed["summary"],
        "Found 3 issues in the authentication module"
    );
    assert_eq!(completed["exit_code"], 0);
}

// ---------------------------------------------------------------------------
// Test 5: mixed event stream roundtrip preserves order and all types
// ---------------------------------------------------------------------------
#[tokio::test]
async fn mixed_event_stream_roundtrip() {
    let events = vec![
        SessionEventMessage::user_message("record-session", "msg-1", "What is Rust?"),
        SessionEventMessage::thinking("record-session", "Let me reason about this"),
        SessionEventMessage::text_delta("record-session", "Rust is "),
        SessionEventMessage::text_delta("record-session", "a systems "),
        SessionEventMessage::text_delta("record-session", "programming language."),
        SessionEventMessage::tool_call(
            "record-session",
            "call-1",
            "read_file",
            json!({"path": "README.md"}),
        ),
        SessionEventMessage::tool_result(
            "record-session",
            "call-1",
            "read_file",
            json!({"content": "# Rust\nSystems language"}),
        ),
        SessionEventMessage::text_delta("record-session", " It emphasizes safety."),
        SessionEventMessage::message_complete(
            "record-session",
            "msg-2",
            "Rust is a systems programming language. It emphasizes safety.",
            None,
        ),
    ];

    let replayed = roundtrip(events).await;

    assert_eq!(
        replayed.len(),
        9,
        "expected 9 events, got {}",
        replayed.len()
    );

    let names: Vec<&str> = replayed.iter().map(|e| e.event.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "user_message",
            "thinking",
            "text_delta",
            "text_delta",
            "text_delta",
            "tool_call",
            "tool_result",
            "text_delta",
            "message_complete",
        ]
    );

    assert_eq!(replayed[0].data["content"], "What is Rust?");
    assert_eq!(replayed[1].data["content"], "Let me reason about this");
    assert_eq!(replayed[5].data["tool"], "read_file");
    assert_eq!(
        replayed[8].data["full_response"],
        "Rust is a systems programming language. It emphasizes safety."
    );
}

// ---------------------------------------------------------------------------
// Test 6: empty recording produces only replay_complete
// ---------------------------------------------------------------------------
#[tokio::test]
async fn empty_recording_produces_only_replay_complete() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("empty.jsonl");

    let (writer, tx) = RecordingWriter::new(
        path.clone(),
        "empty-session".to_string(),
        RecordingMode::Granular,
        None,
    );
    let handle = writer.start();
    drop(tx);
    handle.await.expect("join writer").expect("writer ok");

    let (replay_tx, mut replay_rx) = broadcast::channel(16);
    let replay =
        ReplaySession::new(path, 0.0, replay_tx, "replay-empty".to_string()).expect("replay");
    let replay_handle = replay.start();

    let evt = tokio::time::timeout(Duration::from_secs(5), replay_rx.recv())
        .await
        .expect("replay timed out")
        .expect("broadcast recv failed");

    assert_eq!(evt.event, "replay_complete");
    assert_eq!(evt.data["total_events"], 0);
    assert_eq!(evt.session_id, "replay-empty");

    replay_handle
        .await
        .expect("join replay")
        .expect("replay ok");
}

// ---------------------------------------------------------------------------
// Test 7: replay_harness fixture also round-trips through ReplaySession
// ---------------------------------------------------------------------------
#[tokio::test]
async fn harness_fixture_roundtrips_through_replay() {
    let fixture_events = vec![
        text_delta("alpha"),
        thinking("reasoning step"),
        tool_call("search", json!({"q": "test"})),
        tool_result("search", json!({"found": true})),
        text_delta("beta"),
        user_message("follow up"),
        message_complete(),
    ];

    let path = create_test_recording("fixture-session", fixture_events);

    let (replay_tx, mut replay_rx) = broadcast::channel(64);
    let replay = ReplaySession::new(
        path.to_path_buf(),
        0.0,
        replay_tx,
        "replay-fixture".to_string(),
    )
    .expect("replay");
    let replay_handle = replay.start();

    let events = collect_replay_events(&mut replay_rx).await;
    replay_handle
        .await
        .expect("join replay")
        .expect("replay ok");

    assert_eq!(events.len(), 7);
    let names: Vec<&str> = events.iter().map(|e| e.event.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "text_delta",
            "thinking",
            "tool_call",
            "tool_result",
            "text_delta",
            "user_message",
            "message_complete",
        ]
    );

    assert!(events.iter().all(|e| e.session_id == "replay-fixture"));
    assert_eq!(events[0].data["content"], "alpha");
    assert_eq!(events[1].data["content"], "reasoning step");
    assert_eq!(events[2].data["name"], "search");
}
