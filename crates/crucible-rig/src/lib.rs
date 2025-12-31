//! Crucible Rig Integration
//!
//! This crate provides integration between Crucible and the Rig LLM framework,
//! enabling use of Rig's agent abstractions with Crucible's session management.
//!
//! ## Architecture
//!
//! - **providers**: Factory functions to create Rig clients from Crucible config
//! - **agent**: Agent builder from AgentCard configuration
//! - **session**: Session state types, formatting, and I/O
//!
//! ## Example
//!
//! ```rust,ignore
//! use crucible_rig::{create_client, build_agent};
//! use crucible_config::llm::{LlmProviderConfig, LlmProviderType};
//! use crucible_core::agent::AgentCard;
//!
//! // Create a client from config
//! let config = LlmProviderConfig {
//!     provider_type: LlmProviderType::Ollama,
//!     endpoint: Some("http://localhost:11434".into()),
//!     default_model: Some("llama3.2".into()),
//!     ..Default::default()
//! };
//! let client = create_client(&config)?;
//!
//! // Build an agent from a card
//! let card = load_agent_card("agents/assistant.md")?;
//! let agent = build_agent(&card, client.as_ollama().unwrap())?;
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod agent;
pub mod completion;
pub mod crucible_agent;
pub mod handle;
pub mod providers;
pub mod session;
pub mod tools;
pub mod workspace_tools;

// Re-export key types
pub use agent::{
    build_agent, build_agent_from_config, build_agent_with_model_size, build_agent_with_tools,
    AgentBuildError, AgentConfig,
};
pub use completion::RigCompletionBackend;
pub use crucible_agent::{CrucibleAgent, CrucibleAgentError, CrucibleAgentResult};
pub use handle::RigAgentHandle;
pub use providers::{create_client, RigClient, RigError, RigResult};
pub use session::{
    LoggerError, LoggerResult, MessageRole, SessionEntry, SessionIndex, SessionLogger,
    SessionMessage, SessionMetadata, SessionState, Task, TaskStatus,
};

// Re-export tool utilities when rmcp-full feature is enabled
#[cfg(feature = "rmcp-full")]
pub use tools::{attach_mcp_tools, discover_crucible_tools, McpToolError, McpToolResult};

// Re-export workspace tools (always available)
pub use workspace_tools::{
    BashTool, EditFileTool, GlobTool, GrepTool, ReadFileTool, WorkspaceContext, WorkspaceToolError,
    WriteFileTool,
};
