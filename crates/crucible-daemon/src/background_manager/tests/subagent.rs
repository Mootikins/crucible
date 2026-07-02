use super::*;
use crucible_core::background::JobStatus;
use std::time::Instant;

#[tokio::test]
async fn spawn_subagent_blocking_returns_job_result_with_output() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::DelayedSuccess {
            output: "blocking-complete".to_string(),
            delay: Duration::from_millis(75),
        }),
        None,
    );
    let start = Instant::now();

    let result: Result<JobResult, BackgroundError> = manager
        .spawn_subagent_blocking(
            "session-1",
            "do it".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await;

    let result = result.expect("blocking subagent should complete");
    assert!(start.elapsed() >= Duration::from_millis(70));
    assert_eq!(result.info.status, JobStatus::Completed);
    assert_eq!(result.output.as_deref(), Some("blocking-complete"));
}

#[tokio::test]
async fn spawn_subagent_blocking_timeout_returns_failed_job_result() {
    let manager =
        make_subagent_manager_with_factory(behavior_factory(MockSubagentBehavior::Pending), None);

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "do it".to_string(),
            None,
            SubagentBlockingConfig {
                timeout: Duration::from_millis(50),
                result_max_bytes: 51200,
                max_turns: None,
            },
            None,
        )
        .await
        .expect("timeout should return JobResult");

    assert_eq!(result.info.status, JobStatus::Failed);
    assert!(result.error.as_deref().unwrap_or("").contains("timed out"));
}

#[tokio::test]
async fn spawn_subagent_blocking_cancellation_marks_job_cancelled() {
    let manager =
        make_subagent_manager_with_factory(behavior_factory(MockSubagentBehavior::Pending), None);
    let (cancel_tx, cancel_rx) = oneshot::channel();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        let _ = cancel_tx.send(());
    });

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "do it".to_string(),
            None,
            SubagentBlockingConfig::default(),
            Some(cancel_rx),
        )
        .await
        .expect("cancelled execution should return JobResult");

    assert_eq!(result.info.status, JobStatus::Cancelled);
    assert!(result.error.as_deref().unwrap_or("").contains("cancelled"));
}

#[tokio::test]
async fn spawn_subagent_blocking_factory_failure_returns_background_error() {
    let manager = make_subagent_manager_with_factory(
        Box::new(move |_agent, _workspace| {
            Box::pin(async move { Err("factory failed".to_string()) })
        }),
        None,
    );

    let err = manager
        .spawn_subagent_blocking(
            "session-1",
            "do it".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("factory failure should return BackgroundError");

    assert!(matches!(err, BackgroundError::SpawnFailed(_)));
}

#[tokio::test]
async fn spawn_subagent_blocking_execution_failure_returns_failed_job_result() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::StreamFailure(
            "agent-stream-broke".to_string(),
        )),
        None,
    );

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "do it".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("execution failure should still return JobResult");

    assert_eq!(result.info.status, JobStatus::Failed);
    assert!(result
        .error
        .as_deref()
        .unwrap_or("")
        .contains("agent-stream-broke"));
}

#[tokio::test]
async fn spawn_subagent_blocking_truncates_output_to_configured_max_bytes() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess("x".repeat(512))),
        None,
    );

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "do it".to_string(),
            None,
            SubagentBlockingConfig {
                timeout: Duration::from_secs(1),
                result_max_bytes: 32,
                max_turns: None,
            },
            None,
        )
        .await
        .expect("subagent should complete");

    let output = result.output.unwrap_or_default();
    assert!(output.len() <= 32, "output length was {}", output.len());
}

#[tokio::test]
async fn spawn_subagent_blocking_respects_max_turns_cap() {
    // The mock emits a tool call every turn, so without a cap it would run
    // DEFAULT_SUBAGENT_MAX_TURNS times. With max_turns=1 it must stop after one.
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::RepeatingToolCall("turn".to_string())),
        None,
    );

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "do it".to_string(),
            None,
            SubagentBlockingConfig {
                max_turns: Some(1),
                ..SubagentBlockingConfig::default()
            },
            None,
        )
        .await
        .expect("blocking run should succeed");

    assert_eq!(result.info.status, JobStatus::Completed);
    let output = result.output.unwrap_or_default();
    assert_eq!(
        output.matches("turn").count(),
        1,
        "max_turns=1 should produce exactly one turn of output, got: {output:?}"
    );
}

#[tokio::test]
async fn spawn_subagent_blocking_max_turns_defaults_when_unset() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::RepeatingToolCall("turn".to_string())),
        None,
    );

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "do it".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("blocking run should succeed");

    let output = result.output.unwrap_or_default();
    assert!(
        output.matches("turn").count() > 1,
        "unset max_turns should fall back to the multi-turn default, got: {output:?}"
    );
}

#[tokio::test]
async fn spawn_subagent_blocking_disables_nested_delegation_before_factory() {
    let observed = Arc::new(StdMutex::new(None));
    let observed_for_factory = observed.clone();
    let manager = make_subagent_manager_with_factory_and_identity(
        Box::new(move |agent, _workspace| {
            let mut lock = observed_for_factory
                .lock()
                .expect("observation mutex should be available");
            *lock = Some(agent.delegation_config.clone());
            Box::pin(async move {
                Ok(Box::new(MockSubagentHandle::new(
                    MockSubagentBehavior::ImmediateSuccess("ok".to_string()),
                )) as Box<dyn AgentHandle + Send + Sync>)
            })
        }),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 2,
            allowed_targets: Some(vec!["worker-agent".to_string()]),
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
        Some("parent-agent"),
        Some("worker-agent"),
    );

    let _ = manager
        .spawn_subagent_blocking(
            "session-1",
            "do it".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("blocking run should succeed");

    let observed = observed
        .lock()
        .expect("observation mutex should be available")
        .clone();
    assert_eq!(observed, Some(None));
}
