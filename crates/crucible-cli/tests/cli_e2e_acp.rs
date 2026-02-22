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
