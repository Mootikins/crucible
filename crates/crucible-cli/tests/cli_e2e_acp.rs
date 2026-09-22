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

/// Send one message and assert the mock agent's reply reached stdout.
///
/// `session send` exits 0 on `ended: error` and on a closed event channel
/// (see `rpc::send`), so a zero exit proves nothing about the turn. The
/// reply text on stdout and the `[complete]` marker on stderr do.
fn send_and_expect_reply(daemon: &TestDaemon, session_id: &str, prompt: &str, reply: &str) {
    let output = daemon
        .command()
        .args(["session", "send", session_id, prompt])
        .output()
        .expect("run cru session send");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "session send failed: {stderr}");
    assert_eq!(
        stdout.trim(),
        reply,
        "the mock agent's reply must reach stdout; stderr: {stderr}"
    );
    assert!(
        stderr.contains("[complete]") && !stderr.contains("[ended]"),
        "the turn must complete, not end early: {stderr}"
    );
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
fn session_create_accepts_builtin_acp_profiles() {
    let daemon = TestDaemon::start();

    for profile in [
        "claude", "opencode", "gemini", "codex", "cursor", "hermes", "antigravity",
    ] {
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
#[ignore = "requires: mock-acp-agent — built by `just build fixtures`, which the nextest setup script runs"]
fn session_acp_lifecycle_with_mock_agent_profile() {
    let mock_path = mock_agent_path();
    assert!(
        mock_path.exists(),
        "mock-acp-agent binary not found at {}",
        mock_path.display()
    );

    let daemon = TestDaemon::start_with_extra_config(&format!(
        "cru.config.set({{ acp = {{ agents = {{ mock = {{ command = \"{}\", description = \"Mock ACP agent for CLI E2E tests\", env = {{ CRU_MOCK_SCRIPT = '{{\"turn\":[{{\"text\":\"mock reply over stdio\"}}]}}' }} }} }} }} }})\n",
        path_literal(&mock_path)
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

    send_and_expect_reply(
        &daemon,
        &session_id,
        "hello from cli e2e acp test",
        "mock reply over stdio",
    );

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

/// Test 12: Mock agent with HTTP MCP support creates session successfully.
///
/// Validates that an HTTP-capable mock agent can go through the full
/// create → send → end lifecycle when using capability-aware transport.
#[test]
#[ignore = "requires: mock-acp-agent — built by `just build fixtures`, which the nextest setup script runs"]
fn session_acp_lifecycle_with_http_capable_mock() {
    let mock_path = mock_agent_path();
    assert!(
        mock_path.exists(),
        "mock-acp-agent binary not found at {}",
        mock_path.display()
    );

    let daemon = TestDaemon::start_with_extra_config(&format!(
        "cru.config.set({{ acp = {{ agents = {{ [\"mock-http\"] = {{ command = \"{}\", description = \"Mock ACP agent with HTTP MCP support\", env = {{ CRU_MOCK_SCRIPT = '{{\"mcp_http\":true,\"turn\":[{{\"text\":\"mock reply with http mcp\"}}]}}' }} }} }} }} }})\n",
        path_literal(&mock_path)
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

    send_and_expect_reply(
        &daemon,
        &session_id,
        "hello from http-capable mock",
        "mock reply with http mcp",
    );

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
#[ignore = "requires: mock-acp-agent — built by `just build fixtures`, which the nextest setup script runs"]
fn session_acp_lifecycle_with_stdio_only_mock() {
    let mock_path = mock_agent_path();
    assert!(
        mock_path.exists(),
        "mock-acp-agent binary not found at {}",
        mock_path.display()
    );

    // The script sets `mcp_http` to false, so the agent does not advertise HTTP MCP.
    let daemon = TestDaemon::start_with_extra_config(&format!(
        "cru.config.set({{ acp = {{ agents = {{ [\"mock-stdio\"] = {{ command = \"{}\", description = \"Mock ACP agent (stdio only)\", env = {{ CRU_MOCK_SCRIPT = '{{\"mcp_http\":false,\"turn\":[{{\"text\":\"mock reply over stdio only\"}}]}}' }} }} }} }} }})\n",
        path_literal(&mock_path)
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

    send_and_expect_reply(
        &daemon,
        &session_id,
        "hello from stdio-only mock",
        "mock reply over stdio only",
    );

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
