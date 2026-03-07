use super::*;


/// A mock agent whose stream never yields — blocks forever until cancelled.
struct PendingMockAgent;

#[async_trait::async_trait]
impl AgentHandle for PendingMockAgent {
    fn send_message_stream(&mut self, _: String) -> BoxStream<'static, ChatResult<ChatChunk>> {
        Box::pin(futures::stream::pending())
    }

    fn is_connected(&self) -> bool {
        true
    }

    async fn set_mode_str(&mut self, _: &str) -> ChatResult<()> {
        Ok(())
    }
}

#[tokio::test]
async fn concurrent_send_to_same_session_returns_error() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

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
        .send_message(&session.id, "hello".to_string(), &event_tx)
        .await;

    assert!(
        matches!(result, Err(AgentError::ConcurrentRequest(_))),
        "Second send_message should return ConcurrentRequest, got: {:?}",
        result,
    );
}

#[tokio::test]
async fn cancel_during_streaming_emits_ended_event() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

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
        .send_message(&session.id, "test".to_string(), &event_tx)
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
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

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
        .send_message(&session.id, "test".to_string(), &event_tx)
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

