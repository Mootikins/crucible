//! Tests for `cru.sessions.stage_proposal`.
//!
//! The operation exists so a plugin can stage a proposal file without a kiln
//! path ever crossing into Lua: the daemon resolves the session's first kiln
//! name internally. These tests pin that contract from the Rust side; the
//! reflection plugin's Lua suite drives the same call through the mock.

use super::*;
use crate::session_bridge::DaemonSessionBridge;
use crate::test_support::{kiln_name, temp_session_manager_with_kilns};
use crucible_core::session::SessionType;

fn stage_rig(kiln: &std::path::Path) -> (Arc<SessionManager>, DaemonSessionBridge) {
    let session_manager = temp_session_manager_with_kilns(&[("kiln", kiln)]);
    let agent_manager = build_test_agent_manager(session_manager.clone());
    let (event_tx, _events) = broadcast::channel(16);
    let bridge = DaemonSessionBridge::new(bridge_ctx(
        session_manager.clone(),
        agent_manager,
        event_tx,
        kiln,
    ));
    (session_manager, bridge)
}

/// The bridge resolves the session's first kiln NAME to its directory and
/// writes the file under `.crucible/proposals/` there. Lua sees only the
/// filename, a bad filename is refused before anything touches the disk,
/// and a session with no kiln gets a clean error.
#[tokio::test]
async fn stage_proposal_resolves_the_kiln_name_and_stays_in_the_staging_dir() {
    let tmp = TempDir::new().unwrap();
    let (session_manager, bridge) = stage_rig(tmp.path());
    let session = session_manager
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .unwrap();

    // The name resolves, and the file lands in the staging directory.
    let returned = bridge
        .stage_proposal(
            session.id.to_string(),
            "note-1.md".into(),
            "proposed".into(),
        )
        .await
        .expect("a registered kiln accepts a plain filename");
    assert_eq!(returned, "note-1.md");
    let staged = tmp.path().join(".crucible/proposals/note-1.md");
    assert_eq!(std::fs::read_to_string(&staged).unwrap(), "proposed");

    // A traversing filename is refused, and nothing lands outside the
    // staging directory.
    for bad in ["../evil.md", "a/b.md", "..", ".hidden.md", ""] {
        let err = bridge
            .stage_proposal(session.id.to_string(), bad.to_string(), "x".into())
            .await
            .expect_err("a filename with a separator or a leading dot must be refused");
        assert!(err.contains("filename"), "unhelpful refusal: {err}");
    }
    assert!(!tmp.path().join("evil.md").exists());

    // A session with no kiln errors cleanly instead of inventing a target.
    let bare = session_manager
        .create_session(SessionType::Chat, vec![], None, None)
        .await
        .unwrap();
    let err = bridge
        .stage_proposal(bare.id.to_string(), "n.md".into(), "x".into())
        .await
        .expect_err("no kiln, no staging directory");
    assert!(err.contains("kiln"), "unhelpful refusal: {err}");
}

/// A `.crucible/proposals` that is really a symlink must not carry the
/// write outside the kiln. The filename check cannot see it: the name is
/// clean, and the directory is the part that lies. Bash is outside
/// `FsScope`, so an agent can plant the link; the canonicalize check in
/// `stage_proposal` is what refuses it.
#[cfg(unix)]
#[tokio::test]
async fn stage_proposal_refuses_a_symlinked_proposals_directory() {
    let kiln = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    std::fs::create_dir_all(kiln.path().join(".crucible")).unwrap();
    std::os::unix::fs::symlink(outside.path(), kiln.path().join(".crucible/proposals")).unwrap();

    let (session_manager, bridge) = stage_rig(kiln.path());
    let session = session_manager
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .unwrap();

    let err = bridge
        .stage_proposal(session.id.to_string(), "n.md".into(), "x".into())
        .await
        .expect_err("a symlinked proposals directory must be refused");
    assert!(err.contains("outside the kiln"), "unhelpful refusal: {err}");
    assert!(!outside.path().join("n.md").exists());
}
