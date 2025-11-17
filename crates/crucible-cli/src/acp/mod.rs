//! Agent Client Protocol (ACP) Integration
//!
//! This module provides integration with the Agent Client Protocol,
//! allowing Crucible to interact with external AI agents like claude-code,
//! gemini-cli, and other ACP-compatible agents.

pub mod agent;
pub mod client;
pub mod context;

#[cfg(test)]
mod tests;

pub use agent::{discover_agent, is_agent_available, AgentInfo};
pub use client::CrucibleAcpClient;
pub use context::ContextEnricher;
