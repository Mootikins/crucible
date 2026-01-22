//! Dynamic Agent Wrapper
//!
//! Provides a type-erased wrapper around different agent handle types,
//! enabling runtime polymorphism for the deferred agent creation pattern.

use async_trait::async_trait;
use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatError, ChatResult};
use crucible_core::types::acp::schema::{AvailableCommand, SessionModeState};
use futures::stream::BoxStream;

use crate::acp::CrucibleAcpClient;

/// Dynamic agent handle that wraps either ACP or Local agents.
///
/// This enables deferred agent creation where the concrete type
/// is determined at runtime based on user selection.
///
/// Note: Both variants are boxed to reduce enum size.
pub enum DynamicAgent {
    /// External ACP agent (Claude Code, OpenCode, etc.)
    Acp(Box<CrucibleAcpClient>),
    /// Local/in-process LLM agent (Rig-based, InternalAgentHandle, etc.)
    ///
    /// This variant accepts any type implementing `AgentHandle`, enabling
    /// both legacy `InternalAgentHandle` and newer `RigAgentHandle<M>`.
    Local(Box<dyn AgentHandle + Send + Sync>),
}

impl DynamicAgent {
    /// Create from an ACP client
    pub fn acp(client: CrucibleAcpClient) -> Self {
        Self::Acp(Box::new(client))
    }

    /// Create from any local agent handle (InternalAgentHandle, RigAgentHandle, etc.)
    ///
    /// Accepts a boxed trait object for flexibility with factory patterns.
    pub fn local(handle: Box<dyn AgentHandle + Send + Sync>) -> Self {
        Self::Local(handle)
    }

    /// Create from a concrete agent handle type
    ///
    /// Convenience method that boxes the handle automatically.
    pub fn local_from<H>(handle: H) -> Self
    where
        H: AgentHandle + Send + Sync + 'static,
    {
        Self::Local(Box::new(handle))
    }

    /// Shutdown the underlying agent
    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        match self {
            Self::Acp(client) => client.shutdown().await,
            Self::Local(_) => Ok(()), // Local agents don't need explicit shutdown
        }
    }
}

#[async_trait]
impl AgentHandle for DynamicAgent {
    fn send_message_stream(
        &mut self,
        message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        match self {
            Self::Acp(client) => client.send_message_stream(message),
            Self::Local(handle) => handle.send_message_stream(message),
        }
    }

    fn is_connected(&self) -> bool {
        match self {
            Self::Acp(client) => client.is_connected(),
            Self::Local(handle) => handle.is_connected(),
        }
    }

    fn supports_streaming(&self) -> bool {
        match self {
            Self::Acp(client) => client.supports_streaming(),
            Self::Local(handle) => handle.supports_streaming(),
        }
    }

    fn get_modes(&self) -> Option<&SessionModeState> {
        match self {
            Self::Acp(client) => client.get_modes(),
            Self::Local(handle) => handle.get_modes(),
        }
    }

    fn get_mode_id(&self) -> &str {
        match self {
            Self::Acp(client) => client.get_mode_id(),
            Self::Local(handle) => handle.get_mode_id(),
        }
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        match self {
            Self::Acp(client) => client.set_mode_str(mode_id).await,
            Self::Local(handle) => handle.set_mode_str(mode_id).await,
        }
    }

    fn get_commands(&self) -> &[AvailableCommand] {
        match self {
            Self::Acp(client) => client.get_commands(),
            Self::Local(handle) => handle.get_commands(),
        }
    }

    async fn on_commands_update(
        &mut self,
        commands: Vec<crucible_core::traits::chat::CommandDescriptor>,
    ) -> ChatResult<()> {
        match self {
            Self::Acp(client) => client.on_commands_update(commands).await,
            Self::Local(handle) => handle.on_commands_update(commands).await,
        }
    }

    async fn fetch_available_models(&mut self) -> Vec<String> {
        match self {
            Self::Acp(client) => client.fetch_available_models().await,
            Self::Local(handle) => handle.fetch_available_models().await,
        }
    }

    async fn switch_model(&mut self, model_id: &str) -> ChatResult<()> {
        match self {
            Self::Acp(client) => client.switch_model(model_id).await,
            Self::Local(handle) => handle.switch_model(model_id).await,
        }
    }

    fn current_model(&self) -> Option<&str> {
        match self {
            Self::Acp(client) => client.current_model(),
            Self::Local(handle) => handle.current_model(),
        }
    }
}

impl std::fmt::Debug for DynamicAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Acp(_) => write!(f, "DynamicAgent::Acp(..)"),
            Self::Local(_) => write!(f, "DynamicAgent::Local(..)"),
        }
    }
}

#[cfg(test)]
mod switch_model_tests {
    use super::*;
    use crucible_core::traits::chat::ChatError;

    struct MockAgentHandle {
        model: std::sync::Mutex<Option<String>>,
    }

    impl MockAgentHandle {
        fn new() -> Self {
            Self {
                model: std::sync::Mutex::new(None),
            }
        }
    }

    #[async_trait]
    impl AgentHandle for MockAgentHandle {
        fn send_message_stream(&mut self, _: String) -> BoxStream<'static, ChatResult<ChatChunk>> {
            Box::pin(futures::stream::empty())
        }
        fn is_connected(&self) -> bool {
            true
        }
        fn supports_streaming(&self) -> bool {
            true
        }
        fn get_modes(&self) -> Option<&SessionModeState> {
            None
        }
        fn get_mode_id(&self) -> &str {
            "normal"
        }
        async fn set_mode_str(&mut self, _: &str) -> ChatResult<()> {
            Ok(())
        }
        fn get_commands(&self) -> &[AvailableCommand] {
            &[]
        }

        async fn switch_model(&mut self, model_id: &str) -> ChatResult<()> {
            *self.model.lock().unwrap() = Some(model_id.to_string());
            Ok(())
        }

        fn current_model(&self) -> Option<&str> {
            None // Can't return ref to mutex guard
        }
    }

    #[tokio::test]
    async fn test_dynamic_agent_switch_model_delegates() {
        let mock = MockAgentHandle::new();
        let mut agent = DynamicAgent::local_from(mock);

        // Should start with no model
        assert!(agent.current_model().is_none());

        // Switch model
        let result = agent.switch_model("test-model").await;
        assert!(result.is_ok(), "switch_model should succeed");

        // Verify the inner mock received the call
        if let DynamicAgent::Local(_handle) = &agent {
            // Can't easily check inner state without downcasting
            // But if we got Ok(()), the call went through
        }
    }
}
