use super::*;

#[tokio::test]
async fn test_precognition_skipped_when_disabled() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            Some(tmp.path().to_path_buf()),
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    let mut agent = test_agent();
    agent.precognition_enabled = false;
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .unwrap();

    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            chunks: vec![ChatChunk {
                delta: "ok".to_string(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(&session.id, "hello".to_string(), &event_tx)
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;
    assert_no_event_until_message_complete(&mut event_rx, "precognition_complete").await;
}

#[tokio::test]
async fn test_precognition_skipped_for_search_command() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            Some(tmp.path().to_path_buf()),
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    let mut agent = test_agent();
    agent.precognition_enabled = true;
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .unwrap();

    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            chunks: vec![ChatChunk {
                delta: "ok".to_string(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(&session.id, "/search rust".to_string(), &event_tx)
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;
    assert_no_event_until_message_complete(&mut event_rx, "precognition_complete").await;
}

#[tokio::test]
async fn test_precognition_skipped_when_no_kiln() {
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            std::path::PathBuf::new(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());
    let mut agent = test_agent();
    agent.precognition_enabled = true;
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .unwrap();

    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            chunks: vec![ChatChunk {
                delta: "ok".to_string(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(&session.id, "hello".to_string(), &event_tx)
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;
    assert_no_event_until_message_complete(&mut event_rx, "precognition_complete").await;
}

#[tokio::test]
async fn test_precognition_complete_event_emitted_when_enrichment_runs() {
    crate::embedding::clear_embedding_provider_cache();

    let tmp = TempDir::new().unwrap();

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            Some(tmp.path().to_path_buf()),
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager_with_enrichment(
        session_manager.clone(),
        crucible_config::EmbeddingProviderConfig::mock(Some(384)),
    );
    let mut agent = test_agent();
    agent.precognition_enabled = true;
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .unwrap();

    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            chunks: vec![ChatChunk {
                delta: "ok".to_string(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(&session.id, "hello precognition".to_string(), &event_tx)
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;
    let event = next_event_or_skip(&mut event_rx, "precognition_complete").await;

    assert_eq!(event.data["notes_count"], 0);
    assert_eq!(event.data["query_summary"], "hello precognition");

    crate::embedding::clear_embedding_provider_cache();
}
