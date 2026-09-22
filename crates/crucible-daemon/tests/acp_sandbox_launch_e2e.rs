//! A session with an isolation claim launches its ACP agent inside the sandbox.
//!
//! An ACP agent runs its own tools in its own process. The daemon cannot
//! stop those tools, so a claim is enforceable only when the agent process
//! itself starts inside the sandbox. `acp_launch.rs` has unit tests for the
//! argv that it builds. These tests start a real process through the
//! production path: `AgentManager` reads the claim, the factory builds the
//! handle, and the handle spawns the launcher.
//!
//! The launcher is a shell script. It records its argv in a marker file.
//! Then it replaces itself with `env "$@"`, which runs the mock agent with the
//! profile environment. A completed turn with the configured answer shows
//! that the agent ran behind the launcher and that its environment arrived.

#![cfg(unix)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crucible_core::session::SessionType;
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::test_support::{kiln_name, temp_session_manager_with_kilns};
use crucible_daemon::AgentManager;
use crucible_lua::{IsolationClaim, IsolationRegistry, SandboxEnv, SandboxExec};
use tempfile::TempDir;
use tokio::sync::broadcast;

#[path = "acp_support/mock_agent.rs"]
mod mock_agent;
#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent::{MockScript, Step};
use mock_agent_bin::{
    acp_manager_params, completed_turn, mock_agent_path, mock_profile, profile_session_agent,
    MOCK_PROFILE,
};

const TURN_TIMEOUT: Duration = Duration::from_secs(60);

/// What the mock agent streams. The value comes to the agent only through
/// the launcher argv, so the answer also proves that the environment arrived.
const ANSWER: &str = "answered from inside the sandbox";

/// Write a launcher script that records its argv in `marker`. Then the
/// script runs the rest of its argv through `env`.
fn write_launcher(dir: &Path, marker: &Path) -> PathBuf {
    let script = dir.join("sandbox-launcher.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexec env \"$@\"\n",
            marker.display()
        ),
    )
    .expect("write the launcher script");
    script
}

/// The launcher wraps the spawned agent. The turn completes through it.
#[tokio::test]
async fn an_isolation_claim_launches_the_acp_agent_through_the_sandbox_prefix() {
    let temp = TempDir::new().expect("temp dir");
    let kiln = temp.path().join("kiln");
    std::fs::create_dir_all(&kiln).expect("kiln dir");
    let marker = temp.path().join("launched.txt");
    let launcher = write_launcher(temp.path(), &marker);

    let session_manager = temp_session_manager_with_kilns(&[("kiln", &kiln)]);
    let (event_tx, _events) = broadcast::channel::<SessionEventMessage>(256);
    let script = MockScript {
        turn: vec![Step::Text(ANSWER.to_string())],
        ..MockScript::default()
    };
    let (key, value) = script.env();
    let profile = mock_profile(BTreeMap::from([(key.clone(), value.clone())]));
    let agent_manager = Arc::new(AgentManager::new(acp_manager_params(
        session_manager.clone(),
        BTreeMap::from([(MOCK_PROFILE.to_string(), profile)]),
        &event_tx,
    )));

    let session = session_manager
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .expect("session");

    // The claim that a plugin makes in a `session_start` hook. The launcher
    // is `/bin/sh <script>`, and `env(1)` takes the bare `K=V` operands.
    let isolation = IsolationRegistry::new();
    isolation.claim(
        session.id.as_str(),
        IsolationClaim {
            plugin: "test-sandbox".to_string(),
            exempt: Default::default(),
            exec: SandboxExec {
                prefix: vec![
                    "/bin/sh".to_string(),
                    launcher.to_string_lossy().into_owned(),
                ],
                env: SandboxEnv::Inline,
                suffix: Vec::new(),
            },
        },
    );
    agent_manager.set_isolation(isolation);

    agent_manager
        .configure_agent(&session.id, profile_session_agent(MOCK_PROFILE))
        .await
        .expect("configure the agent");

    let (_id, done) = agent_manager
        .send_message_notified(&session.id, "hello".to_string(), &event_tx, true, None)
        .await
        .expect("the turn is accepted");
    let outcome = completed_turn(done, TURN_TIMEOUT).await;
    assert_eq!(outcome.final_text.trim(), ANSWER, "the agent's answer");

    let argv = std::fs::read_to_string(&marker).unwrap_or_else(|e| {
        panic!(
            "the sandbox launcher did not run: no marker at {} ({e}). \
             The ACP agent started on the host.",
            marker.display()
        )
    });
    let argv: Vec<&str> = argv.lines().collect();
    let mock = mock_agent_path();
    assert_eq!(
        argv.last().copied(),
        Some(mock.to_string_lossy().as_ref()),
        "the launcher must receive the agent command as its last operand: {argv:?}"
    );
    assert!(
        argv.contains(&format!("{key}={value}").as_str()),
        "the profile environment must travel on the launcher argv: {argv:?}"
    );
}
