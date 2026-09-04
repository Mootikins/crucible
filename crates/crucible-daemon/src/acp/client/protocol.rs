use std::path::PathBuf;

use super::CrucibleAcpClient;
use crate::acp::{ClientError, Result};

/// True when a JSON-RPC reply carries error code `-32601` (method not
/// found). The lifecycle methods treat that reply as "the agent does not
/// speak this method", not as a failure.
fn is_method_not_found(response: &serde_json::Value) -> bool {
    response
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(serde_json::Value::as_i64)
        == Some(-32601)
}

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
        request: agent_client_protocol::schema::v1::InitializeRequest,
    ) -> Result<agent_client_protocol::schema::v1::InitializeResponse> {
        use agent_client_protocol::schema::v1::ClientRequest;

        // Send the initialize request
        let response = self
            .send_request(ClientRequest::InitializeRequest(request))
            .await?;

        // Extract the result field from JSON-RPC response
        let result = response.get("result").ok_or_else(|| {
            ClientError::Session("Missing result field in initialize response".to_string())
        })?;

        // Parse the result as InitializeResponse
        let init_response: agent_client_protocol::schema::v1::InitializeResponse =
            serde_json::from_value(result.clone())?;

        // Store agent MCP capabilities for transport negotiation
        self.agent_mcp_capabilities =
            Some(init_response.agent_capabilities.mcp_capabilities.clone());

        // Remember whether the agent takes `session/close`, so shutdown
        // knows to say goodbye before it kills the process.
        self.session_close_supported = init_response
            .agent_capabilities
            .session_capabilities
            .close
            .is_some();

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
        request: agent_client_protocol::schema::v1::NewSessionRequest,
    ) -> Result<agent_client_protocol::schema::v1::NewSessionResponse> {
        use agent_client_protocol::schema::v1::ClientRequest;

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
        let session_response: agent_client_protocol::schema::v1::NewSessionResponse =
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
    /// * `mode_id` - The mode ID to set (e.g., "ask", "plan", "auto", "architect", "code")
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
    ) -> Result<agent_client_protocol::schema::v1::SetSessionModeResponse> {
        use agent_client_protocol::schema::v1::{ClientRequest, SetSessionModeRequest};

        let request = SetSessionModeRequest::new(session_id.into(), mode_id.into());

        let response = self
            .send_request(ClientRequest::SetSessionModeRequest(request))
            .await?;

        // Extract the result field from JSON-RPC response
        let result = response.get("result").ok_or_else(|| {
            ClientError::Session("Missing result field in set mode response".to_string())
        })?;

        // Parse the result as SetSessionModeResponse
        let mode_response: agent_client_protocol::schema::v1::SetSessionModeResponse =
            serde_json::from_value(result.clone())?;

        Ok(mode_response)
    }

    /// Send `session/set_config_option` to change one session config option.
    ///
    /// The model selector is a config option (see `ModelChoice`), so a model
    /// switch goes through this call. The agent answers with the full list
    /// of options and their current values.
    pub async fn set_config_option(
        &mut self,
        session_id: impl Into<String>,
        config_id: impl Into<String>,
        value: impl Into<agent_client_protocol::schema::v1::SessionConfigOptionValue>,
    ) -> Result<agent_client_protocol::schema::v1::SetSessionConfigOptionResponse> {
        use agent_client_protocol::schema::v1::{
            ClientRequest, SessionConfigId, SessionId, SetSessionConfigOptionRequest,
        };

        let request = SetSessionConfigOptionRequest::new(
            SessionId::from(session_id.into()),
            SessionConfigId::new(config_id.into()),
            value,
        );

        let response = self
            .send_request(ClientRequest::SetSessionConfigOptionRequest(request))
            .await?;

        let result = response.get("result").ok_or_else(|| {
            ClientError::Session("Missing result field in set config option response".to_string())
        })?;

        Ok(serde_json::from_value(result.clone())?)
    }

    /// Send `session/resume` to continue an existing agent session.
    ///
    /// Returns `Ok(None)` when the agent answers `-32601`: the agent does
    /// not speak the method, and the caller falls back to `session/new`.
    /// Any other error reply is reported to the caller.
    pub async fn resume_session(
        &mut self,
        request: agent_client_protocol::schema::v1::ResumeSessionRequest,
    ) -> Result<Option<agent_client_protocol::schema::v1::ResumeSessionResponse>> {
        use agent_client_protocol::schema::v1::ClientRequest;

        let response = self
            .send_request(ClientRequest::ResumeSessionRequest(request))
            .await?;

        if is_method_not_found(&response) {
            tracing::info!(
                agent = %self.agent_name,
                "agent has no session/resume; the caller falls back to session/new"
            );
            return Ok(None);
        }

        let result = response
            .get("result")
            .ok_or_else(|| ClientError::Session(format!("session/resume failed: {response}")))?;
        Ok(Some(serde_json::from_value(result.clone())?))
    }

    /// Send `session/close` so the agent frees the session's resources.
    ///
    /// A `-32601` reply is tolerated: an agent without the method (Hermes)
    /// still exits on the pipe close that follows. Any other error reply is
    /// reported to the caller.
    pub async fn close_session(&mut self, session_id: impl Into<String>) -> Result<()> {
        use agent_client_protocol::schema::v1::{ClientRequest, CloseSessionRequest, SessionId};

        let request = CloseSessionRequest::new(SessionId::from(session_id.into()));
        let response = self
            .send_request(ClientRequest::CloseSessionRequest(request))
            .await?;

        if is_method_not_found(&response) {
            tracing::debug!(
                agent = %self.agent_name,
                "agent has no session/close; shutdown continues without it"
            );
            return Ok(());
        }

        response
            .get("result")
            .ok_or_else(|| ClientError::Session(format!("session/close failed: {response}")))?;
        Ok(())
    }

    /// Build a stdio MCP server configuration pointing to `cru mcp`.
    ///
    /// This is the universal fallback — all ACP agents MUST support stdio transport.
    pub(super) fn build_stdio_mcp_server() -> agent_client_protocol::schema::v1::McpServer {
        use agent_client_protocol::schema::v1::{McpServer, McpServerStdio};

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
