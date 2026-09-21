//! Integration tests for ACP client with mock agents
//!
//! These tests use mock stdio agents to verify the complete handshake
//! and communication flow without requiring real agent binaries.

#![allow(unused)]

// Test support utilities
#[path = "../acp_support/mod.rs"]
mod support;

// The hand-scripted agent lives beside the other ACP support code, but only
// this binary drives it, so only this binary compiles it.
#[path = "../acp_support/scripted_agent.rs"]
mod scripted_agent;

// Test modules
mod agent_handshake_tests;
mod concurrent_sessions;
mod context_usage;
mod display_parity;
mod error_propagation;
mod inbound_requests;
mod interleaved_frames;
mod mcp_server_frame;
mod permission_flow;
mod session_modes;
mod streaming_chat;
mod tool_roundtrip;
mod turn_event_parity;

// Consolidated from former tests/acp_mcp_integration.rs +
// tests/acp_in_process_mcp.rs (T12: MCP integration test consolidation)
mod mcp_integration;

// Re-export support for use in test modules
pub use support::*;
