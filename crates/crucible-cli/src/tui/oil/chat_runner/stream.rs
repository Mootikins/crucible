use crate::tui::oil::chat_app::ChatAppMsg;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::commands::payload_msgs;
use crucible_core::protocol::session_events::{SessionEventPayload, SystemPayload, TurnPayload};

/// The session id the daemon addresses genuinely global events to.
///
/// The daemon's own constant, not a second `"*"`: the two ends of this filter
/// have to agree, and one of them already owns the value.
///
/// The daemon treats the wildcard symmetrically — a client subscribed to `"*"`
/// receives everything, and an event addressed to `"*"` reaches every client
/// (`daemon/src/server/core.rs`). This end had only the first half, so a
/// wildcard-addressed event reached the process and was then dropped by the
/// per-session filter: `stream_gap` (the broadcast gap marker, which names no
/// session because `Lagged(n)` does not know one) and `ui_style_changed`'s
/// config-level pushes both went nowhere.
use crucible_daemon::event_map::SYSTEM_SESSION;
use crucible_daemon::subscription::WILDCARD_SESSION;

/// SessionEvent → ChatAppMsg converter for the messages that are not
/// transcript items.
///
/// The daemon folds the transcript (`crucible_core::transcript`), so this
/// keeps no turn state. It holds an optional `context_limit` handle, so that
/// the token count of `message_complete` becomes a `ContextUsage` with the
/// correct `total`. Without a handle, the total is 0.
pub struct SessionEventStream {
    context_limit: Option<Arc<AtomicUsize>>,
}

impl SessionEventStream {
    pub fn new() -> Self {
        Self {
            context_limit: None,
        }
    }

    pub fn with_context_limit(mut self, limit: Arc<AtomicUsize>) -> Self {
        self.context_limit = Some(limit);
        self
    }

    pub fn translate(&mut self, event_type: &str, data: &serde_json::Value) -> Vec<ChatAppMsg> {
        let raw = payload_msgs(SessionEventPayload::from_wire(event_type, data));
        let Some(limit) = &self.context_limit else {
            return raw;
        };
        // When the daemon's setup task resolves the context limit, stamp the
        // atomic, so that later `message_complete` events carry the total.
        for msg in &raw {
            if let ChatAppMsg::ContextLimitResolved { limit: l, .. } = msg {
                limit.store(*l, Ordering::Relaxed);
            }
        }
        let total = limit.load(Ordering::Relaxed);
        raw.into_iter()
            .map(|m| match m {
                ChatAppMsg::ContextUsage { used, .. } => ChatAppMsg::ContextUsage { used, total },
                other => other,
            })
            .collect()
    }
}

impl Default for SessionEventStream {
    fn default() -> Self {
        Self::new()
    }
}

/// The messages of one event: the transcript ops that it carries, then the
/// rest. The ops come first, so that the end of a turn that the same event
/// carries seals the segment that the ops just wrote.
pub(crate) fn event_msgs(
    stream: &mut SessionEventStream,
    event: &crucible_daemon::SessionEvent,
) -> Vec<ChatAppMsg> {
    let mut msgs = Vec::new();
    if !event.transcript.is_empty() {
        msgs.push(ChatAppMsg::Transcript {
            seq: event.seq,
            ops: event.transcript.clone(),
        });
    }
    msgs.extend(stream.translate(&event.event, &event.data));
    msgs
}

/// Shared event-pump used by both replay and live consumers.
///
/// Filters out events for other sessions via `session_filter`, feeds the
/// survivors through `SessionEventStream`, and forwards the resulting
/// `ChatAppMsg`s to the app's event channel. Returns when `event_rx`
/// closes, the filter rejects an event that the caller wants to stop on
/// (via returning `None` from `on_event`), or `msg_tx` closes.
///
/// `on_event` lets the replay path recognize `replay_complete` and emit
/// a terminal Status message. Live mode passes a no-op.
async fn consume_session_events<F, E>(
    mut event_rx: tokio::sync::mpsc::UnboundedReceiver<crucible_daemon::SessionEvent>,
    msg_tx: tokio::sync::mpsc::UnboundedSender<ChatAppMsg>,
    context_limit: Option<Arc<AtomicUsize>>,
    session_filter: F,
    mut on_event: E,
) where
    F: Fn(&crucible_daemon::SessionEvent) -> bool,
    E: FnMut(
        &crucible_daemon::SessionEvent,
        &tokio::sync::mpsc::UnboundedSender<ChatAppMsg>,
    ) -> bool,
{
    let mut stream = SessionEventStream::new();
    if let Some(limit) = context_limit {
        stream = stream.with_context_limit(limit);
    }
    while let Some(event) = event_rx.recv().await {
        if !session_filter(&event) {
            continue;
        }
        if !on_event(&event, &msg_tx) {
            return;
        }
        for msg in event_msgs(&mut stream, &event) {
            if msg_tx.send(msg).is_err() {
                return;
            }
        }
    }
}

/// Unified session event consumer for both live and replay modes.
///
/// Drains `event_rx`, filtering events for `session_id` and translating them
/// through `SessionEventStream` into `ChatAppMsg`s on `msg_tx`. Replay
/// additionally terminates on `replay_complete`, emitting a final Status.
///
/// `context_limit` is `Some(_)` for live (so `message_complete` can fill in
/// the total for `ContextUsage`) and `None` for replay (the recorded events
/// already carry the total).
pub(crate) async fn session_event_consumer(
    session_id: String,
    event_rx: tokio::sync::mpsc::UnboundedReceiver<crucible_daemon::SessionEvent>,
    msg_tx: tokio::sync::mpsc::UnboundedSender<ChatAppMsg>,
    context_limit: Option<Arc<AtomicUsize>>,
) {
    let filter_id = session_id.clone();
    consume_session_events(
        event_rx,
        msg_tx,
        context_limit,
        // The system session carries daemon events that belong to no user
        // session, for example `proposal_changed` and `surface_changed`.
        move |event| {
            event.session_id == filter_id
                || event.session_id == WILDCARD_SESSION
                || event.session_id == SYSTEM_SESSION
        },
        |event, tx| {
            if matches!(
                event.payload(),
                Ok(SessionEventPayload::System(
                    SystemPayload::ReplayComplete { .. }
                ))
            ) {
                let _ = tx.send(ChatAppMsg::Status("Replay complete".to_string()));
                return false;
            }
            true
        },
    )
    .await;
}

/// The live consumer of one session.
///
/// It is [`session_event_consumer`] plus the prompts. The live stream opens
/// a prompt when `interaction_requested` arrives. Stored history and a replay
/// do not open prompts, because their questions are answered or gone.
///
/// `pending` holds the prompts that waited before this client attached. They
/// open first. The subscription can also deliver one of them as an event, so
/// each prompt opens once only.
pub(crate) async fn live_session_event_consumer(
    session_id: String,
    event_rx: tokio::sync::mpsc::UnboundedReceiver<crucible_daemon::SessionEvent>,
    pending: Vec<crucible_core::interaction::InteractionEvent>,
    msg_tx: tokio::sync::mpsc::UnboundedSender<ChatAppMsg>,
    context_limit: Arc<AtomicUsize>,
) {
    let mut opened = std::collections::HashSet::new();
    for prompt in pending {
        opened.insert(prompt.request_id.clone());
        let msg = ChatAppMsg::OpenInteraction {
            request_id: prompt.request_id,
            request: prompt.request,
        };
        if msg_tx.send(msg).is_err() {
            return;
        }
    }

    let filter_id = session_id.clone();
    consume_session_events(
        event_rx,
        msg_tx,
        Some(context_limit),
        move |event| {
            event.session_id == filter_id
                || event.session_id == WILDCARD_SESSION
                || event.session_id == SYSTEM_SESSION
        },
        move |event, tx| {
            if event.session_id == session_id {
                if let Some(msg) = open_prompt(event, &mut opened) {
                    let _ = tx.send(msg);
                }
            }
            true
        },
    )
    .await;
}

/// The message that opens the prompt of an `interaction_requested` event, or
/// `None` for any other event and for a prompt that is already open.
///
/// An event that does not decode is not reported here: the translation of
/// the same event reports it.
fn open_prompt(
    event: &crucible_daemon::SessionEvent,
    opened: &mut std::collections::HashSet<String>,
) -> Option<ChatAppMsg> {
    let Ok(SessionEventPayload::Turn(TurnPayload::InteractionRequested {
        request_id,
        request,
    })) = event.payload()
    else {
        return None;
    };
    opened
        .insert(request_id.clone())
        .then_some(ChatAppMsg::OpenInteraction {
            request_id,
            request,
        })
}
