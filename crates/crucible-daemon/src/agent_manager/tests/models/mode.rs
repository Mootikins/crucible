//! `AgentManager::set_mode` — persistence, live-handle application, and
//! validation. Mirrors the switch_model test structure in `switch.rs`.

use super::super::*;
use std::sync::atomic::{AtomicBool, Ordering};

/// Records the last mode set on it, so tests can assert the live handle was
/// updated (not just the persisted config).
struct ModeRecordingAgent {
    last_mode: Arc<std::sync::Mutex<Option<String>>>,
    reject: Arc<AtomicBool>,
}

crucible_core::impl_noop_agent!(ModeRecordingAgent);

#[async_trait::async_trait]
impl AgentHandle for ModeRecordingAgent {
    async fn send_message_fire_and_forget(&mut self, _: String) -> ChatResult<()> {
        Ok(())
    }
    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        if self.reject.load(Ordering::SeqCst) {
            return Err(crucible_core::traits::chat::ChatError::ModeChange(
                "agent refuses this mode".to_string(),
            ));
        }
        *self.last_mode.lock().unwrap() = Some(mode_id.to_string());
        Ok(())
    }
}

async fn setup_with_agent() -> (
    tempfile::TempDir,
    Arc<SessionManager>,
    crucible_core::session::Session,
    Arc<AgentManager>,
) {
    let (tmp, session_manager, session) = setup_session_manager().await;
    let agent_manager = create_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();
    (tmp, session_manager, session, Arc::new(agent_manager))
}

#[tokio::test]
async fn set_mode_persists_on_session_agent_config() {
    let (_tmp, session_manager, session, agent_manager) = setup_with_agent().await;

    agent_manager
        .set_mode(&session.id, "plan", None)
        .await
        .unwrap();

    let updated = session_manager.get_session(&session.id).unwrap();
    assert_eq!(
        updated.agent.as_ref().unwrap().mode.as_deref(),
        Some("plan"),
        "mode must persist so it applies at handle creation and survives eviction"
    );
}

#[tokio::test]
async fn set_mode_applies_to_cached_live_handle() {
    let (_tmp, _session_manager, session, agent_manager) = setup_with_agent().await;

    let last_mode = Arc::new(std::sync::Mutex::new(None));
    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(ModeRecordingAgent {
            last_mode: last_mode.clone(),
            reject: Arc::new(AtomicBool::new(false)),
        }))),
    );

    agent_manager
        .set_mode(&session.id, "auto", None)
        .await
        .unwrap();

    assert_eq!(
        last_mode.lock().unwrap().as_deref(),
        Some("auto"),
        "a live handle must be updated in place, not only the persisted config"
    );
}

#[tokio::test]
async fn set_mode_rejected_by_handle_persists_nothing() {
    let (_tmp, session_manager, session, agent_manager) = setup_with_agent().await;

    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(ModeRecordingAgent {
            last_mode: Arc::new(std::sync::Mutex::new(None)),
            reject: Arc::new(AtomicBool::new(true)),
        }))),
    );

    let err = agent_manager
        .set_mode(&session.id, "plan", None)
        .await
        .unwrap_err();
    assert!(matches!(err, AgentError::NotSupported(_)));

    let updated = session_manager.get_session(&session.id).unwrap();
    assert_eq!(
        updated.agent.as_ref().unwrap().mode,
        None,
        "a mode the agent rejected must not be persisted"
    );
}

#[tokio::test]
async fn set_mode_rejects_unknown_mode() {
    let (_tmp, session_manager, session, agent_manager) = setup_with_agent().await;

    let err = agent_manager
        .set_mode(&session.id, "yolo", None)
        .await
        .unwrap_err();
    match err {
        AgentError::NotSupported(msg) => {
            assert!(msg.contains("normal") && msg.contains("plan") && msg.contains("auto"));
        }
        other => panic!("expected NotSupported, got {other:?}"),
    }

    let updated = session_manager.get_session(&session.id).unwrap();
    assert_eq!(updated.agent.as_ref().unwrap().mode, None);
}

#[tokio::test]
async fn set_mode_unknown_session_errors() {
    let (_tmp, session_manager, _session, _am) = setup_with_agent().await;
    let agent_manager = create_test_agent_manager(session_manager);

    let err = agent_manager
        .set_mode("no-such-session", "plan", None)
        .await
        .unwrap_err();
    assert!(matches!(err, AgentError::SessionNotFound(_)));
}

#[tokio::test]
async fn set_mode_emits_mode_changed_event() {
    let (_tmp, _session_manager, session, agent_manager) = setup_with_agent().await;

    let (tx, mut rx) = tokio::sync::broadcast::channel(8);
    agent_manager
        .set_mode(&session.id, "plan", Some(&tx))
        .await
        .unwrap();

    let evt = rx.try_recv().expect("mode_changed event emitted");
    assert_eq!(evt.event, "mode_changed");
    // Wire contract: web events.rs and the SSE reducer read data["mode"].
    assert_eq!(evt.data["mode"], "plan");
    assert_eq!(evt.session_id, session.id);
}
