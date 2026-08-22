//! # Crucible ACP - Agent Client Protocol Integration
//!
//! Thin protocol adapter for spawning and communicating with ACP-compatible
//! AI agents. Orchestration (history, context, streaming aggregation) lives
//! in `crucible-daemon`; this crate handles only the wire protocol.

// Re-export commonly used types from agent-client-protocol
// Note: agent-client-protocol exports types directly, not in a types module
pub use agent_client_protocol::{
    AgentNotification, AgentRequest, AgentResponse, ClientNotification, ClientRequest,
    ClientResponse, Error as ProtocolError, IncomingMessage, OutgoingMessage,
};

// Module declarations
pub mod client;
pub mod discovery;
pub mod session;
pub mod streaming;
pub mod tools;

// Mock agent for testing (only included in test builds)
#[cfg(any(test, feature = "test-utils"))]
pub mod mock_agent;

// Public exports
pub use client::CrucibleAcpClient;
pub use discovery::{discover_agent, is_agent_available, reset_agent_cache, AgentInfo};
pub use session::{AcpSession, TransportConfig};
pub use streaming::{
    channel_callback, humanize_tool_title, StreamConfig, StreamHandler, StreamingCallback,
    StreamingChunk, ToolCallInfo,
};

// Re-export test utilities when feature is enabled
#[cfg(feature = "test-utils")]
pub use mock_agent::MockAgent;

// Error types
mod error;
pub use error::{ClientError, Result};
