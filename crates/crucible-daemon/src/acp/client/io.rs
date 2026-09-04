use std::sync::atomic::Ordering;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use super::recording::Direction;
use super::{CrucibleAcpClient, REQUEST_ID};
use crate::acp::{ClientError, Result};

/// How many frames a handshake call reads before it concludes the agent is
/// never going to answer. Each read is separately bounded by the per-read
/// timeout; this bounds a chatty agent that keeps the stream busy without
/// ever responding.
const MAX_FRAMES_BEFORE_RESPONSE: usize = 256;

impl CrucibleAcpClient {
    /// Send a message to the agent
    ///
    /// # Arguments
    ///
    /// * `message` - The JSON-RPC message to send
    ///
    /// # Returns
    ///
    /// The agent's response as a JSON value
    ///
    /// # Errors
    ///
    /// Returns an error if message sending fails or times out
    pub async fn send_message(&mut self, message: serde_json::Value) -> Result<serde_json::Value> {
        // Write the message to agent stdin
        self.write_request(&message).await?;

        // Read the response from agent stdout
        let response_line = self.read_response_line().await?;

        // Parse and return the response
        let response: serde_json::Value = serde_json::from_str(&response_line)?;
        Ok(response)
    }

    /// Write a reply to a request the *agent* sent us.
    ///
    /// Unlike [`Self::write_request`], a missing transport is not an error: the
    /// agent is already gone, so there is nobody left to answer.
    ///
    /// The guard checks both writers — an earlier version checked only
    /// `agent_stdin` and silently dropped every reply on in-process transports.
    pub(super) async fn write_agent_response(&mut self, payload: serde_json::Value) -> Result<()> {
        if self.agent_stdin.is_none() && self.boxed_writer.is_none() {
            tracing::warn!("Agent transport unavailable; cannot send response");
            return Ok(());
        }
        self.write_request(&payload).await
    }

    /// Write a JSON request to the agent's stdin
    ///
    /// # Arguments
    ///
    /// * `request` - The JSON value to write
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails or stdin is not available
    pub async fn write_request(&mut self, request: &serde_json::Value) -> Result<()> {
        // Serialize to JSON and add newline
        let json_str = serde_json::to_string(request)?;
        let line = format!("{}\n", json_str);

        if let Some(rec) = self.recorder.as_mut() {
            rec.record_line(Direction::Out, &json_str);
        }

        // Try boxed writer first (for in-process transports), then fall back to agent_stdin
        if let Some(ref mut writer) = self.boxed_writer {
            writer.write_all(line.as_bytes()).await.map_err(|e| {
                ClientError::Connection(format!("Failed to write to transport: {}", e))
            })?;
            writer.flush().await.map_err(|e| {
                ClientError::Connection(format!("Failed to flush transport: {}", e))
            })?;
        } else if let Some(ref mut stdin) = self.agent_stdin {
            stdin.write_all(line.as_bytes()).await.map_err(|e| {
                ClientError::Connection(format!("Failed to write to agent stdin: {}", e))
            })?;
            stdin.flush().await.map_err(|e| {
                ClientError::Connection(format!("Failed to flush agent stdin: {}", e))
            })?;
        } else {
            return Err(ClientError::Connection(
                "No writer available (agent stdin or transport)".to_string(),
            ));
        }

        Ok(())
    }

    /// Read a single line response from the agent's stdout
    ///
    /// # Returns
    ///
    /// The line read from stdout (without trailing newline)
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails, stdout is not available, or timeout occurs
    pub async fn read_response_line(&mut self) -> Result<String> {
        let mut line = String::new();

        // Read with a generous per-read timeout.
        // Agents may pause for extended periods during tool execution or deep reasoning.
        // Use 5 minutes per-read minimum, or match the overall streaming timeout if configured.
        // The overall streaming timeout (in send_prompt_with_callback) provides the actual limit.
        let per_read_timeout_ms = self
            .config
            .timeout_ms
            .map(|ms| ms.max(300_000)) // At least 5 minutes per read
            .unwrap_or(300_000); // Default 5 minutes
        let duration = tokio::time::Duration::from_millis(per_read_timeout_ms);

        // Try boxed reader first (for in-process transports), then fall back to agent_stdout
        let read_result = if let Some(ref mut reader) = self.boxed_reader {
            match tokio::time::timeout(duration, reader.read_line(&mut line)).await {
                Ok(result) => result,
                Err(_) => return Err(ClientError::Timeout("Read operation timed out".to_string())),
            }
        } else if let Some(ref mut stdout) = self.agent_stdout {
            match tokio::time::timeout(duration, stdout.read_line(&mut line)).await {
                Ok(result) => result,
                Err(_) => return Err(ClientError::Timeout("Read operation timed out".to_string())),
            }
        } else {
            return Err(ClientError::Connection(
                "No reader available (agent stdout or transport)".to_string(),
            ));
        };

        // Handle read result
        match read_result {
            Ok(0) => Err(ClientError::Connection(
                "Agent closed connection".to_string(),
            )),
            Ok(_bytes_read) => {
                let trimmed = line.trim_end().to_string();
                if let Some(rec) = self.recorder.as_mut() {
                    rec.record_line(Direction::In, &trimmed);
                }
                Ok(trimmed)
            }
            Err(e) => Err(ClientError::Connection(format!(
                "Failed to read from agent: {}",
                e
            ))),
        }
    }

    /// Send an ACP protocol request and wait for response
    ///
    /// # Arguments
    ///
    /// * `request` - The ClientRequest to send
    ///
    /// # Returns
    ///
    /// The response as a JSON value
    ///
    /// # Errors
    ///
    /// Returns an error if communication fails
    pub async fn send_request(
        &mut self,
        request: agent_client_protocol::schema::v1::ClientRequest,
    ) -> Result<serde_json::Value> {
        use serde_json::json;

        // The SDK implements `JsonRpcMessage::method()` on this enum
        // (agent-client-protocol-2.0.0, src/schema/enum_impls.rs). We keep
        // this table because the two disagree on one arm: for
        // `ExtMethodRequest` the accessor returns the inner method name,
        // and this table sends the literal "ext". The tests pin this wire.
        let (method, params) = match &request {
            agent_client_protocol::schema::v1::ClientRequest::InitializeRequest(req) => {
                ("initialize", serde_json::to_value(req)?)
            }
            agent_client_protocol::schema::v1::ClientRequest::AuthenticateRequest(req) => {
                ("authenticate", serde_json::to_value(req)?)
            }
            agent_client_protocol::schema::v1::ClientRequest::NewSessionRequest(req) => {
                ("session/new", serde_json::to_value(req)?)
            }
            agent_client_protocol::schema::v1::ClientRequest::LoadSessionRequest(req) => {
                ("session/load", serde_json::to_value(req)?)
            }
            agent_client_protocol::schema::v1::ClientRequest::ResumeSessionRequest(req) => {
                ("session/resume", serde_json::to_value(req)?)
            }
            agent_client_protocol::schema::v1::ClientRequest::CloseSessionRequest(req) => {
                ("session/close", serde_json::to_value(req)?)
            }
            agent_client_protocol::schema::v1::ClientRequest::SetSessionModeRequest(req) => {
                ("session/set_mode", serde_json::to_value(req)?)
            }
            agent_client_protocol::schema::v1::ClientRequest::SetSessionConfigOptionRequest(
                req,
            ) => ("session/set_config_option", serde_json::to_value(req)?),
            agent_client_protocol::schema::v1::ClientRequest::PromptRequest(req) => {
                ("session/prompt", serde_json::to_value(req)?)
            }
            agent_client_protocol::schema::v1::ClientRequest::ExtMethodRequest(req) => {
                ("ext", serde_json::to_value(req)?)
            }
            // Handle any new variants that may be added in future versions
            _ => {
                return Err(ClientError::Session(format!(
                    "Unsupported ClientRequest variant: {:?}",
                    std::any::type_name::<agent_client_protocol::schema::v1::ClientRequest>()
                )))
            }
        };

        // Generate a unique request ID using the global counter
        let id = REQUEST_ID.fetch_add(1, Ordering::SeqCst);

        // Wrap in JSON-RPC 2.0 format
        let json_request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        // Write to agent stdin
        self.write_request(&json_request).await?;

        self.read_response_for(id, method).await
    }

    /// Read frames until the one answering `id` arrives.
    ///
    /// The agent shares one stream between its answers and its own traffic, so
    /// the next line is not necessarily the response. codex-acp announces MCP
    /// server startup as a `session/update` *before* it answers `session/new`;
    /// treating that first line as the response reports "missing result field"
    /// and the session never opens.
    ///
    /// Four frame kinds arrive here, and only the first ends the wait:
    ///
    /// * a response carrying `id` — the answer,
    /// * a notification (no `id`) — the agent talking, skipped; the streaming
    ///   loop is what interprets those, and it is not running during a
    ///   handshake call,
    /// * an inbound request (an `id` we did not mint, plus a `method`) —
    ///   answered `-32601`, because the agent blocks on a reply that never
    ///   comes otherwise,
    /// * a response carrying another `id` — a straggler from an exchange that
    ///   already timed out, skipped. Returning it would hand the caller a
    ///   different request's payload.
    async fn read_response_for(&mut self, id: u64, method: &str) -> Result<serde_json::Value> {
        for _ in 0..MAX_FRAMES_BEFORE_RESPONSE {
            let line = self.read_response_line().await?;
            let frame: serde_json::Value = serde_json::from_str(&line)?;

            // Classify by `method`, not by whether a result is present. A
            // frame with no `method` is a response, and a malformed response
            // carrying our id is still ours — skipping it would turn "the
            // agent answered with nonsense" into a read timeout, which tells
            // the caller far less. Validating the payload is the caller's
            // job; correlating it is this one's.
            let frame_id = frame.get("id");
            match (frame.get("method"), frame_id) {
                // An inbound request. The agent blocks until we answer.
                (Some(inbound), Some(fid)) => {
                    let inbound = inbound.as_str().unwrap_or_default().to_string();
                    let fid = fid.clone();
                    tracing::debug!(
                        agent = %self.agent_name,
                        awaiting = method,
                        inbound = %inbound,
                        "answering an inbound request that arrived mid-call"
                    );
                    self.respond_method_not_found(&fid, &inbound).await?;
                }

                // A notification. The streaming loop interprets those, and it
                // is not running during a handshake call.
                (Some(_), None) => {
                    tracing::debug!(
                        agent = %self.agent_name,
                        awaiting = method,
                        "skipping a notification that arrived mid-call"
                    );
                }

                // A response. Ours if the id matches.
                (None, Some(fid)) if fid.as_u64() == Some(id) => return Ok(frame),

                // A straggler from an exchange that already gave up. Returning
                // it would hand the caller a different request's payload.
                (None, frame_id) => {
                    tracing::debug!(
                        agent = %self.agent_name,
                        awaiting = method,
                        frame_id = ?frame_id,
                        "skipping a response that answers a different request"
                    );
                }
            }
        }

        Err(ClientError::Session(format!(
            "agent sent {MAX_FRAMES_BEFORE_RESPONSE} frames without answering `{method}`"
        )))
    }
}
