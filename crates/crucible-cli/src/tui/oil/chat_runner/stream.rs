use crate::tui::oil::chat_app::ChatAppMsg;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::commands::session_event_to_chat_msgs;

/// Stateful SessionEvent → ChatAppMsg converter.
///
/// Tracks `saw_text_delta` per turn so `message_complete.full_response`
/// only produces a TextDelta when no granular text_deltas preceded it
/// (the "coarse resume" case — daemon drops text_delta during storage
/// compaction, keeping only the final message_complete snapshot).
///
/// Optionally holds a `context_limit` handle so that `message_complete`
/// token counts can be converted into a `ContextUsage` with the correct
/// `total` field. Without a handle, the total defaults to 0.
pub struct SessionEventStream {
    saw_text_delta: bool,
    context_limit: Option<Arc<AtomicUsize>>,
}

impl SessionEventStream {
    pub fn new() -> Self {
        Self {
            saw_text_delta: false,
            context_limit: None,
        }
    }

    pub fn with_context_limit(mut self, limit: Arc<AtomicUsize>) -> Self {
        self.context_limit = Some(limit);
        self
    }

    pub fn translate(&mut self, event_type: &str, data: &serde_json::Value) -> Vec<ChatAppMsg> {
        if event_type == "text_delta" {
            self.saw_text_delta = true;
        } else if event_type == "user_message" {
            self.saw_text_delta = false;
        }

        // Late thinking summaries arrive after text_delta and
        // duplicate incremental thinking deltas — drop them.
        if event_type == "thinking" && self.saw_text_delta {
            return Vec::new();
        }

        let raw = session_event_to_chat_msgs(event_type, data);

        // When the daemon's setup task emits `context_limit_resolved`, also
        // stamp the atomic so that subsequent `message_complete` events pick
        // up the real total for their `ContextUsage` patching.
        if event_type == "context_limit_resolved" {
            if let Some(ref limit) = self.context_limit {
                for msg in &raw {
                    if let ChatAppMsg::ContextLimitResolved { limit: l, .. } = msg {
                        limit.store(*l, Ordering::Relaxed);
                    }
                }
            }
        }

        // For message_complete, filter out the TextDelta if granular deltas
        // were seen, and patch the ContextUsage with the real context limit.
        if event_type == "message_complete" {
            let saw_deltas = self.saw_text_delta;
            let total_limit = self
                .context_limit
                .as_ref()
                .map(|l| l.load(Ordering::Relaxed))
                .unwrap_or(0);
            raw.into_iter()
                .filter_map(|m| match m {
                    ChatAppMsg::TextDelta(_) if saw_deltas => None,
                    ChatAppMsg::ContextUsage { used, .. } => Some(ChatAppMsg::ContextUsage {
                        used,
                        total: total_limit,
                    }),
                    other => Some(other),
                })
                .collect()
        } else {
            raw
        }
    }
}

impl Default for SessionEventStream {
    fn default() -> Self {
        Self::new()
    }
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
        for msg in stream.translate(&event.event_type, &event.data) {
            if msg_tx.send(msg).is_err() {
                return;
            }
        }
    }
}

/// Daemon reports fatal turn failures via `ended { reason: "error: ..." }`.
/// Surface them as an `Error` ChatAppMsg so the status bar shows the cause.
/// Shared by both live and replay paths — replay of an error-ending recording
/// renders identically to a live session that ended with that error.
fn promote_ended_error(
    event: &crucible_daemon::SessionEvent,
    tx: &tokio::sync::mpsc::UnboundedSender<ChatAppMsg>,
) {
    if event.event_type == "ended" {
        if let Some(reason) = event.data.get("reason").and_then(|v| v.as_str()) {
            if let Some(err) = reason.strip_prefix("error: ") {
                let _ = tx.send(ChatAppMsg::Error(err.to_string()));
            }
        }
    }
}

/// Unified session event consumer for both live and replay modes.
///
/// Drains `event_rx`, filtering events for `session_id` and translating them
/// through `SessionEventStream` into `ChatAppMsg`s on `msg_tx`. Both paths
/// share the `ended: error: ...` → `ChatAppMsg::Error` promotion. Replay
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
        move |event| event.session_id == filter_id,
        |event, tx| {
            promote_ended_error(event, tx);
            if event.event_type == "replay_complete" {
                let _ = tx.send(ChatAppMsg::Status("Replay complete".to_string()));
                return false;
            }
            true
        },
    )
    .await;
}
