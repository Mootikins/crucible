//! Reviving a session after a daemon restart, not merely after an eviction.
//!
//! `send_revives_ended_session_from_storage` covers the warm path: the session
//! left the in-memory map but is still resident enough for
//! `get_or_revive_session` to resolve it (`agent_manager/messaging/send.rs`).
//!
//! A restart is a different path. Every in-memory index is built fresh at
//! startup, so a cold daemon knows nothing about the session but its id — and
//! that has to be enough. It is, now that storage is one flat root: revival
//! loads `{sessions_root}/{id}` directly, with no session→kiln index to consult
//! and no open kiln to probe. Before the relocation this test's subject was
//! whether the *kiln* happened to be open, which made reviving a persisted id
//! depend on project registration.
//!
//! This decides whether persisting a chat integration's channel→session map is
//! worth anything: if the id cannot be revived, remembering it is pointless.

use super::{create_test_agent_manager, script, test_agent, StreamingMockAgent};
use crate::agent_manager::{AgentManager, AgentManagerParams};
use crate::background_manager::BackgroundJobManager;
use crate::kiln_manager::KilnManager;
use crate::session_manager::SessionManager;
use crate::session_storage::FileSessionStorage;
use crucible_core::protocol::SessionEventMessage;
use crucible_core::session::SessionType;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::{broadcast, Mutex};

/// A `SessionManager` over `data_home`, resolving `kiln` under the name
/// `"kiln"` — the same root and the same registry on both sides of the
/// simulated restart, which is what a restarted daemon actually has.
///
/// The registry has to be rebuilt on the far side too: a session records the
/// NAME, and the mapping back to a directory is daemon state, not session
/// state. A restart that forgot it would revive a session whose kiln resolves
/// to nothing, which is the thing this test would otherwise not notice.
fn manager_over(data_home: &Path, kiln: &Path) -> Arc<SessionManager> {
    let registry = crate::test_support::kiln_registry(data_home, &[("kiln", kiln)]);
    Arc::new(
        SessionManager::with_storage(Arc::new(
            FileSessionStorage::new(FileSessionStorage::root_for(data_home))
                .with_registry(registry.clone()),
        ))
        .with_kiln_registry(registry),
    )
}

/// Create a session, then throw away every piece of in-memory state — the
/// closest thing to a restart that does not spawn a process. Returns the data
/// home, the kiln, and the surviving session id.
async fn session_surviving_a_restart() -> (TempDir, TempDir, String) {
    let data_home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();

    let sm = manager_over(data_home.path(), kiln.path());
    let session = sm
        .create_session(
            SessionType::Chat,
            vec![crate::test_support::kiln_name("kiln")],
            None,
            None,
        )
        .await
        .unwrap();
    let am = create_test_agent_manager(sm.clone());
    am.configure_agent(&session.id, test_agent()).await.unwrap();

    let id = session.id.clone();
    // Everything in memory goes; only what reached disk remains.
    drop(am);
    drop(sm);
    (data_home, kiln, id.to_string())
}

/// A fresh `AgentManager` over `data_home`, holding no session state — a cold
/// daemon.
///
/// Built by hand rather than via `create_test_agent_manager` because the
/// session manager has to be rooted at a specific data home, and that is only
/// reachable at construction.
async fn cold_manager(
    data_home: &Path,
    kiln: &Path,
    open_kiln: Option<&Path>,
) -> (
    Arc<SessionManager>,
    AgentManager,
    broadcast::Sender<SessionEventMessage>,
) {
    let sm = manager_over(data_home, kiln);

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
    });
    (sm, am, event_tx)
}

/// Put a scripted agent in the cache so the revived turn runs without a
/// provider. Mirrors `ReactorTestHarness::inject_agent`.
fn inject_for(am: &AgentManager, session_id: &str) {
    am.install_agent_for_test(
        session_id.to_string(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![script::text("revived"), script::done()],
        }) as _)),
    );
}

#[tokio::test]
async fn a_session_revives_after_a_restart_from_its_id_alone() {
    let (data_home, kiln, session_id) = session_surviving_a_restart().await;

    // Deliberately no open kiln: the session's own kiln is not registered with
    // this manager, and revival must not care.
    let (sm, am, tx) = cold_manager(data_home.path(), kiln.path(), None).await;
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
    let revived = sm
        .get_session(&session_id)
        .expect("revival should place the session back in memory");
    assert_eq!(
        revived.kilns,
        vec![crate::test_support::kiln_name("kiln")],
        "revival must restore the persisted kiln set, not invent one"
    );
}

/// The other half: an id this daemon's root has never seen is a miss, not a
/// silently-created session. Storage is flat, so "not here" is now the only
/// failure mode — there is no second place to look.
#[tokio::test]
async fn an_id_absent_from_the_sessions_root_is_not_revived() {
    let (_data_home, _kiln, session_id) = session_surviving_a_restart().await;
    let other_home = TempDir::new().unwrap();

    let (_sm, am, tx) = cold_manager(other_home.path(), _kiln.path(), None).await;
    let sent = am
        .send_message(&session_id, "still there?".to_string(), &tx, false, None)
        .await;

    assert!(
        sent.is_err(),
        "an unknown id must fail rather than silently starting a different conversation"
    );
}

/// The create-time trust gate re-runs when a session comes back off disk.
///
/// It used to run exactly once, at `session.create`, against the kilns the
/// session was created with. That is not enough now that a session's kilns are
/// NAMES: what a name reaches is daemon state, so a session's scope — and
/// therefore its classification — can change between the create and the next
/// turn without any request having asked for it. A registry entry appearing, or
/// a project marking a kiln confidential, both land the same way: the session
/// is suddenly reaching a corpus its provider was never cleared for, and no
/// gate is left between them.
///
/// Driven through `send_message`, which is the door a revived session actually
/// comes back through.
#[tokio::test]
async fn the_trust_gate_re_runs_when_a_session_is_revived_from_storage() {
    use crucible_core::config::{BackendType, TrustLevel};

    let data_home = TempDir::new().unwrap();
    let kiln = TempDir::new().unwrap();

    let cloud = crate::test_fixtures::build_llm_config_with_trust(
        "cloud",
        BackendType::OpenAI,
        Some(TrustLevel::Cloud),
    );

    // Created while the kiln carries no classification: a cloud provider is
    // legitimate, and the create-time gate lets it through.
    let sm = manager_over(data_home.path(), kiln.path());
    let session = sm
        .create_session(
            SessionType::Chat,
            vec![crate::test_support::kiln_name("kiln")],
            None,
            None,
        )
        .await
        .unwrap();
    let session_id = session.id.to_string();
    let am = super::create_test_agent_manager_with_llm_config(sm.clone(), cloud.clone());
    am.configure_agent(&session.id, super::test_agent())
        .await
        .expect("an unclassified kiln clears any provider");
    sm.end_session(&session.id).await.unwrap();

    // The corpus becomes confidential after the fact — the kiln's own project
    // config now says so, which is what `find_workspace_and_resolve_classification`
    // walks up to find.
    std::fs::create_dir_all(kiln.path().join(".crucible")).unwrap();
    std::fs::write(
        kiln.path().join(".crucible").join("project.toml"),
        "[[kilns]]\npath = \".\"\ndata_classification = \"confidential\"\n",
    )
    .unwrap();

    // A cold daemon over the same root and the same registry: the session
    // exists only on disk, and the turn below is what revives it.
    let sm = manager_over(data_home.path(), kiln.path());
    let am = super::create_test_agent_manager_with_llm_config(sm.clone(), cloud);
    let (tx, _rx) = broadcast::channel(64);
    assert!(
        sm.get_session(&session_id).is_none(),
        "precondition: the session is on disk only, so the send path revives it"
    );
    inject_for(&am, &session_id);

    let sent = am
        .send_message(&session_id, "still there?".to_string(), &tx, false, None)
        .await;

    let err = sent.expect_err("a revived session must be re-gated against its kilns");
    assert!(
        err.to_string().contains("insufficient"),
        "the refusal must name the trust shortfall, got: {err}"
    );
}
