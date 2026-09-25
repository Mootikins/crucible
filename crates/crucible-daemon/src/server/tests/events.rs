use super::*;

/// The watcher's report of a deleted note removes its row. Each report
/// enters as the watcher sends it, through the kiln's bridge, which queues it
/// for the index owner; the client bus is not on that path.
#[tokio::test]
async fn test_file_deleted_event_removes_note_from_store() {
    use crucible_core::events::{InternalSessionEvent, SessionEvent};
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::NoteRecord;

    let server = TestServer::start().await;
    let kiln_path = server.kiln_path.clone();
    std::fs::create_dir_all(kiln_path.join("notes")).unwrap();

    let km = server.kiln_manager.clone();
    let handle = km.get_or_open(&kiln_path).await.unwrap();
    let note_store = handle.as_note_store();
    let bridge = km
        .watcher_bridge(&kiln_path)
        .expect("the daemon's manager has a bus");
    let scope = crucible_core::storage::Scope::workspace_unchecked(std::path::PathBuf::new());

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
        .get(deleted_note_path, &scope)
        .await
        .unwrap()
        .is_some());
    assert!(note_store
        .get(keep_note_path, &scope)
        .await
        .unwrap()
        .is_some());

    for rel in [deleted_note_path, "notes/ignore.txt", "notes/missing.md"] {
        bridge
            .emit(SessionEvent::internal(InternalSessionEvent::FileDeleted {
                path: kiln_path.join(rel),
            }))
            .await
            .expect("emit");
    }
    km.settle_index().await;

    assert!(
        note_store
            .get(deleted_note_path, &scope)
            .await
            .unwrap()
            .is_none(),
        "deleted note should be removed after event"
    );
    assert!(
        note_store
            .get(keep_note_path, &scope)
            .await
            .unwrap()
            .is_some(),
        "a report for another path must not remove this note"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn test_events_auto_persisted() {
    use std::time::Duration;

    let server = TestServer::start().await;
    let _kiln_path = server.kiln_path.clone();
    let event_tx = server.event_tx.clone();
    let mut client = server.connect().await;

    // Create a session
    let create_req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"session.create","params":{{"type":"chat","kilns":["{}"]}}}}"#,
        TestServer::KILN
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

    // Publish through the emit path: the persist task reads the journal that
    // `emit_event` feeds, not the broadcast ring.
    // Use user_message since text_delta is filtered out to reduce storage
    let event = SessionEventMessage::user_message(&session_id, "msg-1", "hello world");
    crate::event_emitter::emit_event(&event_tx, event);

    let session_dir = server.sessions_root().join(&session_id);
    let jsonl_path = session_dir.join("session.jsonl");

    // Poll for the write instead of sleeping a fixed 100ms. Persistence is
    // asynchronous, so a fixed wait is a bet on how loaded the machine is: it
    // held when the test ran alone and lost under a full `just ci`, where ~8000
    // tests share one disk. The deadline keeps the real failure mode — the event
    // never arrives — and removes the false one, where it merely arrived late.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let content = loop {
        match tokio::fs::read_to_string(&jsonl_path).await {
            Ok(content) if content.contains("hello world") => break content,
            _ if tokio::time::Instant::now() >= deadline => panic!(
                "event was not persisted to {} within 10s",
                jsonl_path.display()
            ),
            _ => tokio::time::sleep(Duration::from_millis(25)).await,
        }
    };
    assert!(content.contains("user_message"));

    server.shutdown().await;
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

/// A turn refused for a missing key answers with the refusal only: the
/// client shows it, and "Internal error: Agent factory error:" in front of it
/// says the daemon failed, which it did not.
#[test]
fn a_missing_key_answers_with_the_refusal_only() {
    let err = AgentError::Factory(crate::agent_factory::AgentFactoryError::MissingApiKey {
        provider: "zai-coding".to_string(),
        fix: "Run `cru auth login --provider zai-coding`.".to_string(),
    });

    let error = agent_error_to_response(Some(RequestId::Number(1)), err)
        .error
        .expect("an error");

    assert_eq!(
        error.message,
        "No API key for provider 'zai-coding'. Run `cru auth login --provider zai-coding`."
    );
}
