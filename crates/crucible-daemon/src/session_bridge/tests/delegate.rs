use super::*;
use crate::delegation::{DelegationRequest, DelegationSpawned, DelegationSpawner};
use crucible_core::background::{JobError, JobInfo, JobResult};
use crucible_core::config::DelegationConfig;

/// Records every spawn and answers with a fixed child.
struct RecordingSpawner {
    requests: std::sync::Mutex<Vec<DelegationRequest>>,
}

impl RecordingSpawner {
    fn shared() -> Arc<Self> {
        Arc::new(Self {
            requests: std::sync::Mutex::new(Vec::new()),
        })
    }

    fn requests(&self) -> Vec<DelegationRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl DelegationSpawner for RecordingSpawner {
    async fn spawn_delegation(
        &self,
        req: DelegationRequest,
    ) -> Result<DelegationSpawned, JobError> {
        self.requests.lock().unwrap().push(req);
        Ok(DelegationSpawned {
            delegation_id: "child-1".to_string(),
            child_session_id: "child-1".to_string(),
            message_id: "msg-1".to_string(),
        })
    }

    async fn await_delegation(
        &self,
        _delegation_id: &str,
        _timeout: Duration,
    ) -> Result<JobResult, JobError> {
        unimplemented!("the bridge's create path never awaits")
    }

    fn list_delegations(&self, _parent_session_id: &str) -> Vec<JobInfo> {
        Vec::new()
    }

    fn get_delegation_result(&self, _delegation_id: &str) -> Option<JobResult> {
        None
    }

    async fn cancel_delegation(&self, _delegation_id: &str) -> bool {
        false
    }
}

fn delegation_config(enabled: bool, targets: Option<Vec<String>>) -> DelegationConfig {
    DelegationConfig {
        enabled,
        max_depth: 1,
        allowed_targets: targets,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
        timeout_secs: 300,
    }
}

/// A bridge whose delegation spawns are recorded, over a fresh manager with
/// one parent session whose agent carries the given delegation config.
async fn delegate_rig(
    config: Option<DelegationConfig>,
) -> (String, Arc<RecordingSpawner>, DaemonSessionBridge) {
    let temp = TempDir::new().unwrap();
    let session_manager =
        crate::test_support::temp_session_manager_with_kilns(&[("kiln", temp.path())]);
    let agent_manager = build_test_agent_manager(Arc::clone(&session_manager));
    let (event_tx, _events) = broadcast::channel(256);
    let bridge = DaemonSessionBridge::new(bridge_ctx(
        Arc::clone(&session_manager),
        Arc::clone(&agent_manager),
        event_tx,
        temp.path(),
    ));

    let session = session_manager
        .create_session(SessionType::Chat, Vec::new(), None, None)
        .await
        .expect("parent session");
    let mut agent = make_test_agent(None);
    agent.delegation_config = config;
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .expect("configure parent agent");

    let spawner = RecordingSpawner::shared();
    let bridge = bridge.with_delegation_spawner(spawner.clone() as Arc<dyn DelegationSpawner>);
    (session.id.to_string(), spawner, bridge)
}

/// The full `delegate = true` path: a parent-stamped create reaches the
/// delegation spawner as a `DelegationRequest` carrying the parent's id, and
/// the caller gets back the job-shaped record `collect_subagents` polls.
#[tokio::test]
async fn a_delegated_create_spawns_through_the_delegation_service() {
    let (parent_id, spawner, bridge) = delegate_rig(Some(delegation_config(
        true,
        Some(vec!["cursor".to_string()]),
    )))
    .await;

    let spawned = bridge
        .create_session(serde_json::json!({
            "parent_session_id": parent_id,
            "prompt": "fix the failing tests",
            "target": "cursor",
            "description": "CI is red",
        }))
        .await
        .expect("delegated create");

    assert_eq!(spawned["delegation_id"], "child-1");
    assert_eq!(spawned["child_session_id"], "child-1");
    assert_eq!(spawned["status"], "spawned");

    let requests = spawner.requests();
    assert_eq!(requests.len(), 1, "one spawn, not a plain create");
    assert_eq!(requests[0].parent_session_id, parent_id);
    assert_eq!(requests[0].prompt, "fix the failing tests");
    assert_eq!(requests[0].target_agent.as_deref(), Some("cursor"));
    assert_eq!(
        requests[0].context.as_deref(),
        Some("Delegated task: CI is red")
    );
}

/// The parent's own config is the gate, exactly as for `delegate_session`:
/// no `delegation_config` (or `enabled = false`) refuses the spawn.
#[tokio::test]
async fn a_delegated_create_refuses_when_delegation_is_disabled() {
    let (parent_id, spawner, bridge) = delegate_rig(None).await;

    let err = bridge
        .create_session(serde_json::json!({
            "parent_session_id": parent_id,
            "prompt": "anything",
        }))
        .await
        .expect_err("disabled delegation must refuse");

    assert!(
        err.contains("disabled"),
        "the refusal should name the gate: {err}"
    );
    assert!(spawner.requests().is_empty());
}

/// A configured allowlist is closed: a target outside it is refused with the
/// allowed names, and nothing spawns.
#[tokio::test]
async fn a_delegated_create_refuses_a_target_outside_the_allowlist() {
    let (parent_id, spawner, bridge) = delegate_rig(Some(delegation_config(
        true,
        Some(vec!["cursor".to_string()]),
    )))
    .await;

    let err = bridge
        .create_session(serde_json::json!({
            "parent_session_id": parent_id,
            "prompt": "anything",
            "target": "claude",
        }))
        .await
        .expect_err("an unlisted target must refuse");

    assert!(
        err.contains("'claude' is not allowed") && err.contains("cursor"),
        "the refusal should name the target and the allowlist: {err}"
    );
    assert!(spawner.requests().is_empty());
}

/// A delegation without a task is a mistake, not a spawn.
#[tokio::test]
async fn a_delegated_create_requires_a_prompt() {
    let (parent_id, _spawner, bridge) = delegate_rig(Some(delegation_config(true, None))).await;

    let err = bridge
        .create_session(serde_json::json!({ "parent_session_id": parent_id }))
        .await
        .expect_err("a promptless delegation must refuse");

    assert!(err.contains("prompt"), "the refusal should name it: {err}");
}

/// A parent id nobody stamped — a session that does not exist — is refused
/// rather than spawning an orphan attributed to nobody.
#[tokio::test]
async fn a_delegated_create_refuses_an_unknown_parent() {
    let (_parent_id, _spawner, bridge) = delegate_rig(Some(delegation_config(true, None))).await;

    let err = bridge
        .create_session(serde_json::json!({
            "parent_session_id": "no-such-session",
            "prompt": "anything",
        }))
        .await
        .expect_err("an unknown parent must refuse");

    assert!(
        err.contains("no-such-session"),
        "the refusal should name it: {err}"
    );
}
