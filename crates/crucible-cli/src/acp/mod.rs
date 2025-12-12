//! Agent Client Protocol (ACP) Integration
//!
//! This module provides integration with the Agent Client Protocol,
//! allowing Crucible to interact with external AI agents like claude-code,
//! gemini-cli, and other ACP-compatible agents.

pub mod client;
pub mod context;

#[cfg(test)]
mod tests;

// Re-export agent discovery from crucible-acp
pub use client::CrucibleAcpClient;
pub use context::{ContextEnricher, EnrichmentResult};
pub use crucible_acp::{discover_agent, is_agent_available, AgentInfo};
