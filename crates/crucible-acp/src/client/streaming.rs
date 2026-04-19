use std::sync::atomic::Ordering;

use agent_client_protocol::{
    ContentBlock, RequestPermissionRequest, SessionNotification, SessionUpdate, ToolCallContent,
    ToolCallStatus,
};

use super::types::StreamingState;
use super::{CrucibleAcpClient, REQUEST_ID};
use crate::streaming::{humanize_tool_title, StreamingCallback, StreamingChunk};
use crate::{ClientError, Result};
use crucible_core::types::acp::{FileDiff, ToolCallInfo};

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
                    let error_msg = error
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("Unknown error");
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

        let streaming_future = async {
            let mut state = StreamingState::default();

            loop {
                let response_line = self.read_response_line().await?;
                let response: serde_json::Value = serde_json::from_str(&response_line)?;

                tracing::trace!("Received line: {}", response_line);

                if let Some(error) = response.get("error") {
                    let error_msg = error
                        .get("message")
                        .and_then(|m| m.as_str())
                        .unwrap_or("Unknown error");
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
    fn apply_session_update_with_callback(
        &mut self,
        notification: SessionNotification,
        state: &mut StreamingState,
        callback: &mut StreamingCallback,
    ) {
        match notification.update {
            SessionUpdate::AgentMessageChunk(chunk) => match chunk.content {
                ContentBlock::Text(text_block) => {
                    // Skip full-text re-sends from agents like cursor-acp that
                    // emit accumulated text as a final notification
                    if state.is_duplicate_resend(&text_block.text) {
                        tracing::debug!(
                            text_len = text_block.text.len(),
                            "Skipping duplicate full-text re-send from agent"
                        );
                        return;
                    }
                    state.append_text(&text_block.text);
                    callback(StreamingChunk::Text(text_block.text));
                }
                other => {
                    tracing::debug!("Ignoring non-text content block: {:?}", other);
                }
            },
            SessionUpdate::ToolCall(tool_call) => {
                let tool_name = humanize_tool_title(&tool_call.title);
                let tool_id = tool_call.tool_call_id.to_string();

                // Emit tool start event
                callback(StreamingChunk::ToolStart {
                    name: tool_name.clone(),
                    id: tool_id.clone(),
                    arguments: tool_call.raw_input.clone(),
                });

                // Record tool call in state
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
                    .collect();
                let mut info = ToolCallInfo::new(tool_call.title.clone())
                    .with_id(tool_id)
                    .with_diffs(diffs);
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
                    callback(StreamingChunk::ToolEnd {
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
                    let title = update
                        .fields
                        .title
                        .clone()
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
                        .collect();

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
                    match serde_json::from_value::<SessionNotification>(params.clone()) {
                        Ok(notification) => {
                            self.apply_session_update(notification, state);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse SessionNotification: {}", e);
                            tracing::debug!("Raw params: {}", params);
                        }
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
                tracing::debug!("Ignoring RPC method: {}", method_name);
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
                    state.append_text(&text_block.text);
                    tracing::trace!(
                        "Accumulated chunk: '{}' (total: {} chars)",
                        text_block.text,
                        state.formatted_length()
                    );
                }
                other => {
                    tracing::debug!("Ignoring non-text content block: {:?}", other);
                }
            },
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
                    .collect();
                let mut info = ToolCallInfo::new(tool_call.title.clone())
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
                        .clone()
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
