//! Mock Agent Client Protocol (ACP) agent
//!
//! This crate provides a configurable mock ACP agent for testing purposes
//! and as a foundation for building real Crucible agents.
//!
//! # Features
//!
//! - Full ACP protocol implementation (initialize, session/new, session/prompt)
//! - Streaming responses via `session/update` notifications
//! - Configurable behaviors (OpenCode, Claude, streaming, etc.)
//! - Error injection for testing error handling
//! - Both library and binary usage
//!
//! # Example
//!
//! ```no_run
//! use crucible_mock_agent::{MockAgent, MockAgentConfig};
//!
//! let config = MockAgentConfig::streaming();
//! let mut agent = MockAgent::new(config);
//! agent.run().expect("Agent failed");
//! ```
//!
//! # Binary Usage
//!
//! The crate also provides a `crucible-mock-agent` binary that can be spawned by tests:
//!
//! ```bash
//! crucible-mock-agent --behavior streaming
//! crucible-mock-agent --behavior streaming-slow
//! ```

mod agent;
mod behaviors;
mod streaming;

// Re-export public API
pub use agent::{MockAgent, MockAgentConfig};
pub use behaviors::AgentBehavior;
