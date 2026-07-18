use super::*;

#[tokio::test]
async fn test_process_batch_emits_per_file_progress_events() {
    use std::time::Duration;

    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().join("kiln");
    std::fs::create_dir_all(&kiln_path).unwrap();

    let good_file = kiln_path.join("ok.md");
    std::fs::write(&good_file, "# ok\n").unwrap();
    let missing_file = kiln_path.join("missing.md");

    let km = Arc::new(KilnManager::new());
    let (event_tx, _) = broadcast::channel(64);
    let mut event_rx = event_tx.subscribe();

    let req = Request {
        jsonrpc: "2.0".to_string(),
        id: Some(RequestId::Number(42)),
        method: "process_batch".to_string(),
        params: serde_json::json!({
            "kiln": kiln_path.to_string_lossy(),
            "paths": [
                good_file.to_string_lossy(),
                missing_file.to_string_lossy()
            ]
        }),
    };

    let response = crate::server::kiln::handle_process_batch(req, &km, &event_tx).await;
    assert!(response.error.is_none());

    let mut events = Vec::new();
    for _ in 0..4 {
        let event = tokio::time::timeout(Duration::from_secs(2), event_rx.recv())
            .await
            .expect("timed out waiting for process event")
            .expect("event channel closed unexpectedly");
        events.push(event);
    }

    let progress_events: Vec<&SessionEventMessage> = events
        .iter()
        .filter(|e| e.event == "process_progress")
        .collect();
    assert_eq!(
        progress_events.len(),
        2,
        "expected 2 process_progress events"
    );

    let processed_event = progress_events
        .iter()
        .find(|e| {
            e.data.get("file").and_then(|v| v.as_str())
                == Some(good_file.to_string_lossy().as_ref())
        })
        .expect("missing progress event for processed file");
    assert_eq!(
        processed_event.data.get("type").and_then(|v| v.as_str()),
        Some("process_progress")
    );
    assert_eq!(
        processed_event.data.get("result").and_then(|v| v.as_str()),
        Some("processed")
    );

    let error_event = progress_events
        .iter()
        .find(|e| {
            e.data.get("file").and_then(|v| v.as_str())
                == Some(missing_file.to_string_lossy().as_ref())
        })
        .expect("missing progress event for failed file");
    assert_eq!(
        error_event.data.get("result").and_then(|v| v.as_str()),
        Some("error")
    );
    assert!(error_event
        .data
        .get("error_msg")
        .and_then(|v| v.as_str())
        .is_some());
}

#[tokio::test]
async fn test_file_deleted_event_removes_note_from_store() {
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::NoteRecord;
    use std::time::Duration;

    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");
    let kiln_path = tmp.path().join("kiln");
    std::fs::create_dir_all(kiln_path.join("notes")).unwrap();

    let server = Server::bind_with_data_home(&sock_path, tmp.path().to_path_buf())
        .await
        .unwrap();
    let km = server.kiln_manager.clone();
    let event_tx = server.event_sender();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(server.run());
    tokio::time::sleep(Duration::from_millis(50)).await;

    let handle = km.get_or_open(&kiln_path).await.unwrap();
    let note_store = handle.as_note_store();

    let deleted_note_path = "notes/deleted.md";
    let keep_note_path = "notes/keep.md";

    note_store
        .upsert(
            NoteRecord::new(deleted_note_path, BlockHash::zero())
                .with_title("Deleted")
                .with_links(vec!["notes/target.md".to_string()]),
        )
        .await
        .unwrap();
    note_store
        .upsert(NoteRecord::new(keep_note_path, BlockHash::zero()).with_title("Keep"))
        .await
        .unwrap();

    assert!(note_store
        .get(
            deleted_note_path,
            &crucible_core::storage::Scope::workspace_unchecked(std::path::PathBuf::new())
        )
        .await
        .unwrap()
        .is_some());
    assert!(note_store
        .get(
            keep_note_path,
            &crucible_core::storage::Scope::workspace_unchecked(std::path::PathBuf::new())
        )
        .await
        .unwrap()
        .is_some());

    event_tx
        .send(SessionEventMessage::new(
            "system",
            "file_deleted",
            json!({ "path": kiln_path.join(deleted_note_path).to_string_lossy() }),
        ))
        .unwrap();

    let removed = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if note_store
                .get(
                    deleted_note_path,
                    &crucible_core::storage::Scope::workspace_unchecked(std::path::PathBuf::new()),
                )
                .await
                .unwrap()
                .is_none()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        removed.is_ok(),
        "deleted note should be removed after event"
    );

    event_tx
        .send(SessionEventMessage::new(
            "system",
            "file_deleted",
            json!({ "path": kiln_path.join("notes/ignore.txt").to_string_lossy() }),
        ))
        .unwrap();
    event_tx
        .send(SessionEventMessage::new(
            "system",
            "file_deleted",
            json!({ "path": kiln_path.join("notes/missing.md").to_string_lossy() }),
        ))
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(note_store
        .get(
            keep_note_path,
            &crucible_core::storage::Scope::workspace_unchecked(std::path::PathBuf::new())
        )
        .await
        .unwrap()
        .is_some());

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_events_auto_persisted() {
    use std::time::Duration;

    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");
    let kiln_path = tmp.path().join("kiln");
    std::fs::create_dir_all(&kiln_path).unwrap();

    let server = Server::bind_with_data_home(&sock_path, tmp.path().to_path_buf())
        .await
        .unwrap();
    let event_tx = server.event_sender();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(server.run());

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();

    // Create a session
    let create_req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"session.create","params":{{"type":"chat","kiln":"{}"}}}}"#,
        kiln_path.display()
    );
    client.write_all(create_req.as_bytes()).await.unwrap();
    client.write_all(b"\n").await.unwrap();

    let mut buf = vec![0u8; 4096];
    let n = client.read(&mut buf).await.unwrap();
    let response: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();
    let session_id = response["result"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Send event through broadcast channel
    // Use user_message since text_delta is filtered out to reduce storage
    let event = SessionEventMessage::user_message(&session_id, "msg-1", "hello world");
    event_tx.send(event).unwrap();

    // Wait for persistence
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Check that event was persisted
    let session_dir = kiln_path
        .join(".crucible")
        .join("sessions")
        .join(&session_id);
    let jsonl_path = session_dir.join("session.jsonl");

    let content = tokio::fs::read_to_string(&jsonl_path).await.unwrap();
    assert!(content.contains("hello world"));
    assert!(content.contains("user_message"));

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[test]
fn test_emitted_event_has_timestamp() {
    let seq_counter = std::sync::atomic::AtomicU64::new(0);
    let event = SessionEventMessage::text_delta("test-session", "hello");

    let stamped = stamp_event(event, &seq_counter);

    assert!(stamped.timestamp.is_some());
}

#[test]
fn test_emitted_events_have_increasing_seq() {
    let seq_counter = std::sync::atomic::AtomicU64::new(0);

    let events: Vec<SessionEventMessage> = (0..5)
        .map(|_| {
            stamp_event(
                SessionEventMessage::text_delta("test-session", "x"),
                &seq_counter,
            )
        })
        .collect();

    let seqs: Vec<u64> = events.into_iter().map(|event| event.seq.unwrap()).collect();
    assert_eq!(seqs, vec![1, 2, 3, 4, 5]);
}

#[test]
fn test_timestamp_not_in_constructor() {
    let event = SessionEventMessage::text_delta("test-session", "hello");
    assert!(event.timestamp.is_none());
}

#[test]
fn test_internal_error_returns_correct_code_and_message() {
    let req_id = Some(RequestId::Number(42));
    let err_msg = "database connection failed";
    let response = internal_error(req_id.clone(), err_msg);

    assert_eq!(response.id, req_id);
    assert!(response.error.is_some());
    let error = response.error.unwrap();
    assert_eq!(error.code, INTERNAL_ERROR);
    assert_eq!(error.message, format!("Internal error: {}", err_msg));
    assert!(response.result.is_none());
}

#[test]
fn test_invalid_state_error_returns_correct_code_and_message() {
    let req_id = Some(RequestId::String("test-id".to_string()));
    let operation = "pause_session";
    let err_msg = "session already paused";
    let response = invalid_state_error(req_id.clone(), operation, err_msg);

    assert_eq!(response.id, req_id);
    assert!(response.error.is_some());
    let error = response.error.unwrap();
    assert_eq!(error.code, INVALID_PARAMS);
    assert!(error.message.contains(operation));
    assert!(error.message.contains("not allowed"));
    assert!(response.result.is_none());
}

#[test]
fn test_session_not_found_includes_session_id() {
    let req_id = Some(RequestId::Number(1));
    let session_id = "sess-123-abc";
    let response = session_not_found(req_id.clone(), session_id);

    assert_eq!(response.id, req_id);
    assert!(response.error.is_some());
    let error = response.error.unwrap();
    assert_eq!(error.code, INVALID_PARAMS);
    assert!(error.message.contains(session_id));
    assert!(error.message.contains("not found"));
    assert!(response.result.is_none());
}

#[test]
fn test_agent_not_configured_includes_session_id() {
    let req_id = None;
    let session_id = "sess-xyz-789";
    let response = agent_not_configured(req_id, session_id);

    assert_eq!(response.id, None);
    assert!(response.error.is_some());
    let error = response.error.unwrap();
    assert_eq!(error.code, INVALID_PARAMS);
    assert!(error.message.contains(session_id));
    assert!(error.message.contains("No agent"));
    assert!(response.result.is_none());
}

#[test]
fn test_concurrent_request_includes_session_id() {
    let req_id = Some(RequestId::Number(99));
    let session_id = "sess-concurrent-test";
    let response = concurrent_request(req_id.clone(), session_id);

    assert_eq!(response.id, req_id);
    assert!(response.error.is_some());
    let error = response.error.unwrap();
    assert_eq!(error.code, INVALID_PARAMS);
    assert!(error.message.contains(session_id));
    assert!(error.message.contains("already in progress"));
    assert!(response.result.is_none());
}

#[test]
fn test_agent_error_to_response_dispatches_correctly() {
    // Test SessionNotFound variant
    let req_id = Some(RequestId::Number(1));
    let err = AgentError::SessionNotFound("sess-1".to_string());
    let response = agent_error_to_response(req_id.clone(), err);

    assert_eq!(response.id, req_id);
    let error = response.error.unwrap();
    assert_eq!(error.code, INVALID_PARAMS);
    assert!(error.message.contains("sess-1"));

    // Test NoAgentConfigured variant
    let err = AgentError::NoAgentConfigured("sess-2".to_string());
    let response = agent_error_to_response(req_id.clone(), err);
    let error = response.error.unwrap();
    assert_eq!(error.code, INVALID_PARAMS);
    assert!(error.message.contains("sess-2"));

    // Test ConcurrentRequest variant
    let err = AgentError::ConcurrentRequest("sess-3".to_string());
    let response = agent_error_to_response(req_id.clone(), err);
    let error = response.error.unwrap();
    assert_eq!(error.code, INVALID_PARAMS);
    assert!(error.message.contains("sess-3"));
}
