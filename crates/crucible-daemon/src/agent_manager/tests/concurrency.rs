use super::*;
use crate::test_support::temp_session_manager;

/// A mock agent whose stream never yields — blocks forever until cancelled.
struct PendingMockAgent;

#[async_trait::async_trait]
impl crucible_core::turn::Agent for PendingMockAgent {
    fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
        crucible_core::turn::AgentCapabilities::default()
    }
    async fn turn<'a>(
        &'a mut self,
        _ctx: crucible_core::turn::TurnContext,
    ) -> Result<
        futures::stream::BoxStream<'a, crucible_core::turn::TurnEvent>,
        crucible_core::turn::AgentError,
    > {
        // Hangs forever until the manager cancels the stream.
        Ok(Box::pin(futures::stream::pending()))
    }
    async fn cancel(&self) -> Result<(), crucible_core::turn::AgentError> {
        Ok(())
    }
    async fn switch_model(&mut self, _: &str) -> Result<(), crucible_core::turn::NotSupported> {
        Err(crucible_core::turn::NotSupported::new("switch_model"))
    }
}

crucible_core::impl_unsupported_session_knobs!(PendingMockAgent);

#[async_trait::async_trait]
impl AgentHandle for PendingMockAgent {
    async fn send_message_fire_and_forget(&mut self, _: String) -> ChatResult<()> {
        Ok(())
    }

    async fn clear_history(&mut self) -> ChatResult<()> {
        Ok(())
    }
    fn get_mode_id(&self) -> &str {
        "ask"
    }
    async fn set_mode_str(&mut self, _: &str) -> ChatResult<()> {
        Ok(())
    }
}

#[tokio::test]
async fn concurrent_send_to_same_session_returns_error() {
    let (_tmp, session_manager, session) = setup_session_manager().await;

    let agent_manager = create_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    agent_manager.request_state.insert(
        session.id.to_string(),
        super::RequestState {
            cancel_tx: None,
            task_handle: None,
            _work: None,
        },
    );

    let (event_tx, _event_rx) = broadcast::channel::<SessionEventMessage>(64);
    let result = agent_manager
        .send_message(&session.id, "hello".to_string(), &event_tx, true, None)
        .await;

    assert!(
        matches!(result, Err(AgentError::ConcurrentRequest(_))),
        "Second send_message should return ConcurrentRequest, got: {:?}",
        result,
    );
}

#[tokio::test]
async fn cancel_during_streaming_emits_ended_event() {
    let (_tmp, session_manager, session) = setup_session_manager().await;

    let agent_manager = create_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    agent_manager.install_agent_for_test(
        session.id.to_string(),
        Arc::new(Mutex::new(Box::new(PendingMockAgent) as BoxedAgentHandle)),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    let _message_id = agent_manager
        .send_message(&session.id, "test".to_string(), &event_tx, true, None)
        .await
        .unwrap();

    let user_msg = next_event_or_skip(&mut event_rx, "user_message").await;
    assert_eq!(user_msg.data["content"], "test");

    tokio::time::sleep(Duration::from_millis(50)).await;

    let cancelled = agent_manager.cancel(&session.id).await;
    assert!(cancelled, "cancel() should return true for active request");

    let ended = next_event_or_skip(&mut event_rx, "ended").await;
    assert_eq!(ended.session_id, session.id);
    assert_eq!(ended.data["reason"], "cancelled");
}

/// A send must not slip into the window while cancel() is still winding a
/// turn down.
///
/// The turn task keeps streaming until it is polled after the cancel signal,
/// and it releases the request slot itself only at its very end. cancel()
/// used to vacate the slot FIRST — so a send arriving inside that window was
/// ADMITTED beside an still-alive stream: two concurrent turns on one
/// session, the new turn's `user_message` recorded while the old turn's
/// thinking and tokens were still being emitted (the wire order the web
/// transcript rendered as a user message in the middle of an answer).
///
/// The task below holds the slot for a bounded 300ms and then removes it —
/// the same shape as the real task's tail — so the window is deterministic:
/// a send INSIDE it must be refused, and after cancel() returns it must be
/// admitted again.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn send_during_cancel_wind_down_is_rejected() {
    let (_tmp, session_manager, session) = setup_session_manager().await;

    let agent_manager = Arc::new(create_test_agent_manager(session_manager.clone()));
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();
    agent_manager.install_agent_for_test(
        session.id.to_string(),
        Arc::new(Mutex::new(Box::new(PendingMockAgent) as BoxedAgentHandle)),
    );

    // A turn task that mirrors the real tail: holds the slot, then releases
    // it. The 300ms stay under cancel()'s 500ms grace.
    let (cancel_tx, _cancel_rx) = tokio::sync::oneshot::channel::<()>();
    let slot_owner = agent_manager.clone();
    let session_id = session.id.to_string();
    let task_handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        slot_owner.request_state.remove(session_id.as_str());
    });
    agent_manager.request_state.insert(
        session.id.to_string(),
        super::RequestState {
            cancel_tx: Some(cancel_tx),
            task_handle: Some(task_handle),
            _work: None,
        },
    );

    // Start the cancel but do not await it: the window under test is the
    // span between the signal and the task releasing the slot.
    let canceller = agent_manager.clone();
    let cancel_session = session.id.to_string();
    let cancelling =
        tokio::spawn(async move { canceller.cancel(cancel_session.as_str()).await });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let (event_tx, _event_rx) = broadcast::channel::<SessionEventMessage>(64);
    let result = agent_manager
        .send_message(&session.id, "during cancel".to_string(), &event_tx, true, None)
        .await;
    assert!(
        matches!(result, Err(AgentError::ConcurrentRequest(_))),
        "A send while cancel() is winding the turn down must be refused, got: {:?}",
        result,
    );

    // Once cancel() has returned, the winding-down task is done and the slot
    // is free again — the refusal must not wedge the session.
    assert!(cancelling.await.unwrap(), "cancel should report an active request");
    let retry = agent_manager
        .send_message(&session.id, "after cancel".to_string(), &event_tx, true, None)
        .await;
    assert!(
        retry.is_ok(),
        "A send after cancel() returned must be admitted, got: {:?}",
        retry,
    );
}

#[tokio::test]
async fn empty_stream_without_done_cleans_up_request_state() {
    let (_tmp, session_manager, session) = setup_session_manager().await;

    let agent_manager = create_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    agent_manager.install_agent_for_test(
        session.id.to_string(),
        Arc::new(Mutex::new(Box::new(MockAgent) as BoxedAgentHandle)),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    let _message_id = agent_manager
        .send_message(&session.id, "test".to_string(), &event_tx, true, None)
        .await
        .unwrap();

    let user_msg = next_event_or_skip(&mut event_rx, "user_message").await;
    assert_eq!(user_msg.data["content"], "test");

    tokio::time::sleep(Duration::from_millis(100)).await;

    assert!(
        !agent_manager
            .request_state
            .contains_key(session.id.as_str()),
        "request_state should be cleaned up after empty stream completes"
    );
}

/// Two workflow steps in a parallel group share one session, but a
/// session supports a single in-flight turn (`request_state` guard) and
/// inline-handler event correlation is session-scoped. The inline
/// handler must therefore serialize its turns instead of surfacing
/// `ConcurrentRequest` failures to the workflow.
#[tokio::test]
async fn parallel_workflow_steps_serialize_llm_turns_on_one_session() {
    use crate::workflow_handlers::DaemonInlineHandler;
    use crucible_core::parser::types::WorkflowStep;
    use crucible_core::workflow::{ExecContext, OutputScope, StepHandler, StepOutcome};

    let (_tmp, session_manager, session) = setup_session_manager().await;

    let agent_manager = Arc::new(create_test_agent_manager(session_manager.clone()));
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    agent_manager.install_agent_for_test(
        session.id.to_string(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![script::text("branch result"), script::done()],
        }) as BoxedAgentHandle)),
    );

    let (event_tx, _event_rx) = broadcast::channel::<SessionEventMessage>(256);
    let handler = DaemonInlineHandler::new(&session.id, agent_manager.clone(), event_tx.clone());

    fn step(title: &str) -> WorkflowStep {
        WorkflowStep {
            level: 2,
            title: title.to_string(),
            agent: None,
            output: None,
            attributes: HashMap::new(),
            body: format!("do {title}"),
            parallel: true,
            children: Vec::new(),
            gates: Vec::new(),
            offset: 0,
        }
    }

    let (step_a, step_b) = (step("A"), step("B"));
    let scope = OutputScope::new();
    let validations: Vec<crucible_core::parser::types::ValidationEntry> = Vec::new();
    let ctx_a = ExecContext {
        step: &step_a,
        step_id: "0",
        scope: &scope,
        validations: &validations,
    };
    let ctx_b = ExecContext {
        step: &step_b,
        step_id: "1",
        scope: &scope,
        validations: &validations,
    };

    let (outcome_a, outcome_b) = tokio::join!(handler.execute(&ctx_a), handler.execute(&ctx_b));

    for (label, outcome) in [("A", outcome_a), ("B", outcome_b)] {
        match outcome {
            StepOutcome::Advance { output } => {
                assert_eq!(
                    output,
                    Some(serde_json::json!("branch result")),
                    "step {label} should capture its own turn's response"
                );
            }
            other => panic!("step {label}: expected Advance, got {other:?}"),
        }
    }
}

/// A scope mutation must claim the session's request slot atomically, exactly
/// like a send. With the slot already held (a turn in flight, represented here
/// by a pre-inserted `RequestState`), a scope mutation is rejected rather than
/// racing in and caching a stale-scope agent after the caches are invalidated.
#[tokio::test]
async fn scope_mutation_rejected_when_request_slot_occupied() {
    let (_tmp, session_manager, session) = setup_session_manager().await;
    let agent_manager = create_test_agent_manager(session_manager.clone());

    // Simulate an in-flight turn holding the slot.
    agent_manager.request_state.insert(
        session.id.to_string(),
        super::RequestState {
            cancel_tx: None,
            task_handle: None,
            _work: None,
        },
    );

    let result = agent_manager
        .connect_kiln(
            &session.id,
            &crate::test_support::kiln_name("other-kiln"),
            None,
        )
        .await;

    assert!(
        matches!(result, Err(AgentError::ConcurrentRequest(_))),
        "scope mutation during an in-flight turn should return ConcurrentRequest, got: {result:?}",
    );
    // The in-flight turn still owns the slot — the rejected mutation must not
    // have touched it.
    assert!(
        agent_manager
            .request_state
            .contains_key(session.id.as_str()),
        "rejected mutation must leave the existing slot claim intact",
    );
}

/// After a scope mutation completes it must release the slot, so the next turn
/// (or mutation) can claim it. The `RequestSlotGuard` drop guarantees this on
/// the success path.
#[tokio::test]
async fn scope_mutation_releases_request_slot_on_completion() {
    let (_tmp, session_manager, session) = setup_session_manager().await;
    let agent_manager = create_test_agent_manager(session_manager.clone());

    agent_manager
        .connect_kiln(
            &session.id,
            &crate::test_support::kiln_name("other-kiln"),
            None,
        )
        .await
        .expect("connect_kiln on an idle session should succeed");

    assert!(
        !agent_manager
            .request_state
            .contains_key(session.id.as_str()),
        "slot must be free once the mutation returns",
    );
}

/// Two first turns arriving together on one session must share a slot.
///
/// The slot carries the spill counter and the start-hook overrides, so two
/// slots means two counters — colliding spill filenames — and a set of
/// overrides that one caller cannot see. `DashMap::entry().or_default()` is
/// atomic; the proof is pointer equality, not two equal-looking slots.
///
/// It used to build the session's Lua VM in that gap, which is what made the
/// old check-then-insert lose handlers. Sessions have no VM now, and the race
/// this guards is smaller — but it is the same race.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_first_uses_share_one_session_slot() {
    let session_manager = temp_session_manager();
    let agent_manager = Arc::new(create_test_agent_manager(session_manager));
    let session_id = "shared-vm-session";

    // A barrier, not a sleep: both threads are inside the call at the same
    // time or the test proves nothing about the race.
    let gate = Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let agent_manager = agent_manager.clone();
            let gate = gate.clone();
            tokio::task::spawn_blocking(move || {
                gate.wait();
                agent_manager.slot(session_id)
            })
        })
        .collect();

    let mut states = Vec::new();
    for handle in handles {
        states.push(handle.await.expect("slot lookup must not panic"));
    }

    assert!(
        Arc::ptr_eq(&states[0], &states[1]),
        "both callers must get the same slot, or the spill counters diverge"
    );
}

/// A cold-start turn must not queue behind another session's plugin hooks.
///
/// `session_lifecycle::fire_session_start` holds the plugin-loader mutex across
/// hook execution, which since the oci work includes container builds. The
/// agent-build path and the title path both used to take that same mutex just
/// to read the `Lua` handle and the plugin registry, so a slow start on session
/// A stalled a first turn on session B.
///
/// The held guard below *is* the parked hook — no plugin, no container, no
/// sleep. The timeout is a deadlock detector, not a synchronisation device: if
/// either accessor still reaches for the loader this test hangs, and 5s turns
/// that hang into a failure.
#[tokio::test]
async fn reading_plugin_state_does_not_queue_behind_the_loader_lock() {
    let session_manager = temp_session_manager();
    let (event_tx, _rx) = broadcast::channel(16);
    let loader = Arc::new(Mutex::new(None));
    let agent_manager = AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager,
        background_manager: Arc::new(BackgroundJobManager::new(event_tx)),
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        context_config: None,
        permission_config: None,
        plugin_loader: Some(loader.clone()),
        card_roots: Default::default(),
        review_snapshot_root: crate::test_support::scratch_snapshot_root(),
    });

    // What the daemon binds at startup, and what the read paths must prefer.
    agent_manager.set_plugin_handlers(
        Arc::new(crucible_lua::LuaScriptHandlerRegistry::new()),
        Arc::new(Lua::new()),
    );
    agent_manager.set_plugin_tool_registry(Arc::new(crate::plugin_tools::PluginRegistry::new()));

    let _parked_session_start = loader.lock().await;

    let plugin_state = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        (
            agent_manager.plugin_lua().await.is_some(),
            agent_manager.plugin_registry().await.is_some(),
        )
    })
    .await
    .expect("neither read may wait on the loader mutex");

    assert_eq!(
        plugin_state,
        (true, true),
        "both values must come from their startup-bound OnceLock"
    );

    // Self-check, so a guard that silently was not held cannot make this test
    // pass vacuously: anything that DOES take the loader must time out here.
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(200), loader.lock())
            .await
            .is_err(),
        "the loader must still be held, or the assertion above proved nothing"
    );
}
