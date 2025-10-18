/// Transport layer error types

use crate::context::types::AgentId;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("Agent not found: {agent_id}")]
    AgentNotFound { agent_id: AgentId },

    #[error("Agent channel closed: {agent_id}")]
    ChannelClosed { agent_id: AgentId },

    #[error("Failed to send message: {0}")]
    SendFailed(String),

    #[error("Failed to receive message: {0}")]
    RecvFailed(String),

    #[error("Broadcast failed: {0}")]
    BroadcastFailed(String),

    #[error("Timeout after {duration:?}")]
    Timeout { duration: Duration },

    #[error("Agent already registered: {agent_id}")]
    AlreadyRegistered { agent_id: AgentId },

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
