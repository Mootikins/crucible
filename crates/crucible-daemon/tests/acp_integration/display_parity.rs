//! `TurnEvent` contract tests — verifies the ACP client's wire parsing
//! preserves tool call arguments, tool results, and handles missing token usage
//! gracefully.
//!
//! **This file does not prove display parity, despite its name.** Everything
//! here stops at [`TurnEvent`], which sits *above* the `SessionEventMessage`
//! parity boundary and two layers upstream of anything a user sees. A green run
//! says the ACP client parsed the agent's wire messages correctly; it says
//! nothing about whether the resulting turn renders like an internal one. The
//! name reading as a parity claim is precisely how the presentation divergences
//! this suite could not see went unnoticed — see the parity boundary section of
//! `docs/Meta/Analysis/Systems.md`.
//!
//! Real parity evidence is a pair of `SessionEvent` recordings of the same
//! behaviour, one per agent, pumped through the shared renderer and compared as
//! frames: `crucible-cli`'s `user_story_tests/acp_parity_tests.rs`, over the
//! `assets/fixtures/acp_parity_*` recordings.
//!
//! The name is kept because `Systems.md` and the branch plan both cite it by
//! path as the example of a test whose name overclaims its layer.

use crate::support::mock_agent::{make_prompt_request, tool_call, tool_call_update};
use crate::support::parity::capture_chunks;
use crate::support::{connect, prompt_with, MockScript, Step};
use crucible_core::turn::TurnEvent;
use serde_json::json;

/// The `rawInput` of a canonical ACP call, when a frame sent one.
fn raw_input(call: &crucible_core::types::CanonicalToolCall) -> Option<&serde_json::Value> {
    call.raw.as_ref().and_then(|raw| raw.raw_input.as_ref())
}

/// A `tool_call` step whose `content` array carries one
/// `ToolCallContent::Diff` entry — exercises the path that surfaces ACP
/// file-mutation previews into the TUI scrollback.
fn tool_call_with_diff(
    tool_call_id: &str,
    title: &str,
    raw_input: Option<serde_json::Value>,
    diff_path: &str,
    old_text: Option<&str>,
    new_text: &str,
) -> Step {
    let mut update = json!({
        "sessionUpdate": "tool_call",
        "toolCallId": tool_call_id,
        "title": title,
        "status": "in_progress",
        "content": [{
            "type": "diff",
            "path": diff_path,
            "oldText": old_text,
            "newText": new_text,
        }],
    });
    if let Some(input) = raw_input {
        update["rawInput"] = input;
    }
    Step::Update(update)
}

/// A `tool_call_update` step whose `content` array carries one
/// `ToolCallContent::Diff` entry — exercises the late-diff path where the
/// ACP agent (e.g. Claude Code) defers diffs until after the initial
/// `tool_call` frame.
fn tool_call_update_with_diff(
    tool_call_id: &str,
    diff_path: &str,
    old_text: Option<&str>,
    new_text: &str,
) -> Step {
    Step::Update(json!({
        "sessionUpdate": "tool_call_update",
        "toolCallId": tool_call_id,
        "content": [{
            "type": "diff",
            "path": diff_path,
            "oldText": old_text,
            "newText": new_text,
        }],
    }))
}

#[tokio::test]
async fn tool_start_with_arguments_emits_chunk_with_args() {
    let (chunks, callback) = capture_chunks();

    let (client, _agent) = connect(
        MockScript {
            turn: vec![tool_call(
                "tool-42",
                "mcp__crucible__semantic_search",
                Some(json!({"query": "rust async patterns", "limit": 5})),
            )],
            ..MockScript::default()
        },
        Some(500),
        None,
    )
    .await;

    let request = make_prompt_request("ses-tool-args", "search something");
    let (summary, _response) = prompt_with(&client, request, callback)
        .await
        .expect("streaming should complete");

    let captured = chunks.lock().unwrap();
    let tool_start = captured
        .iter()
        .find(|c| matches!(c, TurnEvent::ToolCall { .. }))
        .expect("should have received ToolCall chunk");

    match tool_start {
        TurnEvent::ToolCall {
            id,
            call: Some(call),
            ..
        } => {
            assert_eq!(
                call.tool, "semantic_search",
                "a Crucible MCP call is named by the Crucible tool"
            );
            assert_eq!(id, "tool-42");
            let args = raw_input(call).expect("arguments should be Some");
            assert_eq!(args["query"], "rust async patterns");
            assert_eq!(args["limit"], 5);
        }
        _ => unreachable!(),
    }

    assert!(summary.announced_any, "the summary must report the call");
}

#[tokio::test]
async fn tool_start_without_arguments_has_none() {
    let (chunks, callback) = capture_chunks();

    let (client, _agent) = connect(
        MockScript {
            turn: vec![tool_call("tool-99", "list_models", None)],
            ..MockScript::default()
        },
        Some(500),
        None,
    )
    .await;

    let request = make_prompt_request("ses-no-args", "list models");
    prompt_with(&client, request, callback)
        .await
        .expect("streaming should complete");

    let captured = chunks.lock().unwrap();
    let tool_start = captured
        .iter()
        .find(|c| matches!(c, TurnEvent::ToolCall { .. }))
        .expect("should have ToolCall");

    match tool_start {
        TurnEvent::ToolCall {
            call: Some(call), ..
        } => {
            assert!(
                raw_input(call).is_none(),
                "arguments should be None when not provided"
            );
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn tool_start_complex_arguments_preserved() {
    let (chunks, callback) = capture_chunks();

    let complex_args = json!({
        "path": "/home/user/project/src/main.rs",
        "options": {
            "encoding": "utf-8",
            "line_range": [10, 50],
            "include_metadata": true
        },
        "tags": ["rust", "source"],
        "nested": {"deep": {"value": 42}}
    });
    let expected_args = complex_args.clone();

    let (client, _agent) = connect(
        MockScript {
            turn: vec![tool_call("tool-c1", "read_file", Some(complex_args))],
            ..MockScript::default()
        },
        Some(500),
        None,
    )
    .await;

    let request = make_prompt_request("ses-complex", "read file");
    prompt_with(&client, request, callback)
        .await
        .expect("streaming should complete");

    let captured = chunks.lock().unwrap();
    let tool_start = captured
        .iter()
        .find(|c| matches!(c, TurnEvent::ToolCall { .. }))
        .expect("should have ToolCall");

    match tool_start {
        TurnEvent::ToolCall {
            call: Some(call), ..
        } => {
            let args = raw_input(call).expect("complex args should survive roundtrip");
            assert_eq!(
                *args, expected_args,
                "nested JSON should be fully preserved"
            );
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn tool_start_forwards_diff_content_to_streaming_chunk() {
    // Regression: when an ACP `tool_call` notification carries a
    // `ToolCallContent::Diff` entry in its `content` array, the
    // diff must surface on the live `TurnEvent::ToolCall`
    // so the TUI can render it in scrollback as the call appears.

    let (chunks, callback) = capture_chunks();

    let (client, _agent) = connect(
        MockScript {
            turn: vec![tool_call_with_diff(
                "tool-d1",
                "Edit",
                Some(json!({
                    "file_path": "/tmp/foo.rs",
                    "old_string": "fn old() {}",
                    "new_string": "fn new() {}",
                })),
                "/tmp/foo.rs",
                Some("fn old() {}\n"),
                "fn new() {}\n",
            )],
            ..MockScript::default()
        },
        Some(500),
        None,
    )
    .await;

    let request = make_prompt_request("ses-diff", "edit file");
    prompt_with(&client, request, callback)
        .await
        .expect("streaming should complete");

    let captured = chunks.lock().unwrap();
    let tool_start = captured
        .iter()
        .find(|c| matches!(c, TurnEvent::ToolCall { .. }))
        .expect("should have ToolCall");

    match tool_start {
        TurnEvent::ToolCall {
            id,
            call: Some(call),
            ..
        } => {
            assert_eq!(id, "tool-d1");
            assert_eq!(call.diffs.len(), 1, "should forward exactly one diff");
            let diff = &call.diffs[0];
            assert_eq!(diff.path, "/tmp/foo.rs");
            assert_eq!(diff.old_content.as_deref(), Some("fn old() {}\n"));
            assert_eq!(diff.new_content, "fn new() {}\n");
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn tool_call_update_with_late_diffs_emits_a_tool_update() {
    // Regression: when an ACP agent (e.g. Claude Code) sends an empty
    // `tool_call` notification first and then attaches diffs via a later
    // `tool_call_update` frame, the diffs must reach the TUI as a live
    // chunk — they were previously being silently dropped because the
    // post-stream replay in `acp_handle.rs` filters out tool ids that
    // were already announced via `ToolCall`.

    let (chunks, callback) = capture_chunks();

    let (client, _agent) = connect(
        MockScript {
            turn: vec![
                // 1. Initial tool_call frame: NO diffs yet (mimics Claude Code).
                tool_call(
                    "tool-ld1",
                    "Edit",
                    Some(json!({
                        "file_path": "/tmp/late.rs",
                        "old_string": "old",
                        "new_string": "new",
                    })),
                ),
                // 2. Later tool_call_update frame carries the diff content.
                tool_call_update_with_diff(
                    "tool-ld1",
                    "/tmp/late.rs",
                    Some("fn old() {}\n"),
                    "fn new() {}\n",
                ),
            ],
            ..MockScript::default()
        },
        Some(500),
        None,
    )
    .await;

    let request = make_prompt_request("ses-late-diff", "edit late");
    prompt_with(&client, request, callback)
        .await
        .expect("streaming should complete");

    let captured = chunks.lock().unwrap();

    // The initial ToolCall should still be there, with empty diffs.
    let tool_start = captured
        .iter()
        .find(|c| matches!(c, TurnEvent::ToolCall { .. }))
        .expect("should have ToolCall chunk");
    match tool_start {
        TurnEvent::ToolCall {
            id,
            call: Some(call),
            ..
        } => {
            assert_eq!(id, "tool-ld1");
            assert!(
                call.diffs.is_empty(),
                "initial ToolCall should have no diffs (agent deferred them)"
            );
        }
        _ => unreachable!(),
    }

    // The late diffs must surface in a follow-up ToolCallUpdate chunk.
    let diff_update = captured
        .iter()
        .find(|c| matches!(c, TurnEvent::ToolCallUpdate { .. }))
        .expect(
            "should emit a ToolCallUpdate chunk when a tool_call_update \
             carries diffs for an already-announced tool",
        );
    match diff_update {
        TurnEvent::ToolCallUpdate { id, call } => {
            assert_eq!(id, "tool-ld1");
            assert_eq!(call.diffs.len(), 1, "should carry exactly one diff");
            let diff = &call.diffs[0];
            assert_eq!(diff.path, "/tmp/late.rs");
            assert_eq!(diff.old_content.as_deref(), Some("fn old() {}\n"));
            assert_eq!(diff.new_content, "fn new() {}\n");
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn tool_end_with_result_emits_chunk() {
    let (chunks, callback) = capture_chunks();

    let (client, _agent) = connect(
        MockScript {
            turn: vec![
                tool_call("tool-r1", "read_note", Some(json!({"path": "README.md"}))),
                tool_call_update(
                    "tool-r1",
                    "completed",
                    Some(json!("# README\n\nThis is the readme content.")),
                ),
            ],
            ..MockScript::default()
        },
        Some(500),
        None,
    )
    .await;

    let request = make_prompt_request("ses-result", "read readme");
    prompt_with(&client, request, callback)
        .await
        .expect("streaming should complete");

    let captured = chunks.lock().unwrap();

    let tool_start = captured
        .iter()
        .find(|c| matches!(c, TurnEvent::ToolCall { .. }));
    assert!(tool_start.is_some(), "should have ToolCall");

    let tool_end = captured
        .iter()
        .find(|c| matches!(c, TurnEvent::ToolResult { .. }))
        .expect("should have ToolResult chunk");

    match tool_end {
        TurnEvent::ToolResult {
            id, result, error, ..
        } => {
            assert_eq!(id, "tool-r1");
            assert!(
                result.as_str().unwrap().contains("README"),
                "result should contain the tool output"
            );
            assert!(error.is_none(), "successful tool should have no error");
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn tool_end_with_error_emits_error_field() {
    let (chunks, callback) = capture_chunks();

    let (client, _agent) = connect(
        MockScript {
            turn: vec![
                tool_call(
                    "tool-e1",
                    "write_file",
                    Some(json!({"path": "/protected/file.txt", "content": "test"})),
                ),
                tool_call_update(
                    "tool-e1",
                    "failed",
                    Some(json!({"error": "permission denied: /protected/file.txt"})),
                ),
            ],
            ..MockScript::default()
        },
        Some(500),
        None,
    )
    .await;

    let request = make_prompt_request("ses-error", "write file");
    prompt_with(&client, request, callback)
        .await
        .expect("streaming should complete");

    let captured = chunks.lock().unwrap();
    let tool_end = captured
        .iter()
        .find(|c| matches!(c, TurnEvent::ToolResult { .. }))
        .expect("should have ToolResult chunk for failed tool");

    match tool_end {
        TurnEvent::ToolResult { id, error, .. } => {
            assert_eq!(id, "tool-e1");
            assert!(error.is_some(), "failed tool should have error");
            assert!(
                error.as_ref().unwrap().contains("permission denied"),
                "error should contain the failure message"
            );
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn tool_end_failed_without_output_has_generic_error() {
    let (chunks, callback) = capture_chunks();

    let (client, _agent) = connect(
        MockScript {
            turn: vec![
                tool_call("tool-f1", "broken_tool", None),
                tool_call_update("tool-f1", "failed", None),
            ],
            ..MockScript::default()
        },
        Some(500),
        None,
    )
    .await;

    let request = make_prompt_request("ses-fail-no-out", "try broken");
    prompt_with(&client, request, callback)
        .await
        .expect("streaming should complete");

    let captured = chunks.lock().unwrap();
    let tool_end = captured
        .iter()
        .find(|c| matches!(c, TurnEvent::ToolResult { .. }))
        .expect("should have ToolResult for failed tool");

    match tool_end {
        TurnEvent::ToolResult { error, .. } => {
            assert!(
                error.is_some(),
                "failed tool with no output should still report error"
            );
            assert!(
                error.as_ref().unwrap().contains("failed"),
                "should have generic failure message, got: {:?}",
                error
            );
        }
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn stream_without_usage_data_completes_gracefully() {
    let (client, _agent) = connect(
        MockScript {
            turn: vec![Step::Text("Hello from agent".into())],
            ..MockScript::default()
        },
        Some(500),
        None,
    )
    .await;

    let request = make_prompt_request("ses-no-usage", "say hello");
    let (chunks, callback) = capture_chunks();
    let (summary, response) = prompt_with(&client, request, callback)
        .await
        .expect("stream should complete without crash when no usage data");
    let content = crate::support::parity::text_of(&chunks.lock().unwrap());

    assert!(
        content.contains("Hello from agent"),
        "content should be accumulated"
    );
    assert!(!summary.announced_any);
    assert_eq!(
        response.stop_reason,
        agent_client_protocol::schema::v1::StopReason::EndTurn
    );
}

#[tokio::test]
async fn empty_stream_no_usage_no_chunks_completes() {
    let (client, _agent) = connect(MockScript::default(), Some(500), None).await;

    let request = make_prompt_request("ses-empty", "nothing");
    let (chunks, callback) = capture_chunks();
    let (summary, _response) = prompt_with(&client, request, callback)
        .await
        .expect("empty stream should complete without crash");
    let content = crate::support::parity::text_of(&chunks.lock().unwrap());

    assert!(content.is_empty(), "no chunks = empty content");
    assert!(!summary.announced_any);
}

#[tokio::test]
async fn full_flow_text_tool_result_text_via_callback() {
    let (chunks, callback) = capture_chunks();

    let (client, _agent) = connect(
        MockScript {
            turn: vec![
                Step::Text("Let me search for that. ".into()),
                tool_call(
                    "tool-s1",
                    "mcp__crucible__semantic_search",
                    Some(json!({"query": "async patterns"})),
                ),
                tool_call_update(
                    "tool-s1",
                    "completed",
                    Some(json!("Found 3 relevant notes about async patterns.")),
                ),
                Step::Text("Based on the results, here is your answer.".into()),
            ],
            ..MockScript::default()
        },
        Some(500),
        None,
    )
    .await;

    let request = make_prompt_request("ses-full", "search async patterns");
    let (summary, _response) = prompt_with(&client, request, callback)
        .await
        .expect("full flow should complete");
    let content = crate::support::parity::text_of(&chunks.lock().unwrap());

    let captured = chunks.lock().unwrap();

    let kinds: Vec<&str> = captured
        .iter()
        .map(crate::support::parity::chunk_kind)
        .collect();

    assert_eq!(
        kinds,
        vec!["text", "tool_start", "tool_end", "text"],
        "chunks should arrive in order: text → tool_start → tool_end → text"
    );

    assert!(content.contains("Let me search"));
    assert!(content.contains("here is your answer"));

    assert!(summary.announced_any);
    assert!(summary.produced_content);
}
