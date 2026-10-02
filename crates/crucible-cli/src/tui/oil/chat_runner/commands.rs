use crate::tui::oil::chat_app::{ChatAppMsg, McpServerDisplay};
use crucible_core::protocol::session_events::{
    EventDecodeError, JobPayload, NotificationPayload, SessionEventPayload, SettingsPayload,
    SetupPayload, SystemPayload, TurnPayload,
};
use crucible_core::turn::TurnStatus;

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
    payload_msgs(SessionEventPayload::from_wire(event_type, data))
}

/// The messages of one decoded event.
///
/// Keyed on the typed payload: a new event in a group the TUI handles fails
/// to compile here instead of falling through to the `trace!` arm.
pub(crate) fn payload_msgs(
    decoded: Result<SessionEventPayload, EventDecodeError>,
) -> Vec<ChatAppMsg> {
    match decoded {
        Ok(SessionEventPayload::Turn(turn)) => turn_msgs(turn),
        Ok(SessionEventPayload::Setup(setup)) => setup_msgs(setup),
        Ok(SessionEventPayload::Settings(settings)) => settings_msgs(settings),
        Ok(SessionEventPayload::Job(job)) => job_msgs(job),
        Ok(SessionEventPayload::System(system)) => system_msgs(system),
        Ok(SessionEventPayload::Notification(NotificationPayload::NotificationAdded {
            notification: Some(notification),
            ..
        })) => vec![ChatAppMsg::Notification(notification)],
        Ok(SessionEventPayload::Notification(NotificationPayload::NotificationDismissed {
            notification_id,
        })) => vec![ChatAppMsg::DismissNotification(notification_id)],
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

/// Empty strings and absent keys are the same thing here: every one of these
/// fields is `#[serde(default)]`, so a missing key arrives as `""`, and the
/// untyped code this replaces dropped the message rather than pushing an empty
/// one.
fn non_empty(s: String) -> Option<String> {
    Some(s).filter(|s| !s.is_empty())
}

/// The messages of a turn event that are not transcript items. The daemon
/// folds the transcript and sends its ops with each event; the runner turns
/// them into [`ChatAppMsg::Transcript`]. What stays here is turn state: the
/// end of a turn, token use and open prompts.
fn turn_msgs(turn: TurnPayload) -> Vec<ChatAppMsg> {
    match turn {
        TurnPayload::MessageComplete {
            total_tokens,
            cache_read_tokens,
            cache_creation_tokens,
            ..
        } => {
            let mut msgs = Vec::new();
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
        // A prompt ended, answered here, by another client, or with no
        // answer. The prompt leaves the TUI.
        TurnPayload::InteractionCompleted { request_id, .. } => {
            vec![ChatAppMsg::InteractionEnded { request_id }]
        }
        // Transcript items: the ops of the fold draw them.
        TurnPayload::ContextCleared { .. }
        | TurnPayload::UserMessage { .. }
        | TurnPayload::TextDelta { .. }
        | TurnPayload::Thinking { .. }
        | TurnPayload::SegmentComplete { .. }
        | TurnPayload::ToolCall { .. }
        | TurnPayload::ToolCallUpdate { .. }
        | TurnPayload::ToolResult { .. }
        | TurnPayload::ContextInjected { .. }
        | TurnPayload::PrecognitionComplete { .. } => vec![],
        // Interactions ride their own channel, and the model call summary is
        // telemetry.
        TurnPayload::InteractionRequested { .. } | TurnPayload::PostLlmCall { .. } => vec![],
    }
}

/// The setup payloads used to be decoded one at a time, each with its own
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
        // The TUI showed nothing for this event when it had no type, and it
        // shows nothing now. The typed payload changes the decode, not the view.
        SetupPayload::AcpResumeFallback { .. } => vec![],
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
        // The agent advertised a new command list.
        SettingsPayload::CommandsChanged {} => vec![ChatAppMsg::FetchCommands],
        SettingsPayload::TitleChanged { .. } => vec![ChatAppMsg::FetchSessions],
        // The rest are acknowledgements of a change this client either made or
        // can re-read from the session record.
        _ => vec![],
    }
}

/// Delegations are transcript items, and the ops of the fold draw them.
/// Bash and background jobs have no TUI surface yet.
fn job_msgs(_job: JobPayload) -> Vec<ChatAppMsg> {
    vec![]
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
        SystemPayload::StatusItemsChanged { status } => vec![ChatAppMsg::StatusItemsLoaded(status)],
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
        // The event stream lost events. The transcript has a hole, and no later
        // event mentions it, so the user must be told here. It goes out as an
        // error, which shows as a warning. The count comes first: the status bar
        // shows one truncated line, so the number has to survive the truncation.
        // A count of 0 means the loss is unknown, and it still warns.
        SystemPayload::StreamGap { dropped } => vec![ChatAppMsg::Error(match dropped {
            0 => "Events dropped: the event stream fell behind. \
                  This conversation is incomplete — reload the session."
                .to_string(),
            n => format!(
                "{n} events dropped: the event stream fell behind. \
                 This conversation is incomplete — reload the session."
            ),
        })],
        // `replay_complete` is consumed by the replay consumer, not here. The
        // rest is kiln, note and daemon state that the chat does not show.
        SystemPayload::ReplayComplete { .. }
        | SystemPayload::FileChanged { .. }
        | SystemPayload::FileDeleted { .. }
        | SystemPayload::FileMoved { .. }
        | SystemPayload::ClassificationRequired { .. }
        | SystemPayload::ProcessComplete { .. }
        | SystemPayload::NoteCreated { .. }
        | SystemPayload::NoteModified { .. }
        | SystemPayload::NoteDeleted { .. }
        | SystemPayload::NoteRenamed { .. }
        | SystemPayload::BaseChanged { .. }
        | SystemPayload::WebhookReceived { .. }
        | SystemPayload::SessionCreated { .. }
        | SystemPayload::PublicationChanged { .. }
        | SystemPayload::SessionEnded { .. } => vec![],
    }
}
