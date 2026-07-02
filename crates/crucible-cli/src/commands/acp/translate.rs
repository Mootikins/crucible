//! Pure translation between Crucible daemon `SessionEvent`s and ACP wire types.
//!
//! Kept free of I/O so the mapping table can be unit-tested exhaustively. The
//! event pump in [`super::agent`] consumes [`classify_event`] and drives the
//! side effects (sending `session/update`, requesting permission).

use agent_client_protocol::{
    ContentBlock, ContentChunk, PermissionOption, PermissionOptionId, PermissionOptionKind,
    RequestPermissionOutcome, SessionUpdate, StopReason, TextContent, ToolCall, ToolCallContent,
    ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use crucible_core::interaction::{
    InteractionRequest, InteractionResponse, PermResponse, PermissionScope,
};
use crucible_daemon::SessionEvent;

/// Permission option IDs advertised to the host. Matching them back in
/// [`outcome_to_interaction_response`] must use these exact strings.
pub const OPT_ALLOW_ONCE: &str = "allow_once";
pub const OPT_ALLOW_ALWAYS: &str = "allow_always";
pub const OPT_REJECT_ONCE: &str = "reject_once";
pub const OPT_REJECT_ALWAYS: &str = "reject_always";

/// One step of a prompt turn, derived from a single daemon `SessionEvent`.
#[derive(Debug)]
pub enum TurnStep {
    /// Forward this update to the host via `session/update`.
    Update(Box<SessionUpdate>),
    /// The daemon needs a decision; drive an ACP `session/request_permission`.
    Interaction {
        request_id: String,
        request: Box<InteractionRequest>,
    },
    /// The turn is over; respond to `session/prompt` with this stop reason.
    Finished(StopReason),
    /// Nothing to forward (unknown or empty event).
    Ignore,
}

/// Classify a daemon event into a single turn step.
///
/// Mirrors the daemon-proxy mapping in
/// `crucible_daemon::rpc_client::agent::convert` but targets ACP wire types
/// instead of `TurnEvent`.
pub fn classify_event(event: &SessionEvent) -> TurnStep {
    match event.event_type.as_str() {
        "text_delta" => text(event)
            .map(|c| update(SessionUpdate::AgentMessageChunk(chunk(c))))
            .unwrap_or(TurnStep::Ignore),
        "thinking" => text(event)
            .map(|c| update(SessionUpdate::AgentThoughtChunk(chunk(c))))
            .unwrap_or(TurnStep::Ignore),
        "tool_call" => classify_tool_call(event),
        "tool_result" => classify_tool_result(event),
        "message_complete" => TurnStep::Finished(StopReason::EndTurn),
        "ended" => TurnStep::Finished(ended_stop_reason(event)),
        "interaction_requested" => classify_interaction(event),
        _ => TurnStep::Ignore,
    }
}

/// Wrap a session update as a boxed `TurnStep::Update` (the variant is boxed to
/// keep the enum small; `SessionUpdate` is large).
fn update(u: SessionUpdate) -> TurnStep {
    TurnStep::Update(Box::new(u))
}

fn text(event: &SessionEvent) -> Option<String> {
    event
        .data
        .get("content")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn chunk(text: String) -> ContentChunk {
    ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
}

fn classify_tool_call(event: &SessionEvent) -> TurnStep {
    let Some(tool) = event.data.get("tool").and_then(|v| v.as_str()) else {
        return TurnStep::Ignore;
    };
    let call_id = event
        .data
        .get("call_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let args = event.data.get("args").cloned();

    let mut tc = ToolCall::new(call_id, humanize_title(tool))
        .kind(tool_kind(tool))
        .status(ToolCallStatus::InProgress);
    if let Some(args) = args {
        tc = tc.raw_input(args);
    }
    update(SessionUpdate::ToolCall(tc))
}

fn classify_tool_result(event: &SessionEvent) -> TurnStep {
    let Some(result) = event.data.get("result") else {
        return TurnStep::Ignore;
    };
    let call_id = event
        .data
        .get("call_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let error = result.get("error").and_then(|e| e.as_str());
    let (status, text) = match error {
        Some(msg) => (ToolCallStatus::Failed, msg.to_string()),
        None => (ToolCallStatus::Completed, summarize_result(result)),
    };

    let fields = ToolCallUpdateFields::new()
        .status(status)
        .content(vec![ToolCallContent::from(ContentBlock::Text(
            TextContent::new(text),
        ))])
        .raw_output(result.clone());
    update(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
        call_id, fields,
    )))
}

fn classify_interaction(event: &SessionEvent) -> TurnStep {
    let request_id = event.data.get("request_id").and_then(|v| v.as_str());
    let request = event.data.get("request");
    match (request_id, request) {
        (Some(id), Some(req)) => match serde_json::from_value::<InteractionRequest>(req.clone()) {
            Ok(request) => TurnStep::Interaction {
                request_id: id.to_string(),
                request: Box::new(request),
            },
            Err(_) => TurnStep::Ignore,
        },
        _ => TurnStep::Ignore,
    }
}

/// Daemon `ended` events carry a free-form reason. Only an explicit
/// cancellation maps to `Cancelled`; everything else (including `error: ...`)
/// is a normal end of turn as far as ACP's stop reasons are concerned.
fn ended_stop_reason(event: &SessionEvent) -> StopReason {
    let reason = event
        .data
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if reason.contains("cancel") {
        StopReason::Cancelled
    } else {
        StopReason::EndTurn
    }
}

fn summarize_result(result: &serde_json::Value) -> String {
    // Prefer a human-readable field when the tool provides one; otherwise fall
    // back to compact JSON so the host still sees something.
    for key in ["output", "content", "text", "message"] {
        if let Some(s) = result.get(key).and_then(|v| v.as_str()) {
            return s.to_string();
        }
    }
    match result {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Coarse tool-kind classification by name, for host icon/UI selection.
pub fn tool_kind(name: &str) -> ToolKind {
    let n = name.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|w| n.contains(w));
    if has(&["delete", "remove", "rm_", "unlink"]) {
        ToolKind::Delete
    } else if has(&[
        "write", "edit", "create", "patch", "apply", "insert", "replace",
    ]) {
        ToolKind::Edit
    } else if has(&["read", "cat", "open", "view", "get_note", "show"]) {
        ToolKind::Read
    } else if has(&["search", "grep", "find", "query", "vector", "recall"]) {
        ToolKind::Search
    } else if has(&["bash", "shell", "exec", "run", "command"]) {
        ToolKind::Execute
    } else if has(&["fetch", "http", "curl", "download", "web"]) {
        ToolKind::Fetch
    } else {
        ToolKind::Other
    }
}

fn humanize_title(name: &str) -> String {
    name.replace(['_', '-'], " ")
}

/// Build the permission options offered to the host for a Crucible permission
/// request. Always the standard four; a host may present a subset.
pub fn permission_options() -> Vec<PermissionOption> {
    vec![
        PermissionOption::new(
            PermissionOptionId::new(OPT_ALLOW_ONCE),
            "Allow once",
            PermissionOptionKind::AllowOnce,
        ),
        PermissionOption::new(
            PermissionOptionId::new(OPT_ALLOW_ALWAYS),
            "Allow always",
            PermissionOptionKind::AllowAlways,
        ),
        PermissionOption::new(
            PermissionOptionId::new(OPT_REJECT_ONCE),
            "Reject once",
            PermissionOptionKind::RejectOnce,
        ),
        PermissionOption::new(
            PermissionOptionId::new(OPT_REJECT_ALWAYS),
            "Reject always",
            PermissionOptionKind::RejectAlways,
        ),
    ]
}

/// Describe an interaction request as an ACP `ToolCallUpdate` for the permission
/// prompt's `tool_call` field. Non-permission interactions still render a
/// title so the host can show something meaningful.
pub fn interaction_tool_call(request_id: &str, request: &InteractionRequest) -> ToolCallUpdate {
    let (title, kind) = match request {
        InteractionRequest::Permission(perm) => {
            use crucible_core::interaction::PermAction;
            match &perm.action {
                PermAction::Bash { tokens } => {
                    (format!("Run: {}", tokens.join(" ")), ToolKind::Execute)
                }
                PermAction::Read { segments } => {
                    (format!("Read {}", segments.join("/")), ToolKind::Read)
                }
                PermAction::Write { segments } => {
                    (format!("Write {}", segments.join("/")), ToolKind::Edit)
                }
                PermAction::Tool { name, .. } => (humanize_title(name), tool_kind(name)),
            }
        }
        other => (format!("Approve {}", other.kind()), ToolKind::Other),
    };
    ToolCallUpdate::new(
        request_id.to_string(),
        ToolCallUpdateFields::new().title(title).kind(kind),
    )
}

/// Map the host's chosen permission option back to a Crucible
/// [`InteractionResponse`]. `None` outcome (host cancelled) maps to
/// [`InteractionResponse::Cancelled`].
pub fn outcome_to_interaction_response(
    outcome: &RequestPermissionOutcome,
    request: &InteractionRequest,
) -> InteractionResponse {
    let selected = match outcome {
        RequestPermissionOutcome::Selected(sel) => sel.option_id.0.as_ref(),
        _ => return InteractionResponse::Cancelled,
    };

    // Suggested allowlist pattern for the "always" scopes.
    let pattern = match request {
        InteractionRequest::Permission(perm) => Some(perm.suggested_pattern()),
        _ => None,
    };

    let perm = match selected {
        OPT_ALLOW_ONCE => PermResponse::allow(),
        OPT_ALLOW_ALWAYS => match pattern {
            Some(p) => PermResponse::allow_pattern(p, PermissionScope::Session),
            None => PermResponse::allow(),
        },
        OPT_REJECT_ALWAYS => PermResponse::deny_with_reason("denied by host (always)"),
        // Default and OPT_REJECT_ONCE: deny once.
        _ => PermResponse::deny(),
    };
    InteractionResponse::Permission(perm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::interaction::{PermAction, PermRequest};
    use serde_json::json;

    fn event(event_type: &str, data: serde_json::Value) -> SessionEvent {
        SessionEvent {
            session_id: "s1".into(),
            event_type: event_type.into(),
            data,
        }
    }

    #[test]
    fn text_delta_becomes_agent_message_chunk() {
        let step = classify_event(&event("text_delta", json!({"content": "hello"})));
        match step {
            TurnStep::Update(u) => match *u {
                SessionUpdate::AgentMessageChunk(c) => match c.content {
                    ContentBlock::Text(t) => assert_eq!(t.text, "hello"),
                    other => panic!("expected text block, got {other:?}"),
                },
                other => panic!("expected agent message chunk, got {other:?}"),
            },
            other => panic!("expected update, got {other:?}"),
        }
    }

    #[test]
    fn thinking_becomes_agent_thought_chunk() {
        let step = classify_event(&event("thinking", json!({"content": "hmm"})));
        assert!(matches!(
            step,
            TurnStep::Update(u) if matches!(*u, SessionUpdate::AgentThoughtChunk(_))
        ));
    }

    #[test]
    fn tool_call_becomes_in_progress_tool_call() {
        let step = classify_event(&event(
            "tool_call",
            json!({"call_id": "tc1", "tool": "search_notes", "args": {"q": "rust"}}),
        ));
        match step {
            TurnStep::Update(u) => match *u {
                SessionUpdate::ToolCall(tc) => {
                    assert_eq!(tc.tool_call_id.0.as_ref(), "tc1");
                    assert_eq!(tc.status, ToolCallStatus::InProgress);
                    assert_eq!(tc.kind, ToolKind::Search);
                    assert_eq!(tc.raw_input, Some(json!({"q": "rust"})));
                }
                other => panic!("expected tool call, got {other:?}"),
            },
            other => panic!("expected update, got {other:?}"),
        }
    }

    #[test]
    fn tool_result_success_completes() {
        let step = classify_event(&event(
            "tool_result",
            json!({"call_id": "tc1", "tool": "search_notes", "result": {"output": "done"}}),
        ));
        match step {
            TurnStep::Update(u) => match *u {
                SessionUpdate::ToolCallUpdate(tc) => {
                    assert_eq!(tc.tool_call_id.0.as_ref(), "tc1");
                    assert_eq!(tc.fields.status, Some(ToolCallStatus::Completed));
                }
                other => panic!("expected tool call update, got {other:?}"),
            },
            other => panic!("expected update, got {other:?}"),
        }
    }

    #[test]
    fn tool_result_error_fails() {
        let step = classify_event(&event(
            "tool_result",
            json!({"call_id": "tc1", "tool": "bash", "result": {"error": "boom"}}),
        ));
        match step {
            TurnStep::Update(u) => match *u {
                SessionUpdate::ToolCallUpdate(tc) => {
                    assert_eq!(tc.fields.status, Some(ToolCallStatus::Failed));
                }
                other => panic!("expected failed tool update, got {other:?}"),
            },
            other => panic!("expected update, got {other:?}"),
        }
    }

    #[test]
    fn message_complete_finishes_end_turn() {
        let step = classify_event(&event("message_complete", json!({"total_tokens": 5})));
        assert!(matches!(step, TurnStep::Finished(StopReason::EndTurn)));
    }

    #[test]
    fn ended_cancel_maps_to_cancelled() {
        let step = classify_event(&event("ended", json!({"reason": "cancelled by user"})));
        assert!(matches!(step, TurnStep::Finished(StopReason::Cancelled)));
    }

    #[test]
    fn ended_error_maps_to_end_turn() {
        let step = classify_event(&event("ended", json!({"reason": "error: boom"})));
        assert!(matches!(step, TurnStep::Finished(StopReason::EndTurn)));
    }

    #[test]
    fn unknown_event_is_ignored() {
        assert!(matches!(
            classify_event(&event("mystery", json!({}))),
            TurnStep::Ignore
        ));
    }

    #[test]
    fn interaction_event_parses_permission_request() {
        let req = InteractionRequest::Permission(PermRequest::bash(["cargo", "test"]));
        let step = classify_event(&event(
            "interaction_requested",
            json!({"request_id": "r1", "request": serde_json::to_value(&req).unwrap()}),
        ));
        match step {
            TurnStep::Interaction {
                request_id,
                request,
            } => {
                assert_eq!(request_id, "r1");
                assert!(matches!(*request, InteractionRequest::Permission(_)));
            }
            other => panic!("expected interaction, got {other:?}"),
        }
    }

    #[test]
    fn allow_once_maps_to_allow() {
        let req = InteractionRequest::Permission(PermRequest::bash(["ls"]));
        let outcome = RequestPermissionOutcome::Selected(
            agent_client_protocol::SelectedPermissionOutcome::new(PermissionOptionId::new(
                OPT_ALLOW_ONCE,
            )),
        );
        match outcome_to_interaction_response(&outcome, &req) {
            InteractionResponse::Permission(p) => {
                assert!(p.allowed);
                assert_eq!(p.scope, PermissionScope::Once);
            }
            other => panic!("expected permission response, got {other:?}"),
        }
    }

    #[test]
    fn allow_always_carries_pattern_and_session_scope() {
        let req = InteractionRequest::Permission(PermRequest::bash(["cargo", "test"]));
        let outcome = RequestPermissionOutcome::Selected(
            agent_client_protocol::SelectedPermissionOutcome::new(PermissionOptionId::new(
                OPT_ALLOW_ALWAYS,
            )),
        );
        match outcome_to_interaction_response(&outcome, &req) {
            InteractionResponse::Permission(p) => {
                assert!(p.allowed);
                assert_eq!(p.scope, PermissionScope::Session);
                assert_eq!(p.pattern.as_deref(), Some("cargo *"));
            }
            other => panic!("expected permission response, got {other:?}"),
        }
    }

    #[test]
    fn reject_once_maps_to_deny() {
        let req = InteractionRequest::Permission(PermRequest::bash(["ls"]));
        let outcome = RequestPermissionOutcome::Selected(
            agent_client_protocol::SelectedPermissionOutcome::new(PermissionOptionId::new(
                OPT_REJECT_ONCE,
            )),
        );
        match outcome_to_interaction_response(&outcome, &req) {
            InteractionResponse::Permission(p) => assert!(!p.allowed),
            other => panic!("expected permission response, got {other:?}"),
        }
    }

    #[test]
    fn cancelled_outcome_maps_to_cancelled() {
        let req = InteractionRequest::Permission(PermRequest::bash(["ls"]));
        let resp = outcome_to_interaction_response(&RequestPermissionOutcome::Cancelled, &req);
        assert!(matches!(resp, InteractionResponse::Cancelled));
    }

    #[test]
    fn tool_kind_classifies_common_names() {
        assert_eq!(tool_kind("read_file"), ToolKind::Read);
        assert_eq!(tool_kind("write_file"), ToolKind::Edit);
        assert_eq!(tool_kind("bash"), ToolKind::Execute);
        assert_eq!(tool_kind("search_vectors"), ToolKind::Search);
        assert_eq!(tool_kind("delete_note"), ToolKind::Delete);
        assert_eq!(tool_kind("http_fetch"), ToolKind::Fetch);
        assert_eq!(tool_kind("mystery_tool"), ToolKind::Other);
    }

    #[test]
    fn permission_action_titles() {
        let perm = PermRequest {
            action: PermAction::Tool {
                name: "search_notes".into(),
                args: json!({}),
            },
            diffs: vec![],
        };
        let req = InteractionRequest::Permission(perm);
        let tc = interaction_tool_call("r1", &req);
        assert_eq!(tc.fields.title.as_deref(), Some("search notes"));
    }
}
