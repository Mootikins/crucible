/// Agent-to-Agent communication and context management
///
/// This crate provides:
/// - A2A protocol and message routing
/// - Context window management with Rune-based pruning strategies
/// - Multi-agent coordination and collaboration tracking
/// - MCP client integration for external tool access

pub mod context;
pub mod protocol;
pub mod transport;
pub mod registry;
pub mod mcp_client;

// Re-export common types
pub use context::{
    ContextWindow, MessageMetadata, MessageMetadataStore,
    AgentCollaborationGraph, PruningDecision, SummaryRequest, PruneReason,
};

#[derive(Debug, thiserror::Error)]
pub enum A2AError {
    #[error("Context error: {0}")]
    ContextError(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Transport error: {0}")]
    TransportError(String),

    #[error("Strategy error: {0}")]
    StrategyError(String),
}

pub type Result<T> = std::result::Result<T, A2AError>;
