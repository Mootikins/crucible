//! Integration tests for scheduler-driven delegation.
//!
//! Delegated children are real sessions spawned by `DelegationService` and
//! driven through `AgentManager::send_message_notified`. These tests port the
//! behavioral contract of the old background-subagent engine (policy checks,
//! events, concurrency, cancellation) and add the new session-based
//! guarantees (child sessions exist and are parent-linked, timeouts fire,
//! cleanup cascades).

use crucible_core::background::JobStatus;
use crucible_core::config::{BackendType, DelegationConfig};
use crucible_core::session::{OutputValidation, SessionAgent, SessionType};
use crucible_core::traits::chat::AgentHandle;
use crucible_daemon::agent_manager::AgentFactoryOverride;
use crucible_daemon::delegation::{DelegationRequest, DelegationService, DelegationSpawner};
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::test_support::{MockSubagentBehavior, MockSubagentHandle};
use crucible_daemon::tools::workspace::WorkspaceTools;
use crucible_daemon::{
    AgentManager, AgentManagerParams, FileSessionStorage, KilnManager, SessionManager,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::timeout;

fn delegation_config(enabled: bool, max_concurrent: u32) -> DelegationConfig {
    DelegationConfig {
        enabled,
        max_depth: 1,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: max_concurrent,
        timeout_secs: 300,
    }
}

fn parent_agent(delegation: Option<DelegationConfig>) -> SessionAgent {
    SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: Some("parent-agent".to_string()),
        provider_key: Some("ollama".to_string()),
        provider: BackendType::Ollama,
        model: "llama3.2".to_string(),
        system_prompt: "test".to_string(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: delegation,
        precognition_enabled: false,
        precognition_results: 5,
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: Default::default(),
        context_window: None,
        output_validation: OutputValidation::default(),
        validation_retries: 3,
        autocompact_threshold: None,
        mode: None,
    }
}

fn behavior_factory(behavior: MockSubagentBehavior) -> AgentFactoryOverride {
    Box::new(move |_agent: &SessionAgent, _workspace: &Path| {
        let behavior = behavior.clone();
        Box::pin(async move {
            Ok(Box::new(MockSubagentHandle::new(behavior)) as Box<dyn AgentHandle + Send + Sync>)
        })
    })
}

struct Harness {
    _temp: TempDir,
    session_manager: Arc<SessionManager>,
    agent_manager: Arc<AgentManager>,
    service: Arc<DelegationService>,
    event_rx: broadcast::Receiver<SessionEventMessage>,
    parent_id: String,
}

async fn setup(agent: SessionAgent, behavior: MockSubagentBehavior) -> Harness {
    let temp = TempDir::new().expect("temp dir");
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let (event_tx, event_rx) = broadcast::channel(64);

    let service = DelegationService::new(session_manager.clone(), event_tx.clone());
    let agent_manager = Arc::new(AgentManager::new_with_delegation(
        AgentManagerParams {
            kiln_manager: Arc::new(KilnManager::new()),
            session_manager: session_manager.clone(),
            background_manager: Arc::new(crucible_daemon::BackgroundJobManager::new(
                event_tx.clone(),
            )),
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            permission_config: None,
            plugin_loader: None,
            workspace_tools: Arc::new(WorkspaceTools::new(temp.path().to_path_buf())),
        },
        service.clone(),
    ));
    service.bind_agent_manager(&agent_manager);
    agent_manager.set_agent_factory_override(behavior_factory(behavior));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            temp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .expect("parent session");
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .expect("configure parent agent");

    Harness {
        _temp: temp,
        session_manager,
        agent_manager,
        service,
        event_rx,
        parent_id: session.id,
    }
}

fn request(harness: &Harness, prompt: &str) -> DelegationRequest {
    DelegationRequest {
        parent_session_id: harness.parent_id.clone(),
        prompt: prompt.to_string(),
        context: None,
        target_agent: None,
        description: Some("integration test".to_string()),
    }
}

async fn next_event(
    rx: &mut broadcast::Receiver<SessionEventMessage>,
    event_name: &str,
) -> SessionEventMessage {
    timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(event) = rx.recv().await {
                if event.event == event_name {
                    return event;
                }
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {event_name}"))
}

#[tokio::test]
async fn blocking_delegation_completes_with_result_and_events() {
    let mut h = setup(
        parent_agent(Some(delegation_config(true, 3))),
        MockSubagentBehavior::ImmediateSuccess("delegation-result".to_string()),
    )
    .await;

    let spawned = h
        .service
        .spawn_delegation(request(&h, "Delegate this task"))
        .await
        .expect("spawn should succeed");
    assert_eq!(spawned.delegation_id, spawned.child_session_id);

    let result = h
        .service
        .await_delegation(&spawned.delegation_id, Duration::from_secs(5))
        .await
        .expect("await should succeed");
    assert_eq!(result.info.status, JobStatus::Completed);
    assert_eq!(result.output.as_deref(), Some("delegation-result"));

    // Parent-facing lifecycle events carry the child session id.
    let spawned_ev = next_event(&mut h.event_rx, "delegation_spawned").await;
    assert_eq!(spawned_ev.session_id, h.parent_id);
    assert_eq!(
        spawned_ev.data["child_session_id"].as_str(),
        Some(spawned.child_session_id.as_str())
    );
    let completed_ev = next_event(&mut h.event_rx, "delegation_completed").await;
    assert_eq!(completed_ev.session_id, h.parent_id);
    assert_eq!(
        completed_ev.data["result_summary"].as_str(),
        Some("delegation-result")
    );
}

#[tokio::test]
async fn child_is_a_real_parent_linked_session_and_ends_on_completion() {
    let h = setup(
        parent_agent(Some(delegation_config(true, 3))),
        MockSubagentBehavior::ImmediateSuccess("ok".to_string()),
    )
    .await;

    let spawned = h
        .service
        .spawn_delegation(request(&h, "task"))
        .await
        .expect("spawn");
    let _ = h
        .service
        .await_delegation(&spawned.delegation_id, Duration::from_secs(5))
        .await
        .expect("await");

    // The child persisted as a real session in the parent's kiln, linked via
    // parent_session_id, and was ended when its turn completed.
    let parent = h
        .session_manager
        .get_session(&h.parent_id)
        .expect("parent session");
    let storage = FileSessionStorage::new();
    let child = crucible_daemon::session_storage::SessionStorage::load(
        &storage,
        &spawned.child_session_id,
        &parent.kiln,
    )
    .await
    .expect("child session persisted");
    assert_eq!(child.parent_session_id.as_deref(), Some(h.parent_id.as_str()));
    assert_eq!(child.agent.as_ref().map(|a| a.agent_type.as_str()), Some("internal"));
    assert_eq!(
        child.state,
        crucible_core::session::SessionState::Ended,
        "one-shot delegation child ends with its turn"
    );
    assert_eq!(child.title.as_deref(), Some("integration test"));
}

#[tokio::test]
async fn delegation_rejected_when_disabled() {
    let h = setup(
        parent_agent(Some(delegation_config(false, 3))),
        MockSubagentBehavior::ImmediateSuccess("unused".to_string()),
    )
    .await;

    let err = h
        .service
        .spawn_delegation(request(&h, "should not run"))
        .await
        .expect_err("disabled delegation must fail");
    assert!(err.to_string().contains("Delegation is disabled"));
    assert!(h.service.list_delegations(&h.parent_id).is_empty());
}

#[tokio::test]
async fn delegation_rejected_without_delegation_config() {
    let h = setup(
        parent_agent(None),
        MockSubagentBehavior::ImmediateSuccess("unused".to_string()),
    )
    .await;

    let err = h
        .service
        .spawn_delegation(request(&h, "no config"))
        .await
        .expect_err("missing config must fail");
    assert!(err.to_string().contains("Delegation is disabled"));
}

#[tokio::test]
async fn unknown_target_lists_available_agents() {
    let h = setup(
        parent_agent(Some(delegation_config(true, 3))),
        MockSubagentBehavior::ImmediateSuccess("unused".to_string()),
    )
    .await;

    let mut req = request(&h, "delegate to nonexistent");
    req.target_agent = Some("nonexistent".to_string());
    let err = h
        .service
        .spawn_delegation(req)
        .await
        .expect_err("unknown target should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("Delegation target 'nonexistent' not found"),
        "error should name the unknown target, got: {msg}"
    );
    // Built-in ACP profiles are always available for targeting.
    assert!(
        msg.contains("opencode") && msg.contains("claude"),
        "error should list available agents, got: {msg}"
    );
}

#[tokio::test]
async fn target_not_in_allowlist_is_rejected_and_allowlist_requires_target() {
    let mut cfg = delegation_config(true, 3);
    cfg.allowed_targets = Some(vec!["opencode".to_string()]);
    let h = setup(
        parent_agent(Some(cfg)),
        MockSubagentBehavior::ImmediateSuccess("unused".to_string()),
    )
    .await;

    let mut req = request(&h, "delegate");
    req.target_agent = Some("claude".to_string());
    let err = h.service.spawn_delegation(req).await.expect_err("not allowed");
    assert!(err.to_string().contains("'claude' is not allowed"));

    let req = request(&h, "delegate without target");
    let err = h
        .service
        .spawn_delegation(req)
        .await
        .expect_err("allowlist requires an explicit target");
    assert!(err.to_string().contains("could not be determined"));
}

#[tokio::test]
async fn self_delegation_is_rejected() {
    let mut agent = parent_agent(Some(delegation_config(true, 3)));
    agent.agent_name = Some("worker-agent".to_string());
    let h = setup(agent, MockSubagentBehavior::ImmediateSuccess("unused".to_string())).await;

    let mut req = request(&h, "delegate to self");
    req.target_agent = Some("worker-agent".to_string());
    let err = h.service.spawn_delegation(req).await.expect_err("self-delegation");
    assert!(err.to_string().contains("self-delegation guard"));
}

#[tokio::test]
async fn depth_limit_blocks_nested_delegation() {
    let h = setup(
        parent_agent(Some(delegation_config(true, 3))),
        MockSubagentBehavior::ImmediateSuccess("unused".to_string()),
    )
    .await;

    // Manufacture a session that is itself a child (depth 1) with delegation
    // enabled, then try to delegate FROM it: child depth would be 2 > max 1.
    let parent = h.session_manager.get_session(&h.parent_id).unwrap();
    let nested = h
        .session_manager
        .create_child_session(
            &parent,
            parent_agent(Some(delegation_config(true, 3))),
            Some("nested".to_string()),
        )
        .await
        .expect("child session");

    let err = h
        .service
        .spawn_delegation(DelegationRequest {
            parent_session_id: nested.id.clone(),
            prompt: "delegate deeper".to_string(),
            context: None,
            target_agent: None,
            description: None,
        })
        .await
        .expect_err("depth limit should reject");
    let msg = err.to_string();
    assert!(
        msg.contains("Delegation depth limit exceeded") && msg.contains("max_depth = 1"),
        "got: {msg}"
    );
}

#[tokio::test]
async fn concurrency_limit_enforced_and_freed_after_cancel() {
    let h = setup(
        parent_agent(Some(delegation_config(true, 1))),
        MockSubagentBehavior::Pending,
    )
    .await;

    let first = h
        .service
        .spawn_delegation(request(&h, "first"))
        .await
        .expect("first spawn under limit");

    let err = h
        .service
        .spawn_delegation(request(&h, "second"))
        .await
        .expect_err("second spawn must exceed limit 1");
    assert!(
        err.to_string()
            .contains("Maximum concurrent delegations (1) exceeded"),
        "got: {err}"
    );

    assert!(h.service.cancel_delegation(&first.delegation_id).await);
    let result = h
        .service
        .await_delegation(&first.delegation_id, Duration::from_secs(5))
        .await
        .expect("await cancelled");
    assert_eq!(result.info.status, JobStatus::Cancelled);

    // Permit released → a new delegation fits again.
    let third = h
        .service
        .spawn_delegation(request(&h, "third"))
        .await
        .expect("permit must be released after cancel");
    h.service.cancel_delegation(&third.delegation_id).await;
}

#[tokio::test]
async fn delegation_timeout_cancels_child_and_fails() {
    let mut cfg = delegation_config(true, 3);
    cfg.timeout_secs = 1;
    let mut h = setup(parent_agent(Some(cfg)), MockSubagentBehavior::Pending).await;

    let spawned = h
        .service
        .spawn_delegation(request(&h, "hang forever"))
        .await
        .expect("spawn");
    let result = h
        .service
        .await_delegation(&spawned.delegation_id, Duration::from_secs(10))
        .await
        .expect("await resolves via watcher timeout");
    assert_ne!(result.info.status, JobStatus::Completed);
    assert!(
        result.error.as_deref().unwrap_or("").contains("timed out"),
        "got: {:?}",
        result.error
    );
    let failed_ev = next_event(&mut h.event_rx, "delegation_failed").await;
    assert_eq!(failed_ev.session_id, h.parent_id);
}

#[tokio::test]
async fn child_turn_failure_surfaces_as_failed_delegation() {
    let mut h = setup(
        parent_agent(Some(delegation_config(true, 3))),
        MockSubagentBehavior::StreamFailure("child exploded".to_string()),
    )
    .await;

    let spawned = h
        .service
        .spawn_delegation(request(&h, "fail please"))
        .await
        .expect("spawn");
    let result = h
        .service
        .await_delegation(&spawned.delegation_id, Duration::from_secs(5))
        .await
        .expect("await");
    assert_eq!(result.info.status, JobStatus::Failed);
    assert!(
        result.error.as_deref().unwrap_or("").contains("child exploded"),
        "got: {:?}",
        result.error
    );
    let failed_ev = next_event(&mut h.event_rx, "delegation_failed").await;
    assert!(failed_ev.data["error"]
        .as_str()
        .unwrap_or("")
        .contains("child exploded"));
}

#[tokio::test]
async fn factory_failure_fails_spawn_and_emits_failed_event() {
    let mut h = setup(
        parent_agent(Some(delegation_config(true, 3))),
        MockSubagentBehavior::ImmediateSuccess("unused".to_string()),
    )
    .await;
    // Second override call is ignored (first wins), so build a dedicated
    // harness whose override always errors.
    let temp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let (event_tx, event_rx) = broadcast::channel(64);
    let service = DelegationService::new(session_manager.clone(), event_tx.clone());
    let agent_manager = Arc::new(AgentManager::new_with_delegation(
        AgentManagerParams {
            kiln_manager: Arc::new(KilnManager::new()),
            session_manager: session_manager.clone(),
            background_manager: Arc::new(crucible_daemon::BackgroundJobManager::new(
                event_tx.clone(),
            )),
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            permission_config: None,
            plugin_loader: None,
            workspace_tools: Arc::new(WorkspaceTools::new(temp.path().to_path_buf())),
        },
        service.clone(),
    ));
    service.bind_agent_manager(&agent_manager);
    agent_manager.set_agent_factory_override(Box::new(|_, _| {
        Box::pin(async { Err("factory boom".to_string()) })
    }));
    let session = session_manager
        .create_session(
            SessionType::Chat,
            temp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    agent_manager
        .configure_agent(&session.id, parent_agent(Some(delegation_config(true, 3))))
        .await
        .unwrap();
    drop(h.event_rx);
    h.event_rx = event_rx;

    let err = service
        .spawn_delegation(DelegationRequest {
            parent_session_id: session.id.clone(),
            prompt: "will fail to build".to_string(),
            context: None,
            target_agent: None,
            description: None,
        })
        .await
        .expect_err("factory failure fails the spawn");
    assert!(err.to_string().contains("factory boom"), "got: {err}");

    let failed_ev = next_event(&mut h.event_rx, "delegation_failed").await;
    assert_eq!(failed_ev.session_id, session.id);
}

#[tokio::test]
async fn parent_cleanup_cancels_running_children() {
    let h = setup(
        parent_agent(Some(delegation_config(true, 3))),
        MockSubagentBehavior::Pending,
    )
    .await;

    let spawned = h
        .service
        .spawn_delegation(request(&h, "hang"))
        .await
        .expect("spawn");

    // Ending/cleaning the parent must not leave the child running.
    h.agent_manager.cleanup_session(&h.parent_id);

    let result = h
        .service
        .await_delegation(&spawned.delegation_id, Duration::from_secs(5))
        .await;
    // Either the record resolved to cancelled before forget_parent dropped
    // it, or the record is already gone — both prove the child stopped.
    if let Ok(result) = result {
        assert_ne!(result.info.status, JobStatus::Completed);
    }
    // The child's request slot must be free (turn cancelled).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if !h.agent_manager.cancel(&spawned.child_session_id).await {
            break; // nothing left to cancel — child turn is gone
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "child turn still running after parent cleanup"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn list_and_get_delegation_results_report_live_status() {
    let h = setup(
        parent_agent(Some(delegation_config(true, 3))),
        MockSubagentBehavior::Pending,
    )
    .await;

    let spawned = h
        .service
        .spawn_delegation(request(&h, "hang"))
        .await
        .expect("spawn");

    let listed = h.service.list_delegations(&h.parent_id);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, spawned.delegation_id);
    assert_eq!(listed[0].status, JobStatus::Running);

    let running = h
        .service
        .get_delegation_result(&spawned.delegation_id)
        .expect("running result");
    assert_eq!(running.info.status, JobStatus::Running);
    assert!(running.output.is_none());

    h.service.cancel_delegation(&spawned.delegation_id).await;
    let result = h
        .service
        .await_delegation(&spawned.delegation_id, Duration::from_secs(5))
        .await
        .expect("await");
    assert_eq!(result.info.status, JobStatus::Cancelled);
    let final_result = h
        .service
        .get_delegation_result(&spawned.delegation_id)
        .expect("final result");
    assert_eq!(final_result.info.status, JobStatus::Cancelled);
}

#[tokio::test]
async fn child_tool_calls_are_dispatched_by_the_scheduler() {
    // The marquee regression test for the refactor: the old background loop
    // never executed child tool calls (no inbound channel). Children now run
    // through the scheduler, so a child's tool call must actually dispatch
    // and produce a tool_result event on the CHILD session.
    use crucible_core::turn::{StopReason, TurnEvent};

    struct ToolCallingAgent;
    #[async_trait::async_trait]
    impl crucible_core::turn::Agent for ToolCallingAgent {
        fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
            crucible_core::turn::AgentCapabilities::default()
        }
        async fn turn<'a>(
            &'a mut self,
            ctx: crucible_core::turn::TurnContext,
        ) -> Result<
            futures::stream::BoxStream<'a, TurnEvent>,
            crucible_core::turn::AgentError,
        > {
            let mut inbound = ctx.inbound;
            let body = async_stream::stream! {
                yield TurnEvent::TextDelta("checking file ".to_string());
                yield TurnEvent::ToolCall {
                    id: "call-1".to_string(),
                    name: "read_file".to_string(),
                    args: serde_json::json!({"path": "probe.txt"}),
                    diffs: Vec::new(),
                };
                yield TurnEvent::ToolBatchEnd;
                if let Some(rx) = inbound.as_mut() {
                    // The scheduler must feed the ToolResult back; the old
                    // background loop never did.
                    while let Some(ev) = rx.recv().await {
                        if let TurnEvent::ToolResult { result, .. } = ev {
                            let text = result.as_str().unwrap_or_default().to_string();
                            if text.contains("TOOL-PROBE-CONTENT") {
                                yield TurnEvent::TextDelta("tool-result-received".to_string());
                            } else {
                                yield TurnEvent::TextDelta(format!("unexpected: {text}"));
                            }
                            break;
                        }
                    }
                }
                yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
            };
            Ok(Box::pin(body))
        }
        async fn cancel(&self) -> Result<(), crucible_core::turn::AgentError> {
            Ok(())
        }
        async fn switch_model(
            &mut self,
            _: &str,
        ) -> Result<(), crucible_core::turn::NotSupported> {
            Err(crucible_core::turn::NotSupported::new("switch_model"))
        }
    }
    #[async_trait::async_trait]
    impl AgentHandle for ToolCallingAgent {
        async fn send_message_fire_and_forget(
            &mut self,
            _: String,
        ) -> crucible_core::traits::chat::ChatResult<()> {
            Ok(())
        }
        async fn set_mode_str(&mut self, _: &str) -> crucible_core::traits::chat::ChatResult<()> {
            Ok(())
        }
    }

    let temp = TempDir::new().unwrap();
    std::fs::write(temp.path().join("probe.txt"), "TOOL-PROBE-CONTENT").unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let (event_tx, mut event_rx) = broadcast::channel(256);
    let service = DelegationService::new(session_manager.clone(), event_tx.clone());
    let agent_manager = Arc::new(AgentManager::new_with_delegation(
        AgentManagerParams {
            kiln_manager: Arc::new(KilnManager::new()),
            session_manager: session_manager.clone(),
            background_manager: Arc::new(crucible_daemon::BackgroundJobManager::new(
                event_tx.clone(),
            )),
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            permission_config: None,
            plugin_loader: None,
            workspace_tools: Arc::new(WorkspaceTools::new(temp.path().to_path_buf())),
        },
        service.clone(),
    ));
    service.bind_agent_manager(&agent_manager);
    agent_manager.set_agent_factory_override(Box::new(|_, _| {
        Box::pin(async {
            Ok(Box::new(ToolCallingAgent) as Box<dyn AgentHandle + Send + Sync>)
        })
    }));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            temp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    agent_manager
        .configure_agent(&session.id, parent_agent(Some(delegation_config(true, 3))))
        .await
        .unwrap();

    let spawned = service
        .spawn_delegation(DelegationRequest {
            parent_session_id: session.id.clone(),
            prompt: "read the probe file".to_string(),
            context: None,
            target_agent: None,
            description: None,
        })
        .await
        .expect("spawn");
    let result = service
        .await_delegation(&spawned.delegation_id, Duration::from_secs(10))
        .await
        .expect("await");

    assert_eq!(result.info.status, JobStatus::Completed, "err: {:?}", result.error);
    let output = result.output.unwrap_or_default();
    assert!(
        output.contains("tool-result-received"),
        "child must receive the dispatched tool result; got: {output}"
    );

    // The child emitted per-turn events on its OWN session id, including the
    // tool_call/tool_result pair — the observability the old loop lacked.
    let mut saw_child_tool_result = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_millis(200), event_rx.recv()).await {
            Ok(Ok(ev)) => {
                if ev.session_id == spawned.child_session_id && ev.event == "tool_result" {
                    saw_child_tool_result = true;
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(
        saw_child_tool_result,
        "child session must emit tool_result events"
    );
}
