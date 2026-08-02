//! `session.create`'s `isolation` param.
//!
//! The daemon never interprets the value — it stores it and hands it to
//! plugins on the `Session` given to `on_session_start`. What the daemon *is*
//! responsible for is that the three shapes stay distinguishable end to end:
//! absent ("resolve normally"), `false` ("no container even if the project has
//! one") and a profile name / object ("use this instead"). Collapsing `false`
//! into absent would silently containerize a session that opted out; collapsing
//! absent into `false` would silently unsandbox one that did not.

use super::*;

struct Fixture {
    tmp: TempDir,
    sm: Arc<SessionManager>,
    pm: Arc<ProjectManager>,
    km: Arc<KilnManager>,
    am: Arc<AgentManager>,
    event_tx: broadcast::Sender<SessionEventMessage>,
}

impl Fixture {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let sm = Arc::new(SessionManager::with_storage(Arc::new(
            FileSessionStorage::new(),
        )));
        let pm = Arc::new(ProjectManager::new(tmp.path().join("projects.json")));
        let km = Arc::new(KilnManager::new());
        let (event_tx, _rx) = broadcast::channel(16);
        let am = test_agent_manager(km.clone(), sm.clone(), event_tx.clone(), None);
        Self {
            tmp,
            sm,
            pm,
            km,
            am,
            event_tx,
        }
    }

    /// `session.create` with whatever `params` the caller wants merged in.
    async fn create(&self, params: Value) -> String {
        let kiln = self.tmp.path().join("kiln");
        std::fs::create_dir_all(&kiln).unwrap();
        let mut merged = json!({ "type": "chat", "kiln": kiln });
        for (k, v) in params.as_object().unwrap() {
            merged[k] = v.clone();
        }
        let request: Request = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "session.create",
            "params": merged,
        }))
        .unwrap();

        let response = handle_session_create(
            request,
            &self.sm,
            &self.pm,
            self.tmp.path(),
            &None,
            &self.km,
            &self.event_tx,
            &self.am,
            None,
        )
        .await;
        assert!(
            response.error.is_none(),
            "session.create failed: {:?}",
            response.error
        );
        response.result.unwrap()["session_id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn isolation_of(&self, id: &str) -> Option<Value> {
        self.sm.get_session(id).expect("session").isolation
    }
}

#[tokio::test]
async fn a_session_created_without_the_param_carries_no_isolation_override() {
    let f = Fixture::new();
    let id = f.create(json!({})).await;

    assert_eq!(
        f.isolation_of(&id),
        None,
        "absent must stay absent — the plugin resolves normally only when it can \
         tell the caller said nothing"
    );
}

#[tokio::test]
async fn an_isolation_profile_name_reaches_the_session_untouched() {
    let f = Fixture::new();
    let id = f.create(json!({ "isolation": "heavy" })).await;

    assert_eq!(f.isolation_of(&id), Some(json!("heavy")));
}

#[tokio::test]
async fn an_isolation_object_reaches_the_session_untouched() {
    let f = Fixture::new();
    let spec = json!({ "image": "alpine:latest", "runtime": "podman" });
    let id = f.create(json!({ "isolation": spec })).await;

    assert_eq!(
        f.isolation_of(&id),
        Some(spec),
        "the daemon must forward the object as given; only the plugin interprets it"
    );
}

#[tokio::test]
async fn isolation_false_is_stored_as_an_explicit_opt_out_not_as_absent() {
    let f = Fixture::new();
    let id = f.create(json!({ "isolation": false })).await;

    assert_eq!(
        f.isolation_of(&id),
        Some(json!(false)),
        "`false` is a decision, not a default: stored as None it would be \
         indistinguishable from 'resolve normally' and the session would get a \
         container it declined"
    );
}

/// `enforce_session_start` also runs on resume, and resume reloads the session
/// from disk. An `isolation` that lives only in memory means a resumed session
/// silently loses (or silently gains) its sandbox.
#[tokio::test]
async fn isolation_survives_a_round_trip_through_session_storage() {
    let f = Fixture::new();
    let id = f.create(json!({ "isolation": "heavy" })).await;
    let kiln = f.tmp.path().join("kiln");

    // Drop it from memory, then reload the way resume does.
    f.sm.end_session(&id).await.expect("end");
    let reloaded =
        f.sm.resume_session_from_storage(&id, &kiln)
            .await
            .expect("resume from storage");

    assert_eq!(reloaded.isolation, Some(json!("heavy")));
}

/// A child shares its parent's workspace and runs the same tools. If it
/// resolved isolation independently it would land in "no container" while the
/// parent is sandboxed — the escape the shared lifecycle function closed,
/// reopened through the resolution order instead.
#[tokio::test]
async fn a_delegated_child_inherits_its_parents_isolation() {
    let f = Fixture::new();
    let parent_id = f.create(json!({ "isolation": "heavy" })).await;
    let parent = f.sm.get_session(&parent_id).unwrap();

    let child =
        f.sm.create_child_session(
            &parent,
            crucible_core::session::SessionAgent::internal_from_config(
                &crucible_core::config::CliAppConfig::default(),
            ),
            Some("child".to_string()),
        )
        .await
        .expect("child session");

    assert_eq!(
        child.isolation,
        Some(json!("heavy")),
        "an unsandboxed child of a sandboxed parent is the same escape by a \
         different door"
    );
}

/// The wiring that makes a delegated child go through the shared lifecycle.
///
/// `Server::bind` is the only place `bind_session_lifecycle` is called, and
/// nothing observes it: unbound, `enforce_child_isolation` stops firing plugin
/// start hooks and silently reopens the delegation escape — a sandboxed
/// parent's subagent gets no container and runs every tool on the host, with no
/// error anywhere. Asserted against a real `Server`, because the value of the
/// check is entirely that it covers *production* construction.
#[tokio::test]
async fn binding_a_server_wires_delegation_to_the_shared_session_lifecycle() {
    let tmp = TempDir::new().unwrap();
    let sock = tmp.path().join("lifecycle.sock");
    // Isolated data home: without it this loads the developer's real
    // ~/.crucible registry.
    let server = Server::bind_with_data_home(&sock, tmp.path().to_path_buf())
        .await
        .expect("bind");

    assert!(
        server
            .agent_manager
            .delegation_service()
            .session_lifecycle_bound(),
        "Server::bind must bind the delegation service to the session lifecycle, \
         or delegated children fire no plugin start hooks and run unsandboxed"
    );
}
