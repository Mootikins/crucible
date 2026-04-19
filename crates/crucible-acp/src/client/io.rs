use std::sync::atomic::Ordering;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use super::recording::Direction;
use super::{CrucibleAcpClient, REQUEST_ID};
use crate::{ClientError, Result};

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
        // The overall streaming timeout (in send_prompt_with_streaming) provides the actual limit.
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
        request: agent_client_protocol::ClientRequest,
    ) -> Result<serde_json::Value> {
        use serde_json::json;

        // Determine method name and params from ClientRequest
        let (method, params) = match &request {
            agent_client_protocol::ClientRequest::InitializeRequest(req) => {
                ("initialize", serde_json::to_value(req)?)
            }
            agent_client_protocol::ClientRequest::AuthenticateRequest(req) => {
                ("authenticate", serde_json::to_value(req)?)
            }
            agent_client_protocol::ClientRequest::NewSessionRequest(req) => {
                ("session/new", serde_json::to_value(req)?)
            }
            agent_client_protocol::ClientRequest::LoadSessionRequest(req) => {
                ("session/load", serde_json::to_value(req)?)
            }
            agent_client_protocol::ClientRequest::SetSessionModeRequest(req) => {
                ("session/set_mode", serde_json::to_value(req)?)
            }
            agent_client_protocol::ClientRequest::PromptRequest(req) => {
                ("session/prompt", serde_json::to_value(req)?)
            }
            agent_client_protocol::ClientRequest::ExtMethodRequest(req) => {
                ("ext", serde_json::to_value(req)?)
            }
            // Handle any new variants that may be added in future versions
            _ => {
                return Err(ClientError::Session(format!(
                    "Unsupported ClientRequest variant: {:?}",
                    std::any::type_name::<agent_client_protocol::ClientRequest>()
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

        // Read response from agent stdout
        let response_line = self.read_response_line().await?;

        // Parse JSON response
        let response: serde_json::Value = serde_json::from_str(&response_line)?; // Auto-converts to ClientError::Serialization

        Ok(response)
    }
}
