//! Conformance of the real third-party ACP agents Crucible ships profiles for.
//!
//! Every other ACP test in this crate drives a mock process, an in-process
//! duplex pipe, or a recorded capture. Those prove Crucible's half of the
//! protocol. None of them can prove that the command a built-in profile names
//! still exists, or that the agent behind it still speaks the protocol
//! Crucible expects. A profile pointing at an uninstalled binary passes the
//! entire mock suite — which is how the `cursor` profile spent releases
//! naming `cursor-acp`, a package abandoned at 0.1.0 that no supported
//! install path puts on PATH.
//!
//! These tests spawn the real agent named by the production profile table.
//! They stop at the handshake — `initialize` then `session/new` — so they
//! spend no model tokens, need no vendor login, and stay deterministic. What
//! they prove is narrow and otherwise unprovable: the command resolves, the
//! process starts, and it answers ACP.
//!
//! The profile is read from `default_agent_profiles()` rather than written
//! out here. A test that hardcoded the command would pass while the table it
//! is meant to gate was wrong.
//!
//! `just test external` runs these; `just ci` does not.

use std::path::PathBuf;
use std::time::Duration;

use crucible_daemon::acp::client::ClientConfig;
use crucible_daemon::acp::discovery::default_agent_profiles;
use crucible_daemon::acp::CrucibleAcpClient;
use tempfile::TempDir;
use tokio::time::timeout;

/// How long a real agent gets to answer `initialize` and `session/new`.
///
/// Generous on purpose: the `npx` profiles resolve a package on first run,
/// and a cold npm cache is a download, not a hang.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(180);

/// Resolve a command the way a spawn would: absolute paths as given,
/// bare names against PATH.
fn resolve_on_path(command: &str) -> Option<PathBuf> {
    let candidate = PathBuf::from(command);
    if candidate.is_absolute() {
        return candidate.is_file().then_some(candidate);
    }

    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .map(|dir| dir.join(command))
        .find(|path| path.is_file())
}

/// Spawn the built-in profile `name` and complete the ACP handshake.
///
/// Every assertion here is about the real agent, so each failure names the
/// profile and what a reader must install to satisfy it.
async fn assert_profile_completes_handshake(name: &str) {
    let profiles = default_agent_profiles();
    let profile = profiles
        .get(name)
        .unwrap_or_else(|| panic!("`{name}` is not a built-in ACP profile"));

    let command = profile
        .command
        .as_deref()
        .unwrap_or_else(|| panic!("built-in profile `{name}` declares no command"));

    let resolved = resolve_on_path(command).unwrap_or_else(|| {
        panic!(
            "built-in ACP profile `{name}` names the command `{command}`, which is not on PATH.\n\
             This tier spawns the real agent, so an unresolvable command is a failure, not a skip.\n\
             Install the agent, or correct the profile in \
             crates/crucible-daemon/src/acp/discovery.rs."
        )
    });

    // The agent inherits this as its cwd, so `session/new` gets a real
    // directory to canonicalise. A temp dir keeps the developer's tree out of
    // whatever the agent decides to index.
    let workspace = TempDir::new().expect("create workspace temp dir");

    let config = ClientConfig {
        agent_path: resolved,
        agent_args: profile.args.clone(),
        working_dir: Some(workspace.path().to_path_buf()),
        env_vars: Some(profile.env.clone().into_iter().collect()),
        timeout_ms: Some(HANDSHAKE_TIMEOUT.as_millis() as u64),
    };

    let mut client = CrucibleAcpClient::with_name(config, name.to_string());

    // `None` selects the stdio MCP transport, which needs no host. The
    // choice between stdio and HTTP is negotiated elsewhere; this tier is
    // about whether the agent answers at all.
    let session = timeout(HANDSHAKE_TIMEOUT, client.connect_with_best_mcp(None))
        .await
        .unwrap_or_else(|_| {
            panic!(
                "ACP profile `{name}` did not finish the handshake within {}s. \
                 The process started, so the agent accepted the spawn and then \
                 failed to answer `initialize` or `session/new`.",
                HANDSHAKE_TIMEOUT.as_secs()
            )
        })
        .unwrap_or_else(|err| panic!("ACP profile `{name}` failed the handshake: {err}"));

    assert!(
        !session.id().is_empty(),
        "ACP profile `{name}` completed `session/new` but returned an empty session id"
    );

    assert!(
        client.is_connected(),
        "ACP profile `{name}` finished the handshake but the client reports no connection"
    );

    // Nothing downstream reads these here; the assertion is that reading them
    // does not panic on a real agent's capability block, which is the shape
    // the recorded fixtures freeze.
    let _ = client.agent_supports_http_mcp();
    let _ = client.agent_supports_session_close();

    client
        .disconnect(&session)
        .await
        .unwrap_or_else(|err| panic!("ACP profile `{name}` failed to disconnect: {err}"));
}

#[tokio::test]
#[ignore = "requires: ACP agent — spawns the real `claude` profile binary"]
async fn claude_profile_completes_the_real_handshake() {
    assert_profile_completes_handshake("claude").await;
}

#[tokio::test]
#[ignore = "requires: ACP agent — spawns the real `codex` profile binary"]
async fn codex_profile_completes_the_real_handshake() {
    assert_profile_completes_handshake("codex").await;
}

#[tokio::test]
#[ignore = "requires: ACP agent — spawns the real `cursor` profile binary"]
async fn cursor_profile_completes_the_real_handshake() {
    assert_profile_completes_handshake("cursor").await;
}

#[tokio::test]
#[ignore = "requires: ACP agent — spawns the real `opencode` profile binary"]
async fn opencode_profile_completes_the_real_handshake() {
    assert_profile_completes_handshake("opencode").await;
}

#[tokio::test]
#[ignore = "requires: ACP agent — spawns the real `hermes` profile binary"]
async fn hermes_profile_completes_the_real_handshake() {
    assert_profile_completes_handshake("hermes").await;
}
