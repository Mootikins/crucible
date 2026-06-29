use std::path::PathBuf;

use super::CrucibleAcpClient;
use crate::acp::{ClientError, Result};

impl CrucibleAcpClient {
    /// Send InitializeRequest to agent
    ///
    /// This performs the first step of the ACP protocol handshake.
    ///
    /// # Arguments
    ///
    /// * `request` - The InitializeRequest to send
    ///
    /// # Returns
    ///
    /// The InitializeResponse from the agent
    ///
    /// # Errors
    ///
    /// Returns an error if initialization fails
    pub async fn initialize(
        &mut self,
        request: agent_client_protocol::InitializeRequest,
    ) -> Result<agent_client_protocol::InitializeResponse> {
        use agent_client_protocol::ClientRequest;

        // Send the initialize request
        let response = self
            .send_request(ClientRequest::InitializeRequest(request))
            .await?;

        // Extract the result field from JSON-RPC response
        let result = response.get("result").ok_or_else(|| {
            ClientError::Session("Missing result field in initialize response".to_string())
        })?;

        // Parse the result as InitializeResponse
        let init_response: agent_client_protocol::InitializeResponse =
            serde_json::from_value(result.clone())?;

        // Store agent MCP capabilities for transport negotiation
        self.agent_mcp_capabilities =
            Some(init_response.agent_capabilities.mcp_capabilities.clone());

        tracing::debug!(
            http_mcp = ?self.agent_mcp_capabilities.as_ref().map(|c| c.http),
            sse_mcp = ?self.agent_mcp_capabilities.as_ref().map(|c| c.sse),
            "Agent MCP capabilities from InitializeResponse"
        );

        tracing::info!(
            agent = %self.agent_name,
            protocol_version = %init_response.protocol_version,
            http_mcp = init_response.agent_capabilities.mcp_capabilities.http,
            sse_mcp = init_response.agent_capabilities.mcp_capabilities.sse,
            load_session = init_response.agent_capabilities.load_session,
            agent_info = ?init_response.agent_info,
            "ACP initialization complete — agent capabilities received"
        );

        Ok(init_response)
    }

    /// Send NewSessionRequest to create a session
    ///
    /// This performs the second step of the ACP protocol handshake.
    ///
    /// # Arguments
    ///
    /// * `request` - The NewSessionRequest to send
    ///
    /// # Returns
    ///
    /// The NewSessionResponse from the agent
    ///
    /// # Errors
    ///
    /// Returns an error if session creation fails
    pub async fn create_new_session(
        &mut self,
        request: agent_client_protocol::NewSessionRequest,
    ) -> Result<agent_client_protocol::NewSessionResponse> {
        use agent_client_protocol::ClientRequest;

        let client_request = ClientRequest::NewSessionRequest(request);
        if let Ok(json) = serde_json::to_string(&client_request) {
            tracing::debug!(agent = %self.agent_name, payload = %json, "session/new request payload");
        }

        let response = self.send_request(client_request).await?;

        let result = response.get("result").ok_or_else(|| {
            tracing::debug!(
                agent = %self.agent_name,
                response = %response,
                "session/new response missing result field"
            );
            ClientError::Session("Missing result field in new session response".to_string())
        })?;

        // Parse the result as NewSessionResponse
        let session_response: agent_client_protocol::NewSessionResponse =
            serde_json::from_value(result.clone())?;

        Ok(session_response)
    }

    /// Send SetSessionModeRequest to change the session mode
    ///
    /// This sends the `session/set_mode` ACP message to the agent.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session ID to set the mode for
    /// * `mode_id` - The mode ID to set (e.g., "normal", "plan", "auto", "ask", "architect", "code")
    ///
    /// # Returns
    ///
    /// The SetSessionModeResponse from the agent
    ///
    /// # Errors
    ///
    /// Returns an error if the mode change fails
    pub async fn set_session_mode(
        &mut self,
        session_id: impl Into<String>,
        mode_id: impl Into<String>,
    ) -> Result<agent_client_protocol::SetSessionModeResponse> {
        use agent_client_protocol::{ClientRequest, SetSessionModeRequest};

        let request = SetSessionModeRequest::new(session_id.into(), mode_id.into());

        let response = self
            .send_request(ClientRequest::SetSessionModeRequest(request))
            .await?;

        // Extract the result field from JSON-RPC response
        let result = response.get("result").ok_or_else(|| {
            ClientError::Session("Missing result field in set mode response".to_string())
        })?;

        // Parse the result as SetSessionModeResponse
        let mode_response: agent_client_protocol::SetSessionModeResponse =
            serde_json::from_value(result.clone())?;

        Ok(mode_response)
    }

    /// Send `session/set_model` to switch the agent's active model.
    ///
    /// Part of ACP's `unstable_session_model` feature — supported by agents
    /// that advertise a model list in their `session/new` response (e.g.
    /// claude-agent-acp). Unlike a provider switch on an internal agent, this
    /// changes the model on the *running* agent process, preserving history.
    pub async fn set_session_model(
        &mut self,
        session_id: impl Into<String>,
        model_id: impl Into<String>,
    ) -> Result<agent_client_protocol::SetSessionModelResponse> {
        use agent_client_protocol::{ClientRequest, SetSessionModelRequest};

        let request = SetSessionModelRequest::new(session_id.into(), model_id.into());

        let response = self
            .send_request(ClientRequest::SetSessionModelRequest(request))
            .await?;

        let result = response.get("result").ok_or_else(|| {
            ClientError::Session("Missing result field in set model response".to_string())
        })?;

        let model_response: agent_client_protocol::SetSessionModelResponse =
            serde_json::from_value(result.clone())?;

        Ok(model_response)
    }

    /// Build a stdio MCP server configuration pointing to `cru mcp`.
    ///
    /// This is the universal fallback — all ACP agents MUST support stdio transport.
    pub(super) fn build_stdio_mcp_server() -> agent_client_protocol::McpServer {
        use agent_client_protocol::{McpServer, McpServerStdio};

        let cru_command = std::env::current_exe()
            .unwrap_or_else(|_| PathBuf::from("cru"))
            .parent()
            .map(|p| p.join("cru"))
            .unwrap_or_else(|| PathBuf::from("cru"));

        McpServer::Stdio(McpServerStdio::new("crucible", cru_command).args(vec![
            "mcp".to_string(),
            "--stdio".to_string(),
            "--standalone".to_string(),
        ]))
    }
}
