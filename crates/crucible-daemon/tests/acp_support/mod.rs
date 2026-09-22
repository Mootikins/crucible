//! Test support utilities for ACP integration tests
//!
//! This module provides the mock agent and test helpers for integration testing.
//!
//! This file is `#[path]`-included by several test binaries and each imports a
//! different subset, so every re-export below is unused in at least one of
//! them. The allows are per-item rather than a module-level
//! `#![allow(unused_imports)]` so that a re-export no binary uses at all still
//! has to be removed by hand.

pub mod mcp_http;
pub mod mock_agent;
pub mod mock_agent_bin;
pub mod parity;

#[allow(unused_imports)]
pub use mock_agent::{connect, logged, prompt_with, read_log, MockScript, Resume, Step};
#[allow(unused_imports)]
pub use mock_agent_bin::{
    mock_agent_path, mock_handle_params, mock_path_acp_config, mock_session_agent,
};
