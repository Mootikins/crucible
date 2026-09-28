//! Pure translation between Crucible daemon `SessionEvent`s and ACP wire types.
//!
//! Kept free of I/O so the mapping table can be unit-tested exhaustively. The
//! event pump in [`super::agent`] consumes [`classify_event`] and drives the
//! side effects (sending `session/update`, requesting permission).

use agent_client_protocol::schema::v1::{
    AvailableCommand, AvailableCommandInput, PermissionOption, PermissionOptionId,
    PermissionOptionKind, RequestPermissionOutcome, StopReason, ToolCallUpdate,
    ToolCallUpdateFields, ToolKind, UnstructuredCommandInput,
};
use crucible_core::interaction::{
    InteractionRequest, InteractionResponse, PermResponse, PermissionScope,
};
use crucible_core::protocol::session_events::{SessionEventPayload, SettingsPayload, TurnPayload};
use crucible_core::turn::{StopReason as CoreStopReason, TurnStatus};
use crucible_core::types::{CanonicalToolCall, CommandKind, SessionCommand};
use crucible_daemon::SessionEvent;

/// Permission option IDs advertised to the host. Matching them back in
/// [`outcome_to_interaction_response`] must use these exact strings.
pub const OPT_ALLOW_ONCE: &str = "allow_once";
pub const OPT_ALLOW_ALWAYS: &str = "allow_always";
pub const OPT_REJECT_ONCE: &str = "reject_once";

/// One step of a prompt turn, derived from a single daemon `SessionEvent`.
#[derive(Debug)]
pub enum TurnStep {
    /// The daemon needs a decision; drive an ACP `session/request_permission`.
    Interaction {
        request_id: String,
        request: Box<InteractionRequest>,
    },
    /// The whole turn is over (`turn_finished`); respond to `session/prompt`.
    Finished(TurnEnd),
    /// The session's command catalog changed; advertise it again.
    CommandsChanged,
    /// Nothing to forward (unknown or empty event).
    Ignore,
}

/// The daemon's command catalog as the commands an ACP host offers.
///
/// A built-in command is an action of a Crucible client, which a host does
/// not have, so it is left out. The host sends each other command back as
/// prompt text, and the daemon routes it from the same catalog.
pub fn available_commands(catalog: &[SessionCommand]) -> Vec<AvailableCommand> {
    catalog
        .iter()
        .filter(|command| !matches!(command.kind, CommandKind::Builtin { .. }))
        .map(|command| {
            AvailableCommand::new(&command.name, &command.description).input(
                command.input_hint.as_ref().map(|hint| {
                    AvailableCommandInput::Unstructured(UnstructuredCommandInput::new(hint))
                }),
            )
        })
        .collect()
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
/// The event decodes once into its typed payload, and the match on the turn
/// payload is exhaustive: a new turn event must decide here whether ACP shows
/// it. Every other group is session state, not turn content.
pub fn classify_event(event: &SessionEvent) -> TurnStep {
    let turn = match event.payload() {
        Ok(SessionEventPayload::Turn(turn)) => turn,
        Ok(SessionEventPayload::Settings(SettingsPayload::CommandsChanged {})) => {
            return TurnStep::CommandsChanged
        }
        Ok(_) => return TurnStep::Ignore,
        // `turn_finished` is the one event that ends the turn. One that does
        // not decode still ends it, as a failure, so the host does not wait
        // for ever.
        Err(_) if event.event == "turn_finished" => {
            return TurnStep::Finished(TurnEnd::Failed(format!(
                "the daemon sent a turn_finished event that does not decode: {}",
                event.data
            )))
        }
        Err(_) => return TurnStep::Ignore,
    };
    match turn {
        // `turn_finished` ends the turn.
        TurnPayload::TurnFinished {
            status,
            stop_reason,
            error,
        } => TurnStep::Finished(turn_end(status, stop_reason, error)),
        TurnPayload::InteractionRequested {
            request_id,
            request,
        } => TurnStep::Interaction {
            request_id,
            request: Box::new(request),
        },
        // Transcript items: the ops of the daemon's fold carry them
        // (`super::project`).
        TurnPayload::TextDelta { .. }
        | TurnPayload::Thinking { .. }
        | TurnPayload::ToolCall { .. }
        | TurnPayload::ToolResult { .. }
        | TurnPayload::ContextCleared { .. }
        | TurnPayload::UserMessage { .. }
        | TurnPayload::SegmentComplete { .. }
        | TurnPayload::MessageComplete { .. }
        | TurnPayload::ToolCallUpdate { .. }
        | TurnPayload::InteractionCompleted { .. }
        | TurnPayload::ContextInjected { .. }
        | TurnPayload::PrecognitionComplete { .. }
        | TurnPayload::PostLlmCall { .. } => TurnStep::Ignore,
    }
}

/// The ACP title and kind of a call: the canonical tool and the render
/// line, and the ACP kind of the canonical kind. A kind that ACP does not
/// name, and a call with no canonical form, is `Other`.
pub(super) fn describe(name: &str, call: Option<&CanonicalToolCall>) -> (String, ToolKind) {
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
    fn the_host_gets_every_command_but_the_built_in_ones() {
        let catalog = vec![
            crucible_core::types::BuiltinCommand::Help.entry(),
            SessionCommand {
                name: "reflect".into(),
                description: "Reflect".into(),
                input_hint: Some("[turns]".into()),
                kind: CommandKind::Plugin {
                    plugin: "alpha".into(),
                },
            },
            SessionCommand {
                name: "tidy".into(),
                description: "Tidy".into(),
                input_hint: None,
                kind: CommandKind::Skill,
            },
        ];
        let offered = available_commands(&catalog);
        let names: Vec<_> = offered.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["reflect", "tidy"]);
        assert!(matches!(
            &offered[0].input,
            Some(AvailableCommandInput::Unstructured(input)) if input.hint == "[turns]"
        ));
        assert!(offered[1].input.is_none());
    }

    #[test]
    fn a_commands_changed_event_asks_for_a_new_advertisement() {
        assert!(matches!(
            classify_event(&event("commands_changed", json!({}))),
            TurnStep::CommandsChanged
        ));
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
            json!({"message_id": "m-3", "content": "keep going",
                "origin": {"kind": "plugin", "name": "goal"}}),
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
