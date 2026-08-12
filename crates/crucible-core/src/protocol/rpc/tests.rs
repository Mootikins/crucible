//! Tests for the RPC envelope and its typed event constructors.
//!
//! Split out of `mod.rs` to keep both inside the 1000-line module budget
//! enforced by `no_new_oversized_modules`.

use super::*;
use chrono::Utc;

#[test]
fn test_session_event_message_timestamp_roundtrip() {
    let mut event = SessionEventMessage::text_delta("chat-test", "hello");
    let now = Utc::now();
    event.timestamp = Some(now);

    let json = serde_json::to_string(&event).unwrap();
    let deserialized: SessionEventMessage = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.timestamp, Some(now));
    assert_eq!(deserialized.session_id, "chat-test");
}

#[test]
fn test_session_event_message_seq_roundtrip() {
    let mut event = SessionEventMessage::text_delta("chat-test", "hello");
    event.seq = Some(42);

    let json = serde_json::to_string(&event).unwrap();
    let deserialized: SessionEventMessage = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.seq, Some(42));
}

#[test]
fn test_session_event_message_backward_compat() {
    let json = r#"{"type":"event","session_id":"s1","event":"text_delta","data":{"content":"hi"}}"#;
    let deserialized: SessionEventMessage = serde_json::from_str(json).unwrap();

    assert_eq!(deserialized.timestamp, None);
    assert_eq!(deserialized.seq, None);
    assert_eq!(deserialized.session_id, "s1");
    assert_eq!(deserialized.event, "text_delta");
}

#[test]
fn test_session_event_message_omits_none_fields() {
    let event = SessionEventMessage::text_delta("chat-test", "hello");
    let json = serde_json::to_string(&event).unwrap();

    assert!(!json.contains("\"timestamp\""));
    assert!(!json.contains("\"seq\""));
}

#[test]
fn test_response_success_serialization() {
    let resp = Response::success(Some(RequestId::Number(1)), "pong");
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"result\":\"pong\""));
    assert!(json.contains("\"id\":1"));
    assert!(!json.contains("error"));
}

#[test]
fn test_response_error_serialization() {
    let resp = Response::error(
        Some(RequestId::Number(1)),
        METHOD_NOT_FOUND,
        "Unknown method",
    );
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"error\""));
    assert!(json.contains("-32601"));
    assert!(!json.contains("result"));
}

#[test]
fn test_request_deserialization() {
    let json = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
    let req: Request = serde_json::from_str(json).unwrap();
    assert_eq!(req.method, "ping");
    assert_eq!(req.id, Some(RequestId::Number(1)));
}

#[test]
fn test_session_event_text_delta() {
    let event = SessionEventMessage::text_delta("chat-test", "streaming content");
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"event\":\"text_delta\""));
    assert!(json.contains("\"content\":\"streaming content\""));
}

#[test]
fn test_session_event_to_json_line() {
    let event = SessionEventMessage::text_delta("chat-test", "hello");
    let line = event.to_json_line().unwrap();
    assert!(line.ends_with('\n'));
}

#[test]
fn test_session_event_message_complete() {
    let event = SessionEventMessage::message_complete("chat-test", "msg-123", "Hello World!", None);
    let json = serde_json::to_string(&event).unwrap();
    println!("message_complete JSON: {}", json);
    assert!(json.contains("\"event\":\"message_complete\""));
    assert!(json.contains("\"full_response\":\"Hello World!\""));
    assert!(json.contains("\"message_id\":\"msg-123\""));
}

// ── Golden regression tests ──────────────────────────────────────

#[test]
fn error_code_constants_match_jsonrpc_spec() {
    assert_eq!(PARSE_ERROR, -32700);
    assert_eq!(INVALID_REQUEST, -32600);
    assert_eq!(METHOD_NOT_FOUND, -32601);
    assert_eq!(INVALID_PARAMS, -32602);
    assert_eq!(INTERNAL_ERROR, -32603);
}

#[test]
fn request_id_number_json_format() {
    let id = RequestId::Number(42);
    let json = serde_json::to_value(&id).unwrap();
    assert_eq!(json, serde_json::json!(42));
}

#[test]
fn request_id_string_json_format() {
    let id = RequestId::String("abc".to_string());
    let json = serde_json::to_value(&id).unwrap();
    assert_eq!(json, serde_json::json!("abc"));
}

#[test]
fn request_deser_string_id() {
    let json = r#"{"jsonrpc":"2.0","id":"abc-123","method":"test"}"#;
    let req: Request = serde_json::from_str(json).unwrap();
    assert_eq!(req.id, Some(RequestId::String("abc-123".to_string())));
}

#[test]
fn request_deser_no_id() {
    let json = r#"{"jsonrpc":"2.0","method":"notify"}"#;
    let req: Request = serde_json::from_str(json).unwrap();
    assert_eq!(req.id, None);
}

// GOLDEN: captures current behavior — missing params deserializes to Value::Null
#[test]
fn request_deser_no_params() {
    let json = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
    let req: Request = serde_json::from_str(json).unwrap();
    assert_eq!(req.params, Value::Null);
}

#[test]
fn response_success_omits_error() {
    let resp = Response::success(Some(RequestId::Number(1)), "ok");
    let json = serde_json::to_value(&resp).unwrap();
    assert!(json.get("result").is_some());
    assert!(json.get("error").is_none());
}

#[test]
fn response_error_omits_result() {
    let resp = Response::error(Some(RequestId::Number(1)), INTERNAL_ERROR, "boom");
    let json = serde_json::to_value(&resp).unwrap();
    assert!(json.get("error").is_some());
    assert!(json.get("result").is_none());
}

#[test]
fn response_error_with_data() {
    let data = serde_json::json!({"detail": "bad field"});
    let resp = Response::error_with_data(
        Some(RequestId::Number(1)),
        INVALID_PARAMS,
        "invalid",
        Some(data.clone()),
    );
    let json = serde_json::to_value(&resp).unwrap();
    let err = json.get("error").unwrap();
    assert_eq!(err.get("data").unwrap(), &data);
}

#[test]
fn response_error_without_data() {
    let resp = Response::error(Some(RequestId::Number(1)), INVALID_PARAMS, "invalid");
    let json = serde_json::to_value(&resp).unwrap();
    let err = json.get("error").unwrap();
    assert!(err.get("data").is_none());
}

#[test]
fn event_thinking_factory() {
    let evt = SessionEventMessage::thinking("s1", "let me think...");
    assert_eq!(evt.event, "thinking");
    assert_eq!(evt.data["content"], "let me think...");
}

#[test]
fn event_tool_call_factory() {
    let args = serde_json::json!({"path": "/tmp"});
    let evt = SessionEventMessage::tool_call("s1", "call-1", "read_file", args.clone());
    assert_eq!(evt.event, "tool_call");
    assert_eq!(evt.data["call_id"], "call-1");
    assert_eq!(evt.data["tool"], "read_file");
    assert_eq!(evt.data["args"], args);
}

#[test]
fn event_tool_result_factory() {
    let result = serde_json::json!({"content": "file contents"});
    let evt = SessionEventMessage::tool_result("s1", "call-1", "read_file", result.clone());
    assert_eq!(evt.event, "tool_result");
    assert_eq!(evt.data["call_id"], "call-1");
    assert_eq!(evt.data["tool"], "read_file");
    assert_eq!(evt.data["result"], result);
}

#[test]
fn event_ended_factory() {
    let evt = SessionEventMessage::ended("s1", "user_cancel");
    assert_eq!(evt.event, "ended");
    assert_eq!(evt.data["reason"], "user_cancel");
}

#[test]
fn event_model_switched_factory() {
    let evt = SessionEventMessage::model_switched("s1", "gpt-4o", "openai");
    assert_eq!(evt.event, "model_switched");
    assert_eq!(evt.data["model_id"], "gpt-4o");
    assert_eq!(evt.data["provider"], "openai");
}

#[test]
fn event_mode_changed_factory() {
    let evt = SessionEventMessage::mode_changed("s1", "plan");
    assert_eq!(evt.event, "mode_changed");
    // Wire contract: web events.rs and the SSE reducer read data["mode"].
    assert_eq!(evt.data["mode"], "plan");
}

#[test]
fn event_review_changed_factory() {
    let evt = SessionEventMessage::review_changed("s1", "rejected");
    assert_eq!(evt.event, "review_changed");
    // Wire contract: the web ChangesPanel and TUI read data["reason"].
    assert_eq!(evt.data["reason"], "rejected");
    // No hunk identity by design — see the constructor's doc comment.
    assert!(evt.data.get("hunk_id").is_none());
}

#[test]
fn event_message_complete_with_usage() {
    let usage = crate::traits::llm::TokenUsage {
        prompt_tokens: 100,
        completion_tokens: 50,
        total_tokens: 150,
        cache_read_tokens: None,
        cache_creation_tokens: None,
    };
    let evt = SessionEventMessage::message_complete("s1", "msg-1", "done", Some(&usage));
    assert_eq!(evt.event, "message_complete");
    assert_eq!(evt.data["prompt_tokens"], 100);
    assert_eq!(evt.data["completion_tokens"], 50);
    assert_eq!(evt.data["total_tokens"], 150);
    assert_eq!(evt.data["message_id"], "msg-1");
    assert_eq!(evt.data["full_response"], "done");
}

// GOLDEN: captures current behavior — no usage means no token keys at all
#[test]
fn event_message_complete_without_usage() {
    let evt = SessionEventMessage::message_complete("s1", "msg-1", "done", None);
    assert_eq!(evt.event, "message_complete");
    assert!(evt.data.get("prompt_tokens").is_none());
    assert!(evt.data.get("completion_tokens").is_none());
    assert!(evt.data.get("total_tokens").is_none());
    assert_eq!(evt.data["message_id"], "msg-1");
    assert_eq!(evt.data["full_response"], "done");
}

#[test]
fn event_user_message_factory() {
    let evt = SessionEventMessage::user_message("s1", "msg-42", "hello agent");
    assert_eq!(evt.event, "user_message");
    assert_eq!(evt.data["message_id"], "msg-42");
    assert_eq!(evt.data["content"], "hello agent");
}

#[test]
fn tool_call_with_diffs_roundtrip() {
    use crate::types::acp::FileDiff;

    let diffs = vec![
        FileDiff::from_contents(
            "src/foo.rs",
            Some("fn old() {}\n".to_string()),
            "fn new() {}\n",
        ),
        FileDiff::new("src/bar.rs", "// brand new file\n"),
    ];

    // Wire-side construction (daemon path).
    let evt = SessionEventMessage::tool_call_with_metadata(
        "s1",
        "call-1",
        "edit",
        serde_json::json!({"path": "src/foo.rs"}),
        None,
        None,
        None,
        diffs.clone(),
        None,
    );

    // Round-trip the JSON line as the daemon emits and TUI parses.
    let line = evt.to_json_line().unwrap();
    let parsed: SessionEventMessage = serde_json::from_str(line.trim()).unwrap();

    assert_eq!(parsed.event, "tool_call");
    let parsed_diffs: Vec<FileDiff> = serde_json::from_value(
        parsed
            .data
            .get("diffs")
            .cloned()
            .expect("diffs key must round-trip"),
    )
    .expect("diffs must deserialize as Vec<FileDiff>");
    assert_eq!(parsed_diffs, diffs);
}

#[test]
fn tool_call_without_diffs_omits_diffs_key() {
    // Back-compat: empty diffs must not appear in the JSON payload.
    let evt = SessionEventMessage::tool_call(
        "s1",
        "call-1",
        "read_file",
        serde_json::json!({"path": "/tmp/x"}),
    );
    let json = serde_json::to_string(&evt).unwrap();
    assert!(
        !json.contains("\"diffs\""),
        "tool_call without diffs should omit the key, got: {json}"
    );
}

#[test]
fn tool_call_legacy_payload_parses_with_empty_diffs() {
    // An old daemon emitting tool_call without "diffs" must parse cleanly.
    let json = r#"{
        "type":"event",
        "session_id":"s1",
        "event":"tool_call",
        "data":{"call_id":"c","tool":"t","args":{}}
    }"#;
    let parsed: SessionEventMessage = serde_json::from_str(json).unwrap();
    assert!(parsed.data.get("diffs").is_none());
}

#[test]
fn turn_event_tool_call_diffs_roundtrip_json() {
    use crate::turn::TurnEvent;
    use crate::types::acp::FileDiff;

    let diffs = vec![FileDiff::from_contents(
        "src/foo.rs",
        Some("a\n".to_string()),
        "b\n",
    )];
    let ev = TurnEvent::ToolCall {
        id: "call-1".into(),
        name: "edit".into(),
        args: serde_json::json!({"path": "src/foo.rs"}),
        diffs: diffs.clone(),
    };
    let s = serde_json::to_string(&ev).unwrap();
    let r: TurnEvent = serde_json::from_str(&s).unwrap();
    match r {
        TurnEvent::ToolCall {
            diffs: parsed_diffs,
            ..
        } => assert_eq!(parsed_diffs, diffs),
        other => panic!("wrong variant: {other:?}"),
    }
}

#[test]
fn turn_event_tool_call_legacy_json_parses_with_empty_diffs() {
    use crate::turn::TurnEvent;
    // A snapshot from before the diffs field existed.
    let json = r#"{"ToolCall":{"id":"c","name":"t","args":null}}"#;
    let r: TurnEvent = serde_json::from_str(json).unwrap();
    match r {
        TurnEvent::ToolCall { diffs, .. } => assert!(diffs.is_empty()),
        other => panic!("wrong variant: {other:?}"),
    }
}

// ── Turn-group wire goldens ──────────────────────────────────────
//
// One hand-written literal per turn event's `data`. These are the contract
// the typed rewrite must not break: `SessionEventPayload`'s adjacent tagging
// is only wire-compatible if every conditional key, every always-present
// key, and every omitted key lands exactly where the hand-built `json!`
// put it. Written against the pre-rewrite constructors and unchanged since.

/// `data` for `event`, or a panic naming what came instead.
fn wire(msg: &SessionEventMessage, event: &str) -> Value {
    assert_eq!(msg.event, event, "wrong event name");
    assert_eq!(msg.msg_type, "event");
    msg.data.clone()
}

#[test]
fn golden_text_delta() {
    let m = SessionEventMessage::text_delta("s1", "hi");
    assert_eq!(wire(&m, "text_delta"), serde_json::json!({"content": "hi"}));
}

#[test]
fn golden_user_message() {
    let m = SessionEventMessage::user_message("s1", "m-1", "hello");
    assert_eq!(
        wire(&m, "user_message"),
        serde_json::json!({"message_id": "m-1", "content": "hello"})
    );
}

#[test]
fn golden_thinking() {
    let m = SessionEventMessage::thinking("s1", "hmm");
    assert_eq!(wire(&m, "thinking"), serde_json::json!({"content": "hmm"}));
}

#[test]
fn golden_segment_complete() {
    let m = SessionEventMessage::segment_complete("s1", "m-1", 2, "chunk");
    assert_eq!(
        wire(&m, "segment_complete"),
        serde_json::json!({"message_id": "m-1", "index": 2, "content": "chunk"})
    );
}

/// Minimal `tool_call`: `display` is ALWAYS present and computed; the four
/// optional metadata keys and `diffs` are absent.
#[test]
fn golden_tool_call_minimal() {
    let m = SessionEventMessage::tool_call(
        "s1",
        "c-1",
        "read_file",
        serde_json::json!({"path": "/tmp/x"}),
    );
    assert_eq!(
        wire(&m, "tool_call"),
        serde_json::json!({
            "call_id": "c-1",
            "tool": "read_file",
            "args": {"path": "/tmp/x"},
            "display": {"kind": "path", "primary": "/tmp/x"},
        })
    );
}

/// Maximal `tool_call`: every optional argument populated. `lua_primary_arg`
/// also overrides `display.primary`.
#[test]
fn golden_tool_call_maximal() {
    use crate::types::acp::FileDiff;
    let m = SessionEventMessage::tool_call_with_metadata(
        "s1",
        "c-1",
        "edit",
        serde_json::json!({"path": "src/a.rs"}),
        Some("edits a file".into()),
        Some("builtin".into()),
        Some("src/a.rs (lua)".into()),
        vec![FileDiff::new("src/a.rs", "new\n")],
        Some("mode:auto".into()),
    );
    assert_eq!(
        wire(&m, "tool_call"),
        serde_json::json!({
            "call_id": "c-1",
            "tool": "edit",
            "args": {"path": "src/a.rs"},
            "description": "edits a file",
            "source": "builtin",
            "lua_primary_arg": "src/a.rs (lua)",
            "display": {"kind": "path", "primary": "src/a.rs (lua)"},
            "auto_approved": "mode:auto",
            "diffs": [{"path": "src/a.rs", "old_content": null, "new_content": "new\n"}],
        })
    );
}

#[test]
fn golden_tool_call_args_update() {
    let m = SessionEventMessage::tool_call_args_update("s1", "c-1", serde_json::json!({"a": 1}));
    assert_eq!(
        wire(&m, "tool_call_args_update"),
        serde_json::json!({"call_id": "c-1", "args": {"a": 1}})
    );
}

#[test]
fn golden_tool_call_diff_update() {
    use crate::types::acp::FileDiff;
    let m = SessionEventMessage::tool_call_diff_update(
        "s1",
        "c-1",
        vec![FileDiff::new("src/a.rs", "x\n")],
    );
    assert_eq!(
        wire(&m, "tool_call_diff_update"),
        serde_json::json!({
            "call_id": "c-1",
            "diffs": [{"path": "src/a.rs", "old_content": null, "new_content": "x\n"}],
        })
    );
}

/// `terminate` is serialized even when false — an existing subscriber reads
/// `data.terminate` unconditionally, so `skip_serializing_if` would be a
/// wire change.
#[test]
fn golden_tool_result_always_carries_terminate() {
    let m = SessionEventMessage::tool_result("s1", "c-1", "read_file", serde_json::json!("ok"));
    assert_eq!(
        wire(&m, "tool_result"),
        serde_json::json!({
            "call_id": "c-1",
            "tool": "read_file",
            "result": "ok",
            "terminate": false,
        })
    );
}

#[test]
fn golden_tool_result_with_terminate() {
    let m = SessionEventMessage::tool_result_with_terminate(
        "s1",
        "c-1",
        "bash",
        serde_json::json!({"error": "denied"}),
        true,
    );
    assert_eq!(
        wire(&m, "tool_result"),
        serde_json::json!({
            "call_id": "c-1",
            "tool": "bash",
            "result": {"error": "denied"},
            "terminate": true,
        })
    );
}

#[test]
fn golden_ended() {
    let m = SessionEventMessage::ended("s1", "complete");
    assert_eq!(wire(&m, "ended"), serde_json::json!({"reason": "complete"}));
}

/// Three shapes: no usage at all, usage without cache, usage with cache.
#[test]
fn golden_message_complete_three_shapes() {
    let bare = SessionEventMessage::message_complete("s1", "m-1", "done", None);
    assert_eq!(
        wire(&bare, "message_complete"),
        serde_json::json!({"message_id": "m-1", "full_response": "done"})
    );

    let no_cache = SessionEventMessage::message_complete(
        "s1",
        "m-1",
        "done",
        Some(&crate::traits::llm::TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cache_read_tokens: None,
            cache_creation_tokens: None,
        }),
    );
    assert_eq!(
        wire(&no_cache, "message_complete"),
        serde_json::json!({
            "message_id": "m-1",
            "full_response": "done",
            "prompt_tokens": 10,
            "completion_tokens": 5,
            "total_tokens": 15,
        })
    );

    let cached = SessionEventMessage::message_complete(
        "s1",
        "m-1",
        "done",
        Some(&crate::traits::llm::TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cache_read_tokens: Some(8),
            cache_creation_tokens: Some(2),
        }),
    );
    assert_eq!(
        wire(&cached, "message_complete"),
        serde_json::json!({
            "message_id": "m-1",
            "full_response": "done",
            "prompt_tokens": 10,
            "completion_tokens": 5,
            "total_tokens": 15,
            "cache_read_tokens": 8,
            "cache_creation_tokens": 2,
        })
    );
}

#[test]
fn golden_interaction_requested() {
    use crate::interaction::{InteractionRequest, PermAction, PermRequest};
    let request = InteractionRequest::Permission(PermRequest {
        action: PermAction::Bash {
            tokens: vec!["ls".into()],
        },
        diffs: Vec::new(),
    });
    let m = SessionEventMessage::interaction_requested("s1", "r-1", &request);
    assert_eq!(
        wire(&m, "interaction_requested"),
        serde_json::json!({
            "request_id": "r-1",
            "request": {"kind": "permission", "action": {"type": "bash", "tokens": ["ls"]}},
        })
    );
}

#[test]
fn golden_context_limit_resolved() {
    let m = SessionEventMessage::context_limit_resolved(
        "s1",
        128_000,
        crate::protocol::session_events::ContextLimitSource::Config,
    );
    assert_eq!(
        wire(&m, "context_limit_resolved"),
        serde_json::json!({"limit": 128_000, "source": "config"})
    );
}

// ── Settings / review wire goldens ───────────────────────────────

#[test]
fn golden_model_switched() {
    let m = SessionEventMessage::model_switched("s1", "gpt-4o", "openai");
    assert_eq!(
        wire(&m, "model_switched"),
        serde_json::json!({"model_id": "gpt-4o", "provider": "openai"})
    );
}

#[test]
fn golden_mode_changed() {
    let m = SessionEventMessage::mode_changed("s1", "plan");
    assert_eq!(
        wire(&m, "mode_changed"),
        serde_json::json!({"mode": "plan"})
    );
}

#[test]
fn golden_review_changed() {
    let m = SessionEventMessage::review_changed("s1", "rejected");
    assert_eq!(
        wire(&m, "review_changed"),
        serde_json::json!({"reason": "rejected"})
    );
}

// ── Workflow wire goldens ────────────────────────────────────────

#[test]
fn golden_workflow_step_started() {
    let m = SessionEventMessage::workflow_step_started("s1", "st-1", "Build");
    assert_eq!(
        wire(&m, "workflow.step_started"),
        serde_json::json!({"step_id": "st-1", "title": "Build"})
    );
}

/// `output_name: None` is serialized as `null`, not omitted.
#[test]
fn golden_workflow_step_completed_keeps_a_null_output_name() {
    let m = SessionEventMessage::workflow_step_completed("s1", "st-1", None);
    assert_eq!(
        wire(&m, "workflow.step_completed"),
        serde_json::json!({"step_id": "st-1", "output_name": null})
    );
}

#[test]
fn golden_workflow_gate_reached() {
    let m = SessionEventMessage::workflow_gate_reached("s1", "g-1", None, "alice");
    assert_eq!(
        wire(&m, "workflow.gate_reached"),
        serde_json::json!({"gate_id": "g-1", "title": null, "owner": "alice"})
    );
}

#[test]
fn golden_workflow_gate_approved() {
    let m = SessionEventMessage::workflow_gate_approved("s1", "g-1");
    assert_eq!(
        wire(&m, "workflow.gate_approved"),
        serde_json::json!({"gate_id": "g-1"})
    );
}

/// The trap: a UNIT variant under adjacent tagging omits `data`, which
/// surfaces as `null`. Today's wire is `{}`, so the typed form must use an
/// empty STRUCT variant.
#[test]
fn golden_payloadless_workflow_events_keep_an_empty_object_not_null() {
    let done = SessionEventMessage::workflow_completed("s1");
    assert_eq!(wire(&done, "workflow.completed"), serde_json::json!({}));
    let cancelled = SessionEventMessage::workflow_cancelled("s1");
    assert_eq!(
        wire(&cancelled, "workflow.cancelled"),
        serde_json::json!({})
    );
}

#[test]
fn golden_workflow_assessed() {
    let m = SessionEventMessage::workflow_assessed("s1", &[], &[], &["check docs".to_string()]);
    assert_eq!(
        wire(&m, "workflow.assessed"),
        serde_json::json!({
            "runnable_passed": [],
            "runnable_failed": [],
            "manual_entries": ["check docs"],
        })
    );
}

#[test]
fn golden_workflow_failed() {
    let m = SessionEventMessage::workflow_failed("s1", "boom", Some("st-2".into()));
    assert_eq!(
        wire(&m, "workflow.failed"),
        serde_json::json!({"reason": "boom", "at_step": "st-2"})
    );
}

#[test]
fn event_msg_type_always_event() {
    let factories: Vec<SessionEventMessage> = vec![
        SessionEventMessage::text_delta("s1", "x"),
        SessionEventMessage::thinking("s1", "x"),
        SessionEventMessage::tool_call("s1", "c", "t", Value::Null),
        SessionEventMessage::tool_result("s1", "c", "t", Value::Null),
        SessionEventMessage::ended("s1", "done"),
        SessionEventMessage::model_switched("s1", "m", "p"),
        SessionEventMessage::message_complete("s1", "m", "r", None),
        SessionEventMessage::user_message("s1", "m", "c"),
    ];
    for (i, evt) in factories.iter().enumerate() {
        assert_eq!(
            evt.msg_type, "event",
            "factory index {} produced msg_type {:?} instead of \"event\"",
            i, evt.msg_type
        );
    }
}
