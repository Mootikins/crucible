use super::*;

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

#[async_trait::async_trait]
impl AgentHandle for PendingMockAgent {
    async fn send_message_fire_and_forget(&mut self, _: String) -> ChatResult<()> {
        Ok(())
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
        session.id.clone(),
        super::RequestState {
            cancel_tx: None,
            task_handle: None,
            started_at: std::time::Instant::now(),
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

    agent_manager.agent_cache.insert(
        session.id.clone(),
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

#[tokio::test]
async fn empty_stream_without_done_cleans_up_request_state() {
    let (_tmp, session_manager, session) = setup_session_manager().await;

    let agent_manager = create_test_agent_manager(session_manager.clone());
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    agent_manager.agent_cache.insert(
        session.id.clone(),
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
        !agent_manager.request_state.contains_key(&session.id),
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

    agent_manager.agent_cache.insert(
        session.id.clone(),
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
        session.id.clone(),
        super::RequestState {
            cancel_tx: None,
            task_handle: None,
            started_at: std::time::Instant::now(),
        },
    );

    let other_kiln = TempDir::new().unwrap();
    let result = agent_manager
        .connect_kiln(&session.id, other_kiln.path(), None)
        .await;

    assert!(
        matches!(result, Err(AgentError::ConcurrentRequest(_))),
        "scope mutation during an in-flight turn should return ConcurrentRequest, got: {result:?}",
    );
    // The in-flight turn still owns the slot — the rejected mutation must not
    // have touched it.
    assert!(
        agent_manager.request_state.contains_key(&session.id),
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

    let other_kiln = TempDir::new().unwrap();
    agent_manager
        .connect_kiln(&session.id, other_kiln.path(), None)
        .await
        .expect("connect_kiln on an idle session should succeed");

    assert!(
        !agent_manager.request_state.contains_key(&session.id),
        "slot must be free once the mutation returns",
    );
}
