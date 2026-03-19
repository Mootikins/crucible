use super::*;

#[tokio::test]
async fn test_configure_agent() {
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

    let updated = session_manager.get_session(&session.id).unwrap();
    assert!(updated.agent.is_some());
    assert_eq!(updated.agent.as_ref().unwrap().model, "llama3.2");
}

#[tokio::test]
async fn test_configure_agent_not_found() {
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let agent_manager = create_test_agent_manager(session_manager);

    let result = agent_manager
        .configure_agent("nonexistent", test_agent())
        .await;

    assert!(matches!(result, Err(AgentError::SessionNotFound(_))));
}

#[tokio::test]
async fn test_send_message_no_agent() {
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

    let agent_manager = create_test_agent_manager(session_manager);
    let (event_tx, _) = broadcast::channel(16);

    let result = agent_manager
        .send_message(&session.id, "hello".to_string(), &event_tx, true)
        .await;

    assert!(matches!(result, Err(AgentError::NoAgentConfigured(_))));
}

#[tokio::test]
async fn test_cancel_nonexistent() {
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let agent_manager = create_test_agent_manager(session_manager);

    let cancelled = agent_manager.cancel("nonexistent").await;
    assert!(!cancelled);
}

#[tokio::test]
async fn test_switch_model() {
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

    let updated = session_manager.get_session(&session.id).unwrap();
    assert_eq!(updated.agent.as_ref().unwrap().model, "llama3.2");

    agent_manager
        .switch_model(&session.id, "gpt-4", None)
        .await
        .unwrap();

    let updated = session_manager.get_session(&session.id).unwrap();
    assert_eq!(updated.agent.as_ref().unwrap().model, "gpt-4");
}

#[tokio::test]
async fn test_switch_model_no_agent() {
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

    let agent_manager = create_test_agent_manager(session_manager);

    let result = agent_manager.switch_model(&session.id, "gpt-4", None).await;

    assert!(matches!(result, Err(AgentError::NoAgentConfigured(_))));
}

#[tokio::test]
async fn test_switch_model_session_not_found() {
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let agent_manager = create_test_agent_manager(session_manager);

    let result = agent_manager
        .switch_model("nonexistent", "gpt-4", None)
        .await;

    assert!(matches!(result, Err(AgentError::SessionNotFound(_))));
}

#[tokio::test]
async fn test_switch_model_rejects_empty_model_id() {
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let agent_manager = create_test_agent_manager(session_manager);

    let result = agent_manager.switch_model("any-session", "", None).await;
    assert!(matches!(result, Err(AgentError::InvalidModelId(_))));

    let result = agent_manager.switch_model("any-session", "   ", None).await;
    assert!(matches!(result, Err(AgentError::InvalidModelId(_))));
}

#[tokio::test]
async fn test_switch_model_rejected_during_active_request() {
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

    let result = agent_manager.switch_model(&session.id, "gpt-4", None).await;

    assert!(matches!(result, Err(AgentError::ConcurrentRequest(_))));

    let updated = session_manager.get_session(&session.id).unwrap();
    assert_eq!(
        updated.agent.as_ref().unwrap().model,
        "llama3.2",
        "Model should not change during active request"
    );
}

#[tokio::test]
async fn test_switch_model_invalidates_cache() {
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
        Arc::new(Mutex::new(Box::new(MockAgent))),
    );

    assert!(agent_manager.agent_cache.contains_key(&session.id));

    agent_manager
        .switch_model(&session.id, "gpt-4", None)
        .await
        .unwrap();

    assert!(
        !agent_manager.agent_cache.contains_key(&session.id),
        "Cache should be invalidated after model switch"
    );
}

#[tokio::test]
async fn test_broadcast_send_with_no_receivers_returns_error() {
    let (tx, _rx) = broadcast::channel::<SessionEventMessage>(16);

    drop(_rx);

    let result = tx.send(SessionEventMessage::ended("test-session", "cancelled"));

    assert!(
        result.is_err(),
        "Broadcast send should return error when no receivers"
    );
}

#[tokio::test]
async fn test_broadcast_send_with_receiver_succeeds() {
    let (tx, mut rx) = broadcast::channel::<SessionEventMessage>(16);

    let result = tx.send(SessionEventMessage::text_delta("test-session", "hello"));

    assert!(
        result.is_ok(),
        "Broadcast send should succeed with receiver"
    );

    let received = rx.recv().await.unwrap();
    assert_eq!(received.session_id, "test-session");
    assert_eq!(received.event, "text_delta");
}

#[tokio::test]
async fn test_switch_model_multiple_times_updates_each_time() {
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

    let models = ["model-a", "model-b", "model-c", "model-d"];
    for model in models {
        agent_manager
            .switch_model(&session.id, model, None)
            .await
            .unwrap();

        let updated = session_manager.get_session(&session.id).unwrap();
        assert_eq!(
            updated.agent.as_ref().unwrap().model,
            model,
            "Model should be updated to {}",
            model
        );
        assert!(
            !agent_manager.agent_cache.contains_key(&session.id),
            "Cache should be invalidated after each switch"
        );
    }
}

#[tokio::test]
async fn test_switch_model_preserves_other_agent_config() {
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

    let mut agent = test_agent();
    agent.temperature = Some(0.9);
    agent.system_prompt = "Custom prompt".to_string();
    agent.provider = BackendType::Custom;

    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .unwrap();

    agent_manager
        .switch_model(&session.id, "new-model", None)
        .await
        .unwrap();

    let updated = session_manager.get_session(&session.id).unwrap();
    let updated_agent = updated.agent.as_ref().unwrap();

    assert_eq!(updated_agent.model, "new-model");
    assert_eq!(updated_agent.temperature, Some(0.9));
    assert_eq!(updated_agent.system_prompt, "Custom prompt");
    assert_eq!(updated_agent.provider, BackendType::Custom);
}

#[tokio::test]
async fn test_switch_model_emits_event() {
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

    let (tx, mut rx) = broadcast::channel::<SessionEventMessage>(16);

    agent_manager
        .switch_model(&session.id, "gpt-4", Some(&tx))
        .await
        .unwrap();

    let event = rx.recv().await.unwrap();
    assert_eq!(event.session_id, session.id);
    assert_eq!(event.event, "model_switched");
    assert_eq!(event.data["model_id"], "gpt-4");
    assert_eq!(event.data["provider"], "ollama");
}
