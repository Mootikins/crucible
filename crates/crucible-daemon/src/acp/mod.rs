//! # Crucible ACP - Agent Client Protocol Integration
//!
//! Thin protocol adapter for spawning and communicating with ACP-compatible
//! AI agents. Orchestration (history, context, streaming aggregation) lives
//! in `crucible-daemon`; this crate handles only the wire protocol.

// Module declarations
pub mod client;
pub mod discovery;
pub mod session;
pub mod streaming;

// Public exports
pub use client::CrucibleAcpClient;
#[cfg(any(test, feature = "test-utils"))]
pub use discovery::reset_agent_cache;
pub use discovery::{discover_agent, is_agent_available, AgentInfo};
pub use session::{AcpSession, TransportConfig};
pub use streaming::{humanize_tool_title, turn_usage, StreamingChunk, TurnSummary};

// Error types
mod error;
pub use error::{ClientError, Result};
