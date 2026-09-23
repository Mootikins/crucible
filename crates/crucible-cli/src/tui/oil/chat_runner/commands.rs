use crate::tui::oil::chat_app::{ChatAppMsg, McpServerDisplay};
use crucible_core::error_utils::strip_tool_error_prefix;
use crucible_core::events::SessionEvent;
use crucible_core::interaction::InteractionRequest;
use crucible_core::protocol::session_events::{
    EventDecodeError, JobPayload, SessionEventPayload, SettingsPayload, SetupPayload,
    SystemPayload, ToolResultBody, TurnPayload,
};
use crucible_core::turn::TurnStatus;

use super::OilChatRunner;

impl OilChatRunner {
    /// Handle a SessionEvent, dispatching to appropriate ChatAppMsg.
    ///
    /// Returns Some(ChatAppMsg) if the event should be forwarded to the app,
    /// or None if the event was handled internally or should be skipped.
    pub fn handle_session_event(event: SessionEvent) -> Option<ChatAppMsg> {
        match event {
            SessionEvent::InteractionRequested {
                request_id,
                request,
            } => match &request {
                InteractionRequest::Ask(_) | InteractionRequest::Permission(_) => {
                    Some(ChatAppMsg::OpenInteraction {
                        request_id,
                        request,
                    })
                }
                InteractionRequest::AskBatch(_)
                | InteractionRequest::Edit(_)
                | InteractionRequest::Show(_)
                | InteractionRequest::Popup(_)
                | InteractionRequest::Panel(_) => Some(ChatAppMsg::OpenInteraction {
                    request_id,
                    request,
                }),
            },
            // The three delegation arms that used to live here were dead: this
            // function's only caller (`runner.rs`) invokes it exclusively with
            // `SessionEvent::InteractionRequested`, and the live delegation
            // mapping is the wire one in `session_event_to_chat_msgs`. Two
            // implementations of the same mapping had already diverged.
            _ => None,
        }
    }
}

/// Convert a session event into `ChatAppMsg`(s) for the TUI.
///
/// Returns zero or more messages. The `tool_result` event produces two messages
/// (delta + complete), while most events produce one. `replay_complete` and
/// unknown event types return an empty Vec.
///
/// Keyed on the typed payload rather than on `data.get("…")`: a new event in a
/// group the TUI handles now fails to compile here instead of falling through to
/// the `trace!` arm.
pub fn session_event_to_chat_msgs(event_type: &str, data: &serde_json::Value) -> Vec<ChatAppMsg> {
    // `subagent_*` are NOT on the wire — they exist only as
    // `InternalSessionEvent` variants for Lua, so they have no
    // `SessionEventPayload` name. The arms stay because `ChatAppMsg` carries the
    // variants and a test pins them; whether the wire should carry them is a
    // feature question with a missing producer, not three orphan consumers.
    if let Some(msgs) = subagent_msgs(event_type, data) {
        return msgs;
    }

    // Also not in the typed vocabulary, and for a structural reason rather than
    // an oversight: `stream_gap` is minted per *connection* by the daemon's event
    // forwarder when this client's broadcast cursor falls off the ring
    // (`daemon/src/server/core.rs`), so no session produces it and
    // `SessionEventPayload` has no name for it. It must be handled before the
    // typed dispatch, because that dispatch's `UnknownEvent` arm is a silent
    // `trace!` — which is exactly how the gap stayed invisible.
    if event_type == "stream_gap" {
        return vec![stream_gap_msg(data)];
    }

    match SessionEventPayload::from_wire(event_type, data) {
        Ok(SessionEventPayload::Turn(turn)) => turn_msgs(turn),
        Ok(SessionEventPayload::Setup(setup)) => setup_msgs(setup),
        Ok(SessionEventPayload::Settings(settings)) => settings_msgs(settings),
        Ok(SessionEventPayload::Job(job)) => job_msgs(job),
        Ok(SessionEventPayload::System(system)) => system_msgs(system),
        Ok(SessionEventPayload::Review(_))
        | Ok(SessionEventPayload::Notification(_))
        | Ok(SessionEventPayload::Workflow(_)) => vec![],
        Err(EventDecodeError::UnknownEvent { event }) => {
            tracing::trace!(event_type = %event, "Skipping unknown session event");
            vec![]
        }
        Err(e @ EventDecodeError::MalformedPayload { .. }) => {
            tracing::warn!(error = %e, "Dropping malformed session event");
            vec![]
        }
    }
}

/// Say out loud that this transcript is missing events.
///
/// Routed through `ChatAppMsg::Error`, which surfaces as a warning notification.
/// A gap is not a turn failure, but it is the one thing the user cannot find out
/// any other way: the events are gone from this connection and no later event
/// mentions them, so a message that is easy to miss is the same as no message.
///
/// A missing `dropped` still warns. The count is the daemon's own field, so its
/// absence means a version skew — not a reason to swallow the fact of the loss.
fn stream_gap_msg(data: &serde_json::Value) -> ChatAppMsg {
    // Count first: the status bar shows one truncated line, so the number has to
    // survive the truncation to be worth carrying.
    let dropped = data.get("dropped").and_then(|v| v.as_u64());
    ChatAppMsg::Error(match dropped {
        Some(n) => format!(
            "{n} events dropped: the event stream fell behind. \
             This conversation is incomplete — reload the session."
        ),
        None => "Events dropped: the event stream fell behind. \
                 This conversation is incomplete — reload the session."
            .to_string(),
    })
}

/// `Some` only for the three names that never cross the wire.
fn subagent_msgs(event_type: &str, data: &serde_json::Value) -> Option<Vec<ChatAppMsg>> {
    let id = data.get("job_id").and_then(|v| v.as_str())?.to_string();
    let text = |key: &str| {
        data.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    Some(match event_type {
        "subagent_spawned" => vec![ChatAppMsg::SubagentSpawned {
            id,
            prompt: text("prompt"),
        }],
        "subagent_completed" => vec![ChatAppMsg::SubagentCompleted {
            id,
            summary: text("summary"),
        }],
        "subagent_failed" => {
            let error = data
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error")
                .to_string();
            vec![ChatAppMsg::SubagentFailed { id, error }]
        }
        _ => return None,
    })
}

/// Empty strings and absent keys are the same thing here: every one of these
/// fields is `#[serde(default)]`, so a missing key arrives as `""`, and the
/// untyped code this replaces dropped the message rather than pushing an empty
/// one.
fn non_empty(s: String) -> Option<String> {
    Some(s).filter(|s| !s.is_empty())
}

fn turn_msgs(turn: TurnPayload) -> Vec<ChatAppMsg> {
    match turn {
        TurnPayload::ContextCleared { plugin } => vec![ChatAppMsg::SystemNotice(match plugin {
            Some(name) => format!("── ↻ {name} cleared the context ──"),
            None => "── Context cleared ──".to_string(),
        })],
        TurnPayload::UserMessage { content, .. } => non_empty(content)
            .map(|c| vec![ChatAppMsg::UserMessage(c)])
            .unwrap_or_default(),
        TurnPayload::TextDelta { content } => non_empty(content)
            .map(|c| vec![ChatAppMsg::TextDelta(c)])
            .unwrap_or_default(),
        TurnPayload::Thinking { content } => non_empty(content)
            .map(|c| vec![ChatAppMsg::ThinkingDelta(c)])
            .unwrap_or_default(),
        TurnPayload::ToolCall {
            call_id,
            tool,
            args,
            source,
            auto_approved,
            display,
            ..
        } => {
            // The card title is the canonical tool name. A recording without
            // `display` falls back to the name on the event.
            let name = display
                .as_ref()
                .and_then(|d| non_empty(d.tool.clone()))
                .or_else(|| non_empty(tool))
                .unwrap_or_else(|| "tool".to_string());
            vec![ChatAppMsg::ToolCall {
                name,
                args: if args.is_null() {
                    String::new()
                } else {
                    args.to_string()
                },
                call_id: non_empty(call_id),
                // Descriptions are not shown during live streaming (the LLM
                // chunk doesn't include them), so omit them on resume for
                // consistency.
                description: None,
                source,
                // A recording from before the render has none, and the card
                // shows no line.
                render: display.as_ref().and_then(|d| d.render.clone()),
                diffs: display.map(|d| d.diffs).unwrap_or_default(),
                auto_approved,
            }]
        }
        TurnPayload::ToolCallUpdate {
            call_id,
            args,
            display,
            auto_approved,
        } => {
            let Some(call_id) = non_empty(call_id) else {
                return Vec::new();
            };
            // An empty or null payload carries nothing worth disturbing the
            // existing card for, so the card keeps its args.
            let args = (!args.is_null() && args != serde_json::json!({}))
                .then(|| serde_json::to_string(&args).unwrap_or_default())
                .filter(|a| !a.is_empty());
            let render = display.as_ref().and_then(|d| d.render.clone());
            let diffs = display.map(|d| d.diffs);
            if args.is_none() && diffs.is_none() && auto_approved.is_none() {
                return Vec::new();
            }
            vec![ChatAppMsg::ToolCallUpdate {
                call_id,
                args,
                diffs,
                render,
                auto_approved,
            }]
        }
        TurnPayload::ToolResult {
            call_id,
            tool,
            result,
            ..
        } => {
            let name = non_empty(tool).unwrap_or_else(|| "tool".to_string());
            let call_id = non_empty(call_id);
            let body = ToolResultBody::of(&result);
            // The render of the finished call replaces the render of the
            // card, so the card shows the summary of the result.
            let mut msgs: Vec<ChatAppMsg> = call_id
                .clone()
                .zip(body.as_ref().and_then(|b| b.render().cloned()))
                .map(|(call_id, render)| ChatAppMsg::ToolCallUpdate {
                    call_id,
                    args: None,
                    diffs: None,
                    render: Some(render),
                    auto_approved: None,
                })
                .into_iter()
                .collect();
            if let Some(err) = body.as_ref().and_then(|b| b.error()) {
                msgs.push(ChatAppMsg::ToolResultError {
                    name,
                    error: strip_tool_error_prefix(err),
                    call_id,
                });
                return msgs;
            }
            let result_str = match &body {
                Some(ToolResultBody::Ok { result, .. }) => result.as_str().unwrap_or(""),
                _ => "",
            };
            // Strip nested tool-error prefixes from result text that looks like
            // an error (matches old handle_stream_chunk behaviour).
            let result_str = if result_str.starts_with("Error: ") {
                strip_tool_error_prefix(result_str)
            } else {
                result_str.to_string()
            };
            msgs.push(ChatAppMsg::ToolResultDelta {
                name: name.clone(),
                delta: result_str,
                call_id: call_id.clone(),
            });
            msgs.push(ChatAppMsg::ToolResultComplete { name, call_id });
            msgs
        }
        TurnPayload::MessageComplete {
            full_response,
            total_tokens,
            cache_read_tokens,
            cache_creation_tokens,
            stop_reason,
            ..
        } => {
            let mut msgs = Vec::new();
            // Reconstruct the full response text from the persisted snapshot.
            // text_delta events are not persisted (too granular), so this is the
            // only source of assistant text on resume.
            if let Some(text) = non_empty(full_response) {
                msgs.push(ChatAppMsg::TextDelta(text));
            }
            // If the daemon attached token counts, surface them as ContextUsage.
            // The `total` side requires a context-limit lookup, which the
            // standalone converter cannot do — the caller (SessionEventStream)
            // fills it in.
            if let Some(total) = total_tokens {
                msgs.push(ChatAppMsg::ContextUsage {
                    used: total as usize,
                    total: 0,
                });
            }
            // Cache hit rate from the per-event token fields. Both are optional;
            // emit only when at least one is present so the StatusBar's "no
            // data" sentinel still works for older sessions.
            if cache_read_tokens.is_some() || cache_creation_tokens.is_some() {
                let read = u64::from(cache_read_tokens.unwrap_or(0));
                let creation = u64::from(cache_creation_tokens.unwrap_or(0));
                let denom = read + creation;
                let rate = (denom != 0).then(|| read as f64 / denom as f64);
                msgs.push(ChatAppMsg::CacheHitRate(rate));
            }
            // A turn has exactly one `message_complete`, so this and the
            // `turn_finished` below both end the same turn. `StreamComplete`
            // is idempotent, and history from a daemon older than
            // `turn_finished` still ends its turns here.
            msgs.push(ChatAppMsg::StreamComplete);
            // After `StreamComplete`, never before: the completion seals the
            // trailing assistant bubble only while that bubble is the last
            // node, so a notice pushed first would leave the reply unsealed.
            if let Some(notice) = stop_reason.and_then(|r| r.user_notice()) {
                msgs.push(ChatAppMsg::SystemNotice(notice.to_string()));
            }
            msgs
        }
        // The one event that ends the whole turn, for every status. Not
        // `StreamCancelled`: that message also asks the daemon to cancel. A
        // turn that a cancel or a failure stopped sends no `message_complete`,
        // and this is what ends it — a cancel from ANOTHER client included.
        // A failed turn and a turn that a handler cancelled also show why.
        TurnPayload::TurnFinished { status, error, .. } => match (status, error) {
            (TurnStatus::Failed | TurnStatus::HandlerCancelled, Some(error)) => {
                vec![ChatAppMsg::Error(error), ChatAppMsg::StreamComplete]
            }
            _ => vec![ChatAppMsg::StreamComplete],
        },
        TurnPayload::PrecognitionComplete {
            notes_count, notes, ..
        } => {
            if notes_count > 0 {
                vec![ChatAppMsg::PrecognitionResult { notes_count, notes }]
            } else {
                vec![]
            }
        }
        // Rendered by other paths or not rendered at all: `segment_complete` is
        // additive over `message_complete`'s text, interactions ride their own
        // channel, and the rest is context plumbing and telemetry.
        // A prompt ended, answered here, by another client, or with no
        // answer. The prompt leaves the TUI.
        TurnPayload::InteractionCompleted { request_id, .. } => {
            vec![ChatAppMsg::InteractionEnded { request_id }]
        }
        TurnPayload::SegmentComplete { .. }
        | TurnPayload::InteractionRequested { .. }
        | TurnPayload::ContextInjected { .. }
        | TurnPayload::PostLlmCall { .. } => vec![],
    }
}

/// The seven setup payloads used to be decoded one at a time, each with its own
/// warn-and-drop block. One decode, one exhaustive match.
fn setup_msgs(setup: SetupPayload) -> Vec<ChatAppMsg> {
    match setup {
        SetupPayload::SessionInitialized(p) => vec![ChatAppMsg::SessionInitialized(p)],
        SetupPayload::ProvidersListed(p) => vec![ChatAppMsg::ProvidersListed(p.providers)],
        SetupPayload::ContextLimitResolved(p) => vec![ChatAppMsg::ContextLimitResolved {
            limit: p.limit,
            source: p.source,
        }],
        SetupPayload::WorkspaceIndexed(p) => vec![ChatAppMsg::WorkspaceIndexed(p.files)],
        SetupPayload::KilnNotesIndexed(p) => vec![ChatAppMsg::KilnNotesIndexed(p.notes)],
        SetupPayload::PluginsDiscovered(p) => vec![ChatAppMsg::PluginsDiscovered(p.plugins)],
        SetupPayload::McpServersReady(p) => {
            let servers: Vec<McpServerDisplay> =
                p.servers.into_iter().map(McpServerDisplay::from).collect();
            vec![ChatAppMsg::McpServersReady(servers)]
        }
    }
}

fn settings_msgs(settings: SettingsPayload) -> Vec<ChatAppMsg> {
    match settings {
        // A mode change made anywhere else — the web UI, another client, a Lua
        // handler — reaches the statusline only through this arm. Without it the
        // daemon emitted `mode_changed` to nobody on the TUI side, and the badge
        // kept showing the mode this client last set itself.
        SettingsPayload::ModeChanged { mode } => non_empty(mode)
            .map(|m| vec![ChatAppMsg::ModeSynced(m)])
            .unwrap_or_default(),
        // The rest are acknowledgements of a change this client either made or
        // can re-read from the session record.
        _ => vec![],
    }
}

fn job_msgs(job: JobPayload) -> Vec<ChatAppMsg> {
    match job {
        JobPayload::DelegationSpawned {
            delegation_id,
            prompt,
            target_agent,
            ..
        } => match (non_empty(delegation_id), non_empty(prompt)) {
            (Some(id), Some(prompt)) => vec![ChatAppMsg::DelegationSpawned {
                id,
                prompt,
                target_agent,
            }],
            _ => vec![],
        },
        JobPayload::DelegationCompleted {
            delegation_id,
            result_summary,
            ..
        } => match (non_empty(delegation_id), non_empty(result_summary)) {
            (Some(id), Some(summary)) => vec![ChatAppMsg::DelegationCompleted { id, summary }],
            _ => vec![],
        },
        JobPayload::DelegationFailed {
            delegation_id,
            error,
            ..
        } => match (non_empty(delegation_id), non_empty(error)) {
            (Some(id), Some(error)) => vec![ChatAppMsg::DelegationFailed { id, error }],
            _ => vec![],
        },
        // Bash and background jobs have no TUI surface yet.
        _ => vec![],
    }
}

fn system_msgs(system: SystemPayload) -> Vec<ChatAppMsg> {
    match system {
        // Hot reload and runtime theme switching arrive here. Applying the
        // payload and repainting are separate steps: over a socket, with the TUI
        // idle-blocked on input, a changed store repaints nothing by itself.
        SystemPayload::UiStyleChanged(config) => {
            crate::tui::oil::theme::apply_ui_config(&config);
            vec![ChatAppMsg::StyleChanged]
        }
        // A surface moved. Re-request it rather than carrying rows on the event:
        // the event says *what* changed, and the fetch answers with the content,
        // which is why `SurfaceChanged` has a version and no rows.
        //
        // A refresh, never an open: a plugin that pushes rows must not put a
        // full-screen modal over whatever the user is doing. The reducer drops
        // the result when nothing is open.
        //
        // A withdrawal is the exception, and the only change this arm acts on
        // by itself. There is no content left to fetch, so asking for it would
        // spend a round trip to be told what `withdrawn` already said — and the
        // empty answer it came back with is also what a lost race looks like.
        // The daemon settles it instead of each client guessing.
        SystemPayload::SurfaceChanged {
            name, withdrawn, ..
        } => {
            if withdrawn {
                vec![ChatAppMsg::SurfaceWithdrawn(name)]
            } else {
                vec![ChatAppMsg::RefreshSurface(name)]
            }
        }
        // A proposal changed. The event carries only the id; the app reads
        // the proposal again when it needs it.
        SystemPayload::ProposalChanged { id } => vec![ChatAppMsg::ProposalChanged(id)],
        // `replay_complete` is consumed by the stateful wrapper, not here.
        _ => vec![],
    }
}

#[cfg(test)]
#[test]
fn context_cleared_names_the_plugin_in_the_transcript() {
    let messages =
        session_event_to_chat_msgs("context_cleared", &serde_json::json!({"plugin": "alpha"}));
    assert!(
        matches!(&messages[..], [ChatAppMsg::SystemNotice(text)] if text == "── ↻ alpha cleared the context ──")
    );
}
