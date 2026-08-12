use crate::tui::oil::chat_app::ChatAppMsg;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::commands::session_event_to_chat_msgs;

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
use crucible_daemon::subscription::WILDCARD_SESSION;

/// Stateful SessionEvent → ChatAppMsg converter.
///
/// Tracks `saw_text_delta` per turn so `message_complete.full_response`
/// only produces a TextDelta when no granular text_deltas preceded it
/// (the "coarse resume" case — daemon drops text_delta during storage
/// compaction, keeping only the final message_complete snapshot).
///
/// Also de-duplicates a provider's end-of-stream reasoning replay — see
/// [`SessionEventStream::is_thinking_replay`].
///
/// Optionally holds a `context_limit` handle so that `message_complete`
/// token counts can be converted into a `ContextUsage` with the correct
/// `total` field. Without a handle, the total defaults to 0.
pub struct SessionEventStream {
    saw_text_delta: bool,
    /// Thinking rendered since the last replay boundary, concatenated.
    thinking_run: String,
    /// How many `thinking` deltas that run is made of. A run of one is a
    /// single thought, not a stream — see [`MIN_REPLAY_RUN_DELTAS`].
    thinking_run_deltas: usize,
    context_limit: Option<Arc<AtomicUsize>>,
}

/// Shortest delta run a `thinking` payload may be judged a replay of.
///
/// A replay is by construction the *concatenation* of a streamed run, so it
/// takes at least two deltas to make one. Without this floor the rule also
/// matches a thought that simply repeats the one before it — an agent that says
/// `"Hmm."` three times rendered only twice, and a single-delta thought
/// followed by an identical single-delta thought was deleted outright. That
/// window is open exactly when the run is short: at the start of a turn and
/// immediately after every drop.
///
/// The floor costs nothing on real data. All 11 replays across
/// `assets/fixtures` (`demo`, `parity-test`, `reproduce`,
/// `reproduce-formatting`) follow runs of **17–103** deltas and carry 79–515
/// chars; none is anywhere near the boundary.
const MIN_REPLAY_RUN_DELTAS: usize = 2;

impl SessionEventStream {
    pub fn new() -> Self {
        Self {
            saw_text_delta: false,
            thinking_run: String::new(),
            thinking_run_deltas: 0,
            context_limit: None,
        }
    }

    /// Is this `thinking` payload a replay of reasoning already rendered?
    ///
    /// Providers that stream reasoning incrementally *also* hand the whole
    /// block back at stream end (genai's `End.captured_reasoning_content`), so
    /// the replay arrives as one `thinking` event carrying the exact
    /// concatenation of the deltas that preceded it. Painting it duplicates the
    /// entire thought. `ReasoningEmissionState` (`provider/genai_handle.rs`)
    /// drops it at the source, but only since 2026-04-27 — every session
    /// recorded before that still carries the replay, and replay is precisely
    /// what this converter serves.
    ///
    /// Matching on content rather than position is what lets genuinely
    /// interleaved thoughts through. An agent that owns its own tool loop —
    /// any ACP-delegated agent, and the internal agent between tool batches —
    /// alternates thinking and text within one turn; those later thoughts are
    /// new content. The earlier `saw_text_delta` rule discarded all of them,
    /// and still missed the replays that arrive with no text in between.
    ///
    /// Consuming the run on a match keeps this correct across several
    /// reasoning blocks in one turn: each replay covers only the block since
    /// the previous one. Verified against every recording in `assets/fixtures`
    /// — each replay there is byte-identical to its run.
    ///
    /// The [`MIN_REPLAY_RUN_DELTAS`] floor is what keeps a *repetition* from
    /// being mistaken for a replay. Dropping content is unrecoverable, so the
    /// rule must never fire on a run short enough to be one ordinary thought.
    ///
    /// The comparison is byte-exact, deliberately. A replay whose bytes differ
    /// from its run — by a trailing newline, say — renders, painting the block
    /// twice. Equality is the only predicate here that says something about how
    /// the payload was *produced*; every looser one is a similarity heuristic
    /// feeding a code path that deletes without a trace. A visible duplicate is
    /// recoverable and a deleted thought is not, and no recording in
    /// `assets/fixtures` is non-exact, so nothing is bought by loosening it.
    /// Pinned by `a_replay_that_is_not_byte_exact_renders_twice_on_purpose`.
    fn is_thinking_replay(&self, data: &serde_json::Value) -> bool {
        self.thinking_run_deltas >= MIN_REPLAY_RUN_DELTAS
            && data.get("content").and_then(|v| v.as_str()) == Some(self.thinking_run.as_str())
    }

    /// Forget the thinking run: nothing rendered after this can be judged a
    /// replay of what came before it.
    fn reset_thinking_run(&mut self) {
        self.thinking_run.clear();
        self.thinking_run_deltas = 0;
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
            self.reset_thinking_run();
        }

        if event_type == "thinking" {
            if self.is_thinking_replay(data) {
                self.reset_thinking_run();
                return Vec::new();
            }
            if let Some(content) = data.get("content").and_then(|v| v.as_str()) {
                self.thinking_run.push_str(content);
                self.thinking_run_deltas += 1;
            }
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
        move |event| event.session_id == filter_id || event.session_id == WILDCARD_SESSION,
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
