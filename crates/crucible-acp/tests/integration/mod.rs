//! Integration tests for ACP client with mock agents
//!
//! These tests use mock stdio agents to verify the complete handshake
//! and communication flow without requiring real agent binaries.

#![allow(unused)]

// Test support utilities
#[path = "../support/mod.rs"]
mod support;

// Test modules
mod claude_acp_integration;
mod concurrent_sessions;
mod error_propagation;
mod mock_agent_framework;
mod opencode_integration;
mod opencode_streaming;
mod streaming_chat;

// Re-export support for use in test modules
pub use support::*;
