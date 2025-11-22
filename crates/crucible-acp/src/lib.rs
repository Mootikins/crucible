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
//! - `client`: Main ACP client implementation for agent communication
//! - `session`: Session management and lifecycle handling
//! - `filesystem`: File operation handlers for agent file access
//! - `protocol`: Message handling utilities and protocol helpers
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
    ClientRequest, ClientResponse, AgentRequest, AgentResponse,
    ClientNotification, AgentNotification,
    IncomingMessage, OutgoingMessage,
    Error as ProtocolError,
};

// Module declarations
pub mod client;
pub mod session;
pub mod filesystem;
pub mod protocol;
pub mod tools;
pub mod context;

// Mock agent for testing (only included in test builds)
#[cfg(any(test, feature = "test-utils"))]
pub mod mock_agent;

// Public exports - Export traits and types, following Dependency Inversion
pub use client::CrucibleAcpClient;
pub use session::{AcpSession, SessionConfig};
pub use filesystem::FileSystemHandler;
pub use protocol::MessageHandler;
pub use tools::{ToolRegistry, ToolDescriptor, ToolExecutor, discover_crucible_tools};
pub use context::{PromptEnricher, ContextConfig};

// Re-export test utilities when feature is enabled
#[cfg(feature = "test-utils")]
pub use mock_agent::MockAgent;

// Error types
mod error;
pub use error::{AcpError, Result};
