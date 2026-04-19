use super::*;

#[tokio::test]
async fn delegation_spawned_event_emitted_on_parent_channel() {
    let (manager, mut rx) = make_subagent_manager_with_factory_and_events(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
        None,
    );

    let _ = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate task".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("delegation should succeed");

    let mut saw_delegation_spawned = false;
    for _ in 0..10 {
        match tokio::time::timeout(Duration::from_secs(1), rx.recv()).await {
            Ok(Ok(event)) => {
                if event.event == events::DELEGATION_SPAWNED {
                    saw_delegation_spawned = true;
                    assert_eq!(event.session_id, "session-1");
                    assert!(event.data["delegation_id"].as_str().is_some());
                    assert_eq!(event.data["parent_session_id"].as_str(), Some("session-1"));
                    assert!(event.data["prompt"]
                        .as_str()
                        .unwrap_or("")
                        .contains("delegate"));
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(
        saw_delegation_spawned,
        "expected delegation_spawned event on parent channel"
    );
}

#[tokio::test]
async fn delegation_completed_event_emitted_on_parent_channel() {
    let (manager, mut rx) = make_subagent_manager_with_factory_and_events(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess(
            "result-data".to_string(),
        )),
        None,
    );

    let _ = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate task".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("delegation should succeed");

    let mut saw_delegation_completed = false;
    for _ in 0..10 {
        match tokio::time::timeout(Duration::from_secs(1), rx.recv()).await {
            Ok(Ok(event)) => {
                if event.event == events::DELEGATION_COMPLETED {
                    saw_delegation_completed = true;
                    assert_eq!(event.session_id, "session-1");
                    assert!(event.data["delegation_id"].as_str().is_some());
                    assert_eq!(event.data["parent_session_id"].as_str(), Some("session-1"));
                    assert!(event.data["result_summary"]
                        .as_str()
                        .unwrap_or("")
                        .contains("result-data"));
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(
        saw_delegation_completed,
        "expected delegation_completed event on parent channel"
    );
}

#[tokio::test]
async fn delegation_failed_event_emitted_on_parent_channel() {
    let (manager, mut rx) = make_subagent_manager_with_factory_and_events(
        behavior_factory(MockSubagentBehavior::StreamFailure(
            "agent-crashed".to_string(),
        )),
        None,
    );

    let _ = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate task".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("failed delegation still returns a JobResult");

    let mut saw_delegation_failed = false;
    for _ in 0..10 {
        match tokio::time::timeout(Duration::from_secs(1), rx.recv()).await {
            Ok(Ok(event)) => {
                if event.event == events::DELEGATION_FAILED {
                    saw_delegation_failed = true;
                    assert_eq!(event.session_id, "session-1");
                    assert!(event.data["delegation_id"].as_str().is_some());
                    assert_eq!(event.data["parent_session_id"].as_str(), Some("session-1"));
                    assert!(event.data["error"]
                        .as_str()
                        .unwrap_or("")
                        .contains("agent-crashed"));
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(
        saw_delegation_failed,
        "expected delegation_failed event on parent channel"
    );
}

#[tokio::test]
async fn non_delegation_subagent_does_not_emit_delegation_events() {
    let (tx, mut rx) = broadcast::channel(32);
    let manager = BackgroundJobManager::new(tx).with_subagent_factory(behavior_factory(
        MockSubagentBehavior::ImmediateSuccess("ok".to_string()),
    ));
    manager.register_subagent_context(
        "session-1",
        SubagentContext {
            agent: test_session_agent(Some(default_enabled_delegation_config())),
            available_agents: HashMap::new(),
            workspace: std::env::temp_dir(),
            parent_session_id: None,
            parent_session_dir: None,
            delegator_agent_name: Some("parent".to_string()),
            target_agent_name: Some("child".to_string()),
            delegation_depth: 0,
        },
    );

    let _ = manager
        .spawn_subagent_blocking(
            "session-1",
            "do task".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("subagent should succeed");

    let mut delegation_events = vec![];
    while let Ok(Ok(event)) = tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
        if event.event == events::DELEGATION_SPAWNED
            || event.event == events::DELEGATION_COMPLETED
            || event.event == events::DELEGATION_FAILED
        {
            delegation_events.push(event.event.clone());
        }
    }
    assert!(
        delegation_events.is_empty(),
        "non-delegation subagent should not emit delegation events, got: {:?}",
        delegation_events
    );
}
