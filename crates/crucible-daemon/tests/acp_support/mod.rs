#![allow(unused_imports)]

//! Test support utilities for ACP integration tests
//!
//! This module provides mock agents and test helpers for integration testing.

pub mod mock_agent_bin;
pub mod mock_stdio_agent;
pub mod parity;
pub mod threaded_mock_agent;

pub use mock_agent_bin::{mock_agent_path, mock_handle_params, mock_session_agent};
pub use mock_stdio_agent::{AgentBehavior, MockStdioAgent, MockStdioAgentConfig};
pub use threaded_mock_agent::{MockAgentTransport, ThreadedMockAgent, ThreadedMockAgentHandle};
