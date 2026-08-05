//! Boundary tests for the ACP streaming client.
//!
//! Split out of `streaming.rs` for the 1000-line file-size gate, and attached
//! with `#[path]` rather than moved into `tests/` because `describe_rpc_error`
//! and `apply_session_update_with_callback` are crate-private.

use super::*;
use crate::acp::client::ClientConfig;

fn test_client() -> CrucibleAcpClient {
    CrucibleAcpClient::new(ClientConfig {
        agent_path: std::path::PathBuf::from("/nonexistent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: None,
        max_retries: None,
    })
}

#[test]
fn cancel_notification_is_a_valid_jsonrpc_notification() {
    let n = build_cancel_notification("sess-123");
    assert_eq!(n["jsonrpc"], "2.0");
    assert_eq!(n["method"], "session/cancel");
    assert_eq!(n["params"]["sessionId"], "sess-123");
    // Notifications MUST NOT carry an id.
    assert!(
        n.get("id").is_none(),
        "cancel is a notification, not a request"
    );
}

#[test]
fn streaming_callback_returning_false_marks_state_cancelled() {
    use agent_client_protocol::SessionNotification;

    let mut client = test_client();
    let mut state = StreamingState::default();
    // A callback that returns `false` models the daemon's turn stream being
    // dropped (receiver gone) — i.e. the user cancelled. The read loop uses
    // `state.cancelled` to decide to send `session/cancel`.
    let mut cb: StreamingCallback = Box::new(|_chunk| false);

    let notification: SessionNotification = serde_json::from_value(serde_json::json!({
        "sessionId": "s1",
        "update": {
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": "partial answer" }
        }
    }))
    .expect("valid agent_message_chunk notification");

    client.apply_session_update_with_callback(notification, &mut state, &mut cb);

    assert!(
        state.cancelled,
        "a false callback (dropped receiver) must mark the turn cancelled"
    );
}

/// A thought must not enter the answer accumulator, and must not be able to
/// mask a later answer chunk through `is_duplicate_resend`.
#[test]
fn thought_chunks_stay_out_of_the_answer_text() {
    use agent_client_protocol::SessionNotification;

    let mut client = test_client();
    let mut state = StreamingState::default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut cb = crate::acp::streaming::channel_callback(tx);

    let thought: SessionNotification = serde_json::from_value(serde_json::json!({
        "sessionId": "s1",
        "update": {
            "sessionUpdate": "agent_thought_chunk",
            "content": { "type": "text", "text": "weigh the options" }
        }
    }))
    .expect("valid agent_thought_chunk notification");
    client.apply_session_update_with_callback(thought, &mut state, &mut cb);

    // The same words then arrive as the answer. If the thought had been
    // appended to `accumulated_text`, the resend guard would drop it.
    let answer: SessionNotification = serde_json::from_value(serde_json::json!({
        "sessionId": "s1",
        "update": {
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": "weigh the options" }
        }
    }))
    .expect("valid agent_message_chunk notification");
    client.apply_session_update_with_callback(answer, &mut state, &mut cb);
    drop(cb);

    let mut chunks = Vec::new();
    while let Ok(chunk) = rx.try_recv() {
        chunks.push(chunk);
    }

    assert_eq!(
        chunks,
        vec![
            StreamingChunk::Thinking("weigh the options".to_string()),
            StreamingChunk::Text("weigh the options".to_string()),
        ],
        "reasoning and the answer share one accumulator, so one masked the other"
    );
    assert_eq!(
        state.formatted_output(),
        "weigh the options",
        "reasoning leaked into the assistant's answer text"
    );
}

/// Every agent-authored string this function emits passes through the
/// sanitiser, so one hostile payload can be threaded through all of them at
/// once: reasoning, narration, the tool title, and the answer accumulator
/// that gets persisted.
///
/// The C1 introducer is the one worth spelling out: `crucible-oil`'s
/// `blit_line` filter only recognises the 7-bit `\x1b[…m` form, so `\u{9b}`
/// reaches the terminal as `C2 9B` and is decoded back into CSI.
#[test]
fn agent_text_is_sanitised_before_it_leaves_the_acp_boundary() {
    use agent_client_protocol::SessionNotification;

    let mut client = test_client();
    let mut state = StreamingState::default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut cb = crate::acp::streaming::channel_callback(tx);

    let hostile = "a\u{9b}2Jb\u{7}c\rd\u{202E}e";

    for (kind, text) in [
        ("agent_thought_chunk", hostile),
        ("agent_message_chunk", hostile),
    ] {
        let notification: SessionNotification = serde_json::from_value(serde_json::json!({
            "sessionId": "s1",
            "update": { "sessionUpdate": kind, "content": { "type": "text", "text": text } }
        }))
        .expect("valid notification");
        client.apply_session_update_with_callback(notification, &mut state, &mut cb);
    }

    let tool: SessionNotification = serde_json::from_value(serde_json::json!({
        "sessionId": "s1",
        "update": {
            "sessionUpdate": "tool_call",
            "toolCallId": "t1",
            "title": "read\u{202E}txt.exe",
            "status": "pending"
        }
    }))
    .expect("valid tool_call notification");
    client.apply_session_update_with_callback(tool, &mut state, &mut cb);
    drop(cb);

    let mut chunks = Vec::new();
    while let Ok(chunk) = rx.try_recv() {
        chunks.push(chunk);
    }

    let clean = "a2Jbcde";
    assert_eq!(
        chunks,
        vec![
            StreamingChunk::Thinking(clean.to_string()),
            StreamingChunk::Text(clean.to_string()),
            StreamingChunk::ToolStart {
                name: "Readtxt.exe".to_string(),
                id: "t1".to_string(),
                arguments: None,
                diffs: vec![],
            },
        ],
        "agent-controlled text reached the turn stream unsanitised"
    );
    // `formatted_output` is the answer plus a rendering of the tool call,
    // so assert on what must be absent rather than on the exact string.
    let persisted = state.formatted_output();
    assert!(
        persisted.starts_with(clean),
        "the persisted answer kept its control characters: {persisted:?}"
    );
    assert!(
        !persisted
            .chars()
            .any(|c| !matches!(c, '\n' | '\t') && crucible_core::text::is_display_hostile(c)),
        "the persisted turn output still carries hostile characters: {persisted:?}"
    );
    assert_eq!(
        state.title_for_tool("t1").as_deref(),
        Some("readtxt.exe"),
        "the recorded tool title kept its bidi override"
    );
}

/// Prose is not a status bar: paragraphs and indented code are the author's
/// layout, and the renderers downstream already treat them as such.
#[test]
fn newlines_and_tabs_survive_sanitising_of_agent_prose() {
    use agent_client_protocol::SessionNotification;

    let mut client = test_client();
    let mut state = StreamingState::default();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut cb = crate::acp::streaming::channel_callback(tx);

    let prose = "First:\n\n\tlet x = 1;\n\nDone.";
    let notification: SessionNotification = serde_json::from_value(serde_json::json!({
        "sessionId": "s1",
        "update": {
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": prose }
        }
    }))
    .expect("valid agent_message_chunk notification");
    client.apply_session_update_with_callback(notification, &mut state, &mut cb);
    drop(cb);

    assert_eq!(
        rx.try_recv().ok(),
        Some(StreamingChunk::Text(prose.to_string())),
        "sanitising flattened legitimate prose layout"
    );
}

/// `error.data` is agent-authored too, and this branch made it visible.
#[test]
fn rpc_error_prose_is_sanitised() {
    let described = describe_rpc_error(&serde_json::json!({
        "code": -32603,
        "message": "Internal\u{9b}2J error",
        "data": { "message": "quota\u{202E}dexe.gpj exceeded" }
    }));

    assert_eq!(described, "Internal2J error: quotadexe.gpj exceeded");
}

#[test]
fn streaming_callback_returning_true_leaves_state_running() {
    use agent_client_protocol::SessionNotification;

    let mut client = test_client();
    let mut state = StreamingState::default();
    let mut cb: StreamingCallback = Box::new(|_chunk| true);

    let notification: SessionNotification = serde_json::from_value(serde_json::json!({
        "sessionId": "s1",
        "update": {
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": "still going" }
        }
    }))
    .expect("valid agent_message_chunk notification");

    client.apply_session_update_with_callback(notification, &mut state, &mut cb);

    assert!(
        !state.cancelled,
        "an active receiver must not trigger cancellation"
    );
}
