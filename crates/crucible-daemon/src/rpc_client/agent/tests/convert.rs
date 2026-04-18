//! Tests for `SessionEvent` → `ChatChunk` conversion.

use serde_json::json;

use crate::rpc_client::agent::convert::session_event_to_chat_chunk;
use crate::SessionEvent;

#[test]
fn test_text_delta_conversion() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "text_delta".to_string(),
        data: json!({ "content": "Hello world" }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    assert_eq!(chunk.delta, "Hello world");
    assert!(!chunk.done);
    assert!(chunk.tool_calls.is_none());
    assert!(chunk.reasoning.is_none());
}

#[test]
fn test_thinking_conversion() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "thinking".to_string(),
        data: json!({ "content": "Let me think..." }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    assert_eq!(chunk.delta, "");
    assert_eq!(chunk.reasoning, Some("Let me think...".to_string()));
    assert!(!chunk.done);
}

#[test]
fn test_tool_call_conversion() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "tool_call".to_string(),
        data: json!({
            "call_id": "tc-123",
            "tool": "search",
            "args": { "query": "rust async" }
        }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    assert_eq!(chunk.delta, "");
    assert!(!chunk.done);

    let tool_calls = chunk.tool_calls.unwrap();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].name, "search");
    assert_eq!(tool_calls[0].id, Some("tc-123".to_string()));
}

#[test]
fn test_tool_result_conversion() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "tool_result".to_string(),
        data: json!({
            "call_id": "tc-123",
            "result": "Found 5 results"
        }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    assert_eq!(chunk.delta, "");
    assert!(!chunk.done);

    let results = chunk.tool_results.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].result, "Found 5 results");
}

#[test]
fn test_message_complete_conversion() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "message_complete".to_string(),
        data: json!({
            "message_id": "msg-456",
            "full_response": "Complete response text"
        }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    assert!(chunk.done);
    assert_eq!(chunk.delta, "");
}

#[test]
fn test_ended_conversion() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "ended".to_string(),
        data: json!({ "reason": "user_requested" }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    assert!(chunk.done);
}

#[test]
fn test_ended_with_error_reason_detected() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "ended".to_string(),
        data: json!({ "reason": "error: connection refused" }),
    };

    let chunk = session_event_to_chat_chunk(&event).expect("ended should convert to chunk");
    assert!(chunk.done);
    let reason = event
        .data
        .get("reason")
        .and_then(|value| value.as_str())
        .expect("reason should be present");
    assert!(reason.starts_with("error: "));
}

#[test]
fn test_ended_with_cancelled_reason_yields_done_chunk() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "ended".to_string(),
        data: json!({ "reason": "cancelled" }),
    };

    let chunk = session_event_to_chat_chunk(&event).expect("ended should convert to chunk");
    assert!(chunk.done);
}

#[test]
fn test_ended_with_complete_reason_yields_done_chunk() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "ended".to_string(),
        data: json!({ "reason": "complete" }),
    };

    let chunk = session_event_to_chat_chunk(&event).expect("ended should convert to chunk");
    assert!(chunk.done);
}
#[test]
fn test_error_with_communication_prefix_stripped() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "ended".to_string(),
        data: json!({ "reason": "error: Communication error: LLM timeout" }),
    };

    let chunk = session_event_to_chat_chunk(&event).expect("ended should convert to chunk");
    assert!(chunk.done);
    let reason = event
        .data
        .get("reason")
        .and_then(|value| value.as_str())
        .expect("reason should be present");
    assert!(reason.starts_with("error: "));
}

#[test]
fn test_error_with_connection_prefix_stripped() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "ended".to_string(),
        data: json!({ "reason": "error: Connection error: refused" }),
    };

    let chunk = session_event_to_chat_chunk(&event).expect("ended should convert to chunk");
    assert!(chunk.done);
    let reason = event
        .data
        .get("reason")
        .and_then(|value| value.as_str())
        .expect("reason should be present");
    assert!(reason.starts_with("error: "));
}

#[test]
fn test_error_with_internal_prefix_stripped() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "ended".to_string(),
        data: json!({ "reason": "error: Internal error: panic" }),
    };

    let chunk = session_event_to_chat_chunk(&event).expect("ended should convert to chunk");
    assert!(chunk.done);
    let reason = event
        .data
        .get("reason")
        .and_then(|value| value.as_str())
        .expect("reason should be present");
    assert!(reason.starts_with("error: "));
}

#[test]
fn test_unknown_event_returns_none() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "unknown_event".to_string(),
        data: json!({}),
    };

    assert!(session_event_to_chat_chunk(&event).is_none());
}

#[test]
fn test_malformed_event_returns_none() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "text_delta".to_string(),
        data: json!({}),
    };

    assert!(session_event_to_chat_chunk(&event).is_none());
}

#[test]
fn test_error_format_parity_connection() {
    use crucible_core::traits::chat::ChatError;

    let daemon_err = ChatError::Connection("Event channel closed".to_string());
    let local_err = ChatError::Connection("Connection lost".to_string());

    let daemon_msg = daemon_err.to_string();
    let local_msg = local_err.to_string();

    assert!(
        daemon_msg.contains("Connection") || daemon_msg.contains("connection"),
        "Daemon error should mention connection: {}",
        daemon_msg
    );
    assert!(
        local_msg.contains("Connection") || local_msg.contains("connection"),
        "Local error should mention connection: {}",
        local_msg
    );
}

#[test]
fn test_error_format_parity_communication() {
    use crucible_core::traits::chat::ChatError;

    let daemon_err = ChatError::Communication("Failed to send message: timeout".to_string());
    let local_err = ChatError::Communication("Rig LLM error: connection refused".to_string());

    let daemon_msg = daemon_err.to_string();
    let local_msg = local_err.to_string();

    assert!(!daemon_msg.is_empty(), "Daemon error should have message");
    assert!(!local_msg.is_empty(), "Local error should have message");

    assert!(
        !daemon_msg.contains("ChatError"),
        "Error display should not expose internal type: {}",
        daemon_msg
    );
    assert!(
        !local_msg.contains("ChatError"),
        "Error display should not expose internal type: {}",
        local_msg
    );
}

#[test]
fn test_error_types_are_displayable() {
    use crucible_core::traits::chat::ChatError;

    let errors = vec![
        ChatError::Connection("test".to_string()),
        ChatError::Communication("test".to_string()),
        ChatError::InvalidMode("test".to_string()),
    ];

    for err in errors {
        let msg = err.to_string();
        assert!(
            !msg.is_empty(),
            "All ChatError variants should be displayable"
        );
        assert!(
            msg.len() < 1000,
            "Error messages should be reasonably sized for TUI display"
        );
    }
}

#[test]
fn test_tool_result_with_object_result() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "tool_result".to_string(),
        data: json!({
            "call_id": "tc-123",
            "result": { "files": ["a.rs", "b.rs"], "count": 2 }
        }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    let results = chunk.tool_results.unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].result.contains("files"));
    assert!(results[0].result.contains("count"));
}

#[test]
fn test_tool_call_without_call_id() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "tool_call".to_string(),
        data: json!({
            "tool": "search",
            "args": { "query": "test" }
        }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    let tool_calls = chunk.tool_calls.unwrap();
    assert_eq!(tool_calls[0].name, "search");
    assert!(tool_calls[0].id.is_none());
}

#[test]
fn test_tool_call_without_args() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "tool_call".to_string(),
        data: json!({
            "tool": "list_files"
        }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    let tool_calls = chunk.tool_calls.unwrap();
    assert_eq!(tool_calls[0].name, "list_files");
    assert!(tool_calls[0].arguments.is_none());
}

#[test]
fn test_model_switched_event_conversion() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "model_switched".to_string(),
        data: json!({
            "model": "gpt-4",
            "provider": "openai"
        }),
    };

    let chunk = session_event_to_chat_chunk(&event);
    assert!(
        chunk.is_none(),
        "model_switched events should not produce chunks"
    );
}

#[test]
fn test_tool_result_includes_tool_name() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "tool_result".to_string(),
        data: json!({
            "call_id": "tc-123",
            "tool": "read_file",
            "result": "file contents here"
        }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    let results = chunk.tool_results.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].name, "read_file",
        "tool_result should use tool name, not call_id"
    );
    assert_eq!(results[0].result, "file contents here");
}

#[test]
fn test_tool_result_falls_back_to_call_id_when_no_tool_name() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "tool_result".to_string(),
        data: json!({
            "call_id": "tc-456",
            "result": "some result"
        }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    let results = chunk.tool_results.unwrap();
    assert_eq!(
        results[0].name, "tc-456",
        "Should fall back to call_id when tool name not provided"
    );
}

#[test]
fn test_tool_result_unwraps_daemon_format() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "tool_result".to_string(),
        data: json!({
            "call_id": "tc-789",
            "tool": "bash",
            "result": { "result": "line1\nline2\nline3" }
        }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    let results = chunk.tool_results.unwrap();
    assert_eq!(results[0].name, "bash");
    assert_eq!(results[0].result, "line1\nline2\nline3");
    assert!(results[0].result.contains('\n'));
}

#[test]
fn test_tool_result_with_error_extracts_error_field() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "tool_result".to_string(),
        data: json!({
            "call_id": "tc-denied",
            "tool": "bash",
            "result": { "error": "User denied permission to bash echo hello" }
        }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    let results = chunk.tool_results.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "bash");
    assert!(
        results[0].result.is_empty(),
        "Result should be empty when error is present"
    );
    assert_eq!(
        results[0].error,
        Some("User denied permission to bash echo hello".to_string())
    );
}

#[test]
fn test_tool_result_without_error_has_none_error_field() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "tool_result".to_string(),
        data: json!({
            "call_id": "tc-ok",
            "tool": "read_file",
            "result": "file contents"
        }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    let results = chunk.tool_results.unwrap();
    assert_eq!(results[0].name, "read_file");
    assert_eq!(results[0].result, "file contents");
    assert!(
        results[0].error.is_none(),
        "Error should be None for successful results"
    );
}

#[test]
fn test_message_complete_with_usage_extraction() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "message_complete".to_string(),
        data: json!({
            "message_id": "msg-1",
            "full_response": "done",
            "prompt_tokens": 200,
            "completion_tokens": 80,
            "total_tokens": 280
        }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    assert!(chunk.done, "message_complete should set done=true");
    let usage = chunk.usage.expect("Should extract usage from event data");
    assert_eq!(usage.prompt_tokens, 200);
    assert_eq!(usage.completion_tokens, 80);
    assert_eq!(usage.total_tokens, 280);
}

#[test]
fn test_message_complete_without_usage_extraction() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "message_complete".to_string(),
        data: json!({
            "message_id": "msg-2",
            "full_response": "no tokens"
        }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    assert!(chunk.done, "message_complete should set done=true");
    assert!(
        chunk.usage.is_none(),
        "Should be None when no token fields in event data"
    );
}

#[test]
fn test_message_complete_usage_defaults_missing_fields() {
    // total_tokens present but prompt/completion missing → should still extract
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "message_complete".to_string(),
        data: json!({
            "message_id": "msg-3",
            "full_response": "partial usage",
            "total_tokens": 500
        }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    let usage = chunk
        .usage
        .expect("Should extract usage when total_tokens present");
    assert_eq!(usage.total_tokens, 500);
    assert_eq!(
        usage.prompt_tokens, 0,
        "Missing prompt_tokens should default to 0"
    );
    assert_eq!(
        usage.completion_tokens, 0,
        "Missing completion_tokens should default to 0"
    );
}

#[test]
fn test_message_complete_usage_with_cache_tokens() {
    // Anthropic-style response with cache_read_tokens and cache_creation_tokens
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "message_complete".to_string(),
        data: json!({
            "message_id": "msg-4",
            "full_response": "cached response",
            "prompt_tokens": 1000,
            "completion_tokens": 200,
            "total_tokens": 1200,
            "cache_read_tokens": 800,
            "cache_creation_tokens": 150
        }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    let usage = chunk.usage.expect("Should extract usage with cache fields");
    assert_eq!(usage.prompt_tokens, 1000);
    assert_eq!(usage.completion_tokens, 200);
    assert_eq!(usage.total_tokens, 1200);
    assert_eq!(usage.cache_read_tokens, Some(800));
    assert_eq!(usage.cache_creation_tokens, Some(150));
}

#[test]
fn test_precognition_complete_with_notes() {
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "precognition_complete".to_string(),
        data: json!({
            "notes_count": 2,
            "query_summary": "how to use async",
            "notes": [
                { "title": "Async Patterns", "kiln_label": null },
                { "title": "Tokio Guide", "kiln_label": "docs" }
            ]
        }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    assert_eq!(chunk.precognition_notes_count, Some(2));
    let notes = chunk.precognition_notes.expect("notes should be populated");
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0].title, "Async Patterns");
    assert!(notes[0].kiln_label.is_none());
    assert_eq!(notes[1].title, "Tokio Guide");
    assert_eq!(notes[1].kiln_label.as_deref(), Some("docs"));
}

#[test]
fn test_precognition_complete_without_notes_backward_compat() {
    // Old daemon events without "notes" field should still work
    let event = SessionEvent {
        session_id: "test".to_string(),
        event_type: "precognition_complete".to_string(),
        data: json!({
            "notes_count": 3,
            "query_summary": "search query"
        }),
    };

    let chunk = session_event_to_chat_chunk(&event).unwrap();
    assert_eq!(chunk.precognition_notes_count, Some(3));
    assert!(
        chunk.precognition_notes.is_none(),
        "Missing notes field should result in None for backward compatibility"
    );
}
