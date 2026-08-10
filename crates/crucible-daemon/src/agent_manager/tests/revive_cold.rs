//! Reviving a session after a daemon restart, not merely after an eviction.
//!
//! `send_revives_ended_session_from_storage` covers the warm path: the session
//! left the in-memory map but `session_kilns` still knows which kiln it came
//! from, so `get_or_revive_session` resolves it directly
//! (`agent_manager/messaging/send.rs:400-440`).
//!
//! A restart is a different path and nothing covered it. `session_kilns` is a
//! `DashMap` built fresh at startup (`session_manager.rs:51`), so after a
//! restart it is empty and revival falls through to *probing every open kiln*.
//! That makes revival depend on the kiln being open — and the daemon only opens
//! kilns belonging to registered projects (`server/mod.rs:749-793`). Setting
//! `[plugins.discord] kiln` does not register anything.
//!
//! This decides whether persisting a chat integration's channel→session map is
//! worth anything: if the id cannot be revived, remembering it is pointless.

use super::{
    create_test_agent_manager, script, test_agent, test_workspace_tools, StreamingMockAgent,
};
use crate::agent_manager::{AgentManager, AgentManagerParams};
use crate::background_manager::BackgroundJobManager;
use crate::kiln_manager::KilnManager;
use crate::session_manager::SessionManager;
use crate::session_storage::FileSessionStorage;
use crucible_core::protocol::SessionEventMessage;
use crucible_core::session::SessionType;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::{broadcast, Mutex};

/// Create a session, then throw away every piece of in-memory state — the
/// closest thing to a restart that does not spawn a process. Returns the kiln
/// and the surviving session id.
async fn session_surviving_a_restart() -> (TempDir, String) {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().to_path_buf();

    let storage = Arc::new(FileSessionStorage::new());
    let sm = Arc::new(SessionManager::with_storage(storage));
    let session = sm
        .create_session(SessionType::Chat, kiln.clone(), None, vec![], None)
        .await
        .unwrap();
    let am = create_test_agent_manager(sm.clone());
    am.configure_agent(&session.id, test_agent()).await.unwrap();

    let id = session.id.clone();
    // Everything in memory goes; only what reached disk remains.
    drop(am);
    drop(sm);
    (tmp, id)
}

/// A fresh `AgentManager` whose `session_kilns` index is empty — a cold daemon.
///
/// Built by hand rather than via `create_test_agent_manager` because the kiln
/// has to be open in the `KilnManager` the manager holds, and that is only
/// reachable at construction.
async fn cold_manager(
    open_kiln: Option<&std::path::Path>,
) -> (
    Arc<SessionManager>,
    AgentManager,
    broadcast::Sender<SessionEventMessage>,
) {
    let storage = Arc::new(FileSessionStorage::new());
    let sm = Arc::new(SessionManager::with_storage(storage));

    let km = Arc::new(KilnManager::new());
    if let Some(kiln) = open_kiln {
        km.open(kiln).await.unwrap();
    }

    let (event_tx, _) = broadcast::channel(64);
    let background_manager = Arc::new(BackgroundJobManager::new(event_tx.clone()));
    let am = AgentManager::new(AgentManagerParams {
        kiln_manager: km,
        session_manager: sm.clone(),
        background_manager,
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        context_config: None,
        permission_config: None,
        plugin_loader: None,
        workspace_tools: test_workspace_tools(),
    });
    (sm, am, event_tx)
}

/// Put a scripted agent in the cache so the revived turn runs without a
/// provider. Mirrors `ReactorTestHarness::inject_agent`.
fn inject_for(am: &AgentManager, session_id: &str) {
    am.agent_cache.insert(
        session_id.to_string(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![script::text("revived"), script::done()],
        }) as _)),
    );
}

/// With the kiln open, a cold daemon finds the persisted session by probing.
#[tokio::test]
async fn a_session_revives_after_a_restart_when_its_kiln_is_open() {
    let (tmp, session_id) = session_surviving_a_restart().await;

    let (sm, am, tx) = cold_manager(Some(tmp.path())).await;
    assert!(
        sm.get_session(&session_id).is_none(),
        "precondition: a cold manager holds no sessions in memory"
    );
    inject_for(&am, &session_id);

    let sent = am
        .send_message(&session_id, "still there?".to_string(), &tx, false, None)
        .await;

    assert!(
        sent.is_ok(),
        "a persisted session must be revivable after a restart: {sent:?}"
    );
    assert!(
        sm.get_session(&session_id).is_some(),
        "revival should place the session back in memory"
    );
}

/// The same session, with the kiln *not* open, is unreachable — which is the
/// documented requirement, asserted rather than assumed.
#[tokio::test]
async fn a_session_cannot_be_revived_when_its_kiln_is_not_open() {
    let (tmp, session_id) = session_surviving_a_restart().await;

    let (_sm, am, tx) = cold_manager(None).await;
    let sent = am
        .send_message(&session_id, "still there?".to_string(), &tx, false, None)
        .await;

    assert!(
        sent.is_err(),
        "with no open kiln there is nothing to probe, so revival must fail \
         rather than silently starting a different conversation"
    );
    drop(tmp);
}
