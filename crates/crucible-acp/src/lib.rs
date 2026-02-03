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
pub mod acp_client;
pub mod client;
pub mod discovery;
pub mod filesystem;
pub mod mcp_host;
pub mod protocol;
pub mod session;
pub mod streaming;
pub mod tools;
pub mod tracing_utils;

// Mock agent for testing (only included in test builds)
#[cfg(any(test, feature = "test-utils"))]
pub mod mock_agent;

// Public exports
pub use acp_client::{CrucibleClient, WriteInfo};
pub use client::CrucibleAcpClient;
pub use discovery::{
    clear_agent_cache, discover_agent, get_agent_help, get_known_agents, is_agent_available,
    probe_all_agents, AgentInfo, KnownAgent,
};
pub use filesystem::FileSystemHandler;
pub use mcp_host::InProcessMcpHost;
pub use protocol::MessageHandler;
pub use session::{AcpSession, TransportConfig};
pub use streaming::{
    channel_callback, humanize_tool_title, StreamConfig, StreamHandler, StreamingCallback,
    StreamingChunk, ToolCallInfo,
};
pub use tools::get_crucible_system_prompt;
pub use tracing_utils::{LogCapture, TraceContext};

// Re-export test utilities when feature is enabled
#[cfg(feature = "test-utils")]
pub use mock_agent::MockAgent;
#[cfg(any(test, feature = "test-utils"))]
pub use tracing_utils::{create_test_subscriber, init_test_subscriber, CapturedLog};

// Error types
mod error;
pub use error::{ClientError, Result};
