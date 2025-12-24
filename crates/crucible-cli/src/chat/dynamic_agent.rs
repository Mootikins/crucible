//! Dynamic Agent Wrapper
//!
//! Provides a type-erased wrapper around different agent handle types,
//! enabling runtime polymorphism for the deferred agent creation pattern.

use async_trait::async_trait;
use crucible_agents::InternalAgentHandle;
use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatError, ChatResult};
use crucible_core::types::acp::schema::{AvailableCommand, SessionModeState};
use futures::stream::BoxStream;

use crate::acp::CrucibleAcpClient;

/// Dynamic agent handle that wraps either ACP or Internal agents.
///
/// This enables deferred agent creation where the concrete type
/// is determined at runtime based on user selection.
///
/// Note: Both variants are boxed to reduce enum size.
pub enum DynamicAgent {
    /// External ACP agent (Claude Code, OpenCode, etc.)
    Acp(Box<CrucibleAcpClient>),
    /// Internal LLM agent (Ollama, OpenAI direct)
    Internal(Box<InternalAgentHandle>),
}

impl DynamicAgent {
    /// Create from an ACP client
    pub fn acp(client: CrucibleAcpClient) -> Self {
        Self::Acp(Box::new(client))
    }

    /// Create from an internal handle
    pub fn internal(handle: InternalAgentHandle) -> Self {
        Self::Internal(Box::new(handle))
    }

    /// Shutdown the underlying agent
    pub async fn shutdown(&mut self) -> anyhow::Result<()> {
        match self {
            Self::Acp(client) => client.shutdown().await,
            Self::Internal(_) => Ok(()), // Internal agents don't need explicit shutdown
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
            Self::Internal(handle) => handle.send_message_stream(message),
        }
    }

    fn is_connected(&self) -> bool {
        match self {
            Self::Acp(client) => client.is_connected(),
            Self::Internal(handle) => handle.is_connected(),
        }
    }

    fn supports_streaming(&self) -> bool {
        match self {
            Self::Acp(client) => client.supports_streaming(),
            Self::Internal(handle) => handle.supports_streaming(),
        }
    }

    fn get_modes(&self) -> Option<&SessionModeState> {
        match self {
            Self::Acp(client) => client.get_modes(),
            Self::Internal(handle) => handle.get_modes(),
        }
    }

    fn get_mode_id(&self) -> &str {
        match self {
            Self::Acp(client) => client.get_mode_id(),
            Self::Internal(handle) => handle.get_mode_id(),
        }
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        match self {
            Self::Acp(client) => client.set_mode_str(mode_id).await,
            Self::Internal(handle) => handle.set_mode_str(mode_id).await,
        }
    }

    fn get_commands(&self) -> &[AvailableCommand] {
        match self {
            Self::Acp(client) => client.get_commands(),
            Self::Internal(handle) => handle.get_commands(),
        }
    }

    async fn on_commands_update(
        &mut self,
        commands: Vec<crucible_core::traits::chat::CommandDescriptor>,
    ) -> ChatResult<()> {
        match self {
            Self::Acp(client) => client.on_commands_update(commands).await,
            Self::Internal(handle) => handle.on_commands_update(commands).await,
        }
    }
}

impl std::fmt::Debug for DynamicAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Acp(_) => write!(f, "DynamicAgent::Acp(..)"),
            Self::Internal(_) => write!(f, "DynamicAgent::Internal(..)"),
        }
    }
}
