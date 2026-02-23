//! CLI binary E2E tests for ACP session lifecycle and `--agent` handling.
//!
//! These tests validate `cru session create --agent <profile>` behavior at the
//! binary boundary, including help text, built-in profile resolution, unknown
//! profile errors, and a full create -> send -> end lifecycle with a mock ACP
//! agent profile.

#[allow(deprecated)]
mod cli_e2e_helpers;

use cli_e2e_helpers::*;
use predicates::prelude::*;
use std::path::PathBuf;

fn mock_agent_path() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_mock-acp-agent") {
        return PathBuf::from(path);
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/mock-acp-agent")
}

#[test]
fn session_create_help_shows_agent_flag() {
    cru()
        .args(["session", "create", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-a, --agent <AGENT>"))
        .stdout(predicate::str::contains("ACP agent profile"));
}

#[test]
#[ignore = "requires daemon"]
fn session_create_rejects_unknown_agent_profile() {
    let daemon = TestDaemon::start();

    daemon
        .command()
        .args(["session", "create", "--agent", "nonexistent-profile"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Unknown ACP agent profile: nonexistent-profile",
        ));
}

#[test]
#[ignore = "requires daemon"]
fn session_create_rejects_empty_agent_profile() {
    let daemon = TestDaemon::start();

    daemon
        .command()
        .args(["session", "create", "--agent", ""])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unknown ACP agent profile"));
}

#[test]
#[ignore = "requires daemon"]
fn session_create_accepts_builtin_acp_profiles() {
    let daemon = TestDaemon::start();

    for profile in ["claude", "opencode", "gemini", "codex", "cursor"] {
        daemon
            .command()
            .args(["session", "create", "--agent", profile])
            .assert()
            .success()
            .stdout(predicate::str::contains(format!(
                "Configured agent: {} (acp)",
                profile
            )));
    }
}

#[test]
#[ignore = "requires daemon and mock-acp-agent binary"]
fn session_acp_lifecycle_with_mock_agent_profile() {
    let mock_path = mock_agent_path();
    assert!(
        mock_path.exists(),
        "mock-acp-agent binary not found at {}",
        mock_path.display()
    );

    let daemon = TestDaemon::start_with_extra_config(&format!(
        "\n[acp.agents.mock]\ncommand = \"{}\"\nargs = []\ndescription = \"Mock ACP agent for CLI E2E tests\"\n",
        toml_escape(&mock_path)
    ));

    let create_output = daemon
        .command()
        .args(["session", "create", "--agent", "mock"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Configured agent: mock (acp)"))
        .get_output()
        .stdout
        .clone();

    let session_id = extract_session_id(&create_output);

    daemon
        .command()
        .args([
            "session",
            "send",
            &session_id,
            "hello from cli e2e acp test",
        ])
        .assert()
        .success();

    daemon
        .command()
        .args(["session", "end", &session_id])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "Ended session: {}",
            session_id
        )));
}

/// Test 12: Mock agent with --mcp-http flag creates session successfully.
///
/// Validates that an HTTP-capable mock agent can go through the full
/// create → send → end lifecycle when using capability-aware transport.
#[test]
#[ignore = "requires daemon and mock-acp-agent binary"]
fn session_acp_lifecycle_with_http_capable_mock() {
    let mock_path = mock_agent_path();
    assert!(
        mock_path.exists(),
        "mock-acp-agent binary not found at {}",
        mock_path.display()
    );

    let daemon = TestDaemon::start_with_extra_config(&format!(
        "\n[acp.agents.mock-http]\ncommand = \"{}\"\nargs = [\"--mcp-http\"]\ndescription = \"Mock ACP agent with HTTP MCP support\"\n",
        toml_escape(&mock_path)
    ));

    let create_output = daemon
        .command()
        .args(["session", "create", "--agent", "mock-http"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Configured agent: mock-http (acp)",
        ))
        .get_output()
        .stdout
        .clone();

    let session_id = extract_session_id(&create_output);

    daemon
        .command()
        .args([
            "session",
            "send",
            &session_id,
            "hello from http-capable mock",
        ])
        .assert()
        .success();

    daemon
        .command()
        .args(["session", "end", &session_id])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "Ended session: {}",
            session_id
        )));
}

/// Test 13: Mock agent without HTTP support (stdio-only) still creates session.
///
/// Validates that a stdio-only mock agent can go through the full lifecycle
/// even when the daemon has an in-process MCP host running.
#[test]
#[ignore = "requires daemon and mock-acp-agent binary"]
fn session_acp_lifecycle_with_stdio_only_mock() {
    let mock_path = mock_agent_path();
    assert!(
        mock_path.exists(),
        "mock-acp-agent binary not found at {}",
        mock_path.display()
    );

    // No --mcp-http flag: agent reports mcp_http=false
    let daemon = TestDaemon::start_with_extra_config(&format!(
        "\n[acp.agents.mock-stdio]\ncommand = \"{}\"\nargs = []\ndescription = \"Mock ACP agent (stdio only)\"\n",
        toml_escape(&mock_path)
    ));

    let create_output = daemon
        .command()
        .args(["session", "create", "--agent", "mock-stdio"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Configured agent: mock-stdio (acp)",
        ))
        .get_output()
        .stdout
        .clone();

    let session_id = extract_session_id(&create_output);

    daemon
        .command()
        .args(["session", "send", &session_id, "hello from stdio-only mock"])
        .assert()
        .success();

    daemon
        .command()
        .args(["session", "end", &session_id])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "Ended session: {}",
            session_id
        )));
}
