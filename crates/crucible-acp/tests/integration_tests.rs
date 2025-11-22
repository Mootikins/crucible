//! Integration tests for ACP chat components
//!
//! These tests verify that components work together correctly.

use crucible_acp::{
    ChatSession, ChatConfig,
    HistoryConfig, ContextConfig, StreamConfig,
};

/// Integration test: Full chat pipeline with all components
#[tokio::test]
async fn test_full_chat_pipeline() {
    // Create a chat session with all features enabled
    let config = ChatConfig {
        history: HistoryConfig {
            max_messages: 10,
            max_tokens: 1000,
            enable_persistence: false,
        },
        context: ContextConfig {
            enabled: true,
            context_size: 5,
            use_reranking: true,
            rerank_candidates: Some(10),
            enable_cache: true,
            cache_ttl_secs: 300,
        },
        streaming: StreamConfig {
            show_thoughts: true,
            show_tool_calls: true,
            use_colors: false,
        },
        auto_prune: true,
        enrich_prompts: true,
    };

    let mut session = ChatSession::new(config);

    // Verify initial state
    assert_eq!(session.state().turn_count, 0);
    assert_eq!(session.history().message_count(), 0);

    // Send multiple messages
    let response1 = session.send_message("What is semantic search?").await;
    assert!(response1.is_ok(), "First message should succeed");

    let response2 = session.send_message("How does it work?").await;
    assert!(response2.is_ok(), "Second message should succeed");

    let response3 = session.send_message("Can you give me an example?").await;
    assert!(response3.is_ok(), "Third message should succeed");

    // Verify state tracking
    assert_eq!(session.state().turn_count, 3, "Should have 3 turns");
    assert_eq!(session.history().message_count(), 6, "Should have 6 messages (3 user + 3 agent)");
    assert!(session.state().total_tokens_used > 0, "Should have tracked tokens");
    assert!(session.state().last_message_at.is_some(), "Should have last message timestamp");

    // Verify session metadata was updated
    let metadata = session.metadata();
    assert!(metadata.updated_at >= metadata.created_at, "Metadata should be updated");
}

/// Integration test: Context enrichment with caching
#[tokio::test]
async fn test_context_enrichment_caching() {
    let config = ChatConfig {
        context: ContextConfig {
            enabled: true,
            enable_cache: true,
            cache_ttl_secs: 60,
            ..Default::default()
        },
        enrich_prompts: true,
        ..Default::default()
    };

    let mut session = ChatSession::new(config);

    // Send the same query twice
    let response1 = session.send_message("What is a knowledge graph?").await.unwrap();
    let response2 = session.send_message("What is a knowledge graph?").await.unwrap();

    // Both should succeed (cache should work transparently)
    assert!(!response1.is_empty());
    assert!(!response2.is_empty());

    // Should have 2 turns recorded
    assert_eq!(session.state().turn_count, 2);
}

/// Integration test: History auto-pruning with state tracking
#[tokio::test]
async fn test_history_auto_pruning_integration() {
    let config = ChatConfig {
        history: HistoryConfig {
            max_messages: 4, // Very small limit
            max_tokens: 10000,
            enable_persistence: false,
        },
        auto_prune: true,
        ..Default::default()
    };

    let mut session = ChatSession::new(config);

    // Send enough messages to trigger pruning
    for i in 1..=5 {
        let msg = format!("Message number {}", i);
        session.send_message(&msg).await.unwrap();
    }

    // History should be pruned to max_messages
    assert!(
        session.history().message_count() <= 4,
        "History should be pruned to max 4 messages"
    );

    // Prune count should be tracked
    assert!(
        session.state().prune_count > 0,
        "Prune count should be incremented"
    );

    // Turn count should still be accurate
    assert_eq!(session.state().turn_count, 5, "Turn count should still be 5");
}

/// Integration test: Error handling doesn't corrupt state
#[tokio::test]
async fn test_error_handling_state_integrity() {
    let mut session = ChatSession::new(ChatConfig::default());

    // Send valid messages
    session.send_message("First valid message").await.unwrap();
    session.send_message("Second valid message").await.unwrap();

    let state_before = session.state().turn_count;
    let history_before = session.history().message_count();

    // Try to send invalid messages
    let _ = session.send_message("").await;
    let _ = session.send_message("   ").await;
    let _ = session.send_message("x".repeat(100_000).as_str()).await;

    // State should be unchanged
    assert_eq!(
        session.state().turn_count,
        state_before,
        "Turn count should not change on errors"
    );
    assert_eq!(
        session.history().message_count(),
        history_before,
        "History should not change on errors"
    );

    // Should still be able to send valid messages
    session.send_message("Third valid message").await.unwrap();
    assert_eq!(session.state().turn_count, 3);
}

/// Integration test: Multi-turn conversation with metadata
#[tokio::test]
async fn test_multi_turn_with_metadata() {
    let mut session = ChatSession::new(ChatConfig::default());

    // Set session metadata
    session.set_title("Integration Test Session");
    session.add_tag("integration");
    session.add_tag("testing");

    let initial_updated = session.metadata().updated_at;

    // Small delay to ensure timestamp changes
    tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

    // Have a multi-turn conversation
    session.send_message("Hello").await.unwrap();
    session.send_message("How are you?").await.unwrap();
    session.send_message("What can you help me with?").await.unwrap();

    // Verify conversation state
    assert_eq!(session.state().turn_count, 3);
    assert!(session.state().avg_tokens_per_turn() > 0.0);

    // Verify metadata was updated
    assert!(
        session.metadata().updated_at > initial_updated,
        "Metadata timestamp should be updated"
    );

    // Verify metadata content is preserved
    assert_eq!(session.metadata().title, Some("Integration Test Session".to_string()));
    assert!(session.metadata().tags.contains(&"integration".to_string()));
    assert!(session.metadata().tags.contains(&"testing".to_string()));
}

/// Integration test: Enrichment can be toggled per session
#[tokio::test]
async fn test_enrichment_toggle() {
    // Session with enrichment enabled
    let mut session_enriched = ChatSession::new(ChatConfig {
        enrich_prompts: true,
        ..Default::default()
    });

    // Session with enrichment disabled
    let mut session_plain = ChatSession::new(ChatConfig {
        enrich_prompts: false,
        ..Default::default()
    });

    // Both should work
    let response1 = session_enriched.send_message("Test query").await;
    let response2 = session_plain.send_message("Test query").await;

    assert!(response1.is_ok());
    assert!(response2.is_ok());

    // Both should have recorded the turn
    assert_eq!(session_enriched.state().turn_count, 1);
    assert_eq!(session_plain.state().turn_count, 1);
}

/// Integration test: Session state accurately tracks conversation
#[tokio::test]
async fn test_state_tracking_accuracy() {
    let mut session = ChatSession::new(ChatConfig::default());

    let start_time = session.state().started_at;

    // Send messages
    session.send_message("First").await.unwrap();

    // Small delay
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    session.send_message("Second").await.unwrap();

    let state = session.state();

    // Verify all state fields
    assert_eq!(state.turn_count, 2);
    assert!(state.total_tokens_used > 0);
    assert!(state.last_message_at.is_some());
    assert!(state.last_message_at.unwrap() >= start_time);
    assert!(state.duration_secs() < 5); // Should be very quick
    assert!(state.avg_tokens_per_turn() > 0.0);
}

/// Integration test: Complete session lifecycle
#[tokio::test]
async fn test_complete_session_lifecycle() {
    // Create session
    let mut session = ChatSession::new(ChatConfig::default());
    let session_id = session.session_id().to_string();

    // Configure session
    session.set_title("Lifecycle Test");
    session.add_tag("test");

    // Use session
    for i in 1..=3 {
        let msg = format!("Message {}", i);
        session.send_message(&msg).await.unwrap();
    }

    // Verify session state
    assert_eq!(session.state().turn_count, 3);

    // Clear history (but metadata/state should remain)
    session.clear_history();
    assert_eq!(session.history().message_count(), 0);

    // Session ID should be unchanged
    assert_eq!(session.session_id(), session_id);

    // Metadata should be unchanged
    assert_eq!(session.metadata().title, Some("Lifecycle Test".to_string()));
}

/// Integration test: Token counting consistency
#[tokio::test]
async fn test_token_counting_consistency() {
    let mut session = ChatSession::new(ChatConfig::default());

    // Send messages of different lengths
    session.send_message("Short").await.unwrap();
    let tokens_after_1 = session.state().total_tokens_used;

    session.send_message("This is a much longer message with many more words").await.unwrap();
    let tokens_after_2 = session.state().total_tokens_used;

    // Second message should add more tokens
    assert!(tokens_after_2 > tokens_after_1);

    // Average should be calculated correctly
    let avg = session.state().avg_tokens_per_turn();
    let expected_avg = tokens_after_2 as f64 / 2.0;
    assert_eq!(avg, expected_avg);
}

/// Integration test: Multiple sessions are isolated
#[tokio::test]
async fn test_session_isolation() {
    let mut session1 = ChatSession::new(ChatConfig::default());
    let mut session2 = ChatSession::new(ChatConfig::default());

    session1.set_title("Session 1");
    session2.set_title("Session 2");

    session1.send_message("Message to session 1").await.unwrap();
    session2.send_message("Message to session 2").await.unwrap();

    // Sessions should have different IDs
    assert_ne!(session1.session_id(), session2.session_id());

    // Each should only have their own messages
    assert_eq!(session1.history().message_count(), 2); // user + agent
    assert_eq!(session2.history().message_count(), 2); // user + agent

    // State should be independent
    assert_eq!(session1.state().turn_count, 1);
    assert_eq!(session2.state().turn_count, 1);
}
