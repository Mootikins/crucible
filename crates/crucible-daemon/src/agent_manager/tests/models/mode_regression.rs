//! Regression tests for `AgentManager::set_mode` — the concurrency
//! invariants (no deadlock on recursive handle, no block on busy handle,
//! deferred mode drains on next turn). Basic persistence/validation/event
//! coverage lives in `mode.rs`.

use super::super::*;
use super::mode::{setup_with_agent, ModeRecordingAgent};
use std::sync::atomic::AtomicBool;

/// Simulates a handle whose `set_mode_str` RPCs back into the SAME
/// `AgentManager` that holds this handle's mutex — the shape a
/// `DaemonAgentHandle` takes when cached inside the daemon's own
/// `agent_cache` (test setups, possible future in-process callers).
///
/// Without `apply_mode`, `AgentManager::set_mode` would call `set_mode_str`
/// on this handle, which re-issues `set_mode` while the mutex is still held
/// by the outer call. The `entered` flag short-circuits the recursion with
/// an error so the test fails fast if the regression returns; a real
/// recursive RPC would deadlock at the second `lock().await`.
struct DaemonLikeRecursingAgent {
    session_id: String,
    manager: Arc<std::sync::OnceLock<Arc<AgentManager>>>,
    entered: Arc<std::sync::Mutex<bool>>,
    set_mode_str_calls: Arc<std::sync::Mutex<u32>>,
    apply_mode_calls: Arc<std::sync::Mutex<u32>>,
}

impl DaemonLikeRecursingAgent {
    fn new(session_id: String) -> Self {
        Self {
            session_id,
            manager: Arc::new(std::sync::OnceLock::new()),
            entered: Arc::new(std::sync::Mutex::new(false)),
            set_mode_str_calls: Arc::new(std::sync::Mutex::new(0)),
            apply_mode_calls: Arc::new(std::sync::Mutex::new(0)),
        }
    }
}

crucible_core::impl_noop_agent!(DaemonLikeRecursingAgent);

#[async_trait::async_trait]
impl AgentHandle for DaemonLikeRecursingAgent {
    async fn send_message_fire_and_forget(&mut self, _: String) -> ChatResult<()> {
        Ok(())
    }
    fn get_mode_id(&self) -> &str {
        "normal"
    }
    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        *self.set_mode_str_calls.lock().unwrap() += 1;
        let already = std::mem::replace(&mut *self.entered.lock().unwrap(), true);
        if already {
            return Err(crucible_core::traits::chat::ChatError::ModeChange(
                "recursion: set_mode_str re-entered while mutex held".to_string(),
            ));
        }
        let Some(manager) = self.manager.get().cloned() else {
            *self.entered.lock().unwrap() = false;
            return Ok(());
        };
        let result = manager.set_mode(&self.session_id, mode_id, None).await;
        *self.entered.lock().unwrap() = false;
        result.map_err(|e| crucible_core::traits::chat::ChatError::ModeChange(e.to_string()))
    }

    async fn apply_mode(&mut self, _mode_id: &str) -> ChatResult<()> {
        *self.apply_mode_calls.lock().unwrap() += 1;
        Ok(())
    }
}

/// Regression: a cached handle whose `set_mode_str` re-enters the daemon
/// must not be invoked from inside `AgentManager::set_mode`. Before the
/// `apply_mode` split, that re-entry awaited the same mutex the outer call
/// held → deadlock.
#[tokio::test]
async fn set_mode_does_not_recurse_when_cached_handle_rpcs_back() {
    let (_tmp, session_manager, session, agent_manager) = setup_with_agent().await;

    let recursing = DaemonLikeRecursingAgent::new(session.id.clone());
    let set_mode_str_calls = recursing.set_mode_str_calls.clone();
    let apply_mode_calls = recursing.apply_mode_calls.clone();
    let manager_cell = recursing.manager.clone();
    let handle = Arc::new(tokio::sync::Mutex::new(
        Box::new(recursing) as BoxedAgentHandle
    ));
    let _ = manager_cell.set(agent_manager.clone());
    agent_manager.install_agent_for_test(session.id.clone(), handle.clone());

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        agent_manager.set_mode(&session.id, "plan", None),
    )
    .await;
    assert!(
        result.is_ok(),
        "set_mode must not hang when the cached handle RPCs back"
    );
    result.unwrap().expect("set_mode must succeed");

    assert_eq!(
        *set_mode_str_calls.lock().unwrap(),
        0,
        "set_mode_str must not be invoked on the cached handle — apply_mode is the daemon-internal path"
    );
    assert_eq!(
        *apply_mode_calls.lock().unwrap(),
        1,
        "apply_mode is the daemon-internal mirror sync"
    );

    let updated = session_manager.get_session(&session.id).unwrap();
    assert_eq!(
        updated.agent.as_ref().unwrap().mode.as_deref(),
        Some("plan"),
    );
}

/// Regression: the cached handle's mutex is held for the whole agent loop
/// (LLM calls + tool execution). A `set_mode` RPC arriving mid-turn used to
/// `handle.lock().await` and block until the turn ended — every mid-session
/// mode switch on the web hung for the whole RPC timeout. The fix is
/// `try_lock`: if the handle is busy, defer the mode change into
/// `pending_mode` so the NEXT turn drains it into `apply_mode` right after
/// acquiring the lock. The cached handle stays alive (an AcpAgentHandle owns
/// its spawned process's history — eviction would terminate that process and
/// lose the transcript); the in-flight turn keeps running under the old
/// mode (you can't reshape a turn already in progress).
#[tokio::test]
async fn set_mode_does_not_block_when_cached_handle_is_busy_with_a_turn() {
    let (_tmp, session_manager, session, agent_manager) = setup_with_agent().await;

    let handle = Arc::new(tokio::sync::Mutex::new(Box::new(ModeRecordingAgent {
        last_mode: Arc::new(std::sync::Mutex::new(None)),
        reject: Arc::new(AtomicBool::new(false)),
        current_mode: "normal".to_string(),
    }) as BoxedAgentHandle));
    agent_manager.install_agent_for_test(session.id.clone(), handle.clone());

    let _guard = handle.lock().await;

    let result = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        agent_manager.set_mode(&session.id, "plan", None),
    )
    .await;
    assert!(
        result.is_ok(),
        "set_mode must not block when the cached handle is busy with a turn"
    );
    result.unwrap().expect("set_mode must succeed");

    assert!(
        agent_manager.has_cached_agent(&session.id),
        "busy handle must NOT be evicted — ACP handles own spawned-process history"
    );
    assert_eq!(
        agent_manager.slot(&session.id).peek_pending_mode(),
        Some("plan".to_string()),
        "deferred mode change must be staged for the next turn"
    );

    let updated = session_manager.get_session(&session.id).unwrap();
    assert_eq!(
        updated.agent.as_ref().unwrap().mode.as_deref(),
        Some("plan"),
    );
}

/// Regression: a deferred mode must not resurrect itself after a LATER mode
/// change was applied directly to the handle. Sequence: turn busy →
/// `set_mode("plan")` defers → turn ends → `set_mode("auto")` takes the lock
/// and applies. If the stale "plan" entry survives, the next turn drains it
/// and silently reverts the live handle to plan while the persisted config
/// and every UI still say auto. `set_mode` is the single writer of
/// `pending_mode`: each call must clear any earlier deferral before deciding
/// whether to apply or defer.
#[tokio::test]
async fn direct_mode_change_clears_an_earlier_deferred_mode() {
    let (_tmp, session_manager, session, agent_manager) = setup_with_agent().await;

    let last_mode = Arc::new(std::sync::Mutex::new(None));
    let handle = Arc::new(tokio::sync::Mutex::new(Box::new(ModeRecordingAgent {
        last_mode: last_mode.clone(),
        reject: Arc::new(AtomicBool::new(false)),
        current_mode: "normal".to_string(),
    }) as BoxedAgentHandle));
    agent_manager.install_agent_for_test(session.id.clone(), handle.clone());

    // Turn in flight → "plan" is deferred.
    let busy = handle.lock().await;
    agent_manager
        .set_mode(&session.id, "plan", None)
        .await
        .expect("deferred set_mode must succeed");
    drop(busy);

    // Turn over → "auto" applies directly to the handle.
    agent_manager
        .set_mode(&session.id, "auto", None)
        .await
        .expect("direct set_mode must succeed");

    assert!(
        agent_manager
            .slot(&session.id)
            .peek_pending_mode()
            .is_none(),
        "the superseded deferral must be cleared, or the next turn reverts the mode to plan"
    );
    assert_eq!(
        session_manager
            .get_session(&session.id)
            .unwrap()
            .agent
            .as_ref()
            .unwrap()
            .mode
            .as_deref(),
        Some("auto"),
    );
}

/// Regression: the same stale-deferral hazard with the handle evicted in
/// between. After eviction, `set_mode` has no cached handle to apply to and
/// only persists — but the next turn builds a fresh handle from the persisted
/// config and then drains the leftover deferral over the top of it.
#[tokio::test]
async fn mode_change_after_eviction_clears_an_earlier_deferred_mode() {
    let (_tmp, _session_manager, session, agent_manager) = setup_with_agent().await;

    let handle = Arc::new(tokio::sync::Mutex::new(Box::new(ModeRecordingAgent {
        last_mode: Arc::new(std::sync::Mutex::new(None)),
        reject: Arc::new(AtomicBool::new(false)),
        current_mode: "normal".to_string(),
    }) as BoxedAgentHandle));
    agent_manager.install_agent_for_test(session.id.clone(), handle.clone());

    let busy = handle.lock().await;
    agent_manager
        .set_mode(&session.id, "plan", None)
        .await
        .expect("deferred set_mode must succeed");
    drop(busy);

    agent_manager.invalidate_agent_cache(&session.id);

    agent_manager
        .set_mode(&session.id, "auto", None)
        .await
        .expect("set_mode with no cached handle must succeed");

    assert!(
        agent_manager
            .slot(&session.id)
            .peek_pending_mode()
            .is_none(),
        "a deferral for an evicted handle must not outlive the mode change that superseded it"
    );
}

/// Once the in-flight turn releases the lock, the next lock acquisition
/// (start of the next turn) must drain the deferred mode into the handle
/// via `apply_mode`. Without this, the cached handle would keep using its
/// old mode forever — the deferred store would never be consumed.
#[tokio::test]
async fn pending_mode_is_drained_into_handle_on_next_lock_acquisition() {
    let (_tmp, _session_manager, session, agent_manager) = setup_with_agent().await;

    let handle = Arc::new(tokio::sync::Mutex::new(Box::new(ModeRecordingAgent {
        last_mode: Arc::new(std::sync::Mutex::new(None)),
        reject: Arc::new(AtomicBool::new(false)),
        current_mode: "normal".to_string(),
    }) as BoxedAgentHandle));
    agent_manager.install_agent_for_test(session.id.clone(), handle.clone());

    agent_manager.slot(&session.id).set_pending_mode("plan");

    let mut guard = handle.lock().await;
    let pending = agent_manager.slot(&session.id).take_pending_mode();
    assert_eq!(
        pending,
        Some("plan".to_string()),
        "pending mode must be present before drain"
    );
    guard
        .apply_mode("plan")
        .await
        .expect("apply_mode must succeed on the cached handle");
    assert_eq!(
        guard.get_mode_id(),
        "plan",
        "handle's mode mirror must reflect the drained mode"
    );
    assert!(
        agent_manager
            .slot(&session.id)
            .peek_pending_mode()
            .is_none(),
        "pending mode must be consumed after drain"
    );
}
