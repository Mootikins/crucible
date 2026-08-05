//! Test support utilities for ACP integration tests
//!
//! This module provides mock agents and test helpers for integration testing.
//!
//! This file is `#[path]`-included by several test binaries and each imports a
//! different subset, so every re-export below is unused in at least one of
//! them. The allows are per-item rather than a module-level
//! `#![allow(unused_imports)]` so that a re-export no binary uses at all still
//! has to be removed by hand.

pub mod mock_agent_bin;
pub mod mock_stdio_agent;
pub mod parity;
pub mod threaded_mock_agent;

#[allow(unused_imports)]
pub use mock_agent_bin::{mock_agent_path, mock_handle_params, mock_session_agent};
#[allow(unused_imports)]
pub use mock_stdio_agent::{AgentBehavior, MockStdioAgent, MockStdioAgentConfig};
#[allow(unused_imports)]
pub use threaded_mock_agent::{MockAgentTransport, ThreadedMockAgent, ThreadedMockAgentHandle};
