//! Agent card module for defining reusable agent configurations
//!
//! Agent cards follow the "Model Card" pattern - they are metadata
//! about agents, not the agents themselves.

pub mod loader;
pub mod types;

pub use loader::AgentCardLoader;
pub use types::{AgentCard, AgentCardFrontmatter, ToolPolicy, ToolPolicyMap};

#[cfg(test)]
mod tests;
