//! The mode set an ACP session reports.
//!
//! An agent declares its modes in the `session/new` reply — claude-agent-acp
//! sends five (`default`, `acceptEdits`, `plan`, `auto`,
//! `bypassPermissions`), codex-acp sends its own three. The handle used to
//! discard them and answer `get_modes` with `default_internal_modes()`,
//! Crucible's own set, and start every session on `"normal"` — an id no ACP
//! agent has. A front end therefore offered modes the agent would reject and
//! showed a current mode the agent was not in.
//!
//! The modes are the agent's, so the handle reports the agent's. Only when an
//! agent declares none does the internal set stand in, which is what
//! `session/set_mode`-less agents like the mock's default profile need.

use crucible_core::traits::chat::AgentHandle;
use crucible_daemon::acp_handle::AcpAgentHandle;
use tempfile::TempDir;
use tokio::time::{timeout, Duration};

use crate::support::{mock_agent_path, mock_handle_params, mock_session_agent};

/// Enough for a cold spawn plus the handshake.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Connect a handle to the spawned mock, optionally telling it which mode to
/// declare current. `None` means the agent declares no modes at all.
async fn handle_with_modes(current: Option<&str>) -> (AcpAgentHandle, TempDir) {
    let mut agent = mock_session_agent(&mock_agent_path().to_string_lossy());
    if let Some(current) = current {
        agent
            .env_overrides
            .insert("CRU_MOCK_ADVERTISE_MODES".to_string(), current.to_string());
    }

    let workspace = TempDir::new().expect("create workspace temp dir");
    let handle = timeout(
        CONNECT_TIMEOUT,
        AcpAgentHandle::new(mock_handle_params(&agent, workspace.path())),
    )
    .await
    .expect("the handshake finished inside the timeout")
    .expect("the mock agent connected");

    (handle, workspace)
}

#[tokio::test]
async fn an_agent_that_declares_modes_has_them_reported() {
    let (handle, _workspace) = handle_with_modes(Some("acceptEdits")).await;

    let modes = handle
        .get_modes()
        .expect("a session always reports some mode set");

    let ids: Vec<&str> = modes
        .available_modes
        .iter()
        .map(|mode| mode.id.0.as_ref())
        .collect();
    assert_eq!(
        ids,
        ["default", "acceptEdits", "plan"],
        "the reported modes must be the ones the agent declared, not Crucible's own"
    );

    // Names travel too: a front end renders these, and an id is not a label.
    let names: Vec<&str> = modes
        .available_modes
        .iter()
        .map(|mode| mode.name.as_str())
        .collect();
    assert_eq!(names, ["Manual", "Accept edits", "Plan"]);
}

#[tokio::test]
async fn the_session_starts_in_the_mode_the_agent_declared_current() {
    let (handle, _workspace) = handle_with_modes(Some("acceptEdits")).await;

    assert_eq!(
        handle.get_mode_id(),
        "acceptEdits",
        "the session must start in the agent's current mode, not on a Crucible default"
    );

    let modes = handle.get_modes().expect("mode set");
    assert_eq!(
        modes.current_mode_id.0.as_ref(),
        "acceptEdits",
        "the mode set's own current id must agree with the handle's"
    );
}

#[tokio::test]
async fn a_declared_current_mode_is_honoured_whichever_one_it_is() {
    // The previous test would also pass if the handle simply took the first
    // declared mode. Pick one that is neither first nor a Crucible default.
    let (handle, _workspace) = handle_with_modes(Some("plan")).await;

    assert_eq!(
        handle.get_mode_id(),
        "plan",
        "the current mode comes from the agent's `currentModeId`"
    );
}

#[tokio::test]
async fn an_agent_that_declares_no_modes_falls_back_to_the_internal_set() {
    let (handle, _workspace) = handle_with_modes(None).await;

    let modes = handle
        .get_modes()
        .expect("a session always reports some mode set");
    let ids: Vec<&str> = modes
        .available_modes
        .iter()
        .map(|mode| mode.id.0.as_ref())
        .collect();

    assert!(
        !ids.is_empty(),
        "an agent with no modes still needs a set to offer"
    );
    assert!(
        ids.contains(&handle.get_mode_id()),
        "the current mode must be one of the offered ones; got {:?} among {ids:?}",
        handle.get_mode_id()
    );
}

/// After a switch, the handle's two mode accessors agree.
///
/// `get_mode_id` and `get_modes().current_mode_id` are both "the mode this
/// session is in", read by different callers — the second is what the whole
/// mode set hands out, and it is what a front end hydrates. `set_mode_str`
/// used to move only the first, so every switch left the set naming whatever
/// the agent declared at the handshake.
#[tokio::test]
async fn a_switch_moves_the_current_mode_in_the_reported_set_too() {
    use crucible_core::traits::chat::AgentHandle;

    let (mut handle, _workspace) = handle_with_modes(Some("default")).await;
    assert_eq!(
        handle.get_mode_id(),
        "default",
        "the declared starting mode"
    );

    handle
        .set_mode_str("plan")
        .await
        .expect("the agent accepts a mode it declared");

    assert_eq!(handle.get_mode_id(), "plan");
    assert_eq!(
        handle
            .get_modes()
            .expect("a session always reports some mode set")
            .current_mode_id
            .0
            .as_ref(),
        "plan",
        "the reported set must name the mode the session actually switched to"
    );
}
