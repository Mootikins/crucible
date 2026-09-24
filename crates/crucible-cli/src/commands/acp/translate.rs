//! Pure translation between Crucible daemon `SessionEvent`s and ACP wire types.
//!
//! Kept free of I/O so the mapping table can be unit-tested exhaustively. The
//! event pump in [`super::agent`] consumes [`classify_event`] and drives the
//! side effects (sending `session/update`, requesting permission).

use agent_client_protocol::schema::v1::{
    ContentBlock, ContentChunk, PermissionOption, PermissionOptionId, PermissionOptionKind,
    RequestPermissionOutcome, SessionUpdate, StopReason, TextContent, ToolCall, ToolCallContent,
    ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use crucible_core::interaction::{
    InteractionRequest, InteractionResponse, PermResponse, PermissionScope,
};
use crucible_core::protocol::session_events::{SessionEventPayload, TurnPayload};
use crucible_core::turn::{StopReason as CoreStopReason, TurnStatus};
use crucible_core::types::{CanonicalToolCall, ToolRender};
use crucible_daemon::SessionEvent;

/// Permission option IDs advertised to the host. Matching them back in
/// [`outcome_to_interaction_response`] must use these exact strings.
pub const OPT_ALLOW_ONCE: &str = "allow_once";
pub const OPT_ALLOW_ALWAYS: &str = "allow_always";
pub const OPT_REJECT_ONCE: &str = "reject_once";

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
    /// The whole turn is over (`turn_finished`); respond to `session/prompt`.
    Finished(TurnEnd),
    /// Nothing to forward (unknown or empty event).
    Ignore,
}

/// How `cru acp` answers `session/prompt` when the turn is over.
#[derive(Debug, PartialEq)]
pub enum TurnEnd {
    /// Reply with this stop reason.
    Stop(StopReason),
    /// A handler stopped the turn. Send the reason as turn text, then reply
    /// with `refusal`. ACP keeps `cancelled` for a user cancel.
    Refused(String),
    /// The turn failed or timed out. Reply with a JSON-RPC error that holds
    /// this text, and with no result.
    Failed(String),
}

/// The one table from a daemon turn status to an ACP answer.
pub fn turn_end(
    status: TurnStatus,
    stop_reason: Option<CoreStopReason>,
    error: Option<String>,
) -> TurnEnd {
    let text = || {
        error
            .clone()
            .unwrap_or_else(|| format!("the turn ended: {status:?}"))
    };
    match status {
        TurnStatus::Completed => TurnEnd::Stop(match stop_reason {
            // An empty turn is still an end of turn for the editor.
            None | Some(CoreStopReason::EndTurn | CoreStopReason::Empty) => StopReason::EndTurn,
            Some(CoreStopReason::MaxTokens) => StopReason::MaxTokens,
            Some(CoreStopReason::Refusal) => StopReason::Refusal,
            Some(CoreStopReason::Cancelled) => StopReason::Cancelled,
        }),
        TurnStatus::Cancelled => TurnEnd::Stop(StopReason::Cancelled),
        TurnStatus::HandlerCancelled => TurnEnd::Refused(text()),
        TurnStatus::Failed | TurnStatus::TimedOut => TurnEnd::Failed(text()),
    }
}

/// Does this event open the turn that `message_id` names?
///
/// A `turn:complete` handler can start a turn of its own after a client
/// answered, so a client's queue can hold the events of a turn it never
/// started. Every turn opens with the `user_message` that carries its id, and
/// a client reads nothing before its own.
pub fn opens_turn(event: &SessionEvent, message_id: &str) -> bool {
    matches!(
        event.payload(),
        Ok(SessionEventPayload::Turn(TurnPayload::UserMessage { message_id: id, .. }))
            if id == message_id
    )
}

/// Classify a daemon event into a single turn step.
///
/// Mirrors the daemon-proxy mapping in
/// `crucible_daemon::rpc_client::agent::convert` but targets ACP wire types
/// instead of `TurnEvent`.
pub fn classify_event(event: &SessionEvent) -> TurnStep {
    match event.event.as_str() {
        "text_delta" => text(event)
            .map(|c| update(SessionUpdate::AgentMessageChunk(chunk(c))))
            .unwrap_or(TurnStep::Ignore),
        "thinking" => text(event)
            .map(|c| update(SessionUpdate::AgentThoughtChunk(chunk(c))))
            .unwrap_or(TurnStep::Ignore),
        "tool_call" => classify_tool_call(event),
        "tool_result" => classify_tool_result(event),
        // `message_complete` seals one reply. `turn_finished` is the one
        // event that ends the turn.
        "turn_finished" => match event.payload() {
            Ok(SessionEventPayload::Turn(TurnPayload::TurnFinished {
                status,
                stop_reason,
                error,
            })) => TurnStep::Finished(turn_end(status, stop_reason, error)),
            _ => TurnStep::Finished(TurnEnd::Failed(format!(
                "the daemon sent a turn_finished event that does not decode: {}",
                event.data
            ))),
        },
        "interaction_requested" => classify_interaction(event),
        _ => TurnStep::Ignore,
    }
}

/// Wrap a session update as a boxed `TurnStep::Update` (the variant is boxed to
/// keep the enum small; `SessionUpdate` is large).
fn update(u: SessionUpdate) -> TurnStep {
    TurnStep::Update(Box::new(u))
}

/// One step of a `session/load` replay, derived from a recorded daemon event.
///
/// Same mapping as [`classify_event`], plus the user's prompt. The live pump
/// never forwards the prompt — the host renders the text it just sent — but a
/// host keeps no transcript across restarts, so the recorded `user_message`
/// frames are the only copy of the user's side of the conversation and replay
/// forwards them as `UserMessageChunk`. Terminal steps and interactions still
/// classify as themselves so the replay caller can skip them: a finished turn
/// answers nothing, and an answered permission request must not be re-asked.
pub fn replay_step(event: &SessionEvent) -> TurnStep {
    if event.event == "user_message" {
        return match text(event) {
            Some(content) => update(SessionUpdate::UserMessageChunk(chunk(content))),
            None => TurnStep::Ignore,
        };
    }
    classify_event(event)
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
    // The daemon sends the canonical call with its render. An event with
    // no call is `Other`: the name alone does not say the kind.
    let call = (event.data.get("display"))
        .and_then(|d| serde_json::from_value::<CanonicalToolCall>(d.clone()).ok());
    let (title, kind) = describe(tool, call.as_ref());

    let mut tc = ToolCall::new(call_id, title)
        .kind(kind)
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

    let mut fields = ToolCallUpdateFields::new()
        .status(status)
        .content(vec![ToolCallContent::from(ContentBlock::Text(
            TextContent::new(text),
        ))])
        .raw_output(result.clone());
    // The render of the finished call gives the title its summary.
    let render =
        (result.get("render")).and_then(|r| serde_json::from_value::<ToolRender>(r.clone()).ok());
    if let Some(render) = render.filter(|r| r.summary.is_some()) {
        let tool = event
            .data
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("tool");
        let summary = render.summary.clone().unwrap_or_default();
        let call = CanonicalToolCall {
            render: Some(render),
            ..CanonicalToolCall::crucible_tool(tool, &serde_json::Value::Null)
        };
        let (title, _) = describe(tool, Some(&call));
        fields = fields.title(format!("{title} → {summary}"));
    }
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

/// The ACP title and kind of a call: the canonical tool and the render
/// line, and the ACP kind of the canonical kind. A kind that ACP does not
/// name, and a call with no canonical form, is `Other`.
fn describe(name: &str, call: Option<&CanonicalToolCall>) -> (String, ToolKind) {
    let Some(call) = call else {
        return (humanize_title(name), ToolKind::Other);
    };
    let tool = humanize_title(if call.tool.is_empty() {
        name
    } else {
        &call.tool
    });
    let title = match call.render.as_ref().and_then(|r| r.line.as_deref()) {
        Some(line) => format!("{tool}: {line}"),
        None => tool,
    };
    let kind = match call.kind.as_str() {
        "command" => ToolKind::Execute,
        "file_edit" => ToolKind::Edit,
        "file_read" => ToolKind::Read,
        "fetch" => ToolKind::Fetch,
        "search" => ToolKind::Search,
        _ => ToolKind::Other,
    };
    (title, kind)
}

fn humanize_title(name: &str) -> String {
    name.replace(['_', '-'], " ")
}

/// Build the permission options offered to the host for a Crucible permission
/// request. ACP lets an agent offer any subset of the four kinds. There is no
/// "reject always", because the daemon cannot store a deny rule: the option
/// would give a one-time deny under a wider name. For the same reason there
/// is no "allow always" when no grant can name the call.
pub fn permission_options(request: &InteractionRequest) -> Vec<PermissionOption> {
    let grant =
        matches!(request, InteractionRequest::Permission(p) if p.suggested_pattern().is_some());
    [
        (
            OPT_ALLOW_ONCE,
            "Allow once",
            PermissionOptionKind::AllowOnce,
        ),
        (
            OPT_ALLOW_ALWAYS,
            "Allow always",
            PermissionOptionKind::AllowAlways,
        ),
        (
            OPT_REJECT_ONCE,
            "Reject once",
            PermissionOptionKind::RejectOnce,
        ),
    ]
    .into_iter()
    .filter(|(id, _, _)| grant || *id != OPT_ALLOW_ALWAYS)
    .map(|(id, name, kind)| PermissionOption::new(PermissionOptionId::new(id), name, kind))
    .collect()
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
                PermAction::Tool { name, .. } => describe(name, perm.call.as_deref()),
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
        InteractionRequest::Permission(perm) => perm.suggested_pattern(),
        _ => None,
    };

    let perm = match selected {
        OPT_ALLOW_ONCE => PermResponse::allow(),
        OPT_ALLOW_ALWAYS => match pattern {
            Some(p) => PermResponse::allow_pattern(p, PermissionScope::Session),
            None => PermResponse::allow(),
        },
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
        SessionEvent::new("s1", event_type, data)
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
            json!({
                "call_id": "tc1", "tool": "search_notes", "args": {"q": "rust"},
                "display": {
                    "kind": "search", "tool": "search_notes", "query": "rust",
                    "render": { "line": "rust" },
                },
            }),
        ));
        match step {
            TurnStep::Update(u) => match *u {
                SessionUpdate::ToolCall(tc) => {
                    assert_eq!(tc.tool_call_id.0.as_ref(), "tc1");
                    assert_eq!(tc.status, ToolCallStatus::InProgress);
                    assert_eq!(tc.kind, ToolKind::Search, "the canonical kind");
                    assert_eq!(tc.title, "search notes: rust");
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

    /// The render of the result gives the title its summary.
    #[test]
    fn tool_result_render_titles_the_summary() {
        let step = classify_event(&event(
            "tool_result",
            json!({"call_id": "tc1", "tool": "read_file", "result": {
                "result": "a", "render": {"line": "a.rs", "summary": "1 lines"},
            }}),
        ));
        match step {
            TurnStep::Update(u) => match *u {
                SessionUpdate::ToolCallUpdate(tc) => {
                    assert_eq!(
                        tc.fields.title.as_deref(),
                        Some("read file: a.rs → 1 lines")
                    );
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

    /// `message_complete` seals one reply, so it must not end the prompt
    /// turn: only `turn_finished` does.
    #[test]
    fn message_complete_does_not_finish_the_turn() {
        let step = classify_event(&event(
            "message_complete",
            json!({"total_tokens": 5, "stop_reason": "max_tokens"}),
        ));
        assert!(matches!(step, TurnStep::Ignore), "got {step:?}");
    }

    fn finished(data: serde_json::Value) -> TurnEnd {
        match classify_event(&event("turn_finished", data)) {
            TurnStep::Finished(end) => end,
            other => panic!("expected Finished, got {other:?}"),
        }
    }

    #[test]
    fn turn_finished_completed_maps_the_stop_reason() {
        let cases = [
            (None, StopReason::EndTurn),
            (Some("end_turn"), StopReason::EndTurn),
            (Some("empty"), StopReason::EndTurn),
            (Some("max_tokens"), StopReason::MaxTokens),
            (Some("refusal"), StopReason::Refusal),
            (Some("cancelled"), StopReason::Cancelled),
        ];
        for (stop_reason, expected) in cases {
            assert_eq!(
                finished(json!({"status": "completed", "stop_reason": stop_reason})),
                TurnEnd::Stop(expected),
                "{stop_reason:?}"
            );
        }
    }

    #[test]
    fn turn_finished_user_cancel_maps_to_cancelled() {
        assert_eq!(
            finished(json!({"status": "cancelled"})),
            TurnEnd::Stop(StopReason::Cancelled)
        );
    }

    /// ACP keeps `cancelled` for a user cancel. A handler cancel is a refusal
    /// that carries the handler's reason.
    #[test]
    fn turn_finished_handler_cancel_maps_to_refused_with_the_reason() {
        assert_eq!(
            finished(json!({
                "status": "handler_cancelled",
                "error": "cancelled by pre_llm_call handler"
            })),
            TurnEnd::Refused("cancelled by pre_llm_call handler".to_string())
        );
    }

    #[test]
    fn turn_finished_failure_maps_to_failed_even_when_its_text_says_cancel() {
        for status in ["failed", "timed_out"] {
            assert_eq!(
                finished(json!({"status": status, "error": "request cancelled upstream"})),
                TurnEnd::Failed("request cancelled upstream".to_string()),
                "{status}"
            );
        }
    }

    /// A client reads nothing before the turn it started. A `turn:complete`
    /// handler can start a turn after the client answered, and that turn's
    /// events then wait in the client's queue.
    #[test]
    fn opens_turn_matches_only_the_user_message_of_that_turn() {
        let ours = event(
            "user_message",
            json!({"message_id": "m-2", "content": "hi"}),
        );
        assert!(opens_turn(&ours, "m-2"));
        assert!(!opens_turn(&ours, "m-1"));

        let plugin_turn = event(
            "user_message",
            json!({"message_id": "m-3", "content": "keep going", "origin": "plugin"}),
        );
        assert!(!opens_turn(&plugin_turn, "m-2"));
        assert!(opens_turn(&plugin_turn, "m-3"));

        let stale_end = event("turn_finished", json!({"status": "completed"}));
        assert!(!opens_turn(&stale_end, "m-2"));
    }

    #[test]
    fn unknown_event_is_ignored() {
        assert!(matches!(
            classify_event(&event("mystery", json!({}))),
            TurnStep::Ignore
        ));
    }

    #[test]
    fn replay_user_message_becomes_user_chunk() {
        let step = replay_step(&event(
            "user_message",
            json!({"message_id": "m1", "content": "Fix the parser"}),
        ));
        match step {
            TurnStep::Update(u) => match *u {
                SessionUpdate::UserMessageChunk(c) => match c.content {
                    ContentBlock::Text(t) => assert_eq!(t.text, "Fix the parser"),
                    other => panic!("expected text block, got {other:?}"),
                },
                other => panic!("expected user message chunk, got {other:?}"),
            },
            other => panic!("expected update, got {other:?}"),
        }
    }

    #[test]
    fn replay_user_message_without_text_is_ignored() {
        assert!(matches!(
            replay_step(&event("user_message", json!({"message_id": "m1"}))),
            TurnStep::Ignore
        ));
    }

    /// A recorded permission request is history, not a live question: the
    /// replay caller must see an Interaction (to skip), not an update that
    /// re-asks the host to approve something already answered.
    #[test]
    fn replay_keeps_interactions_classified_so_the_caller_skips_them() {
        let req = InteractionRequest::Permission(PermRequest::bash(["cargo", "test"]));
        let step = replay_step(&event(
            "interaction_requested",
            json!({"request_id": "r1", "request": serde_json::to_value(&req).unwrap()}),
        ));
        assert!(matches!(step, TurnStep::Interaction { .. }));
    }

    /// Terminal steps classify as themselves: the replay loop skips them
    /// instead of ending the replay early or answering a prompt nobody sent.
    #[test]
    fn replay_maps_turn_finished_to_finished_not_an_update() {
        let step = replay_step(&event("turn_finished", json!({"status": "completed"})));
        assert!(matches!(step, TurnStep::Finished(_)), "got {step:?}");
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
            agent_client_protocol::schema::v1::SelectedPermissionOutcome::new(
                PermissionOptionId::new(OPT_ALLOW_ONCE),
            ),
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
            agent_client_protocol::schema::v1::SelectedPermissionOutcome::new(
                PermissionOptionId::new(OPT_ALLOW_ALWAYS),
            ),
        );
        match outcome_to_interaction_response(&outcome, &req) {
            InteractionResponse::Permission(p) => {
                assert!(p.allowed);
                assert_eq!(p.scope, PermissionScope::Session);
                // The grant is the command the host displayed, not every
                // `cargo` invocation: a suggestion never widens the request.
                assert_eq!(p.pattern.as_deref(), Some("cargo test"));
            }
            other => panic!("expected permission response, got {other:?}"),
        }
    }

    #[test]
    fn reject_once_maps_to_deny() {
        let req = InteractionRequest::Permission(PermRequest::bash(["ls"]));
        let outcome = RequestPermissionOutcome::Selected(
            agent_client_protocol::schema::v1::SelectedPermissionOutcome::new(
                PermissionOptionId::new(OPT_REJECT_ONCE),
            ),
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

    /// The name alone does not give a kind: `cru acp` guesses nothing.
    #[test]
    fn a_tool_call_with_no_canonical_call_is_other() {
        let step = classify_event(&event(
            "tool_call",
            json!({"call_id": "tc1", "tool": "read_file", "args": {}}),
        ));
        let TurnStep::Update(u) = step else {
            panic!("expected update");
        };
        let SessionUpdate::ToolCall(tc) = *u else {
            panic!("expected tool call");
        };
        assert_eq!(tc.kind, ToolKind::Other);
    }

    #[test]
    fn permission_action_titles() {
        let perm = PermRequest {
            action: PermAction::Tool {
                name: "search_notes".into(),
                args: json!({}),
            },
            ..PermRequest::bash(["x"])
        };
        let req = InteractionRequest::Permission(perm);
        let tc = interaction_tool_call("r1", &req);
        assert_eq!(tc.fields.title.as_deref(), Some("search notes"));
    }

    fn selected(option_id: &str) -> RequestPermissionOutcome {
        RequestPermissionOutcome::Selected(
            agent_client_protocol::schema::v1::SelectedPermissionOutcome::new(
                PermissionOptionId::new(option_id),
            ),
        )
    }

    /// The host sees the three options in this order, and each id matches
    /// the constant `outcome_to_interaction_response` reads back. There is no
    /// "reject always", because the daemon cannot store a deny rule.
    #[test]
    fn permission_options_offer_the_three_ids_in_order() {
        let request = InteractionRequest::Permission(PermRequest::bash(["ls"]));
        let offered: Vec<(String, PermissionOptionKind)> = permission_options(&request)
            .into_iter()
            .map(|o| (o.option_id.0.to_string(), o.kind))
            .collect();
        assert_eq!(
            offered,
            vec![
                (OPT_ALLOW_ONCE.to_string(), PermissionOptionKind::AllowOnce),
                (
                    OPT_ALLOW_ALWAYS.to_string(),
                    PermissionOptionKind::AllowAlways
                ),
                (
                    OPT_REJECT_ONCE.to_string(),
                    PermissionOptionKind::RejectOnce
                ),
            ]
        );
    }

    /// With no grant that can name the call, "allow always" would save
    /// nothing, so the host is not offered it.
    #[test]
    fn permission_options_offer_no_allow_always_without_a_grant() {
        let request = InteractionRequest::Permission(PermRequest::tool("tool", json!({})));
        let offered = permission_options(&request);
        assert_eq!(offered.len(), 2);
        assert!(offered
            .iter()
            .all(|o| o.kind != PermissionOptionKind::AllowAlways));
    }

    /// Allow once grants this call only: no pattern, no reason.
    #[test]
    fn allow_once_is_exactly_a_one_time_allow() {
        let req = InteractionRequest::Permission(PermRequest::bash(["ls"]));
        let resp = outcome_to_interaction_response(&selected(OPT_ALLOW_ONCE), &req);
        assert!(matches!(resp, InteractionResponse::Permission(p) if p == PermResponse::allow()));
    }

    /// Reject once denies this call only, with no scope that outlives it.
    #[test]
    fn reject_once_is_exactly_a_one_time_deny() {
        let req = InteractionRequest::Permission(PermRequest::bash(["ls"]));
        let resp = outcome_to_interaction_response(&selected(OPT_REJECT_ONCE), &req);
        assert!(matches!(resp, InteractionResponse::Permission(p) if p == PermResponse::deny()));
    }

    /// An id the adapter never offered fails closed: a one-time deny, never
    /// an allow.
    #[test]
    fn an_unknown_option_id_is_a_one_time_deny() {
        let req = InteractionRequest::Permission(PermRequest::bash(["ls"]));
        for id in ["", "allow", "ALLOW_ONCE", "allow_once "] {
            let resp = outcome_to_interaction_response(&selected(id), &req);
            assert!(
                matches!(&resp, InteractionResponse::Permission(p) if *p == PermResponse::deny()),
                "option id {id:?} gave {resp:?}"
            );
        }
    }
}
