use std::time::Duration;

use agent_client_protocol::schema::v1::{
    CancelNotification, ContentBlock, PromptRequest, PromptResponse, SessionId, SessionUpdate,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::types::StreamingState;
use super::CrucibleAcpClient;
use crate::acp::streaming::{StreamingChunk, TurnSummary};
use crate::acp::{ClientError, Result};
use crucible_core::text::{sanitize_multiline, sanitize_single_line};
use crucible_core::turn::is_visible_content;

/// The wire spelling of a stop reason, for the error a call that never
/// completed carries: `end_turn`, `cancelled`, and so on.
fn stop_reason_label(stop_reason: agent_client_protocol::schema::v1::StopReason) -> String {
    serde_json::to_value(stop_reason)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{stop_reason:?}"))
}

/// How far to chase a nested error payload before giving up. Deep enough for
/// the shapes agents actually send (an upstream envelope forwarded as a string,
/// with the sentence one key further in), shallow enough that a hostile or
/// self-referential payload terminates.
const MAX_DETAIL_DEPTH: u8 = 4;

/// Longest error text shown to the user, in characters.
///
/// This is a one-line failure label, not a log: past a couple of paragraphs
/// nobody reads further, and the full payload is already in the trace at
/// `debug` level for whoever needs it.
const MAX_DETAIL_CHARS: usize = 512;

/// Largest agent payload this will copy while looking for a sentence, in
/// characters.
///
/// The display cap alone is not enough. `data` is whatever the agent chose to
/// send, and unwrapping it holds the raw string, its sanitised copy, the
/// nested `Value` it parses to, and the formatted result at once — roughly
/// four times the payload. Bounding the *input* bounds all four: a 500 MB
/// `error.data` costs the one already-parsed copy, not 2 GB of derived ones.
///
/// Sized well above any error body an agent actually forwards (an upstream
/// JSON envelope with headers and a request id runs to a few kilobytes) so
/// that the nested unwrap keeps working on real payloads; anything larger is
/// not a sentence, so there is nothing to dig for and the head is all the user
/// can use.
const MAX_DETAIL_INPUT_CHARS: usize = 16 * 1024;

/// Truncate to the display cap, marking the cut so a clipped sentence does not
/// read as the agent's own words.
pub(super) fn elide(text: &str) -> String {
    match text.char_indices().nth(MAX_DETAIL_CHARS) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

/// One human-readable line for a JSON-RPC `error` object.
///
/// JSON-RPC pins down only `code` and `message`, and agents routinely leave
/// `message` a generic label — codex-acp sends "Internal error" — while the
/// reason the turn failed sits in the agent-defined `data`. Surfacing only
/// `message` tells the user nothing they can act on, so fold the two together.
///
/// `data` is agent-defined, so no shape is assumed: an object with a string
/// `message`, a bare string, or an upstream error envelope forwarded as a JSON
/// string all yield their innermost sentence. Anything else contributes
/// nothing, rather than rendering `null` or `{}` at the user. The detail is
/// dropped when `message` already contains it, so an agent that echoes its own
/// message into `data` does not read like two separate failures.
///
/// Both halves are agent-authored and both are rendered as a single-line
/// failure label, so both are sanitised — see `crucible_core::text` — and both
/// are capped. Capping only `data` would be theatre: an agent that wants to
/// hand the daemon half a gigabyte of prose would simply put it in `message`.
pub(super) fn describe_rpc_error(error: &serde_json::Value) -> String {
    // Elide before sanitising, so no copy of the agent's string larger than
    // the cap is ever made. Sanitising only shrinks, so the order is safe.
    let message = sanitize_single_line(&elide(
        error
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("Unknown error"),
    ));

    match error.get("data").and_then(|data| detail_text(data, 0)) {
        Some(detail) if !message.contains(detail.as_str()) => format!("{message}: {detail}"),
        _ => message,
    }
}

/// The innermost readable string in an agent-defined error payload, following
/// `message` then `error` and unwrapping stringified JSON on the way down.
fn detail_text(value: &serde_json::Value, depth: u8) -> Option<String> {
    match value {
        serde_json::Value::String(text) => {
            // Bound the input on a borrow, before anything copies it.
            let (text, oversized) = match text.char_indices().nth(MAX_DETAIL_INPUT_CHARS) {
                Some((end, _)) => (&text[..end], true),
                None => (text.as_str(), false),
            };
            if oversized {
                tracing::warn!(
                    depth,
                    "ACP error detail exceeded the input cap; truncating without unwrapping"
                );
            }
            // Sanitise before the emptiness check, not after: a payload that is
            // nothing *but* control characters must read as "no detail" rather
            // than as an empty-looking detail that still carries them.
            let text = sanitize_single_line(text);
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            // Agents proxying an upstream API often forward its error body
            // verbatim as a string; dig into it rather than printing JSON.
            // Skipped once truncated — a cut envelope is not parseable JSON,
            // and re-parsing it is the allocation the cap exists to refuse.
            let nested = if !oversized && depth < MAX_DETAIL_DEPTH {
                serde_json::from_str::<serde_json::Value>(text)
                    .ok()
                    .filter(serde_json::Value::is_object)
                    .and_then(|inner| detail_text(&inner, depth + 1))
            } else {
                None
            };
            // A nested result already came back elided by this same arm.
            Some(nested.unwrap_or_else(|| elide(text)))
        }
        serde_json::Value::Object(_) if depth < MAX_DETAIL_DEPTH => value
            .get("message")
            .or_else(|| value.get("error"))
            .and_then(|inner| detail_text(inner, depth + 1)),
        _ => None,
    }
}

/// Clears the turn slot when the turn ends, also when its future drops.
struct TurnSlot<'a>(&'a std::sync::Mutex<super::Shared>);

impl Drop for TurnSlot<'_> {
    fn drop(&mut self) {
        super::lock(self.0).turn = None;
    }
}

impl CrucibleAcpClient {
    /// Run one turn. Each chunk of the turn goes to `out`.
    ///
    /// A closed `out` is a cancel: the daemon dropped the turn. The client
    /// then sends `session/cancel` once, answers a pending permission request
    /// with `cancelled`, and waits for the agent to end the turn. The turn
    /// has a deadline of ten times `timeout_ms` (30 s without it). At the
    /// deadline the client sends `session/cancel` and returns a timeout.
    ///
    /// The SDK dispatches every update that comes before the response before
    /// it gives the response. So the updates that are in the channel when
    /// the response comes are all of the turn.
    pub async fn prompt(
        &self,
        request: PromptRequest,
        out: &mpsc::UnboundedSender<StreamingChunk>,
    ) -> Result<(TurnSummary, PromptResponse)> {
        let session_id = request.session_id.clone();
        let (updates_tx, mut updates) = mpsc::unbounded_channel();
        let cancel = CancellationToken::new();
        {
            let mut shared = super::lock(&self.shared);
            shared.turn = Some(super::Turn {
                updates: updates_tx,
                cancel: cancel.clone(),
            });
            // The tool names of the turn that ended answer nothing now.
            shared.tool_names.clear();
        }
        let _slot = TurnSlot(&self.shared);

        let limit = self
            .config
            .timeout_ms
            .map(|ms| Duration::from_millis(ms * 10))
            .unwrap_or(Duration::from_secs(30));
        let deadline = tokio::time::sleep(limit);
        tokio::pin!(deadline);
        let response = self.cx.send_request(request).block_task();
        tokio::pin!(response);

        let mut state = StreamingState::default();
        let result = loop {
            tokio::select! {
                biased;
                Some(update) = updates.recv() => apply_update(update, &mut state, out),
                () = out.closed(), if !cancel.is_cancelled() => {
                    tracing::debug!(%session_id, "Turn dropped; sending session/cancel to ACP agent");
                    cancel.cancel();
                    self.send_cancel(&session_id);
                }
                result = &mut response => break result,
                () = &mut deadline => {
                    cancel.cancel();
                    self.send_cancel(&session_id);
                    return Err(ClientError::Timeout(format!(
                        "Streaming operation timed out after {limit:?}"
                    )));
                }
            }
        };
        while let Ok(update) = updates.try_recv() {
            apply_update(update, &mut state, out);
        }

        let response = result.map_err(|error| {
            if super::connection_lost(&error) {
                return ClientError::Connection("the agent closed the connection mid-turn".into());
            }
            let error = serde_json::to_value(&error).unwrap_or_default();
            ClientError::Session(format!(
                "Agent error during streaming: {} (code: {})",
                describe_rpc_error(&error),
                error.get("code").unwrap_or(&serde_json::Value::Null)
            ))
        })?;

        // The turn is over: name every call the agent never named, and close
        // every call it never completed.
        for chunk in state
            .tool_calls
            .flush(&stop_reason_label(response.stop_reason))
        {
            let _ = out.send(chunk);
        }
        Ok((state.summary(), response))
    }

    /// Send `session/cancel`. The agent must end the turn with `cancelled`.
    fn send_cancel(&self, session_id: &SessionId) {
        if let Err(error) = self
            .cx
            .send_notification(CancelNotification::new(session_id.clone()))
        {
            tracing::debug!(%error, "session/cancel could not go to the agent");
        }
    }
}

/// Apply one update of the turn. Each chunk that the update makes goes to
/// `out`. A closed `out` is not an error here: the turn loop sees it.
pub(super) fn apply_update(
    update: SessionUpdate,
    state: &mut StreamingState,
    out: &mpsc::UnboundedSender<StreamingChunk>,
) {
    let emit = |chunk| {
        let _ = out.send(chunk);
    };
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => match chunk.content {
            ContentBlock::Text(text_block) => {
                // The agent process owns every byte of this, so it is
                // sanitised here, at the one point all agents pass through.
                // Sanitising before the resend check keeps the comparison
                // like-for-like with what `append_text` stores.
                let text = sanitize_multiline(&text_block.text);
                // cursor-acp sends the whole text again as a last chunk.
                if state.is_duplicate_resend(&text) {
                    tracing::debug!(
                        text_len = text.len(),
                        "Skipping duplicate full-text re-send from agent"
                    );
                    return;
                }
                state.append_text(&text);
                state.produced_content |= is_visible_content(&text);
                emit(StreamingChunk::Text(text));
            }
            other => tracing::debug!("Ignoring non-text content block: {:?}", other),
        },
        // Reasoning. The resend guard compares against the answer text, so
        // it does not apply here: a thought must not suppress an answer
        // chunk. `SessionEventStream::is_thinking_replay` in the CLI handles
        // an agent that replays its whole reasoning block.
        SessionUpdate::AgentThoughtChunk(chunk) => match chunk.content {
            ContentBlock::Text(text_block) => {
                let text = sanitize_multiline(&text_block.text);
                state.produced_content |= is_visible_content(&text);
                emit(StreamingChunk::Thinking(text));
            }
            other => tracing::debug!("Ignoring non-text thought block: {:?}", other),
        },
        // Both frames merge into the per-turn table, which decides what the
        // stream sees. See `tool_table.rs`.
        SessionUpdate::ToolCall(tool_call) => {
            state
                .tool_calls
                .upsert_call(tool_call)
                .into_iter()
                .for_each(emit);
        }
        SessionUpdate::ToolCallUpdate(update) => {
            state
                .tool_calls
                .upsert_update(update)
                .into_iter()
                .for_each(emit);
        }
        // Hermes streams a `user_message_chunk` when it drains a queued
        // prompt. The chunk is the user's own text, not the answer.
        SessionUpdate::UserMessageChunk(chunk) => {
            tracing::debug!("Ignoring user_message_chunk: {:?}", chunk.content);
        }
        // The agent reports its context-window occupancy. `size: 0`
        // describes no window, so it is refused: the statusline shows the
        // no-data state for it, while a `limit: 0` claims a resolved window.
        // `used: 0` is a real reading. `cost` is dropped, because nothing in
        // Crucible shows a monetary figure.
        SessionUpdate::UsageUpdate(update) => {
            if update.size > 0 {
                emit(StreamingChunk::ContextWindow {
                    used: update.used,
                    limit: update.size,
                });
            }
        }
        other => tracing::debug!("Ignoring session update: {:?}", other),
    }
}

#[cfg(test)]
#[path = "streaming_tests.rs"]
mod tests;
