use std::sync::atomic::Ordering;

use agent_client_protocol::schema::v1::{
    ContentBlock, RequestPermissionRequest, SessionNotification, SessionUpdate,
};

use super::types::StreamingState;
use super::{CrucibleAcpClient, REQUEST_ID};
use crate::acp::streaming::{StreamingCallback, StreamingChunk, TurnSummary};
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

/// Build the `session/cancel` JSON-RPC notification (no `id` — notifications
/// are fire-and-forget). The agent must abort the in-flight turn and end it
/// with `StopReason::Cancelled`.
pub(super) fn build_cancel_notification(session_id: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "session/cancel",
        "params": { "sessionId": session_id }
    })
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

impl CrucibleAcpClient {
    /// Send a prompt request with streaming and a callback for real-time chunks.
    ///
    /// The ACP streaming protocol sends `session/update` notifications while
    /// the agent works, then a final response with a `stopReason`. This method
    /// calls the callback for each chunk as it arrives, so a caller can show
    /// the turn in real time.
    ///
    /// # Arguments
    ///
    /// * `request` - The PromptRequest to send
    /// * `callback` - Callback invoked for each streaming chunk. Return `false` to cancel.
    ///
    /// # Returns
    ///
    /// What the turn showed the user, and the final PromptResponse. The
    /// chunks of the turn reach the caller only through the callback.
    pub async fn send_prompt_with_callback(
        &mut self,
        request: agent_client_protocol::schema::v1::PromptRequest,
        mut callback: StreamingCallback,
    ) -> Result<(
        TurnSummary,
        agent_client_protocol::schema::v1::PromptResponse,
    )> {
        use serde_json::json;

        let request_id = REQUEST_ID.fetch_add(1, Ordering::SeqCst);
        tracing::info!(
            "Starting streaming request with callback, ID {}",
            request_id
        );

        let json_request = json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "session/prompt",
            "params": serde_json::to_value(&request)?
        });

        self.write_request(&json_request).await?;

        let overall_timeout = self
            .config
            .timeout_ms
            .map(|ms| tokio::time::Duration::from_millis(ms * 10))
            .unwrap_or(tokio::time::Duration::from_secs(30));

        // Needed if the turn is cancelled mid-stream, to tell the agent to stop.
        let session_id = request.session_id.to_string();

        let streaming_future = async {
            let mut state = StreamingState::default();
            let mut cancel_sent = false;

            loop {
                let response_line = self.read_response_line().await?;
                let response: serde_json::Value = serde_json::from_str(&response_line)?;

                tracing::trace!("Received line: {}", response_line);

                if let Some(error) = response.get("error") {
                    let error_msg = describe_rpc_error(error);
                    let error_code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);

                    return Err(ClientError::Session(format!(
                        "Agent error during streaming: {} (code: {})",
                        error_msg, error_code
                    )));
                }

                if let Some(prompt_response) = self
                    .process_streaming_message_with_callback(
                        &response,
                        request_id,
                        &mut state,
                        &mut callback,
                    )
                    .await?
                {
                    // The turn is over: name every call the agent never
                    // named, and close every call it never completed.
                    let stop_reason = stop_reason_label(prompt_response.stop_reason);
                    for chunk in state.tool_calls.flush(&stop_reason) {
                        state.cancelled |= !callback(chunk);
                    }
                    return Ok((state, prompt_response));
                }

                // A callback returned `false`: the daemon's turn stream was
                // dropped (cancelled). Tell the agent to stop generating so it
                // doesn't run to completion server-side and burn tokens. Send
                // `session/cancel` once, then keep reading until the agent
                // returns its final (Cancelled) response, which exits the loop
                // above and leaves the connection clean for the next turn.
                if state.cancelled && !cancel_sent {
                    tracing::debug!(session_id = %session_id, "Turn cancelled; sending session/cancel to ACP agent");
                    self.send_session_cancel(&session_id).await?;
                    cancel_sent = true;
                }
            }
        };

        match tokio::time::timeout(overall_timeout, streaming_future).await {
            Ok(Ok((state, response))) => Ok((state.summary(), response)),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(ClientError::Timeout(format!(
                "Streaming operation timed out after {}s",
                overall_timeout.as_secs()
            ))),
        }
    }

    /// Handle an inbound frame whose method we do not implement.
    ///
    /// A frame with an `id` is a request and gets a `-32601` reply; one without
    /// is a notification, which by JSON-RPC must not be answered at all. The
    /// *presence* of the key is what distinguishes the two, so every id shape
    /// the protocol allows — string, negative, null — is answered rather than
    /// mistaken for a notification and dropped.
    async fn refuse_unhandled_method(
        &mut self,
        frame: &serde_json::Value,
        method_name: &str,
    ) -> Result<()> {
        match frame.get("id") {
            Some(request_id) => self.respond_method_not_found(request_id, method_name).await,
            None => {
                tracing::debug!("Ignoring RPC notification: {}", method_name);
                Ok(())
            }
        }
    }

    /// Send a `session/cancel` notification so the agent stops the in-flight
    /// turn. Per ACP, the agent then ends the turn with `StopReason::Cancelled`.
    async fn send_session_cancel(&mut self, session_id: &str) -> Result<()> {
        self.write_request(&build_cancel_notification(session_id))
            .await
    }

    /// Process a streaming message and invoke callback for chunks.
    pub(super) async fn process_streaming_message_with_callback(
        &mut self,
        response: &serde_json::Value,
        request_id: u64,
        state: &mut StreamingState,
        callback: &mut StreamingCallback,
    ) -> Result<Option<agent_client_protocol::schema::v1::PromptResponse>> {
        if let Some(method_value) = response.get("method") {
            state.notification_count += 1;
            let method_name = method_value.as_str().unwrap_or_default();

            if method_name == "session/update" {
                if let Some(params) = response.get("params") {
                    // Handled ahead of the typed parse: see
                    // `usage.rs::extract_context_window`.
                    if let Some((used, limit)) = super::usage::extract_context_window(params) {
                        if !callback(StreamingChunk::ContextWindow { used, limit }) {
                            state.cancelled = true;
                        }
                        return Ok(None);
                    }
                    match serde_json::from_value::<SessionNotification>(params.clone()) {
                        Ok(notification) => {
                            self.apply_session_update_with_callback(notification, state, callback);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse SessionNotification: {}", e);
                        }
                    }
                }
            } else if method_name == "session/request_permission" {
                if let Some(params) = response.get("params") {
                    if let Ok(request) =
                        serde_json::from_value::<RequestPermissionRequest>(params.clone())
                    {
                        if let Some(id_value) = response.get("id") {
                            if let Some(permission_id) = self.parse_request_id(id_value) {
                                self.respond_to_permission_request(permission_id, request)
                                    .await?;
                            }
                        }
                    }
                }
            } else {
                self.refuse_unhandled_method(response, method_name).await?;
            }

            return Ok(None);
        }

        if let Some(id_value) = response.get("id") {
            let id_matches = match id_value {
                serde_json::Value::Number(n) => n.as_u64() == Some(request_id),
                serde_json::Value::String(s) => s.parse::<u64>().ok() == Some(request_id),
                _ => false,
            };

            if id_matches {
                let result = response.get("result").ok_or_else(|| {
                    ClientError::Session("Missing result in prompt response".to_string())
                })?;
                self.last_usage = super::usage::extract_usage(result);
                let prompt_response = serde_json::from_value(result.clone())?;
                return Ok(Some(prompt_response));
            }

            return Ok(None);
        }

        Err(ClientError::Session(
            "Received message without id or method".to_string(),
        ))
    }

    /// Apply a session update and invoke callback for streaming chunks.
    pub(super) fn apply_session_update_with_callback(
        &mut self,
        notification: SessionNotification,
        state: &mut StreamingState,
        callback: &mut StreamingCallback,
    ) {
        match notification.update {
            SessionUpdate::AgentMessageChunk(chunk) => match chunk.content {
                ContentBlock::Text(text_block) => {
                    // The agent process owns every byte of this, so it is
                    // sanitised here — at the one point all agents pass
                    // through, before anything is accumulated, broadcast,
                    // persisted or replayed. Sanitising *before* the resend
                    // check keeps the comparison like-for-like with what
                    // `append_text` stores.
                    let text = sanitize_multiline(&text_block.text);
                    // Skip full-text re-sends from agents like cursor-acp that
                    // emit accumulated text as a final notification
                    if state.is_duplicate_resend(&text) {
                        tracing::debug!(
                            text_len = text.len(),
                            "Skipping duplicate full-text re-send from agent"
                        );
                        return;
                    }
                    state.append_text(&text);
                    state.produced_content |= is_visible_content(&text);
                    state.cancelled |= !callback(StreamingChunk::Text(text));
                }
                other => {
                    tracing::debug!("Ignoring non-text content block: {:?}", other);
                }
            },
            // Reasoning. Every conforming ACP agent streams it, and without
            // this arm it fell through to the terminal "ignoring session
            // update" case below, leaving `StreamingChunk::Thinking` with no
            // producer — so a delegated session showed no thinking blocks while
            // the internal agent showed them.
            //
            // Deliberately *not* guarded by `is_duplicate_resend`: that guard
            // compares against `accumulated_text`, which is the assistant's
            // answer. Sharing it would let a thought suppress an answer chunk
            // (and vice versa) whenever the two happened to match. A thinking
            // twin of it is not added either — an agent that replays its whole
            // reasoning block is already handled downstream, source-agnostically
            // and turn-scoped, by `SessionEventStream::is_thinking_replay`
            // (`crucible-cli/src/tui/oil/chat_runner/stream.rs`).
            SessionUpdate::AgentThoughtChunk(chunk) => match chunk.content {
                ContentBlock::Text(text_block) => {
                    let text = sanitize_multiline(&text_block.text);
                    state.produced_content |= is_visible_content(&text);
                    state.cancelled |= !callback(StreamingChunk::Thinking(text));
                }
                other => {
                    tracing::debug!("Ignoring non-text thought block: {:?}", other);
                }
            },
            // Both frames merge into the per-turn table, which decides what
            // the stream sees: one announcement per call, a held result for
            // a call with no name yet, and an update only when a value
            // changed. See `tool_table.rs`.
            SessionUpdate::ToolCall(tool_call) => {
                for chunk in state.tool_calls.upsert_call(tool_call) {
                    state.cancelled |= !callback(chunk);
                }
            }
            SessionUpdate::ToolCallUpdate(update) => {
                for chunk in state.tool_calls.upsert_update(update) {
                    state.cancelled |= !callback(chunk);
                }
            }
            SessionUpdate::AvailableCommandsUpdate(update) => {
                tracing::info!(
                    "Received {} available command(s) from agent",
                    update.available_commands.len()
                );
                self.available_commands = update.available_commands;
            }
            // Hermes streams a `user_message_chunk` when it drains a queued
            // prompt inside one `session/prompt` reply. Crucible refuses a
            // concurrent turn at the handle lock, so the queue path is not
            // reachable from here. The chunk is the user's own text, not the
            // agent's answer, so it must not reach `accumulated_text`.
            SessionUpdate::UserMessageChunk(chunk) => {
                tracing::debug!("Ignoring user_message_chunk: {:?}", chunk.content);
            }
            other => {
                tracing::debug!("Ignoring session update: {:?}", other);
            }
        }
    }
}

#[cfg(test)]
#[path = "streaming_tests.rs"]
mod tests;
