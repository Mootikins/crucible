//! # Crucible ACP - Agent Client Protocol Integration
//!
//! This crate provides integration with the Agent Client Protocol (ACP), enabling
//! Crucible to communicate with AI agents following the Model Context Protocol (MCP)
//! standard. It implements the client-side of the protocol for agent orchestration.
//!
//! ## Architecture
//!
//! This crate follows SOLID principles:
//!
//! - **Single Responsibility**: Each module handles one aspect of ACP integration
//! - **Open/Closed**: Extensible through traits defined in `crucible-core`
//! - **Liskov Substitution**: Implements core traits without breaking contracts
//! - **Interface Segregation**: Focused, specific traits for each capability
//! - **Dependency Inversion**: Depends on `crucible-core` abstractions, not implementations
//!
//! ## Module Organization
//!
//! - `acp_client`: ACP client implementation (Crucible as IDE, spawns agents)
//! - `client`: Legacy client implementation (will be replaced)
//! - `session`: Session management and lifecycle handling
//! - `chat`: Interactive chat interface with history and context enrichment
//! - `context`: Prompt enrichment with semantic search
//! - `streaming`: Response streaming and formatting
//! - `history`: Conversation history management
//! - `filesystem`: File operation handlers for agent file access
//! - `protocol`: Message handling utilities and protocol helpers
//! - `tools`: Tool discovery and execution
//! - `mock_agent`: Mock agent implementation for testing (test-only)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_acp::{CrucibleAcpClient, AcpSession};
//!
//! // Create a client and connect to an agent
//! let client = CrucibleAcpClient::new(agent_path, config)?;
//! let session = client.connect().await?;
//!
//! // Use the session to interact with the agent
//! let response = session.send_message(request).await?;
//! ```

// Re-export commonly used types from agent-client-protocol
// Note: agent-client-protocol exports types directly, not in a types module
pub use agent_client_protocol::{
    AgentNotification, AgentRequest, AgentResponse, ClientNotification, ClientRequest,
    ClientResponse, Error as ProtocolError, IncomingMessage, OutgoingMessage,
};

// Module declarations
pub mod acp_client;
pub mod chat;
pub mod client;
pub mod context;
pub mod discovery;
pub mod filesystem;
pub mod history;
pub mod mcp_host;
pub mod protocol;
pub mod session;
pub mod streaming;
pub mod tools;
pub mod tracing_utils;

// Mock agent for testing (only included in test builds)
#[cfg(any(test, feature = "test-utils"))]
pub mod mock_agent;

// Public exports - Export traits and types, following Dependency Inversion
pub use acp_client::{CrucibleClient, WriteInfo};
pub use chat::{ChatConfig, ChatSession, ConversationState, SessionMetadata};
pub use client::CrucibleAcpClient; // Legacy, will be removed
pub use context::{ContextConfig, PromptEnricher};
pub use discovery::{clear_agent_cache, discover_agent, is_agent_available, AgentInfo};
pub use filesystem::FileSystemHandler;
pub use history::{ConversationHistory, HistoryConfig, HistoryMessage, MessageRole};
pub use mcp_host::InProcessMcpHost;
pub use protocol::MessageHandler;
pub use session::{AcpSession, SessionConfig};
pub use streaming::{humanize_tool_title, StreamConfig, StreamHandler, ToolCallInfo};
pub use tools::{
    discover_crucible_tools, get_crucible_system_prompt, ToolDescriptor, ToolExecutor, ToolRegistry,
};
pub use tracing_utils::{LogCapture, TraceContext};

// Re-export test utilities when feature is enabled
#[cfg(feature = "test-utils")]
pub use mock_agent::MockAgent;
#[cfg(any(test, feature = "test-utils"))]
pub use tracing_utils::{create_test_subscriber, init_test_subscriber, CapturedLog};

// Error types
mod error;
pub use error::{AcpError, Result};
