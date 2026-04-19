use async_trait::async_trait;

use super::CrucibleAcpClient;
use crucible_core::traits::acp::{AcpResult, SessionManager};
use crucible_core::types::acp::{SessionConfig, SessionId};

#[async_trait]
impl SessionManager for CrucibleAcpClient {
    type Session = SessionId;
    type Config = SessionConfig;

    async fn create_session(&mut self, config: Self::Config) -> AcpResult<Self::Session> {
        // For now, we create a session ID and track it internally
        // Full agent connection will be implemented in later cycles

        // Generate a new session ID
        let session_id = SessionId::new();

        // Store session configuration in metadata
        let mut metadata = config.metadata.clone();
        metadata.insert(
            "cwd".to_string(),
            serde_json::json!(config.cwd.to_string_lossy()),
        );
        metadata.insert(
            "mode".to_string(),
            serde_json::json!(config.mode_id.as_str()),
        );

        // Track as active session
        self.active_session = Some(session_id.clone());

        Ok(session_id)
    }

    async fn load_session(&mut self, session: Self::Session) -> AcpResult<()> {
        // For now, just set it as active (actual restoration comes later)
        self.active_session = Some(session);
        Ok(())
    }

    async fn end_session(&mut self, session: Self::Session) -> AcpResult<()> {
        if self.active_session.as_ref() == Some(&session) {
            self.active_session = None;
        }
        Ok(())
    }
}
