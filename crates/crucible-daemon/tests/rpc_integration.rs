//! Integration tests for DaemonClient with real daemon
//!
//! These tests verify that the client library correctly communicates
//! with a real daemon process.

#[path = "rpc_integration/server.rs"]
mod server;

#[path = "rpc_integration/client.rs"]
mod client;
#[path = "rpc_integration/event_flow.rs"]
mod event_flow;
#[path = "rpc_integration/models.rs"]
mod models;
#[path = "rpc_integration/notes.rs"]
mod notes;
#[path = "rpc_integration/recording.rs"]
mod recording;
#[path = "rpc_integration/scope.rs"]
mod scope;
#[path = "rpc_integration/sessions.rs"]
mod sessions;
#[path = "rpc_integration/tui_flow.rs"]
mod tui_flow;
