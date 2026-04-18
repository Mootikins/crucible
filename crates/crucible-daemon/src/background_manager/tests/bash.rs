use super::*;
use crucible_core::background::JobStatus;

#[tokio::test]
async fn spawn_bash_returns_job_id_immediately() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash("session-1", "echo hello".to_string(), None, None)
        .await
        .unwrap();

    assert!(job_id.starts_with("job-"));
}

#[tokio::test]
async fn job_appears_in_list_while_running() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash("session-1", "sleep 5".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    let jobs = manager.list_jobs("session-1");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, job_id);
    assert_eq!(jobs[0].status, JobStatus::Running);

    manager.cancel_job(&job_id).await;
}

#[tokio::test]
async fn completed_job_moves_to_history() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash("session-1", "echo done".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let result = manager.get_job_result(&job_id);
    assert!(result.is_some());

    let result = result.unwrap();
    assert!(result.info.status.is_terminal());
}

#[tokio::test]
async fn cancel_job_stops_running_job() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash("session-1", "sleep 60".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(manager.running.contains_key(&job_id));

    let cancelled = manager.cancel_job(&job_id).await;
    assert!(cancelled);

    assert!(!manager.running.contains_key(&job_id));
}

#[tokio::test]
async fn history_eviction_at_limit() {
    let (tx, _) = broadcast::channel(16);
    let mut manager = BackgroundJobManager::new(tx);
    manager.max_history = 3;

    for i in 0..5 {
        let _ = manager
            .spawn_bash("session-1", format!("echo job-{i}"), None, None)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    let jobs = manager.list_jobs("session-1");
    assert!(
        jobs.len() <= 3,
        "Should have at most 3 jobs, got {}",
        jobs.len()
    );
}

#[tokio::test]
async fn get_job_result_for_running_job() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash("session-1", "sleep 5".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    let result = manager.get_job_result(&job_id);
    assert!(result.is_some());
    assert_eq!(result.unwrap().info.status, JobStatus::Running);

    manager.cancel_job(&job_id).await;
}

#[tokio::test]
async fn cleanup_session_cancels_all_jobs() {
    let manager = create_manager();

    for i in 0..3 {
        let _ = manager
            .spawn_bash("session-1", format!("sleep {}", 10 + i), None, None)
            .await
            .unwrap();
    }

    let _ = manager
        .spawn_bash("session-2", "sleep 10".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(manager.running_count("session-1"), 3);
    assert_eq!(manager.running_count("session-2"), 1);

    manager.cleanup_session("session-1", true).await;

    assert_eq!(manager.running_count("session-1"), 0);
    assert_eq!(manager.running_count("session-2"), 1);

    manager.cleanup_session("session-2", false).await;
}

#[tokio::test]
async fn job_timeout() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash(
            "session-1",
            "sleep 10".to_string(),
            None,
            Some(Duration::from_millis(100)),
        )
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(300)).await;

    let result = manager.get_job_result(&job_id);
    assert!(result.is_some());

    let result = result.unwrap();
    assert_eq!(result.info.status, JobStatus::Failed);
    assert!(result
        .error
        .as_ref()
        .is_some_and(|e| e.contains("timed out")));
}

#[tokio::test]
async fn different_sessions_have_separate_histories() {
    let manager = create_manager();

    let _ = manager
        .spawn_bash("session-1", "echo one".to_string(), None, None)
        .await
        .unwrap();
    let _ = manager
        .spawn_bash("session-2", "echo two".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let jobs_1 = manager.list_jobs("session-1");
    let jobs_2 = manager.list_jobs("session-2");

    assert_eq!(jobs_1.len(), 1);
    assert_eq!(jobs_2.len(), 1);
    assert_ne!(jobs_1[0].id, jobs_2[0].id);
}

#[tokio::test]
async fn completed_job_preserves_started_at_for_duration() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash("session-1", "sleep 0.1".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let result = manager.get_job_result(&job_id).unwrap();
    let duration = result
        .info
        .duration()
        .expect("completed job should have duration");
    let millis = duration.num_milliseconds();

    assert!(
        millis >= 100,
        "Duration {}ms should be >= 100ms (job ran sleep 0.1)",
        millis
    );
    assert!(
        millis < 5000,
        "Duration {}ms should be < 5000ms (sanity check)",
        millis
    );
}

#[tokio::test]
async fn failed_bash_command_has_error_output() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash("session-1", "false".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let result = manager.get_job_result(&job_id);
    assert!(result.is_some());

    let result = result.unwrap();
    assert_eq!(result.info.status, JobStatus::Failed);
    assert!(result.error.is_some());
    let error = result.error.unwrap();
    assert!(error.contains("Exit code") || error.contains("1"));
}

#[tokio::test]
async fn bash_with_workdir_executes_in_directory() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash(
            "session-1",
            "pwd".to_string(),
            Some(PathBuf::from("/tmp")),
            None,
        )
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let result = manager.get_job_result(&job_id);
    assert!(result.is_some());

    let result = result.unwrap();
    assert!(result.info.status.is_terminal());
    let output = result.output.unwrap_or_default();
    assert!(output.contains("/tmp"));
}

#[tokio::test]
async fn cancel_job_for_wrong_session_is_denied() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash("session-1", "sleep 60".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    let cancelled = manager
        .cancel_job_for_session(&job_id, Some("session-2"))
        .await;
    assert!(!cancelled);

    assert!(manager.running.contains_key(&job_id));

    manager.cancel_job(&job_id).await;
}

#[tokio::test]
async fn cancel_nonexistent_job_returns_false() {
    let manager = create_manager();

    let fake_job_id = JobId::from("job-nonexistent");
    let cancelled = manager.cancel_job(&fake_job_id).await;

    assert!(!cancelled);
}

#[tokio::test]
async fn bash_events_are_broadcast() {
    let (tx, mut rx) = broadcast::channel(16);
    let manager = BackgroundJobManager::new(tx);

    let _job_id = manager
        .spawn_bash("session-1", "echo test".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("timeout waiting for event")
        .expect("failed to receive event");

    assert_eq!(event.session_id, "session-1");
    assert_eq!(event.event, events::BASH_SPAWNED);

    tokio::time::sleep(Duration::from_millis(200)).await;

    let completion_event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("timeout waiting for completion event")
        .expect("failed to receive completion event");

    assert_eq!(completion_event.session_id, "session-1");
    assert!(
        completion_event.event == events::BASH_COMPLETED
            || completion_event.event == events::BASH_FAILED
    );
}

#[tokio::test]
async fn total_running_count_across_sessions() {
    let manager = create_manager();

    let job_id_1 = manager
        .spawn_bash("session-1", "sleep 10".to_string(), None, None)
        .await
        .unwrap();

    let job_id_2 = manager
        .spawn_bash("session-2", "sleep 10".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(manager.total_running_count(), 2);

    manager.cancel_job(&job_id_1).await;

    assert_eq!(manager.total_running_count(), 1);

    manager.cancel_job(&job_id_2).await;

    assert_eq!(manager.total_running_count(), 0);
}

#[tokio::test]
async fn cleanup_session_with_clear_history_removes_history() {
    let manager = create_manager();

    let _job_id = manager
        .spawn_bash("session-1", "echo done".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let jobs_before = manager.list_jobs("session-1");
    assert_eq!(jobs_before.len(), 1);

    manager.cleanup_session("session-1", true).await;

    let jobs_after = manager.list_jobs("session-1");
    assert_eq!(jobs_after.len(), 0);
}

#[tokio::test]
async fn cleanup_session_preserves_history_when_clear_history_false() {
    let manager = create_manager();

    let _job_id = manager
        .spawn_bash("session-1", "echo done".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let jobs_before = manager.list_jobs("session-1");
    assert_eq!(jobs_before.len(), 1);

    manager.cleanup_session("session-1", false).await;

    let jobs_after = manager.list_jobs("session-1");
    assert_eq!(jobs_after.len(), 1);
}

#[tokio::test]
async fn background_spawner_trait_spawn_bash() {
    let manager = create_manager();
    let spawner: &dyn BackgroundSpawner = &manager;

    let job_id = spawner
        .spawn_bash("session-1", "echo trait".to_string(), None, None)
        .await
        .unwrap();

    assert!(job_id.starts_with("job-"));

    tokio::time::sleep(Duration::from_millis(200)).await;

    let result = spawner.get_job_result(&job_id);
    assert!(result.is_some());
    assert!(result.unwrap().info.status.is_terminal());
}
