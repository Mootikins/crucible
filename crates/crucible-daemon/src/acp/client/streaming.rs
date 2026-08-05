use std::sync::atomic::Ordering;

use agent_client_protocol::{
    ContentBlock, RequestPermissionRequest, SessionNotification, SessionUpdate, ToolCallContent,
    ToolCallStatus,
};

use super::types::StreamingState;
use super::{CrucibleAcpClient, REQUEST_ID};
use crate::acp::streaming::{humanize_tool_title, StreamingCallback, StreamingChunk};
use crate::acp::{ClientError, Result};
use crucible_core::text::{sanitize_multiline, sanitize_single_line};
use crucible_core::types::acp::{FileDiff, ToolCallInfo};

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
    /// Send a prompt request and handle streaming responses
    ///
    /// This method properly handles the ACP streaming protocol where:
    /// 1. Agent sends `session/update` notifications during processing
    /// 2. Agent sends final response with `stopReason` when complete
    ///
    /// # Arguments
    ///
    /// * `request` - The PromptRequest to send
    /// * `request_id` - The JSON-RPC request ID to match the final response
    ///
    /// # Returns
    ///
    /// Tuple of (formatted_content, tool_calls, PromptResponse)
    ///
    /// # Errors
    ///
    /// Returns an error if communication fails
    pub async fn send_prompt_with_streaming(
        &mut self,
        request: agent_client_protocol::PromptRequest,
    ) -> Result<(
        String,
        Vec<ToolCallInfo>,
        agent_client_protocol::PromptResponse,
    )> {
        use serde_json::json;

        // Use the global REQUEST_ID counter (shared with send_request) to avoid ID collisions
        let request_id = REQUEST_ID.fetch_add(1, Ordering::SeqCst);

        tracing::info!("Starting streaming request with ID {}", request_id);

        // Wrap in JSON-RPC 2.0 format
        let json_request = json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "session/prompt",
            "params": serde_json::to_value(&request)?
        });

        // Write to agent stdin
        self.write_request(&json_request).await?;

        // Create overall timeout (10x per-read timeout or 30s default)
        let overall_timeout = self
            .config
            .timeout_ms
            .map(|ms| tokio::time::Duration::from_millis(ms * 10))
            .unwrap_or(tokio::time::Duration::from_secs(30));

        // Wrap the streaming loop in a timeout
        let streaming_future = async {
            let mut state = StreamingState::default();

            // Read lines until we get the final response (with matching id)
            loop {
                let response_line = self.read_response_line().await?;
                let response: serde_json::Value = serde_json::from_str(&response_line)?;

                tracing::trace!("Received line: {}", response_line);
                tracing::debug!(
                    "Received from agent: {}",
                    serde_json::to_string_pretty(&response).unwrap_or_default()
                );

                // Check for error responses
                if let Some(error) = response.get("error") {
                    let error_msg = describe_rpc_error(error);
                    let error_code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);

                    tracing::error!("Agent returned error: {} (code: {})", error_msg, error_code);
                    return Err(ClientError::Session(format!(
                        "Agent error during streaming: {} (code: {}, accumulated {} chars)",
                        error_msg,
                        error_code,
                        state.formatted_length()
                    )));
                }

                if let Some(prompt_response) = self
                    .process_streaming_message(&response, request_id, &mut state)
                    .await?
                {
                    tracing::info!(
                        "Final response received (ID: {:?}) after {} notifications, {} chars",
                        request_id,
                        state.notification_count,
                        state.formatted_length()
                    );

                    return Ok((state, prompt_response));
                }
            }
        };

        // Apply overall timeout
        match tokio::time::timeout(overall_timeout, streaming_future).await {
            Ok(Ok((state, response))) => Ok((state.formatted_output(), state.tool_calls, response)),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(ClientError::Timeout(format!(
                "Streaming operation timed out after {}s",
                overall_timeout.as_secs()
            ))),
        }
    }

    /// Send a prompt request with streaming and a callback for real-time chunks.
    ///
    /// This method is similar to `send_prompt_with_streaming` but calls the provided
    /// callback for each chunk as it arrives, enabling real-time display.
    ///
    /// # Arguments
    ///
    /// * `request` - The PromptRequest to send
    /// * `callback` - Callback invoked for each streaming chunk. Return `false` to cancel.
    ///
    /// # Returns
    ///
    /// Tuple of (formatted_content, tool_calls, PromptResponse)
    pub async fn send_prompt_with_callback(
        &mut self,
        request: agent_client_protocol::PromptRequest,
        mut callback: StreamingCallback,
    ) -> Result<(
        String,
        Vec<ToolCallInfo>,
        agent_client_protocol::PromptResponse,
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
            Ok(Ok((state, response))) => Ok((state.formatted_output(), state.tool_calls, response)),
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
    /// is a notification, which by JSON-RPC must not be answered at all.
    async fn refuse_unhandled_method(
        &mut self,
        frame: &serde_json::Value,
        method_name: &str,
    ) -> Result<()> {
        match frame.get("id").and_then(|id| self.parse_request_id(id)) {
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
    async fn process_streaming_message_with_callback(
        &mut self,
        response: &serde_json::Value,
        request_id: u64,
        state: &mut StreamingState,
        callback: &mut StreamingCallback,
    ) -> Result<Option<agent_client_protocol::PromptResponse>> {
        if let Some(method_value) = response.get("method") {
            state.notification_count += 1;
            let method_name = method_value.as_str().unwrap_or_default();

            if method_name == "session/update" {
                if let Some(params) = response.get("params") {
                    // Handled ahead of the typed parse, which cannot see it:
                    // see `usage.rs::extract_context_window`.
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
                    state.cancelled |= !callback(StreamingChunk::Thinking(sanitize_multiline(
                        &text_block.text,
                    )));
                }
                other => {
                    tracing::debug!("Ignoring non-text thought block: {:?}", other);
                }
            },
            SessionUpdate::ToolCall(tool_call) => {
                // A title is a one-line label naming what the agent is about
                // to do, so it gets the single-line form: a newline or a bidi
                // override in it makes the card claim one action and perform
                // another.
                let title = sanitize_single_line(&tool_call.title);
                let tool_name = humanize_tool_title(&title);
                let tool_id = tool_call.tool_call_id.to_string();

                // Extract diffs once from this notification's content; reuse
                // for both the live `ToolStart` chunk and the `ToolCallInfo`
                // recorded in `state.tool_calls`.
                let diffs: Vec<FileDiff> = tool_call
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        ToolCallContent::Diff(diff) => Some(FileDiff::from_contents(
                            diff.path.to_string_lossy().to_string(),
                            diff.old_text.clone(),
                            diff.new_text.clone(),
                        )),
                        _ => None,
                    })
                    .filter(filter_oversize_diff)
                    .collect();

                // Emit tool start event with the diffs we just extracted so
                // the TUI can render them in scrollback as the call appears.
                state.cancelled |= !callback(StreamingChunk::ToolStart {
                    name: tool_name.clone(),
                    id: tool_id.clone(),
                    arguments: tool_call.raw_input.clone(),
                    diffs: diffs.clone(),
                });

                let mut info = ToolCallInfo::new(title).with_id(tool_id).with_diffs(diffs);
                if let Some(args) = tool_call.raw_input.clone() {
                    info = info.with_arguments(args);
                }
                self.record_tool_call(info, state);
            }
            SessionUpdate::ToolCallUpdate(update) => {
                let tool_id = update.tool_call_id.to_string();
                // Tool updates often indicate completion
                if matches!(
                    update.fields.status,
                    Some(ToolCallStatus::Completed | ToolCallStatus::Failed)
                ) {
                    state.cancelled |= !callback(StreamingChunk::ToolEnd {
                        id: tool_id.clone(),
                        result: Self::extract_tool_result(update.fields.raw_output.as_ref()),
                        error: Self::extract_tool_error(
                            update.fields.status,
                            update.fields.raw_output.as_ref(),
                        ),
                    });
                }

                // Check if update has interesting fields (title, raw_input, or content with diffs)
                let has_content_diffs = update
                    .fields
                    .content
                    .as_ref()
                    .map(|c| {
                        c.iter()
                            .any(|item| matches!(item, ToolCallContent::Diff(_)))
                    })
                    .unwrap_or(false);

                if update.fields.title.is_some()
                    || update.fields.raw_input.is_some()
                    || has_content_diffs
                {
                    // Only the wire title needs sanitising; the fallback comes
                    // from `state`, which is only ever written with one that
                    // already passed through here.
                    let title = update
                        .fields
                        .title
                        .as_deref()
                        .map(sanitize_single_line)
                        .or_else(|| state.title_for_tool(&tool_id))
                        .unwrap_or_else(|| "Unnamed tool".to_string());

                    let diffs: Vec<FileDiff> = update
                        .fields
                        .content
                        .iter()
                        .flatten()
                        .filter_map(|c| match c {
                            ToolCallContent::Diff(diff) => Some(FileDiff::from_contents(
                                diff.path.to_string_lossy().to_string(),
                                diff.old_text.clone(),
                                diff.new_text.clone(),
                            )),
                            _ => None,
                        })
                        .filter(filter_oversize_diff)
                        .collect();

                    // Late-diff path: if the tool was already announced via
                    // a prior `ToolStart` and this update brings *changed*
                    // diff content (e.g. Claude Code defers diffs), fire a
                    // live `ToolDiffUpdate` chunk so the TUI can replace the
                    // diff snapshot in the existing scrollback entry.
                    // Without this, the diffs are recorded into
                    // `state.tool_calls` but the post-stream replay in
                    // `acp_handle.rs` filters out already-announced ids and
                    // silently drops them.
                    //
                    // Skip the emit when the prior recorded diffs already
                    // match — re-rendering an identical snapshot causes a
                    // visual flash with no informational gain.
                    if has_content_diffs && !diffs.is_empty() {
                        let prior_diffs = state
                            .tool_calls
                            .iter()
                            .find(|tc| tc.id.as_deref() == Some(tool_id.as_str()))
                            .map(|tc| &tc.diffs);
                        let mut cancelled = false;
                        if let Some(prior) = prior_diffs {
                            if prior != &diffs {
                                cancelled = !callback(StreamingChunk::ToolDiffUpdate {
                                    call_id: tool_id.clone(),
                                    diffs: diffs.clone(),
                                });
                            }
                        }
                        state.cancelled |= cancelled;
                    }

                    // Late-args path, mirroring the late-diff path above: the
                    // call was announced without `rawInput` (claude-agent-acp
                    // defers it), so re-emit once the arguments are known.
                    // Skip when the recorded call already carries the same
                    // arguments — an identical snapshot is pure noise.
                    if let Some(args) = update.fields.raw_input.as_ref() {
                        let prior_args = state
                            .tool_calls
                            .iter()
                            .find(|tc| tc.id.as_deref() == Some(tool_id.as_str()))
                            .and_then(|tc| tc.arguments.as_ref());
                        if prior_args != Some(args) {
                            state.cancelled |= !callback(StreamingChunk::ToolArgsUpdate {
                                call_id: tool_id.clone(),
                                arguments: args.clone(),
                            });
                        }
                    }

                    let mut info = ToolCallInfo::new(title).with_id(tool_id).with_diffs(diffs);
                    if let Some(args) = update.fields.raw_input.clone() {
                        info = info.with_arguments(args);
                    }
                    self.record_tool_call(info, state);
                }
            }
            SessionUpdate::AvailableCommandsUpdate(update) => {
                tracing::info!(
                    "Received {} available command(s) from agent",
                    update.available_commands.len()
                );
                self.available_commands = update.available_commands;
            }
            other => {
                tracing::debug!("Ignoring session update: {:?}", other);
            }
        }
    }

    pub(super) async fn process_streaming_message(
        &mut self,
        response: &serde_json::Value,
        request_id: u64,
        state: &mut StreamingState,
    ) -> Result<Option<agent_client_protocol::PromptResponse>> {
        if let Some(method_value) = response.get("method") {
            state.notification_count += 1;
            let method_name = method_value.as_str().unwrap_or_default();
            tracing::debug!(
                "Notification #{}: {}",
                state.notification_count,
                method_name
            );

            if method_name == "session/update" {
                if let Some(params) = response.get("params") {
                    // Consumed here too, though this path has nowhere to put
                    // it: its only output is `formatted_output()`, the
                    // assistant's answer. Recognising the frame keeps it from
                    // being reported as a parse failure — it parses fine, the
                    // variant is just feature-gated out of the typed enum.
                    if let Some((used, limit)) = super::usage::extract_context_window(params) {
                        tracing::debug!(
                            used,
                            limit,
                            "Context window reported on a non-streaming prompt; no consumer"
                        );
                    } else if let Err(e) =
                        serde_json::from_value::<SessionNotification>(params.clone())
                            .map(|notification| self.apply_session_update(notification, state))
                    {
                        tracing::warn!("Failed to parse SessionNotification: {}", e);
                        tracing::debug!("Raw params: {}", params);
                    }
                } else {
                    tracing::warn!("session/update notification missing params");
                }
            } else if method_name == "session/request_permission" {
                if let Some(params) = response.get("params") {
                    match serde_json::from_value::<RequestPermissionRequest>(params.clone()) {
                        Ok(request) => {
                            if let Some(id_value) = response.get("id") {
                                if let Some(permission_id) = self.parse_request_id(id_value) {
                                    self.respond_to_permission_request(permission_id, request)
                                        .await?;
                                } else {
                                    tracing::warn!(
                                        "Permission request missing valid ID: {:?}",
                                        id_value
                                    );
                                }
                            } else {
                                tracing::warn!("Permission request missing ID field");
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse RequestPermissionRequest: {}", e);
                            tracing::debug!("Raw params: {}", params);
                        }
                    }
                } else {
                    tracing::warn!("session/request_permission missing params");
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
            } else {
                tracing::warn!(
                    "Received response with non-matching ID: {:?} (expected: {})",
                    id_value,
                    request_id
                );
            }

            return Ok(None);
        }

        Err(ClientError::Session(
            "Received message without id or method".to_string(),
        ))
    }

    fn apply_session_update(
        &mut self,
        notification: SessionNotification,
        state: &mut StreamingState,
    ) {
        match notification.update {
            SessionUpdate::AgentMessageChunk(chunk) => match chunk.content {
                ContentBlock::Text(text_block) => {
                    // Same boundary, same treatment as the streaming path:
                    // this accumulator becomes `formatted_output()`, which is
                    // what gets persisted as the assistant's answer.
                    let text = sanitize_multiline(&text_block.text);
                    state.append_text(&text);
                    tracing::trace!(
                        "Accumulated chunk: '{}' (total: {} chars)",
                        text,
                        state.formatted_length()
                    );
                }
                other => {
                    tracing::debug!("Ignoring non-text content block: {:?}", other);
                }
            },
            // Matched so reasoning is no longer swallowed by the terminal arm,
            // but deliberately *not* folded into `state`: this path's only
            // output is `formatted_output()`, which is the assistant's answer.
            // Reasoning is not the answer, and there is no thinking channel in
            // this function's return type — the live path
            // (`apply_session_update_with_callback`) is the one that carries
            // thoughts, via `StreamingChunk::Thinking`. An accumulator here
            // would have no reader.
            SessionUpdate::AgentThoughtChunk(chunk) => {
                tracing::trace!("Agent thought chunk (not part of the answer): {:?}", chunk);
            }
            SessionUpdate::ToolCall(tool_call) => {
                tracing::info!("Tool call: {}", tool_call.title);
                // Extract diffs from ToolCallContent::Diff entries
                let diffs: Vec<FileDiff> = tool_call
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        ToolCallContent::Diff(diff) => Some(FileDiff::from_contents(
                            diff.path.to_string_lossy().to_string(),
                            diff.old_text.clone(),
                            diff.new_text.clone(),
                        )),
                        _ => None,
                    })
                    .filter(filter_oversize_diff)
                    .collect();
                let mut info = ToolCallInfo::new(sanitize_single_line(&tool_call.title))
                    .with_id(tool_call.tool_call_id.to_string())
                    .with_diffs(diffs);
                if let Some(args) = tool_call.raw_input.clone() {
                    info = info.with_arguments(args);
                }
                self.record_tool_call(info, state);
            }
            SessionUpdate::ToolCallUpdate(update) => {
                tracing::debug!("Tool call update: {:?}", update.tool_call_id);
                // Check if update has interesting fields (title, raw_input, or content with diffs)
                let has_content_diffs = update
                    .fields
                    .content
                    .as_ref()
                    .map(|c| {
                        c.iter()
                            .any(|item| matches!(item, ToolCallContent::Diff(_)))
                    })
                    .unwrap_or(false);

                if update.fields.title.is_some()
                    || update.fields.raw_input.is_some()
                    || has_content_diffs
                {
                    let id = update.tool_call_id.to_string();
                    let title = update
                        .fields
                        .title
                        .as_deref()
                        .map(sanitize_single_line)
                        .or_else(|| state.title_for_tool(&id))
                        .unwrap_or_else(|| "Unnamed tool".to_string());

                    // Extract diffs from content if present
                    let diffs: Vec<FileDiff> = update
                        .fields
                        .content
                        .iter()
                        .flatten()
                        .filter_map(|c| match c {
                            ToolCallContent::Diff(diff) => Some(FileDiff::from_contents(
                                diff.path.to_string_lossy().to_string(),
                                diff.old_text.clone(),
                                diff.new_text.clone(),
                            )),
                            _ => None,
                        })
                        .filter(filter_oversize_diff)
                        .collect();

                    let mut info = ToolCallInfo::new(title).with_id(id).with_diffs(diffs);
                    if let Some(args) = update.fields.raw_input.clone() {
                        info = info.with_arguments(args);
                    }
                    self.record_tool_call(info, state);
                }
            }
            SessionUpdate::AvailableCommandsUpdate(update) => {
                tracing::info!(
                    "Received {} available command(s) from agent",
                    update.available_commands.len()
                );
                self.available_commands = update.available_commands;
            }
            other => {
                tracing::debug!("Ignoring update type: {:?}", other);
            }
        }
    }
}

/// True if this diff is within the size cap; logs and returns false otherwise.
/// Use as a `.filter()` predicate when ingesting ACP-supplied diffs so the
/// cache and renderer never have to hold huge payloads.
fn filter_oversize_diff(d: &FileDiff) -> bool {
    if d.is_oversize() {
        tracing::debug!(
            path = %d.path,
            "ACP-supplied diff exceeded MAX_DIFF_BYTES; dropping at edge"
        );
        false
    } else {
        true
    }
}

#[cfg(test)]
#[path = "streaming_tests.rs"]
mod tests;
