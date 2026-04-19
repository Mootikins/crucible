use super::super::*;
use tokio::sync::mpsc;

#[tokio::test]
async fn replay_consumer_handles_delegation_spawned() {
    use serde_json::json;
    use tokio::time::{timeout, Duration};

    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
    let (event_tx, event_rx) = mpsc::unbounded_channel();

    let replay_session_id = "test-session-delegation-spawned".to_string();
    let session_id_clone = replay_session_id.clone();

    let consumer_task = tokio::spawn(async move {
        session_event_consumer(session_id_clone, event_rx, msg_tx, None).await;
    });

    event_tx
        .send(crucible_daemon::SessionEvent {
            session_id: replay_session_id.clone(),
            event_type: "delegation_spawned".to_string(),
            data: json!({
                "delegation_id": "d1",
                "prompt": "test prompt",
                "target_agent": "opencode"
            }),
        })
        .unwrap();

    let msg = timeout(Duration::from_secs(1), msg_rx.recv())
        .await
        .expect("Timeout waiting for message")
        .expect("Should receive a message");

    match msg {
        ChatAppMsg::DelegationSpawned {
            id,
            prompt,
            target_agent,
        } => {
            assert_eq!(id, "d1");
            assert_eq!(prompt, "test prompt");
            assert_eq!(target_agent, Some("opencode".to_string()));
        }
        other => panic!("Expected DelegationSpawned, got {:?}", other),
    }

    event_tx
        .send(crucible_daemon::SessionEvent {
            session_id: replay_session_id,
            event_type: "replay_complete".to_string(),
            data: json!({}),
        })
        .unwrap();
    drop(event_tx);

    timeout(Duration::from_secs(1), consumer_task)
        .await
        .expect("Timeout waiting for consumer task")
        .expect("Consumer task should complete");
}

#[tokio::test]
async fn replay_consumer_handles_delegation_completed() {
    use serde_json::json;
    use tokio::time::{timeout, Duration};

    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
    let (event_tx, event_rx) = mpsc::unbounded_channel();

    let replay_session_id = "test-session-delegation-completed".to_string();
    let session_id_clone = replay_session_id.clone();

    let consumer_task = tokio::spawn(async move {
        session_event_consumer(session_id_clone, event_rx, msg_tx, None).await;
    });

    event_tx
        .send(crucible_daemon::SessionEvent {
            session_id: replay_session_id.clone(),
            event_type: "delegation_completed".to_string(),
            data: json!({
                "delegation_id": "d1",
                "result_summary": "test summary"
            }),
        })
        .unwrap();

    let msg = timeout(Duration::from_secs(1), msg_rx.recv())
        .await
        .expect("Timeout waiting for message")
        .expect("Should receive a message");

    match msg {
        ChatAppMsg::DelegationCompleted { id, summary } => {
            assert_eq!(id, "d1");
            assert_eq!(summary, "test summary");
        }
        other => panic!("Expected DelegationCompleted, got {:?}", other),
    }

    event_tx
        .send(crucible_daemon::SessionEvent {
            session_id: replay_session_id,
            event_type: "replay_complete".to_string(),
            data: json!({}),
        })
        .unwrap();
    drop(event_tx);

    timeout(Duration::from_secs(1), consumer_task)
        .await
        .expect("Timeout waiting for consumer task")
        .expect("Consumer task should complete");
}

#[tokio::test]
async fn replay_consumer_handles_delegation_failed() {
    use serde_json::json;
    use tokio::time::{timeout, Duration};

    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel();
    let (event_tx, event_rx) = mpsc::unbounded_channel();

    let replay_session_id = "test-session-delegation-failed".to_string();
    let session_id_clone = replay_session_id.clone();

    let consumer_task = tokio::spawn(async move {
        session_event_consumer(session_id_clone, event_rx, msg_tx, None).await;
    });

    event_tx
        .send(crucible_daemon::SessionEvent {
            session_id: replay_session_id.clone(),
            event_type: "delegation_failed".to_string(),
            data: json!({
                "delegation_id": "d1",
                "error": "test failure"
            }),
        })
        .unwrap();

    let msg = timeout(Duration::from_secs(1), msg_rx.recv())
        .await
        .expect("Timeout waiting for message")
        .expect("Should receive a message");

    match msg {
        ChatAppMsg::DelegationFailed { id, error } => {
            assert_eq!(id, "d1");
            assert_eq!(error, "test failure");
        }
        other => panic!("Expected DelegationFailed, got {:?}", other),
    }

    event_tx
        .send(crucible_daemon::SessionEvent {
            session_id: replay_session_id,
            event_type: "replay_complete".to_string(),
            data: json!({}),
        })
        .unwrap();
    drop(event_tx);

    timeout(Duration::from_secs(1), consumer_task)
        .await
        .expect("Timeout waiting for consumer task")
        .expect("Consumer task should complete");
}
