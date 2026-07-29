//! Integration tests for ACP client with mock agents
//!
//! These tests use mock stdio agents to verify the complete handshake
//! and communication flow without requiring real agent binaries.

#![allow(unused)]

// Test support utilities
#[path = "../acp_support/mod.rs"]
mod support;

// Test modules
mod agent_handshake_tests;
mod concurrent_sessions;
mod display_parity;
mod error_propagation;
mod mock_agent_framework;
mod permission_flow;
mod streaming_chat;
mod tool_roundtrip;

// Re-export support for use in test modules
pub use support::*;
