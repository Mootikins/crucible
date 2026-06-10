//! Contract tests for crucible-web HTTP routes.
//!
//! Tests the HTTP API contract (status codes, response shapes, content types)
//! WITHOUT requiring a running daemon. Uses a mock Unix socket daemon to handle
//! JSON-RPC calls from the DaemonClient.

#[path = "route_contract_tests/shared.rs"]
mod shared;

#[path = "route_contract_tests/chat.rs"]
mod chat;
#[path = "route_contract_tests/commands.rs"]
mod commands;
#[path = "route_contract_tests/errors.rs"]
mod errors;
#[path = "route_contract_tests/health.rs"]
mod health;
#[path = "route_contract_tests/kilns.rs"]
mod kilns;
#[path = "route_contract_tests/plugins.rs"]
mod plugins;
#[path = "route_contract_tests/projects.rs"]
mod projects;
#[path = "route_contract_tests/router.rs"]
mod router;
#[path = "route_contract_tests/session_config.rs"]
mod session_config;
#[path = "route_contract_tests/sessions.rs"]
mod sessions;
#[path = "route_contract_tests/skills.rs"]
mod skills;
