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
                precognition_notes_count: None,
                precognition_notes: None,
            }],
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
            chunks: vec![ChatChunk {
                delta: "ok".to_string(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }],
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
            chunks: vec![ChatChunk {
                delta: "ok".to_string(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }],
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
            chunks: vec![ChatChunk {
                delta: "ok".to_string(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }],
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
    agent_manager.agent_cache.insert(
        session.id.clone(),
        Arc::new(Mutex::new(Box::new(PromptCapturingAgent {
            received_prompt: received.clone(),
            chunks: vec![ChatChunk {
                delta: "ok".to_string(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }],
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

    // The agent should have received enriched content with precognition context
    let enriched = received
        .lock()
        .unwrap()
        .clone()
        .expect("agent should have received a message");
    assert!(
        enriched.contains("Rust Ownership"),
        "enriched content should contain the note title, got: {enriched}"
    );
    assert!(
        enriched.contains("Tell me about Rust ownership"),
        "enriched content should preserve the original user message, got: {enriched}"
    );
    // The enriched content should be longer than the original (context was prepended)
    assert!(
        enriched.len() > "Tell me about Rust ownership".len(),
        "enriched content should be longer than original message"
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
            chunks: vec![ChatChunk {
                delta: "ok".to_string(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                precognition_notes_count: None,
                precognition_notes: None,
            }],
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
