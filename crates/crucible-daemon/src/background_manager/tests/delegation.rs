use super::*;
use crucible_core::background::JobStatus;

#[tokio::test]
async fn delegation_happy_path_returns_result_to_parent() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess(
            "delegation-result".to_string(),
        )),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
    );

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            Some("delegation-context".to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("delegation should succeed");

    assert_eq!(result.info.status, JobStatus::Completed);
    assert_eq!(result.output.as_deref(), Some("delegation-result"));
}

#[tokio::test]
async fn delegation_rejected_when_disabled() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
        Some(DelegationConfig {
            enabled: false,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
    );

    let err = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("disabled delegation should be rejected");

    assert!(matches!(err, BackgroundError::SpawnFailed(_)));
    assert!(err.to_string().contains("Delegation is disabled"));
}

#[tokio::test]
async fn delegation_rejected_when_target_not_allowed() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: Some(vec!["allowed-agent".to_string()]),
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
    );

    let err = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("unauthorized delegation target should be rejected");

    assert!(matches!(err, BackgroundError::SpawnFailed(_)));
    assert!(err.to_string().contains("not allowed"));
}

#[tokio::test]
async fn test_delegation_with_target_uses_different_session_agent() {
    let observed = Arc::new(StdMutex::new(None));
    let observed_for_factory = observed.clone();
    let manager = make_subagent_manager_with_factory_and_identity(
        Box::new(move |agent, _workspace| {
            let mut lock = observed_for_factory
                .lock()
                .expect("observation mutex should be available");
            *lock = Some(agent.clone());
            Box::pin(async move {
                Ok(Box::new(MockSubagentHandle::new(
                    MockSubagentBehavior::ImmediateSuccess("ok".to_string()),
                )) as Box<dyn AgentHandle + Send + Sync>)
            })
        }),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: Some(vec!["cursor".to_string()]),
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
        Some("parent-agent"),
        Some("worker-agent"),
    );
    let mut agent_profiles = HashMap::new();
    agent_profiles.insert(
        "cursor".to_string(),
        test_agent_profile("cursor-acp", &["acp"]),
    );
    manager.register_subagent_context(
        "session-1",
        SubagentContext {
            agent: test_session_agent(Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: Some(vec!["cursor".to_string()]),
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            })),
            available_agents: agent_profiles,
            workspace: std::env::temp_dir(),
            parent_session_id: Some("session-1".to_string()),
            parent_session_dir: None,
            delegator_agent_name: Some("parent-agent".to_string()),
            target_agent_name: Some("worker-agent".to_string()),
            delegation_depth: 0,
        },
    );

    let _ = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            Some("Target agent: cursor".to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("delegation with explicit target should succeed");

    let observed = observed
        .lock()
        .expect("observation mutex should be available")
        .clone()
        .expect("factory should have observed target agent config");
    assert_eq!(observed.agent_name.as_deref(), Some("cursor"));
    assert_eq!(observed.model, "cursor");
}

#[tokio::test]
async fn test_delegation_with_target_validates_allowed() {
    let manager = make_subagent_manager_with_factory_and_identity(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: Some(vec!["allowed-agent".to_string()]),
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
        Some("parent-agent"),
        None,
    );
    let mut agent_profiles = HashMap::new();
    agent_profiles.insert(
        "cursor".to_string(),
        test_agent_profile("cursor-acp", &["acp"]),
    );
    manager.register_subagent_context(
        "session-1",
        SubagentContext {
            agent: test_session_agent(Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: Some(vec!["allowed-agent".to_string()]),
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            })),
            available_agents: agent_profiles,
            workspace: std::env::temp_dir(),
            parent_session_id: Some("session-1".to_string()),
            parent_session_dir: None,
            delegator_agent_name: Some("parent-agent".to_string()),
            target_agent_name: None,
            delegation_depth: 0,
        },
    );

    let err = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            Some("Target agent: cursor".to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("unauthorized explicit target should be rejected");

    assert!(matches!(err, BackgroundError::SpawnFailed(_)));
    assert!(err.to_string().contains("not allowed"));
}

#[tokio::test]
async fn test_delegation_with_unknown_target_returns_available_agents() {
    let manager = make_subagent_manager_with_factory_and_identity(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: Some(vec!["ghost".to_string()]),
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
        Some("parent-agent"),
        None,
    );

    let err = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            Some("Target agent: ghost".to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("unknown explicit target should fail with available list");

    let msg = err.to_string();
    assert!(msg.contains("Delegation target 'ghost' not found"));
    assert!(msg.contains("Available agents:"));
}

#[tokio::test]
async fn test_delegation_without_target_uses_parent_agent() {
    let observed = Arc::new(StdMutex::new(None));
    let observed_for_factory = observed.clone();
    let manager = make_subagent_manager_with_factory_and_identity(
        Box::new(move |agent, _workspace| {
            let mut lock = observed_for_factory
                .lock()
                .expect("observation mutex should be available");
            *lock = Some(agent.clone());
            Box::pin(async move {
                Ok(Box::new(MockSubagentHandle::new(
                    MockSubagentBehavior::ImmediateSuccess("ok".to_string()),
                )) as Box<dyn AgentHandle + Send + Sync>)
            })
        }),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
        Some("parent-agent"),
        None,
    );

    let _ = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            Some("Delegation ID: deleg-1\nDescription: no explicit target".to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("delegation without explicit target should succeed");

    let observed = observed
        .lock()
        .expect("observation mutex should be available")
        .clone()
        .expect("factory should have observed parent agent config");
    assert_eq!(observed.agent_name.as_deref(), Some("test-agent"));
    assert_eq!(observed.model, "test-agent");
}

#[tokio::test]
async fn delegation_timeout_returns_failed_job_result() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::Pending),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
    );

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            None,
            SubagentBlockingConfig {
                timeout: Duration::from_millis(30),
                result_max_bytes: 51200,
            },
            None,
        )
        .await
        .expect("timeout should return a failed JobResult");

    assert_eq!(result.info.status, JobStatus::Failed);
    assert!(result.error.as_deref().unwrap_or("").contains("timed out"));
}

#[tokio::test]
async fn delegation_unavailable_agent_returns_error() {
    let manager = make_subagent_manager_with_factory(
        Box::new(move |_agent, _workspace| {
            Box::pin(async move { Err("command not found: mock-subagent".to_string()) })
        }),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
    );

    let err = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("unavailable target agent should return error");

    assert!(matches!(err, BackgroundError::SpawnFailed(_)));
    assert!(err.to_string().contains("command not found"));
}

#[tokio::test]
async fn delegation_self_delegation_guard_rejects_same_agent() {
    let manager = make_subagent_manager_with_factory_and_identity(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: Some(vec!["parent-agent".to_string()]),
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
        Some("parent-agent"),
        Some("parent-agent"),
    );

    let err = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("self delegation must be rejected");

    assert!(matches!(err, BackgroundError::SpawnFailed(_)));
    assert!(err.to_string().contains("self-delegation"));
}

#[tokio::test]
async fn delegation_result_truncation_respects_config_limit() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess("y".repeat(200))),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 16,
            max_concurrent_delegations: 3,
        }),
    );

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            None,
            SubagentBlockingConfig {
                timeout: Duration::from_secs(1),
                result_max_bytes: 16,
            },
            None,
        )
        .await
        .expect("delegation should complete");

    let output = result.output.unwrap_or_default();
    assert!(output.len() <= 16, "output length was {}", output.len());
}

#[tokio::test]
async fn delegation_blocking_emits_spawned_and_completed_events() {
    let (manager, mut rx) = make_subagent_manager_with_factory_and_events(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess(
            "eventful-result".to_string(),
        )),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
    );

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("blocking delegation should succeed");

    assert_eq!(result.info.status, JobStatus::Completed);

    let mut saw_spawned = false;
    let mut saw_completed = false;
    for _ in 0..5 {
        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timeout waiting for delegation event")
            .expect("failed to receive delegation event");

        if event.event == events::SUBAGENT_SPAWNED {
            saw_spawned = true;
        }
        if event.event == events::SUBAGENT_COMPLETED {
            saw_completed = true;
        }

        if saw_spawned && saw_completed {
            break;
        }
    }

    assert!(saw_spawned, "expected {} event", events::SUBAGENT_SPAWNED);
    assert!(
        saw_completed,
        "expected {} event",
        events::SUBAGENT_COMPLETED
    );
}

#[tokio::test]
async fn delegation_rejected_when_max_concurrent_delegations_reached() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::Pending),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 1,
        }),
    );

    let first_job_id = manager
        .spawn_subagent("session-1", "delegate first".to_string(), None)
        .await
        .expect("first delegation should spawn");

    tokio::time::sleep(Duration::from_millis(25)).await;

    let err = manager
        .spawn_subagent("session-1", "delegate second".to_string(), None)
        .await
        .expect_err("second delegation should be rejected at concurrency limit");

    assert!(err
        .to_string()
        .contains("Maximum concurrent delegations (1) exceeded"));

    manager.cancel_job(&first_job_id).await;
}

#[tokio::test]
async fn delegation_under_max_concurrent_delegations_is_allowed() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::Pending),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 2,
        }),
    );

    let first_job_id = manager
        .spawn_subagent("session-1", "delegate first".to_string(), None)
        .await
        .expect("first delegation should spawn");
    let second_job_id = manager
        .spawn_subagent("session-1", "delegate second".to_string(), None)
        .await
        .expect("second delegation should still be allowed under limit");

    manager.cancel_job(&first_job_id).await;
    manager.cancel_job(&second_job_id).await;
}

#[tokio::test]
async fn completed_delegation_frees_concurrency_slot() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::DelayedSuccess {
            output: "done".to_string(),
            delay: Duration::from_millis(80),
        }),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 1,
        }),
    );

    let _first = manager
        .spawn_subagent("session-1", "delegate first".to_string(), None)
        .await
        .expect("first delegation should spawn");

    tokio::time::sleep(Duration::from_millis(10)).await;

    let blocked = manager
        .spawn_subagent("session-1", "delegate blocked".to_string(), None)
        .await;
    assert!(
        blocked.is_err(),
        "second delegation should be blocked while first is running"
    );

    tokio::time::sleep(Duration::from_millis(120)).await;

    let second = manager
        .spawn_subagent("session-1", "delegate second".to_string(), None)
        .await
        .expect("delegation slot should be freed after completion");

    manager.cancel_job(&second).await;
}

#[tokio::test]
async fn failed_delegation_frees_concurrency_slot() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::DelayedFailure {
            error: "boom".to_string(),
            delay: Duration::from_millis(80),
        }),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 1,
        }),
    );

    let _first = manager
        .spawn_subagent("session-1", "delegate first".to_string(), None)
        .await
        .expect("first delegation should spawn");

    tokio::time::sleep(Duration::from_millis(10)).await;

    let blocked = manager
        .spawn_subagent("session-1", "delegate blocked".to_string(), None)
        .await;
    assert!(
        blocked.is_err(),
        "second delegation should be blocked while first is running"
    );

    tokio::time::sleep(Duration::from_millis(120)).await;

    let second = manager
        .spawn_subagent("session-1", "delegate second".to_string(), None)
        .await
        .expect("delegation slot should be freed after failure");

    manager.cancel_job(&second).await;
}

#[tokio::test]
async fn delegation_writes_parent_session_id_and_incremented_depth_to_child_session() {
    let parent_dir = tempfile::TempDir::new().expect("temp dir should be created");
    let (tx, _) = broadcast::channel(16);
    let manager = BackgroundJobManager::new(tx).with_subagent_factory(behavior_factory(
        MockSubagentBehavior::ImmediateSuccess("ok".to_string()),
    ));
    manager.register_subagent_context(
        "session-1",
        SubagentContext {
            agent: test_session_agent(Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: None,
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            })),
            available_agents: HashMap::new(),
            workspace: std::env::temp_dir(),
            parent_session_id: Some("session-1".to_string()),
            parent_session_dir: Some(parent_dir.path().to_path_buf()),
            delegator_agent_name: Some("parent-agent".to_string()),
            target_agent_name: Some("worker-agent".to_string()),
            delegation_depth: 0,
        },
    );

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("delegation should complete");

    let session_path = result
        .info
        .session_path
        .expect("subagent session path should exist");
    let jsonl_path = session_path.join("session.jsonl");
    let contents = tokio::fs::read_to_string(&jsonl_path)
        .await
        .expect("subagent session jsonl should be readable");

    let metadata_line = contents
        .lines()
        .find_map(|line| {
            let event: serde_json::Value = serde_json::from_str(line).ok()?;
            if event.get("type")?.as_str()? != "system" {
                return None;
            }
            let content = event.get("content")?.as_str()?;
            serde_json::from_str::<serde_json::Value>(content).ok()
        })
        .expect("delegation metadata system event should exist");

    assert_eq!(
        metadata_line["delegation_metadata"]["parent_session_id"]
            .as_str()
            .expect("parent_session_id should be present"),
        "session-1"
    );
    assert_eq!(
        metadata_line["delegation_metadata"]["delegation_depth"]
            .as_u64()
            .expect("delegation_depth should be present"),
        1
    );
}
