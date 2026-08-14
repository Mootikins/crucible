//! CLI binary E2E tests for ACP session lifecycle and `--agent` handling.
//!
//! These tests validate `cru session create --agent <profile>` behavior at the
//! binary boundary, including help text, built-in profile resolution, unknown
//! profile errors, and a full create -> send -> end lifecycle with a mock ACP
//! agent profile.

mod cli_e2e_helpers;

use cli_e2e_helpers::*;
use predicates::prelude::*;
use std::path::PathBuf;

fn mock_agent_path() -> PathBuf {
    // mock-acp-agent is a crucible-daemon bin, so CARGO_BIN_EXE_mock-acp-agent
    // is never set for this crate's tests. It lands in the same directory as
    // cru, whose CARGO_BIN_EXE_cru is set — resolving relative to it stays
    // correct under a redirected CARGO_TARGET_DIR (shared cargo cache).
    PathBuf::from(env!("CARGO_BIN_EXE_cru")).with_file_name("mock-acp-agent")
}

/// `--agent` is the card and `--acp` is the subprocess, and the help has to say
/// which is which: they were one flag until agent cards became selectable, and
/// `--agent` named the ACP profile then.
#[test]
fn session_create_help_distinguishes_the_card_and_acp_flags() {
    cru()
        .args(["session", "create", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-a, --agent <AGENT>"))
        .stdout(predicate::str::contains("Agent card"))
        .stdout(predicate::str::contains("--acp <ACP>"))
        .stdout(predicate::str::contains("ACP profile"));
}

#[test]
#[ignore = "requires: cru binary"]
fn session_create_rejects_unknown_agent_profile() {
    let daemon = TestDaemon::start();

    daemon
        .command()
        .args(["session", "create", "--acp", "nonexistent-profile"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Unknown ACP agent profile: nonexistent-profile",
        ));
}

#[test]
#[ignore = "requires: cru binary"]
fn session_create_rejects_empty_agent_profile() {
    let daemon = TestDaemon::start();

    daemon
        .command()
        .args(["session", "create", "--acp", ""])
        .assert()
        .failure()
        // An empty `--acp` is a missing name, not an unknown profile — the
        // daemon now says so specifically, and the refusal has to name the
        // parameter or there is nothing to act on.
        .stderr(predicate::str::contains("agent_name is required"));
}

#[test]
#[ignore = "requires: cru binary"]
fn session_create_accepts_builtin_acp_profiles() {
    let daemon = TestDaemon::start();

    for profile in ["claude", "opencode", "gemini", "codex", "cursor"] {
        daemon
            .command()
            .args(["session", "create", "--acp", profile, "--format", "json"])
            .assert()
            .success()
            // The structured field, not the prose line: `Configured agent: …`
            // is printed only on a terminal, and a test harness never is.
            .stdout(predicate::str::contains(format!(r#""acp": "{profile}""#)));
    }
}

#[test]
#[ignore = "requires: cru binary, mock-acp-agent"]
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
        .args(["session", "create", "--acp", "mock", "--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""acp": "mock""#))
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
#[ignore = "requires: cru binary, mock-acp-agent"]
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
        .args([
            "session",
            "create",
            "--acp",
            "mock-http",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""acp": "mock-http""#))
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
#[ignore = "requires: cru binary, mock-acp-agent"]
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
        .args([
            "session",
            "create",
            "--acp",
            "mock-stdio",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""acp": "mock-stdio""#))
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
