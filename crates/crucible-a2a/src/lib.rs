pub mod bus;
/// Agent-to-Agent communication and context management
///
/// This crate provides:
/// - A2A protocol and message routing
/// - Context window management with Rune-based pruning strategies
/// - Multi-agent coordination and collaboration tracking
/// - MCP client integration for external tool access
pub mod context;
pub mod mcp_client;
pub mod protocol;
pub mod registry;
pub mod transport;

// Re-export common types
pub use bus::{EntityExtractor, MessageBus};
pub use context::{
    AgentCollaborationGraph, ContextWindow, MessageMetadata, MessageMetadataStore, PruneReason,
    PruningDecision, SummaryRequest,
};
pub use protocol::{MessageEnvelope, SystemEvent, TypedMessage};
pub use transport::{AgentHandle, LocalAgentBus};

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
