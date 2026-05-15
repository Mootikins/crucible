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
            events: vec![script::text("ok"), script::done()],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(&session.id, "hello".to_string(), &event_tx, true, None)
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
            events: vec![script::text("ok"), script::done()],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(
            &session.id,
            "/search rust".to_string(),
            &event_tx,
            true,
            None,
        )
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
            events: vec![script::text("ok"), script::done()],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(&session.id, "hello".to_string(), &event_tx, true, None)
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
        crucible_core::config::EmbeddingProviderConfig::mock(Some(384)),
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
            events: vec![script::text("ok"), script::done()],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(
            &session.id,
            "hello precognition".to_string(),
            &event_tx,
            true,
            None,
        )
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;
    let event = next_event_or_skip(&mut event_rx, "precognition_complete").await;

    assert_eq!(event.data["notes_count"], 0);
    assert_eq!(event.data["query_summary"], "hello precognition");

    crate::embedding::clear_embedding_provider_cache();
}

#[tokio::test]
async fn test_precognition_runs_only_on_first_user_message_of_session() {
    // Pi-style heuristic (project_context_injection_frequency memory):
    // even when precognition_enabled, only inject on the first user
    // message of a session — not every turn.
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
        crucible_core::config::EmbeddingProviderConfig::mock(Some(384)),
    );
    let mut agent = test_agent();
    agent.precognition_enabled = true;
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .unwrap();

    // First turn: agent emits a quick "ok" + done so we can send a second
    // message after it. Reused for both turns; scripted_events_stream
    // returns once Done is yielded so the agent_cache instance survives.
    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(MultiTurnScriptedAgent {
            scripts: std::sync::Mutex::new(vec![
                vec![script::text("ok"), script::done()],
                vec![script::text("ok2"), script::done()],
            ]),
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(128);

    // Turn 1.
    agent_manager
        .send_message(&session.id, "first question".into(), &event_tx, true, None)
        .await
        .unwrap();
    let _ = next_event_or_skip(&mut event_rx, "precognition_complete").await;
    // Drain to message_complete so we know the turn ended.
    let _ = next_event_or_skip(&mut event_rx, "message_complete").await;

    // Turn 2. Precognition must NOT fire — assertion is "no precog event
    // arrives before the next message_complete."
    agent_manager
        .send_message(&session.id, "follow-up question".into(), &event_tx, true, None)
        .await
        .unwrap();
    assert_no_event_until_message_complete(&mut event_rx, "precognition_complete").await;

    crate::embedding::clear_embedding_provider_cache();
}

#[tokio::test]
async fn test_precognition_does_not_re_fire_when_session_has_prior_history() {
    // Defends against the daemon-restart-style bug: if session storage
    // recorded prior conversation (subagent fork, session bridge copy,
    // future event persistence), a fresh AgentManager must rebuild its
    // in-memory tree from JSONL so the first-message gate sees the
    // session's true history, not "this daemon process's history."
    crate::embedding::clear_embedding_provider_cache();

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage.clone()));
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
    let session_id = session.id.clone();

    // Simulate prior history persisted to session.jsonl (as a fork or
    // subagent copy would produce). One User event is enough — the
    // rebuild_tree path counts User nodes.
    use crate::session_storage::SessionStorage;
    let prior_event = r#"{"type":"user","ts":"2026-05-15T12:00:00Z","content":"earlier message"}"#;
    storage
        .append_event(&session, prior_event)
        .await
        .unwrap();

    // Fresh AgentManager — session_trees is empty, simulating restart
    // or a fresh client attaching to a persisted session.
    let am = create_test_agent_manager_with_enrichment(
        session_manager.clone(),
        crucible_core::config::EmbeddingProviderConfig::mock(Some(384)),
    );
    let mut agent_cfg = test_agent();
    agent_cfg.precognition_enabled = true;
    am.configure_agent(&session_id, agent_cfg).await.unwrap();
    am.agent_cache.insert(
        session_id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![script::text("ok"), script::done()],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    am.send_message(
        &session_id,
        "follow-up after resume".into(),
        &event_tx,
        true,
        None,
    )
    .await
    .unwrap();

    // Critical assertion: precognition must NOT fire on the resumed
    // session — there's prior history, so this isn't the first user
    // message of the session.
    assert_no_event_until_message_complete(&mut event_rx, "precognition_complete").await;

    crate::embedding::clear_embedding_provider_cache();
}

#[tokio::test]
async fn test_precognition_enriched_content_reaches_agent() {
    crate::embedding::clear_embedding_provider_cache();

    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().to_path_buf();

    // Create a markdown file in the kiln so the snippet reader finds it
    std::fs::write(
        kiln_path.join("Rust Ownership.md"),
        "---\ntitle: Rust Ownership\ntags:\n  - rust\n  - memory\n---\n\n\
         Rust uses ownership with borrowing and lifetimes to guarantee memory safety \
         without a garbage collector.\n",
    )
    .unwrap();

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let session = session_manager
        .create_session(
            SessionType::Chat,
            kiln_path.clone(),
            Some(kiln_path.clone()),
            vec![],
            None,
        )
        .await
        .unwrap();

    let enrichment_config = crucible_core::config::EmbeddingProviderConfig::mock(Some(384));
    let agent_manager =
        create_test_agent_manager_with_enrichment(session_manager.clone(), enrichment_config);

    // Open kiln and insert a note with an embedding that will match the mock provider's output
    let handle = agent_manager
        .kiln_manager
        .get_or_open(&kiln_path)
        .await
        .unwrap();
    let note_store = handle.as_note_store();
    note_store
        .upsert(
            crucible_core::storage::note_store::NoteRecord::new(
                "Rust Ownership.md",
                crucible_core::parser::BlockHash::zero(),
            )
            .with_title("Rust Ownership")
            .with_embedding(vec![0.1; 384])
            .with_embedding_metadata("mock-model".to_string(), 384),
        )
        .await
        .unwrap();

    let mut agent = test_agent();
    agent.precognition_enabled = true;
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .unwrap();

    let received = Arc::new(StdMutex::new(None::<String>));
    let received_messages = Arc::new(StdMutex::new(None));
    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(PromptCapturingAgent {
            received_prompt: received.clone(),
            received_messages: received_messages.clone(),
            events: vec![script::text("ok"), script::done()],
        }) as BoxedAgentHandle)),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(
            &session.id,
            "Tell me about Rust ownership".to_string(),
            &event_tx,
            true,
            None,
        )
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;
    let event = next_event_or_skip(&mut event_rx, "precognition_complete").await;

    // Should find the indexed note
    assert!(
        event.data["notes_count"].as_u64().unwrap() > 0,
        "precognition should find at least one note, got: {:?}",
        event.data
    );

    // Wait for the agent stream to complete so the mock agent has been called
    let _ = next_event_or_skip(&mut event_rx, "message_complete").await;

    // Post-migration: the user content is no longer string-mutated.
    // The agent receives the original prompt verbatim AND a separate
    // system ContextMessage carrying the kiln-injected block, prepended
    // by apply_transform_context_handlers.
    let user_content = received
        .lock()
        .unwrap()
        .clone()
        .expect("agent should have received a prompt");
    assert_eq!(user_content, "Tell me about Rust ownership");

    let messages = received_messages
        .lock()
        .unwrap()
        .clone()
        .expect("agent should have received messages");
    let kiln_msg = messages
        .iter()
        .find(|m| m.content.contains("Rust Ownership"))
        .unwrap_or_else(|| {
            panic!(
                "expected a message containing the kiln note title, got: {:?}",
                messages.iter().map(|m| (&m.role, &m.content)).collect::<Vec<_>>()
            )
        });
    assert!(
        matches!(kiln_msg.role, crucible_core::traits::llm::MessageRole::System),
        "kiln-injected note should be a system message, got role={:?}",
        kiln_msg.role
    );

    crate::embedding::clear_embedding_provider_cache();
}

#[tokio::test]
async fn test_precognition_emits_note_info_in_event() {
    crate::embedding::clear_embedding_provider_cache();

    let tmp = TempDir::new().unwrap();
    let kiln_path = tmp.path().to_path_buf();

    std::fs::write(
        kiln_path.join("Agent Context Protocol.md"),
        "---\ntitle: Agent Context Protocol\ntags:\n  - acp\n---\n\n\
         The Agent Context Protocol enables delegation of tasks to external AI agents.\n",
    )
    .unwrap();
    std::fs::write(
        kiln_path.join("Precognition.md"),
        "---\ntitle: Precognition\ntags:\n  - core\n---\n\n\
         Precognition auto-injects relevant knowledge graph context before every turn.\n",
    )
    .unwrap();

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let session = session_manager
        .create_session(
            SessionType::Chat,
            kiln_path.clone(),
            Some(kiln_path.clone()),
            vec![],
            None,
        )
        .await
        .unwrap();

    let enrichment_config = crucible_core::config::EmbeddingProviderConfig::mock(Some(384));
    let agent_manager =
        create_test_agent_manager_with_enrichment(session_manager.clone(), enrichment_config);

    let handle = agent_manager
        .kiln_manager
        .get_or_open(&kiln_path)
        .await
        .unwrap();
    let note_store = handle.as_note_store();

    // Insert two notes with identical embeddings (mock provider returns the same vector)
    for (path, title) in [
        ("Agent Context Protocol.md", "Agent Context Protocol"),
        ("Precognition.md", "Precognition"),
    ] {
        note_store
            .upsert(
                crucible_core::storage::note_store::NoteRecord::new(
                    path,
                    crucible_core::parser::BlockHash::zero(),
                )
                .with_title(title)
                .with_embedding(vec![0.1; 384])
                .with_embedding_metadata("mock-model".to_string(), 384),
            )
            .await
            .unwrap();
    }

    let mut agent = test_agent();
    agent.precognition_enabled = true;
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .unwrap();

    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: vec![script::text("ok"), script::done()],
        }))),
    );

    let (event_tx, mut event_rx) = broadcast::channel::<SessionEventMessage>(64);
    agent_manager
        .send_message(
            &session.id,
            "How does ACP work?".to_string(),
            &event_tx,
            true,
            None,
        )
        .await
        .unwrap();

    let _ = next_event_or_skip(&mut event_rx, "user_message").await;
    let event = next_event_or_skip(&mut event_rx, "precognition_complete").await;

    let notes_count = event.data["notes_count"].as_u64().unwrap();
    assert!(
        notes_count >= 2,
        "should find both indexed notes, got notes_count={notes_count}"
    );

    // The event should include notes with titles
    let note_info = event.data["notes"]
        .as_array()
        .expect("notes should be an array");
    let titles: Vec<&str> = note_info
        .iter()
        .filter_map(|n| n["title"].as_str())
        .collect();
    assert!(
        titles.contains(&"Agent Context Protocol"),
        "note_info should contain 'Agent Context Protocol', got: {titles:?}"
    );
    assert!(
        titles.contains(&"Precognition"),
        "note_info should contain 'Precognition', got: {titles:?}"
    );

    crate::embedding::clear_embedding_provider_cache();
}
