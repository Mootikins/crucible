//! Session writers must not put back a field that another writer changed.
//!
//! Each writer here once read a copy of the session, awaited, and saved the
//! whole copy. A `set_title` that ran in that gap came back as `None`, and the
//! web rail showed the session as "Untitled". The helper puts `set_title` in
//! the gap with a parked storage write, so no test sleeps.

use super::build_race::GatedStorage;
use super::*;
use crate::session_storage::{FileSessionStorage, SessionStorage};
use crucible_core::session::SessionId;
use futures::FutureExt;
use std::future::Future;

const TITLE: &str = "Kept title";

struct Fixture {
    _tmp: TempDir,
    storage: Arc<GatedStorage>,
    session_manager: Arc<SessionManager>,
    agents: Arc<AgentManager>,
    id: String,
}

async fn fixture() -> Fixture {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(GatedStorage::new(FileSessionStorage::root_for(tmp.path())));
    let session_manager = Arc::new(SessionManager::with_storage(storage.clone()));
    let session = session_manager
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .unwrap();
    let agents = create_test_agent_manager(session_manager.clone());
    agents
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();
    Fixture {
        _tmp: tmp,
        storage,
        session_manager,
        agents,
        id: session.id.to_string(),
    }
}

/// Run `write` in the gap that a lost update needs, then check the title.
///
/// A parked `update_last_activity` holds the persist guard. `set_title` is
/// polled once, so it queues on the guard first. `write` is polled once, so it
/// reads the session and queues second. The release lets all three finish in
/// queue order. A write that saves the copy it read puts the title back.
async fn assert_title_survives<T>(fx: &Fixture, write: impl Future<Output = T>) -> T {
    let (parked, release) = fx.storage.arm();
    let holder = {
        let session_manager = fx.session_manager.clone();
        let id = fx.id.clone();
        tokio::spawn(async move {
            session_manager
                .update_last_activity(&id, chrono::Utc::now())
                .await
        })
    };
    parked.await.expect("the holder must park in storage");

    let mut title = std::pin::pin!(fx.session_manager.set_title(&fx.id, TITLE.to_string()));
    assert!(
        title.as_mut().now_or_never().is_none(),
        "set_title must wait for the guard"
    );
    let mut write = std::pin::pin!(write);
    assert!(
        write.as_mut().now_or_never().is_none(),
        "the write must wait for the guard behind set_title"
    );

    release.send(()).unwrap();
    holder.await.unwrap().unwrap();
    title.await.unwrap();
    let output = write.await;

    let live = fx.session_manager.get_session(&fx.id).unwrap();
    assert_eq!(
        live.title.as_deref(),
        Some(TITLE),
        "the live title was lost"
    );
    let stored = fx
        .storage
        .load(&SessionId::parse(&fx.id).unwrap())
        .await
        .unwrap();
    assert_eq!(
        stored.title.as_deref(),
        Some(TITLE),
        "the stored title was lost"
    );
    output
}

fn live_agent(fx: &Fixture) -> SessionAgent {
    fx.session_manager
        .get_session(&fx.id)
        .and_then(|s| s.agent)
        .unwrap()
}

#[tokio::test]
async fn persist_variables_keeps_a_title_set_in_its_gap() {
    let fx = fixture().await;
    let slot = fx.agents.slot(&fx.id);
    slot.variables().set("seen", serde_json::json!(true));

    assert_title_survives(
        &fx,
        crate::agent_manager::session_config::persist_variables(&fx.session_manager, &slot, &fx.id),
    )
    .await
    .unwrap();

    let live = fx.session_manager.get_session(&fx.id).unwrap();
    assert_eq!(live.variables.get("seen"), Some(&serde_json::json!(true)));
}

#[tokio::test]
async fn configure_agent_keeps_a_title_set_in_its_gap() {
    let fx = fixture().await;
    let mut agent = test_agent();
    agent.model = "llama3.3".to_string();

    assert_title_survives(&fx, fx.agents.configure_agent(&fx.id, agent))
        .await
        .unwrap();

    assert_eq!(live_agent(&fx).model, "llama3.3");
}

#[tokio::test]
async fn connect_kiln_keeps_a_title_set_in_its_gap() {
    let fx = fixture().await;
    let other = kiln_name("other");

    assert_title_survives(&fx, fx.agents.connect_kiln(&fx.id, &other, None))
        .await
        .unwrap();

    let live = fx.session_manager.get_session(&fx.id).unwrap();
    assert!(live.kilns.contains(&other));
}

#[tokio::test]
async fn switch_model_keeps_a_title_set_in_its_gap() {
    let fx = fixture().await;

    assert_title_survives(&fx, fx.agents.switch_model(&fx.id, "llama3.3", None))
        .await
        .unwrap();

    assert_eq!(live_agent(&fx).model, "llama3.3");
}

#[tokio::test]
async fn set_mode_keeps_a_title_set_in_its_gap() {
    let fx = fixture().await;

    assert_title_survives(&fx, fx.agents.set_mode(&fx.id, "plan", None))
        .await
        .unwrap();

    assert_eq!(live_agent(&fx).mode.as_deref(), Some("plan"));
}

#[tokio::test]
async fn a_knob_setter_keeps_a_title_set_in_its_gap() {
    let fx = fixture().await;

    assert_title_survives(&fx, fx.agents.set_precognition(&fx.id, true, None))
        .await
        .unwrap();

    assert!(live_agent(&fx).precognition_enabled);
}

#[tokio::test]
async fn record_discovered_context_window_keeps_a_title_set_in_its_gap() {
    let fx = fixture().await;

    assert_title_survives(
        &fx,
        fx.agents.record_discovered_context_window(&fx.id, 8192),
    )
    .await
    .unwrap();

    assert_eq!(live_agent(&fx).context_budget, Some(8192));
}

#[tokio::test]
async fn add_notification_keeps_a_title_set_in_its_gap() {
    let fx = fixture().await;
    let note = crucible_core::types::Notification::toast("hello");

    assert_title_survives(&fx, fx.agents.add_notification(&fx.id, note, None))
        .await
        .unwrap();

    let live = fx.session_manager.get_session(&fx.id).unwrap();
    assert_eq!(live.notifications.list().len(), 1);
}

#[tokio::test]
async fn dismiss_notification_keeps_a_title_set_in_its_gap() {
    let fx = fixture().await;
    let note = crucible_core::types::Notification::toast("hello");
    let note_id = note.id.clone();
    fx.agents
        .add_notification(&fx.id, note, None)
        .await
        .unwrap();

    let dismissed =
        assert_title_survives(&fx, fx.agents.dismiss_notification(&fx.id, &note_id, None))
            .await
            .unwrap();

    assert!(dismissed);
    let live = fx.session_manager.get_session(&fx.id).unwrap();
    assert!(live.notifications.list().is_empty());
}

#[tokio::test]
async fn persist_acp_session_id_keeps_a_title_set_in_its_gap() {
    let fx = fixture().await;

    assert_title_survives(
        &fx,
        fx.agents
            .persist_acp_session_id(&fx.id, "acp-1".to_string()),
    )
    .await
    .unwrap();

    let live = fx.session_manager.get_session(&fx.id).unwrap();
    assert_eq!(live.acp_session_id.as_deref(), Some("acp-1"));
}
