//! Test support utilities for ACP integration tests
//!
//! This module provides mock agents and test helpers for integration testing.

pub mod mock_stdio_agent;
pub mod threaded_mock_agent;

pub use mock_stdio_agent::{AgentBehavior, MockStdioAgent, MockStdioAgentConfig};
pub use threaded_mock_agent::{MockAgentTransport, ThreadedMockAgent, ThreadedMockAgentHandle};
