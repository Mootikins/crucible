//! Contract tests for crucible-web HTTP routes.
//!
//! Tests the HTTP API contract (status codes, response shapes, content types).
//! A test that needs only normal daemon behavior runs against a real daemon in
//! this process (`start_real_daemon_with_kilns`). A test that needs a failure
//! or a fixed reply shape runs against the mock daemon (`start_mock_daemon`).

#[path = "route_contract_tests/shared.rs"]
mod shared;

#[path = "route_contract_tests/agents.rs"]
mod agents;
#[path = "route_contract_tests/chat.rs"]
mod chat;
#[path = "route_contract_tests/commands.rs"]
mod commands;
#[path = "route_contract_tests/daemon_errors.rs"]
mod daemon_errors;
#[path = "route_contract_tests/diff.rs"]
mod diff;
#[path = "route_contract_tests/errors.rs"]
mod errors;
#[path = "route_contract_tests/fs.rs"]
mod fs;
#[path = "route_contract_tests/health.rs"]
mod health;
#[path = "route_contract_tests/kilns.rs"]
mod kilns;
#[path = "route_contract_tests/mcp.rs"]
mod mcp;
#[path = "route_contract_tests/plugins.rs"]
mod plugins;
#[path = "route_contract_tests/projects.rs"]
mod projects;
#[path = "route_contract_tests/proposals.rs"]
mod proposals;
#[path = "route_contract_tests/router.rs"]
mod router;
#[path = "route_contract_tests/session_config.rs"]
mod session_config;
#[path = "route_contract_tests/sessions.rs"]
mod sessions;
#[path = "route_contract_tests/skills.rs"]
mod skills;
#[path = "route_contract_tests/stream_version.rs"]
mod stream_version;
#[path = "route_contract_tests/surface.rs"]
mod surface;
#[path = "route_contract_tests/system_events.rs"]
mod system_events;
