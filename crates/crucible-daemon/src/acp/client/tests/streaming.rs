use super::test_path;
use crate::acp::client::types::{ClientConfig, StreamingState};
use crate::acp::client::CrucibleAcpClient;
use crate::acp::streaming::{StreamingCallback, StreamingChunk, TurnSummary};
use agent_client_protocol::schema::v1::SessionNotification;
use serde_json::json;

#[test]
fn streaming_state_merges_chunks_without_newlines() {
    let mut state = StreamingState::default();
    state.append_text("I'll rea");
    state.append_text("d a few notes from the kiln.");

    assert_eq!(
        state.accumulated_text,
        "I'll read a few notes from the kiln."
    );
}

#[test]
fn streaming_state_drops_whitespace_only_chunks() {
    let mut state = StreamingState::default();
    state.append_text("Hello");
    state.append_text("   ");
    state.append_text("World");

    assert_eq!(state.accumulated_text, "HelloWorld");
}

/// A second frame for the same call id merges into the first entry, so the
/// recorded call carries the fuller arguments of the later frame and the
/// table still holds one entry per id.
#[test]
fn tool_call_updates_existing_entry() {
    let mut client = make_client();
    let mut state = StreamingState::default();

    for path in ["PRIME", "PRIME.md"] {
        capture_apply(
            &mut client,
            &mut state,
            json!({
                "sessionId": "s1",
                "update": {
                    "sessionUpdate": "tool_call",
                    "toolCallId": "tool-42",
                    "title": "mcp__crucible__read_note",
                    "rawInput": {"path": path},
                },
            }),
        );
    }

    assert_eq!(
        state.tool_calls.len(),
        1,
        "same id must merge, not duplicate"
    );
    assert_eq!(
        state.tool_calls.args_of("tool-42").unwrap()["path"],
        json!("PRIME.md")
    );
}

/// Two calls with the same tool and arguments but different ids are two
/// entries.
#[test]
fn tool_calls_with_different_ids_are_both_recorded() {
    let mut client = make_client();
    let mut state = StreamingState::default();

    for id in ["call-1", "call-2"] {
        capture_apply(
            &mut client,
            &mut state,
            json!({
                "sessionId": "s1",
                "update": {
                    "sessionUpdate": "tool_call",
                    "toolCallId": id,
                    "title": "read_file",
                    "rawInput": {"path": "test.md"},
                },
            }),
        );
    }

    assert_eq!(state.tool_calls.len(), 2);
}

// =========================================================================
// Late-diff predicate tests
// Verify ToolCallUpdate carrying changed diffs emits a ToolDiffUpdate chunk,
// and that an unchanged repeat does NOT — preventing visual flashes when
// agents send idempotent tool_call updates.
// =========================================================================

fn make_client() -> CrucibleAcpClient {
    let config = ClientConfig {
        agent_path: test_path("test-agent"),
        agent_args: None,
        timeout_ms: Some(1000),
        ..Default::default()
    };
    CrucibleAcpClient::new(config)
}

fn capture_apply(
    client: &mut CrucibleAcpClient,
    state: &mut StreamingState,
    notification_json: serde_json::Value,
) -> Vec<StreamingChunk> {
    let notification: SessionNotification =
        serde_json::from_value(notification_json).expect("notification should deserialize");
    let collected: std::sync::Arc<std::sync::Mutex<Vec<StreamingChunk>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = collected.clone();
    let mut callback: StreamingCallback = Box::new(move |chunk| {
        sink.lock().unwrap().push(chunk);
        true
    });
    client.apply_session_update_with_callback(notification, state, &mut callback);
    let guard = collected.lock().unwrap();
    guard.clone()
}

#[test]
fn tool_call_update_with_changed_diffs_emits_diff_update_chunk() {
    let mut client = make_client();
    let mut state = StreamingState::default();

    // Initial tool_call with empty diffs (Claude Code defers diffs).
    capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "tool_call",
                "toolCallId": "tool-1",
                "title": "edit_file",
                "rawInput": {"path": "x.rs"},
            },
        }),
    );

    // Follow-up tool_call_update with diff content.
    let chunks = capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "tool-1",
                "content": [{
                    "type": "diff",
                    "path": "/tmp/x.rs",
                    "oldText": "fn old() {}\n",
                    "newText": "fn new() {}\n",
                }],
            },
        }),
    );

    assert!(
        chunks.iter().any(|c| matches!(c,
            StreamingChunk::ToolDiffUpdate { call_id, diffs }
                if call_id == "tool-1" && diffs.len() == 1
        )),
        "expected ToolDiffUpdate for changed diffs, got: {:?}",
        chunks
    );
}

#[test]
fn tool_call_update_with_unchanged_diffs_does_not_emit_diff_update() {
    let mut client = make_client();
    let mut state = StreamingState::default();

    let initial_diff = json!([{
        "type": "diff",
        "path": "/tmp/x.rs",
        "oldText": "old\n",
        "newText": "new\n",
    }]);

    // Initial tool_call carries a diff.
    capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "tool_call",
                "toolCallId": "tool-1",
                "title": "edit_file",
                "content": initial_diff.clone(),
            },
        }),
    );

    // Follow-up update repeats the same diff verbatim — should be silent.
    let chunks = capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "tool-1",
                "content": initial_diff,
            },
        }),
    );

    assert!(
        !chunks
            .iter()
            .any(|c| matches!(c, StreamingChunk::ToolDiffUpdate { .. })),
        "expected no ToolDiffUpdate for unchanged diff, got: {:?}",
        chunks
    );
}

/// An update for an unseen id that carries a title announces the call
/// itself, with the diffs on the `ToolStart`. No `ToolDiffUpdate` follows,
/// because nothing was announced before it.
#[test]
fn tool_call_update_with_a_title_for_an_unseen_id_announces_it_with_its_diffs() {
    let mut client = make_client();
    let mut state = StreamingState::default();

    let chunks = capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "ghost-1",
                "title": "edit_file",
                "content": [{
                    "type": "diff",
                    "path": "/tmp/y.rs",
                    "oldText": "a\n",
                    "newText": "b\n",
                }],
            },
        }),
    );

    assert_eq!(chunks.len(), 1, "got {chunks:?}");
    match &chunks[0] {
        StreamingChunk::ToolStart {
            id, name, diffs, ..
        } => {
            assert_eq!(id, "ghost-1");
            assert_eq!(name, "Edit File");
            assert_eq!(diffs.len(), 1);
        }
        other => panic!("expected ToolStart, got {other:?}"),
    }
}

/// An update for an unseen id with a diff and no title announces nothing.
/// The table holds the diff until a frame names the call, or until the turn
/// ends.
#[test]
fn tool_call_update_without_a_title_for_an_unseen_id_emits_nothing() {
    let mut client = make_client();
    let mut state = StreamingState::default();

    let chunks = capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "ghost-1",
                "content": [{
                    "type": "diff",
                    "path": "/tmp/y.rs",
                    "oldText": "a\n",
                    "newText": "b\n",
                }],
            },
        }),
    );

    assert!(chunks.is_empty(), "got {chunks:?}");
    assert_eq!(state.tool_calls.len(), 1);

    // The turn ends. The flush announces the call under the placeholder
    // label, with the diff it held, and then closes it with the stop reason.
    let flushed = state.tool_calls.flush("end_turn");
    assert_eq!(flushed.len(), 2, "got {flushed:?}");
    match &flushed[0] {
        StreamingChunk::ToolStart {
            id, name, diffs, ..
        } => {
            assert_eq!(id, "ghost-1");
            assert_eq!(name, "Unnamed tool");
            assert_eq!(diffs.len(), 1);
        }
        other => panic!("expected ToolStart, got {other:?}"),
    }
    match &flushed[1] {
        StreamingChunk::ToolEnd {
            id,
            name,
            result,
            error,
        } => {
            assert_eq!(id, "ghost-1");
            assert_eq!(name, "Unnamed tool");
            assert_eq!(result, &None);
            assert_eq!(error.as_deref(), Some("turn ended: end_turn"));
        }
        other => panic!("expected ToolEnd, got {other:?}"),
    }
}

/// A `ToolEnd` carries the name of its call, so the handle does not keep a
/// name table of its own.
#[test]
fn tool_end_carries_the_name_of_its_call() {
    let mut client = make_client();
    let mut state = StreamingState::default();

    capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "tool_call",
                "toolCallId": "tool-7",
                "title": "mcp__crucible__read_note",
            },
        }),
    );
    let chunks = capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "tool-7",
                "status": "completed",
                "rawOutput": "the note body",
            },
        }),
    );

    assert_eq!(
        chunks,
        vec![StreamingChunk::ToolEnd {
            id: "tool-7".into(),
            name: "Read Note".into(),
            result: Some("the note body".into()),
            error: None,
        }]
    );
}

/// The client counts what the turn showed the user. Text and thoughts
/// count; a tool announcement counts as a batch; whitespace counts as
/// nothing.
#[test]
fn turn_summary_reports_visible_content_and_announced_calls() {
    let mut client = make_client();

    let mut state = StreamingState::default();
    assert_eq!(
        state.summary(),
        TurnSummary {
            produced_content: false,
            announced_any: false,
        }
    );

    capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "  \n"},
            },
        }),
    );
    assert!(
        !state.summary().produced_content,
        "whitespace is not content"
    );

    capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "tool_call",
                "toolCallId": "tool-1",
                "title": "read_file",
            },
        }),
    );
    assert_eq!(
        state.summary(),
        TurnSummary {
            produced_content: false,
            announced_any: true,
        }
    );

    let mut state = StreamingState::default();
    capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "agent_thought_chunk",
                "content": {"type": "text", "text": "thinking"},
            },
        }),
    );
    assert!(state.summary().produced_content, "a thought is visible");
}

// ---------------------------------------------------------------------------
// describe_rpc_error — folding the agent-defined `data` into the message
// ---------------------------------------------------------------------------
//
// JSON-RPC only guarantees `code` and `message`. `data` is agent-defined, so
// each shape below is something an agent has been or could be observed to
// send, and none of them may panic or leak JSON noise into user-facing text.

use crate::acp::client::streaming::describe_rpc_error;

#[test]
fn describe_rpc_error_uses_message_when_there_is_no_data() {
    let err = json!({ "code": -32601, "message": "Method not found" });
    assert_eq!(describe_rpc_error(&err), "Method not found");
}

#[test]
fn describe_rpc_error_falls_back_when_message_is_missing() {
    assert_eq!(describe_rpc_error(&json!({ "code": -1 })), "Unknown error");
}

#[test]
fn describe_rpc_error_appends_data_message() {
    let err = json!({
        "code": -32603,
        "message": "Internal error",
        "data": { "message": "rate limit exceeded, retry in 30s" },
    });
    assert_eq!(
        describe_rpc_error(&err),
        "Internal error: rate limit exceeded, retry in 30s"
    );
}

#[test]
fn describe_rpc_error_unwraps_a_stringified_error_envelope() {
    // The codex-acp shape: `data.message` is itself a JSON document, and the
    // sentence worth reading is nested another level down under `error`.
    let err = json!({
        "code": -32603,
        "message": "Internal error",
        "data": {
            "message": r#"{"type":"error","status":400,"error":{"type":"invalid_request_error","message":"The 'gpt-5.2-codex' model is not supported when using Codex with a ChatGPT account."}}"#,
            "codex_error_info": "other",
        },
    });
    assert_eq!(
        describe_rpc_error(&err),
        "Internal error: The 'gpt-5.2-codex' model is not supported when using \
         Codex with a ChatGPT account."
    );
}

#[test]
fn describe_rpc_error_accepts_a_bare_string_data() {
    let err = json!({
        "code": -32000,
        "message": "Agent failure",
        "data": "model quota exhausted",
    });
    assert_eq!(
        describe_rpc_error(&err),
        "Agent failure: model quota exhausted"
    );
}

#[test]
fn describe_rpc_error_ignores_data_without_readable_text() {
    // An object with no `message`, a null, an empty object, an array, a
    // whitespace-only string: nothing to say, so say nothing rather than
    // rendering `null` or `{}` at the user.
    for data in [
        json!({ "retryable": true }),
        json!(null),
        json!({}),
        json!([1, 2, 3]),
        json!("   "),
        json!({ "message": "" }),
        json!({ "message": 42 }),
    ] {
        let err = json!({ "code": -32603, "message": "Internal error", "data": data });
        assert_eq!(
            describe_rpc_error(&err),
            "Internal error",
            "data {data} should contribute nothing"
        );
    }
}

#[test]
fn describe_rpc_error_does_not_repeat_a_duplicate_detail() {
    let err = json!({
        "code": -32603,
        "message": "Internal error",
        "data": { "message": "Internal error" },
    });
    assert_eq!(describe_rpc_error(&err), "Internal error");
}

#[test]
fn describe_rpc_error_does_not_repeat_a_detail_the_message_already_contains() {
    let err = json!({
        "code": -32000,
        "message": "auth failed: token expired",
        "data": { "message": "token expired" },
    });
    assert_eq!(describe_rpc_error(&err), "auth failed: token expired");
}

#[test]
fn describe_rpc_error_survives_a_non_object_error_payload() {
    // Nothing guarantees the peer sent an object at all.
    assert_eq!(describe_rpc_error(&json!("boom")), "Unknown error");
    assert_eq!(describe_rpc_error(&json!(null)), "Unknown error");
}

#[test]
fn describe_rpc_error_stops_unwrapping_self_referential_payloads() {
    // Deeply nested stringified envelopes must terminate, not recurse away
    // the stack. The exact text below the bound does not matter; returning
    // at all does. Eight levels is comfortably past MAX_DETAIL_DEPTH — and
    // stays small, since each round re-escapes the whole payload and so
    // roughly doubles it.
    let mut nested = String::from(r#"{"message":"innermost"}"#);
    for _ in 0..8 {
        nested = serde_json::to_string(&json!({ "message": nested })).unwrap();
    }
    let err = json!({ "code": -1, "message": "Internal error", "data": { "message": nested } });
    let rendered = describe_rpc_error(&err);
    assert!(rendered.starts_with("Internal error: "), "got {rendered:?}");
}

// ─── extract_tool_error ────────────────────────────────────────────────────

/// A failed call whose `rawOutput` is a content-block array must surface the
/// blocks' text as the error. This is the shape claude-agent-acp actually
/// sends on an MCP tool failure (`rawOutput: chunk.content`, i.e. the
/// tool_result content blocks) — the precise reason, e.g.
/// "File not found: …", is in there, and collapsing it to a generic
/// "Tool call failed" hides the only actionable part.
#[test]
fn failed_tool_error_surfaces_content_block_text() {
    use agent_client_protocol::schema::v1::ToolCallStatus;

    let raw = json!([
        {"type": "text", "text": "MCP error -32602: File not found: Concepts/Target.md"}
    ]);
    let err = CrucibleAcpClient::extract_tool_error(Some(ToolCallStatus::Failed), Some(&raw), &[]);
    assert_eq!(
        err.as_deref(),
        Some("MCP error -32602: File not found: Concepts/Target.md")
    );
}

/// An explicit `error` field still wins over content blocks.
#[test]
fn failed_tool_error_prefers_explicit_error_field() {
    use agent_client_protocol::schema::v1::ToolCallStatus;

    let raw = json!({
        "error": "explicit reason",
        "content": [{"type": "text", "text": "secondary text"}]
    });
    let err = CrucibleAcpClient::extract_tool_error(Some(ToolCallStatus::Failed), Some(&raw), &[]);
    assert_eq!(err.as_deref(), Some("explicit reason"));
}

/// With nothing usable in the output, the generic label remains the floor.
#[test]
fn failed_tool_error_without_detail_falls_back_to_generic() {
    use agent_client_protocol::schema::v1::ToolCallStatus;

    for raw in [
        None,
        Some(json!({})),
        Some(json!([])),
        Some(json!([{"type": "image"}])),
    ] {
        let err =
            CrucibleAcpClient::extract_tool_error(Some(ToolCallStatus::Failed), raw.as_ref(), &[]);
        assert_eq!(err.as_deref(), Some("Tool call failed"), "raw={raw:?}");
    }
}

/// The surfaced text is agent-authored: it renders as a one-line error label,
/// so it gets the same single-line sanitising and display cap as
/// `describe_rpc_error`'s output — control characters stripped, length elided.
#[test]
fn failed_tool_error_text_is_sanitized_and_capped() {
    use agent_client_protocol::schema::v1::ToolCallStatus;

    let hostile = format!("bad\x1b[31m\r{}", "x".repeat(4096));
    let raw = json!([{"type": "text", "text": hostile}]);
    let err = CrucibleAcpClient::extract_tool_error(Some(ToolCallStatus::Failed), Some(&raw), &[])
        .expect("failed status must yield an error");
    assert!(
        !err.contains('\x1b'),
        "control chars must be stripped: {err:?}"
    );
    assert!(
        err.chars().count() <= 520,
        "error label must be display-capped, got {} chars",
        err.chars().count()
    );
}

/// claude-agent-acp announces a tool call with no `rawInput` and only
/// supplies the arguments in a later `tool_call_update`. Those late args must
/// be re-emitted the way late diffs are — otherwise every downstream surface
/// (session log, recording, TUI card) shows `args: {}` forever, which is
/// exactly what made the demo recording's failed `read_note` calls
/// undiagnosable.
#[test]
fn tool_call_update_with_late_raw_input_emits_args_update_chunk() {
    let mut client = make_client();
    let mut state = StreamingState::default();

    capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "tool_call",
                "toolCallId": "tool-1",
                "title": "Read Note",
            },
        }),
    );

    let chunks = capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "tool-1",
                "rawInput": {"path": "Concepts/Target.md"},
            },
        }),
    );

    assert!(
        chunks.iter().any(|c| matches!(c,
            StreamingChunk::ToolArgsUpdate { call_id, arguments }
                if call_id == "tool-1"
                    && arguments == &json!({"path": "Concepts/Target.md"})
        )),
        "expected ToolArgsUpdate for late rawInput, got: {chunks:?}"
    );
}

/// An update repeating the arguments the call was announced with is silent —
/// re-emitting an identical snapshot has no informational gain.
#[test]
fn tool_call_update_with_unchanged_raw_input_does_not_emit_args_update() {
    let mut client = make_client();
    let mut state = StreamingState::default();

    capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "tool_call",
                "toolCallId": "tool-1",
                "title": "Read Note",
                "rawInput": {"path": "Concepts/Target.md"},
            },
        }),
    );

    let chunks = capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "tool-1",
                "rawInput": {"path": "Concepts/Target.md"},
            },
        }),
    );

    assert!(
        !chunks
            .iter()
            .any(|c| matches!(c, StreamingChunk::ToolArgsUpdate { .. })),
        "unchanged rawInput must not re-emit, got: {chunks:?}"
    );
}

/// Hermes sends `rawOutput: null` for most tools and puts the result text in
/// `content` blocks. A completed update with no `rawOutput` must read the
/// text blocks as the result. Diff blocks are not text.
#[test]
fn completed_update_without_raw_output_reads_content_text() {
    let mut client = make_client();
    let mut state = StreamingState::default();
    let chunks = capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "tc-1",
                "title": "read_file",
                "kind": "read",
                "status": "completed",
                "rawOutput": null,
                "content": [
                    {"type": "content", "content": {"type": "text", "text": "line one"}},
                    {"type": "diff", "path": "/tmp/a.rs", "oldText": "a", "newText": "b"},
                    {"type": "content", "content": {"type": "text", "text": "line two"}}
                ]
            }
        }),
    );
    let end = chunks
        .iter()
        .find_map(|c| match c {
            StreamingChunk::ToolEnd { result, error, .. } => Some((result.clone(), error.clone())),
            _ => None,
        })
        .expect("completed update must emit ToolEnd");
    assert_eq!(end, (Some("line one\nline two".to_string()), None));
}

/// A failed update with no `rawOutput` must read the reason from the
/// content text, not collapse to the generic label.
#[test]
fn failed_update_without_raw_output_reads_content_text() {
    let mut client = make_client();
    let mut state = StreamingState::default();
    let chunks = capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "tc-1",
                "title": "terminal",
                "kind": "execute",
                "status": "failed",
                "rawOutput": null,
                "content": [
                    {"type": "content", "content": {"type": "text", "text": "Error executing tool 'terminal': exit 2"}}
                ]
            }
        }),
    );
    let end = chunks
        .iter()
        .find_map(|c| match c {
            StreamingChunk::ToolEnd { result, error, .. } => Some((result.clone(), error.clone())),
            _ => None,
        })
        .expect("failed update must emit ToolEnd");
    assert_eq!(
        end,
        (
            Some("Error executing tool 'terminal': exit 2".to_string()),
            Some("Error executing tool 'terminal': exit 2".to_string())
        )
    );
}

/// When both exist, `rawOutput` is the result and content is ignored.
#[test]
fn raw_output_wins_over_content_when_both_exist() {
    let mut client = make_client();
    let mut state = StreamingState::default();
    let chunks = capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "tool_call_update",
                "toolCallId": "tc-1",
                "title": "search",
                "status": "completed",
                "rawOutput": {"hits": 3},
                "content": [
                    {"type": "content", "content": {"type": "text", "text": "3 hits"}}
                ]
            }
        }),
    );
    let result = chunks
        .iter()
        .find_map(|c| match c {
            StreamingChunk::ToolEnd { result, .. } => Some(result.clone()),
            _ => None,
        })
        .expect("completed update must emit ToolEnd");
    assert_eq!(result.as_deref(), Some(r#"{"hits":3}"#));
}

/// Hermes drains a queued prompt inside one `session/prompt` reply. It then
/// streams the queued text as a `user_message_chunk`. The chunk is the
/// user's own text. It must emit nothing, and it must not enter the answer.
#[test]
fn user_message_chunk_emits_nothing_and_stays_out_of_the_answer() {
    let mut client = make_client();
    let mut state = StreamingState::default();
    let chunks = capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s1",
            "update": {
                "sessionUpdate": "user_message_chunk",
                "content": {"type": "text", "text": "also delete the cache"}
            }
        }),
    );
    assert!(
        chunks.is_empty(),
        "a user chunk must emit no StreamingChunk"
    );
    assert_eq!(state.accumulated_text, "");
    assert!(!state.produced_content);
}

#[test]
fn usage_update_emits_a_context_window_chunk() {
    // The claude wire shape, recorded in
    // `tests/fixtures/acp/recorded/claude/basic-chat.jsonl`. The typed
    // parse must carry it; no raw reader runs ahead of it.
    let mut client = make_client();
    let mut state = StreamingState::default();
    let chunks = capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "c299d62f",
            "update": {
                "sessionUpdate": "usage_update",
                "used": 22700,
                "size": 1_000_000,
                "cost": { "amount": 0.14204, "currency": "USD" }
            }
        }),
    );
    assert_eq!(
        chunks,
        vec![StreamingChunk::ContextWindow {
            used: 22700,
            limit: 1_000_000
        }]
    );
}

#[test]
fn a_zero_size_usage_update_is_not_a_window() {
    // An explicit `size: 0` describes no window. To pass it on emits
    // `context_limit_resolved { limit: 0, source: Agent }`, a claim that
    // the window is resolved while the statusline renders the no-data
    // state. To report nothing keeps unresolved unresolved.
    let mut client = make_client();
    let mut state = StreamingState::default();
    let chunks = capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "s",
            "update": { "sessionUpdate": "usage_update", "used": 22700, "size": 0 }
        }),
    );
    assert!(chunks.is_empty(), "size 0 must not become a ContextWindow");
}

#[test]
fn a_zero_used_usage_update_is_a_window() {
    // A fresh turn used nothing yet. That is a real reading of a real
    // window, so only `size` is refused for a zero value.
    let mut client = make_client();
    let mut state = StreamingState::default();
    let chunks = capture_apply(
        &mut client,
        &mut state,
        json!({
            "sessionId": "ses_257dac",
            "update": {
                "sessionUpdate": "usage_update",
                "used": 0,
                "size": 200_000,
                "cost": { "amount": 0, "currency": "USD" }
            }
        }),
    );
    assert_eq!(
        chunks,
        vec![StreamingChunk::ContextWindow {
            used: 0,
            limit: 200_000
        }]
    );
}
