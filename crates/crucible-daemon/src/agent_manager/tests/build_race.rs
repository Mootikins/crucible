//! What a model switch and an agent build do to each other when they overlap.
//!
//! The two are not mutually exclusive and cannot be made so: `switch_model`
//! *checks* the session's request slot (`models.rs`, `ConcurrentRequest`) but
//! never claims it, and a mid-turn switch is meant to be deferrable rather than
//! refused — `pending_mode` is the in-repo precedent. So the two interleave by
//! design, and the build has to notice.

use super::*;
use crate::session_storage::{FileSessionStorage, SessionStorage};
use crucible_core::session::{Session, SessionSummary};
use std::path::{Path, PathBuf};
use std::sync::Mutex as StdMutex;

/// Session storage that parks the next `save` until the test releases it.
///
/// The barrier `switch_model` needs: its slot check and its cache invalidation
/// sit either side of `update_session().await`, so parking that write suspends
/// it in exactly the state the race requires — check passed, nothing persisted,
/// nothing invalidated. Arm once with [`Self::arm`]; every other save is
/// straight through, because the send path writes too.
struct GatedStorage {
    inner: FileSessionStorage,
    gate: StdMutex<Option<oneshot::Receiver<()>>>,
    entered: StdMutex<Option<oneshot::Sender<()>>>,
}

impl GatedStorage {
    fn new(sessions_root: PathBuf) -> Self {
        Self {
            inner: FileSessionStorage::new(sessions_root),
            gate: StdMutex::new(None),
            entered: StdMutex::new(None),
        }
    }

    /// Park the next save. Returns a receiver that resolves when it has parked,
    /// and a sender that releases it.
    fn arm(&self) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
        let (release_tx, release_rx) = oneshot::channel();
        let (entered_tx, entered_rx) = oneshot::channel();
        *self.gate.lock().unwrap() = Some(release_rx);
        *self.entered.lock().unwrap() = Some(entered_tx);
        (entered_rx, release_tx)
    }
}

#[async_trait]
impl SessionStorage for GatedStorage {
    fn sessions_root(&self) -> &Path {
        self.inner.sessions_root()
    }

    async fn save(&self, session: &Session) -> Result<(), SessionError> {
        let parked = self.gate.lock().unwrap().take();
        if let Some(release) = parked {
            if let Some(entered) = self.entered.lock().unwrap().take() {
                let _ = entered.send(());
            }
            let _ = release.await;
        }
        self.inner.save(session).await
    }

    async fn load(
        &self,
        session_id: &crucible_core::session::SessionId,
    ) -> Result<Session, SessionError> {
        self.inner.load(session_id).await
    }

    async fn list(&self) -> Result<Vec<SessionSummary>, SessionError> {
        self.inner.list().await
    }

    async fn append_event(&self, session: &Session, event: &str) -> Result<(), SessionError> {
        self.inner.append_event(session, event).await
    }

    async fn append_markdown(
        &self,
        session: &Session,
        role: &str,
        content: &str,
    ) -> Result<(), SessionError> {
        self.inner.append_markdown(session, role, content).await
    }

    async fn load_events(
        &self,
        session_id: &crucible_core::session::SessionId,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<serde_json::Value>, SessionError> {
        self.inner.load_events(session_id, limit, offset).await
    }

    async fn count_events(
        &self,
        session_id: &crucible_core::session::SessionId,
    ) -> Result<usize, SessionError> {
        self.inner.count_events(session_id).await
    }
}

/// A model switch that lands while another task is building the session's agent
/// handle must not be overwritten by that build.
///
/// The interleaving being reproduced, step for step — the numbering matches
/// `get_or_create_agent`'s doc comment, which is where the fix lives:
///
/// 1. `switch_model` reads the request slot, finds it free, proceeds. It does
///    not *claim* the slot, which is why the check does not save it.
/// 2. The turn claims the slot and starts the slow agent build.
/// 3. `switch_model` completes its storage write, persists the new model, and
///    invalidates an agent cache that is still empty — removing nothing — then
///    reports success.
/// 4. The build finishes and wants to install a handle for the *old* model.
///
/// Before the generation check, step 4 won and its handle survived every later
/// turn: `session.get_model` read storage and answered the new model while the
/// agent answered as the old one.
///
/// Two channels drive it and there are no sleeps. The storage gate is what makes
/// step 1-before-step-2 achievable: `switch_model`'s slot check and its cache
/// invalidation sit either side of one `update_session().await`, so parking that
/// write is the only barrier that suspends it in the required state. That also
/// means this test needs no access to anything private — it runs the real
/// `send_message_notified` and the real `switch_model`, which is what makes the
/// race it demonstrates a production one rather than a constructed one.
///
/// Note which ordering this is. A switch arriving *after* the turn has claimed
/// the slot is a different story and is not a bug: it gets `ConcurrentRequest`
/// at step 1 and never reaches storage. Only this order is reachable.
#[tokio::test]
async fn a_model_switch_during_an_agent_build_is_not_lost() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(GatedStorage::new(FileSessionStorage::root_for(tmp.path())));
    let session_manager = Arc::new(SessionManager::with_storage(storage.clone()));
    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![crate::test_support::kiln_name("kiln")],
            None,
            None,
        )
        .await
        .unwrap();

    let agent_manager = Arc::new(create_test_agent_manager(session_manager.clone()));
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    // The factory records the model of every handle it builds, and parks the
    // first build until released. `models_built.last()` is therefore "the model
    // the handle now serving this session was built for".
    let models_built: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(Vec::new()));
    let (build_entered_tx, build_entered_rx) = oneshot::channel();
    let (build_release_tx, build_release_rx) = oneshot::channel();
    let build_gate = Arc::new(StdMutex::new(Some((build_entered_tx, build_release_rx))));
    let recorder = models_built.clone();
    agent_manager.set_agent_factory_override(Box::new(move |config, _workspace| {
        let model = config.model.clone();
        let recorder = recorder.clone();
        let gate = build_gate.lock().unwrap().take();
        Box::pin(async move {
            if let Some((entered, release)) = gate {
                let _ = entered.send(());
                let _ = release.await;
            }
            recorder.lock().unwrap().push(model);
            Ok(Box::new(StreamingMockAgent {
                events: vec![script::text("ok"), script::done()],
            }) as BoxedAgentHandle)
        })
    }));

    // 1. The switch starts first and parks mid-persist, past its slot check.
    let (switch_parked, release_switch) = storage.arm();
    let switching = {
        let agent_manager = agent_manager.clone();
        let session_id = session.id.clone();
        tokio::spawn(async move {
            agent_manager
                .switch_model(&session_id, "llama3.3", None)
                .await
        })
    };
    switch_parked
        .await
        .expect("the switch must park in storage");

    // 2. Now the turn claims the slot and parks in the build. The switch is
    //    already past the check that would have rejected it.
    let (event_tx, _event_rx) = broadcast::channel(64);
    let sending = {
        let agent_manager = agent_manager.clone();
        let session_id = session.id.clone();
        let event_tx = event_tx.clone();
        tokio::spawn(async move {
            agent_manager
                .send_message_notified(&session_id, "hello".to_string(), &event_tx, true, None)
                .await
        })
    };
    build_entered_rx.await.expect("the build must park");

    // 3. Release the switch. It persists the new model and invalidates a cache
    //    that is still empty, then reports success.
    let _ = release_switch.send(());
    switching
        .await
        .expect("switch task")
        .expect("switch_model must succeed — it is not concurrent with a claim it can see");

    // 4. Release the build, which now wants to install a handle for the model
    //    the switch replaced.
    let _ = build_release_tx.send(());
    let (_message_id, completion) = sending.await.expect("send task").expect("send_message");
    let _ = completion.await;

    // Storage's answer, which is what `session.get_model` reports.
    let persisted = session_manager
        .get_session(&session.id)
        .and_then(|s| s.agent.map(|a| a.model))
        .expect("session has an agent config");
    assert_eq!(persisted, "llama3.3", "the switch reported success");

    // The next turn's handle must have been built for that same model. A stale
    // handle installed over the completed switch is invisible any other way:
    // the session reports one model and answers as the other.
    let (_message_id, completion) = agent_manager
        .send_message_notified(&session.id, "again".to_string(), &event_tx, true, None)
        .await
        .expect("second send");
    let _ = completion.await;

    let built = models_built.lock().unwrap().clone();
    assert_eq!(
        built.last().map(String::as_str),
        Some("llama3.3"),
        "the handle serving this session must be built for the model storage \
         reports; got build sequence {built:?}"
    );
}
