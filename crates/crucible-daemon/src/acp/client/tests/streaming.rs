use std::io::Write;

use serde_json::json;
use tempfile::NamedTempFile;

use super::{test_path, upsert_tool_info};
use crate::acp::client::types::{ClientConfig, StreamingState};
use crate::acp::client::CrucibleAcpClient;
use crate::acp::streaming::{StreamingCallback, StreamingChunk};
use agent_client_protocol::SessionNotification;
use crucible_core::types::acp::ToolCallInfo;

#[test]
fn streaming_state_merges_chunks_without_newlines() {
    let mut state = StreamingState::default();
    state.append_text("I'll rea");
    state.append_text("d a few notes from the kiln.");

    assert_eq!(
        state.formatted_output(),
        "I'll read a few notes from the kiln."
    );
}

#[test]
fn streaming_state_adds_padding_after_tools() {
    let config = ClientConfig {
        agent_path: test_path("agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();
    state.append_text("First chunk");

    let tool_call = ToolCallInfo::new("test_tool");
    client.record_tool_call(tool_call, &mut state);
    state.append_text("Response after the tool call.");

    assert_eq!(
        state.formatted_output(),
        "First chunk\n\n  ▷ Test Tool()\n\nResponse after the tool call."
    );
}

#[test]
fn tool_call_indents_after_text() {
    let config = ClientConfig {
        agent_path: test_path("test-agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();

    state.append_text("Hello");

    client.record_tool_call(
        ToolCallInfo::new("mcp__crucible__read_note")
            .with_id("tool-1")
            .with_arguments(json!({"path": "PRIME"})),
        &mut state,
    );

    state.append_text("World");

    let output = state.formatted_output();
    // Tool block has blank line before and after
    assert!(output.contains("Hello\n\n  ▷ Read Note"));
    assert!(output.contains("\n\nWorld"));
}

#[test]
fn tool_call_updates_existing_entry() {
    let config = ClientConfig {
        agent_path: test_path("test-agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();

    client.record_tool_call(
        ToolCallInfo::new("mcp__crucible__read_note")
            .with_id("tool-42")
            .with_arguments(json!({"path": "PRIME"})),
        &mut state,
    );

    client.record_tool_call(
        ToolCallInfo::new("mcp__crucible__read_note")
            .with_id("tool-42")
            .with_arguments(json!({"path": "PRIME.md"})),
        &mut state,
    );

    let output = state.formatted_output();
    assert_eq!(output.matches("▷ Read Note").count(), 1);
    assert!(output.contains("PRIME.md"));
}

#[test]
fn test_formatted_output_includes_diff() {
    let config = ClientConfig {
        agent_path: test_path("agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();

    // Create a temp file with initial content
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "old content").unwrap();
    let path = temp_file.path().to_string_lossy().to_string();

    // Record a write tool call
    client.record_tool_call(
        ToolCallInfo::new("update_note")
            .with_id("tool-1")
            .with_arguments(json!({
                "path": path,
                "content": "new content\n"
            })),
        &mut state,
    );

    let output = state.formatted_output();
    assert!(output.contains("▷ Update Note"), "Should have tool label");
    assert!(
        output.contains("-old content"),
        "Should show deleted line in diff"
    );
    assert!(
        output.contains("+new content"),
        "Should show inserted line in diff"
    );
}

// =========================================================================
// RED Tests: StreamingState Formatting Edge Cases
// These tests are designed to expose formatting issues (TDD approach)
// =========================================================================

#[test]
fn test_streaming_state_empty_text_handling() {
    // RED: Verify whitespace-only chunks don't create spurious newlines
    let mut state = StreamingState::default();
    state.append_text("Hello");
    state.append_text("   "); // whitespace only - should be ignored
    state.append_text("World");

    let output = state.formatted_output();
    // Whitespace-only text is ignored by append_text, so Hello and World
    // should be concatenated without extra spacing
    assert!(
        !output.contains("\n\n"),
        "Should not have double newlines from whitespace: {:?}",
        output
    );
    assert_eq!(output.trim(), "HelloWorld");
}

#[test]
fn test_streaming_state_consecutive_tools_no_double_spacing() {
    // RED: Multiple consecutive tools should be in one block with single spacing
    let config = ClientConfig {
        agent_path: test_path("test-agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();

    client.record_tool_call(ToolCallInfo::new("tool1").with_id("t1"), &mut state);
    client.record_tool_call(ToolCallInfo::new("tool2").with_id("t2"), &mut state);
    client.record_tool_call(ToolCallInfo::new("tool3").with_id("t3"), &mut state);

    let output = state.formatted_output();
    // Should only have one blank line before the tool block, not between each tool
    // The tool block should have format: "\n\n  ▷ tool1()\n  ▷ tool2()\n  ▷ tool3()\n\n"
    let tool_section: &str = output.trim();
    let blank_line_pairs = tool_section.matches("\n\n").count();
    assert!(
        blank_line_pairs <= 1,
        "Should have max 1 blank line separator at start, got {} in: {:?}",
        blank_line_pairs,
        output
    );
}

#[test]
fn test_streaming_state_text_tool_text_formatting() {
    // RED: Text -> Tools -> Text should have proper separation
    let config = ClientConfig {
        agent_path: test_path("test-agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();

    state.append_text("Before tools\n");
    client.record_tool_call(
        ToolCallInfo::new("read_file")
            .with_id("t1")
            .with_arguments(json!({"path": "test.md"})),
        &mut state,
    );
    state.append_text("After tools");

    let output = state.formatted_output();
    assert!(
        output.contains("Before tools"),
        "Should contain text before tools"
    );
    assert!(output.contains("▷"), "Should contain tool indicator");
    assert!(
        output.contains("After tools"),
        "Should contain text after tools"
    );
    // Verify proper blank line separation before tool block
    assert!(
        output.contains("\n\n  ▷"),
        "Tool block should have blank line before it: {:?}",
        output
    );
}

#[test]
fn test_tool_deduplication_different_ids_same_args() {
    // RED: Same tool+args but different IDs should both be recorded
    let mut state = StreamingState::default();

    let tool1 = ToolCallInfo::new("read_file")
        .with_id("call-1")
        .with_arguments(json!({"path": "test.md"}));
    let tool2 = ToolCallInfo::new("read_file")
        .with_id("call-2")
        .with_arguments(json!({"path": "test.md"}));

    // Use upsert_tool_info directly to test deduplication logic
    // (record_tool_call also modifies segments, we want to isolate the dedup logic)
    upsert_tool_info(tool1, &mut state);
    upsert_tool_info(tool2, &mut state);

    assert_eq!(
        state.tool_calls.len(),
        2,
        "Both tool calls should be recorded (different IDs)"
    );
}

#[test]
fn test_tool_deduplication_same_id_updates() {
    // Verify that same ID correctly updates existing entry
    let mut state = StreamingState::default();

    let tool1 = ToolCallInfo::new("read_file")
        .with_id("same-id")
        .with_arguments(json!({"path": "old.md"}));
    let tool2 = ToolCallInfo::new("read_file")
        .with_id("same-id")
        .with_arguments(json!({"path": "new.md"}));

    upsert_tool_info(tool1, &mut state);
    upsert_tool_info(tool2, &mut state);

    assert_eq!(
        state.tool_calls.len(),
        1,
        "Same ID should update, not duplicate"
    );
    // Should have the updated arguments
    let args = state.tool_calls[0].arguments.as_ref().unwrap();
    assert_eq!(
        args.get("path").and_then(|v| v.as_str()),
        Some("new.md"),
        "Arguments should be updated to new values"
    );
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
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
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

#[test]
fn tool_call_update_without_prior_announcement_does_not_emit_diff_update() {
    // If we never saw the initial tool_call, a tool_call_update with diffs
    // should not fire a late-diff chunk — the post-stream replay will
    // handle the brand-new tool call as usual.
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

    let prior = state
        .tool_calls
        .iter()
        .filter(|tc| tc.id.as_deref() == Some("ghost-1"))
        .count();
    assert_eq!(prior, 1, "tool_calls should record the new entry");

    assert!(
        !chunks
            .iter()
            .any(|c| matches!(c, StreamingChunk::ToolDiffUpdate { .. })),
        "expected no ToolDiffUpdate without prior announcement, got: {:?}",
        chunks
    );
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
    use agent_client_protocol::ToolCallStatus;

    let raw = json!([
        {"type": "text", "text": "MCP error -32602: File not found: Concepts/Target.md"}
    ]);
    let err = CrucibleAcpClient::extract_tool_error(Some(ToolCallStatus::Failed), Some(&raw));
    assert_eq!(
        err.as_deref(),
        Some("MCP error -32602: File not found: Concepts/Target.md")
    );
}

/// An explicit `error` field still wins over content blocks.
#[test]
fn failed_tool_error_prefers_explicit_error_field() {
    use agent_client_protocol::ToolCallStatus;

    let raw = json!({
        "error": "explicit reason",
        "content": [{"type": "text", "text": "secondary text"}]
    });
    let err = CrucibleAcpClient::extract_tool_error(Some(ToolCallStatus::Failed), Some(&raw));
    assert_eq!(err.as_deref(), Some("explicit reason"));
}

/// With nothing usable in the output, the generic label remains the floor.
#[test]
fn failed_tool_error_without_detail_falls_back_to_generic() {
    use agent_client_protocol::ToolCallStatus;

    for raw in [
        None,
        Some(json!({})),
        Some(json!([])),
        Some(json!([{"type": "image"}])),
    ] {
        let err = CrucibleAcpClient::extract_tool_error(Some(ToolCallStatus::Failed), raw.as_ref());
        assert_eq!(err.as_deref(), Some("Tool call failed"), "raw={raw:?}");
    }
}

/// The surfaced text is agent-authored: it renders as a one-line error label,
/// so it gets the same single-line sanitising and display cap as
/// `describe_rpc_error`'s output — control characters stripped, length elided.
#[test]
fn failed_tool_error_text_is_sanitized_and_capped() {
    use agent_client_protocol::ToolCallStatus;

    let hostile = format!("bad\x1b[31m\r{}", "x".repeat(4096));
    let raw = json!([{"type": "text", "text": hostile}]);
    let err = CrucibleAcpClient::extract_tool_error(Some(ToolCallStatus::Failed), Some(&raw))
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
