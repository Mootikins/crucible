//! Tests for event_type, identifier, category helpers, type_name, summary,
//! payload, estimate_tokens, and truncate helper.

use super::*;
use serde_json::Value as JsonValue;

#[test]
fn test_session_event_type() {
    assert_eq!(
        SessionEvent::MessageReceived {
            content: "".into(),
            participant_id: "".into()
        }
        .event_type(),
        "message_received"
    );
    assert_eq!(
        SessionEvent::ToolCalled {
            name: "".into(),
            args: JsonValue::Null,
            description: None,
            source: None,
        }
        .event_type(),
        "tool_called"
    );
    assert_eq!(
        SessionEvent::internal(InternalSessionEvent::FileChanged {
            path: PathBuf::new(),
            kind: FileChangeKind::Created
        })
        .event_type(),
        "file_changed"
    );
    assert_eq!(
        SessionEvent::internal(InternalSessionEvent::FileDeleted {
            path: PathBuf::new()
        })
        .event_type(),
        "file_deleted"
    );
    assert_eq!(
        SessionEvent::internal(InternalSessionEvent::FileMoved {
            from: PathBuf::new(),
            to: PathBuf::new()
        })
        .event_type(),
        "file_moved"
    );
    assert_eq!(
        SessionEvent::internal(InternalSessionEvent::NoteParsed {
            path: PathBuf::new(),
            block_count: 0,
            payload: None,
        })
        .event_type(),
        "note_parsed"
    );
    // Storage events
    assert_eq!(
        SessionEvent::internal(InternalSessionEvent::EntityStored {
            entity_id: "".into(),
            entity_type: EntityType::Note
        })
        .event_type(),
        "entity_stored"
    );
    assert_eq!(
        SessionEvent::internal(InternalSessionEvent::EntityDeleted {
            entity_id: "".into(),
            entity_type: EntityType::Note
        })
        .event_type(),
        "entity_deleted"
    );
    assert_eq!(
        SessionEvent::internal(InternalSessionEvent::BlocksUpdated {
            entity_id: "".into(),
            block_count: 0
        })
        .event_type(),
        "blocks_updated"
    );
    assert_eq!(
        SessionEvent::internal(InternalSessionEvent::RelationStored {
            from_id: "".into(),
            to_id: "".into(),
            relation_type: "".into()
        })
        .event_type(),
        "relation_stored"
    );
    assert_eq!(
        SessionEvent::internal(InternalSessionEvent::RelationDeleted {
            from_id: "".into(),
            to_id: "".into(),
            relation_type: "".into()
        })
        .event_type(),
        "relation_deleted"
    );
    assert_eq!(
        SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
            entity_id: "".into(),
            block_id: None,
            priority: Priority::Normal
        })
        .event_type(),
        "embedding_requested"
    );
    assert_eq!(
        SessionEvent::internal(InternalSessionEvent::EmbeddingStored {
            entity_id: "".into(),
            block_id: None,
            dimensions: 0,
            model: "".into()
        })
        .event_type(),
        "embedding_stored"
    );
    assert_eq!(
        SessionEvent::internal(InternalSessionEvent::EmbeddingFailed {
            entity_id: "".into(),
            block_id: None,
            error: "".into()
        })
        .event_type(),
        "embedding_failed"
    );
    assert_eq!(
        SessionEvent::Custom {
            name: "test".into(),
            payload: JsonValue::Null
        }
        .event_type(),
        "custom"
    );
}

#[test]
fn test_session_event_identifier() {
    let event = SessionEvent::ToolCalled {
        name: "search".into(),
        args: JsonValue::Null,
        description: None,
        source: None,
    };
    assert_eq!(event.identifier(), "search");

    let event = SessionEvent::internal(InternalSessionEvent::NoteParsed {
        path: PathBuf::from("/notes/test.md"),
        block_count: 5,
        payload: None,
    });
    assert_eq!(event.identifier(), "/notes/test.md");

    let event = SessionEvent::MessageReceived {
        content: "hello".into(),
        participant_id: "user".into(),
    };
    assert_eq!(event.identifier(), "message:user");

    // File events identifiers
    let event = SessionEvent::internal(InternalSessionEvent::FileChanged {
        path: PathBuf::from("/notes/changed.md"),
        kind: FileChangeKind::Modified,
    });
    assert_eq!(event.identifier(), "/notes/changed.md");

    let event = SessionEvent::internal(InternalSessionEvent::FileDeleted {
        path: PathBuf::from("/notes/deleted.md"),
    });
    assert_eq!(event.identifier(), "/notes/deleted.md");

    // FileMoved uses the "to" path as identifier
    let event = SessionEvent::internal(InternalSessionEvent::FileMoved {
        from: PathBuf::from("/notes/old.md"),
        to: PathBuf::from("/notes/new.md"),
    });
    assert_eq!(event.identifier(), "/notes/new.md");

    // Storage events identifiers
    let event = SessionEvent::internal(InternalSessionEvent::EntityStored {
        entity_id: "entities:note:test".into(),
        entity_type: EntityType::Note,
    });
    assert_eq!(event.identifier(), "entities:note:test");

    let event = SessionEvent::internal(InternalSessionEvent::EntityDeleted {
        entity_id: "entities:note:test".into(),
        entity_type: EntityType::Note,
    });
    assert_eq!(event.identifier(), "entities:note:test");

    let event = SessionEvent::internal(InternalSessionEvent::BlocksUpdated {
        entity_id: "entities:note:test".into(),
        block_count: 5,
    });
    assert_eq!(event.identifier(), "entities:note:test");

    let event = SessionEvent::internal(InternalSessionEvent::RelationStored {
        from_id: "entities:note:a".into(),
        to_id: "entities:note:b".into(),
        relation_type: "wikilink".into(),
    });
    assert_eq!(event.identifier(), "entities:note:a:entities:note:b");

    let event = SessionEvent::internal(InternalSessionEvent::RelationDeleted {
        from_id: "entities:note:a".into(),
        to_id: "entities:note:b".into(),
        relation_type: "wikilink".into(),
    });
    assert_eq!(event.identifier(), "entities:note:a:entities:note:b");

    // EmbeddingRequested with block_id
    let event = SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
        entity_id: "entities:note:test".into(),
        block_id: Some("block:0".into()),
        priority: Priority::High,
    });
    assert_eq!(event.identifier(), "entities:note:test#block:0");

    // EmbeddingRequested without block_id
    let event = SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
        entity_id: "entities:note:test".into(),
        block_id: None,
        priority: Priority::Normal,
    });
    assert_eq!(event.identifier(), "entities:note:test");

    // EmbeddingStored with block_id
    let event = SessionEvent::internal(InternalSessionEvent::EmbeddingStored {
        entity_id: "entities:note:test".into(),
        block_id: Some("block:0".into()),
        dimensions: 384,
        model: "nomic-embed-text".into(),
    });
    assert_eq!(event.identifier(), "entities:note:test#block:0");

    // EmbeddingStored without block_id
    let event = SessionEvent::internal(InternalSessionEvent::EmbeddingStored {
        entity_id: "entities:note:test".into(),
        block_id: None,
        dimensions: 384,
        model: "nomic-embed-text".into(),
    });
    assert_eq!(event.identifier(), "entities:note:test");

    // EmbeddingFailed with block_id
    let event = SessionEvent::internal(InternalSessionEvent::EmbeddingFailed {
        entity_id: "entities:note:test".into(),
        block_id: Some("block:0".into()),
        error: "provider timeout".into(),
    });
    assert_eq!(event.identifier(), "entities:note:test#block:0");

    // EmbeddingFailed without block_id
    let event = SessionEvent::internal(InternalSessionEvent::EmbeddingFailed {
        entity_id: "entities:note:test".into(),
        block_id: None,
        error: "provider timeout".into(),
    });
    assert_eq!(event.identifier(), "entities:note:test");
}

#[test]
fn test_session_event_category_helpers() {
    // Tool events
    assert!(SessionEvent::ToolCalled {
        name: "".into(),
        args: JsonValue::Null,
        description: None,
        source: None,
    }
    .is_tool_event());
    assert!(SessionEvent::ToolCompleted {
        name: "".into(),
        result: "".into(),
        error: None
    }
    .is_tool_event());
    assert!(!SessionEvent::MessageReceived {
        content: "".into(),
        participant_id: "".into()
    }
    .is_tool_event());

    // Note events
    assert!(SessionEvent::internal(InternalSessionEvent::NoteParsed {
        path: PathBuf::new(),
        block_count: 0,
        payload: None,
    })
    .is_note_event());
    assert!(SessionEvent::internal(InternalSessionEvent::NoteCreated {
        path: PathBuf::new(),
        title: None
    })
    .is_note_event());
    assert!(SessionEvent::internal(InternalSessionEvent::NoteDeleted {
        path: PathBuf::from("/notes/test.md"),
        existed: true,
    })
    .is_note_event());
    assert!(!SessionEvent::ToolCalled {
        name: "".into(),
        args: JsonValue::Null,
        description: None,
        source: None,
    }
    .is_note_event());

    // Lifecycle events
    assert!(SessionEvent::SessionStarted {
        config: SessionEventConfig::default()
    }
    .is_lifecycle_event());
    assert!(SessionEvent::SessionEnded { reason: "".into() }.is_lifecycle_event());

    // Agent events
    assert!(SessionEvent::AgentResponded {
        content: "".into(),
        tool_calls: vec![]
    }
    .is_agent_event());
    assert!(SessionEvent::AgentThinking { thought: "".into() }.is_agent_event());

    // Subagent events
    assert!(
        SessionEvent::internal(InternalSessionEvent::SubagentSpawned {
            id: "".into(),
            prompt: "".into()
        })
        .is_subagent_event()
    );

    // Streaming events
    assert!(SessionEvent::TextDelta {
        delta: "".into(),
        seq: 0
    }
    .is_streaming_event());

    // File events
    assert!(SessionEvent::internal(InternalSessionEvent::FileChanged {
        path: PathBuf::new(),
        kind: FileChangeKind::Created
    })
    .is_file_event());
    assert!(SessionEvent::internal(InternalSessionEvent::FileDeleted {
        path: PathBuf::new()
    })
    .is_file_event());
    assert!(SessionEvent::internal(InternalSessionEvent::FileMoved {
        from: PathBuf::new(),
        to: PathBuf::new()
    })
    .is_file_event());
    // File events are not note events
    assert!(!SessionEvent::internal(InternalSessionEvent::FileChanged {
        path: PathBuf::new(),
        kind: FileChangeKind::Modified
    })
    .is_note_event());

    // Embedding events
    assert!(
        SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
            entity_id: "".into(),
            block_id: None,
            priority: Priority::Normal
        })
        .is_embedding_event()
    );
    assert!(
        SessionEvent::internal(InternalSessionEvent::EmbeddingStored {
            entity_id: "".into(),
            block_id: None,
            dimensions: 0,
            model: "".into()
        })
        .is_embedding_event()
    );
    assert!(
        SessionEvent::internal(InternalSessionEvent::EmbeddingFailed {
            entity_id: "".into(),
            block_id: None,
            error: "".into()
        })
        .is_embedding_event()
    );
    assert!(
        SessionEvent::internal(InternalSessionEvent::EmbeddingBatchComplete {
            entity_id: "".into(),
            count: 5,
            duration_ms: 100
        })
        .is_embedding_event()
    );
    // Non-embedding events
    assert!(!SessionEvent::internal(InternalSessionEvent::EntityStored {
        entity_id: "".into(),
        entity_type: EntityType::Note
    })
    .is_embedding_event());

    // Storage events
    assert!(SessionEvent::internal(InternalSessionEvent::EntityStored {
        entity_id: "".into(),
        entity_type: EntityType::Note
    })
    .is_storage_event());
    assert!(SessionEvent::internal(InternalSessionEvent::EntityDeleted {
        entity_id: "".into(),
        entity_type: EntityType::Note
    })
    .is_storage_event());
    assert!(SessionEvent::internal(InternalSessionEvent::BlocksUpdated {
        entity_id: "".into(),
        block_count: 0
    })
    .is_storage_event());
    assert!(
        SessionEvent::internal(InternalSessionEvent::RelationStored {
            from_id: "".into(),
            to_id: "".into(),
            relation_type: "".into()
        })
        .is_storage_event()
    );
    assert!(
        SessionEvent::internal(InternalSessionEvent::RelationDeleted {
            from_id: "".into(),
            to_id: "".into(),
            relation_type: "".into()
        })
        .is_storage_event()
    );
    assert!(
        SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
            entity_id: "".into(),
            block_id: None,
            priority: Priority::Normal
        })
        .is_storage_event()
    );
    assert!(
        SessionEvent::internal(InternalSessionEvent::EmbeddingStored {
            entity_id: "".into(),
            block_id: None,
            dimensions: 0,
            model: "".into()
        })
        .is_storage_event()
    );
    assert!(
        SessionEvent::internal(InternalSessionEvent::EmbeddingFailed {
            entity_id: "".into(),
            block_id: None,
            error: "".into()
        })
        .is_storage_event()
    );
    assert!(
        SessionEvent::internal(InternalSessionEvent::EmbeddingBatchComplete {
            entity_id: "".into(),
            count: 5,
            duration_ms: 100
        })
        .is_storage_event()
    );
    // Storage events are not note events
    assert!(!SessionEvent::internal(InternalSessionEvent::EntityStored {
        entity_id: "".into(),
        entity_type: EntityType::Note
    })
    .is_note_event());

    // MCP events
    assert!(SessionEvent::internal(InternalSessionEvent::McpAttached {
        server: "".into(),
        tool_count: 0
    })
    .is_mcp_event());

    // Custom events
    assert!(SessionEvent::Custom {
        name: "".into(),
        payload: JsonValue::Null
    }
    .is_custom_event());
}

#[test]
fn test_type_name() {
    // Test a few representative variants
    assert_eq!(
        SessionEvent::MessageReceived {
            content: "".into(),
            participant_id: "".into()
        }
        .type_name(),
        "MessageReceived"
    );
    assert_eq!(
        SessionEvent::ToolCalled {
            name: "".into(),
            args: JsonValue::Null,
            description: None,
            source: None,
        }
        .type_name(),
        "ToolCalled"
    );
    assert_eq!(
        SessionEvent::internal(InternalSessionEvent::SessionStateChanged {
            session_id: "".into(),
            state: crate::session::SessionState::Active,
            previous_state: None,
        })
        .type_name(),
        "SessionStateChanged"
    );
    assert_eq!(
        SessionEvent::Custom {
            name: "".into(),
            payload: JsonValue::Null
        }
        .type_name(),
        "Custom"
    );
}

#[test]
fn test_summary() {
    // Test MessageReceived summary
    let event = SessionEvent::MessageReceived {
        content: "Hello world".into(),
        participant_id: "user".into(),
    };
    let summary = event.summary(100);
    assert!(summary.contains("from=user"));
    assert!(summary.contains("content_len=11"));

    // Test ToolCalled summary
    let event = SessionEvent::ToolCalled {
        name: "search".into(),
        args: serde_json::json!({"query": "test"}),
        description: None,
        source: None,
    };
    let summary = event.summary(100);
    assert!(summary.contains("tool=search"));
    assert!(summary.contains("args_size="));

    // Test truncation in summary
    let event = SessionEvent::SessionEnded {
        reason: "This is a very long reason that should be truncated when max_len is small".into(),
    };
    let summary = event.summary(20);
    assert!(summary.contains("reason="));
    // The truncated reason should be <= 20 chars
    assert!(summary.len() < 50);
}

#[test]
fn test_payload() {
    // Test MessageReceived payload
    let event = SessionEvent::MessageReceived {
        content: "Hello world".into(),
        participant_id: "user".into(),
    };
    let payload = event.payload(100);
    assert_eq!(payload, Some("Hello world".to_string()));

    // Test SessionStarted has no payload
    let event = SessionEvent::SessionStarted {
        config: SessionEventConfig::default(),
    };
    let payload = event.payload(100);
    assert_eq!(payload, None);

    // Test truncation
    let event = SessionEvent::MessageReceived {
        content: "This is a long message that should be truncated".into(),
        participant_id: "user".into(),
    };
    let payload = event.payload(10);
    assert!(payload.is_some());
    assert!(payload.unwrap().len() <= 10);
}

#[test]
fn test_estimate_tokens() {
    // Test MessageReceived token estimate
    let event = SessionEvent::MessageReceived {
        content: "Hello world".into(), // 11 chars -> ~3 tokens + 10 overhead
        participant_id: "user".into(),
    };
    let tokens = event.estimate_tokens();
    assert!(tokens >= 11); // At least 10 overhead + 1 minimum
    assert!(tokens < 20); // Should be reasonable

    // Test SessionStarted fixed overhead
    let event = SessionEvent::SessionStarted {
        config: SessionEventConfig::default(),
    };
    let tokens = event.estimate_tokens();
    assert_eq!(tokens, 100 / 4 + 10); // 100 fixed + overhead

    // Test small metadata events
    let event = SessionEvent::internal(InternalSessionEvent::FileChanged {
        path: PathBuf::from("/notes/test.md"),
        kind: FileChangeKind::Modified,
    });
    let tokens = event.estimate_tokens();
    assert_eq!(tokens, 50 / 4 + 10); // 50 fixed + overhead
}

#[test]
fn test_truncate_helper() {
    // Test short string (no truncation needed)
    let short = "hello";
    assert_eq!(truncate(short, 10), "hello");

    // Test exact length
    let exact = "hello";
    assert_eq!(truncate(exact, 5), "hello");

    // Test truncation
    let long = "hello world";
    assert_eq!(truncate(long, 5), "hello");

    // Test UTF-8 boundary handling
    let utf8 = "hello\u{00e9}world"; // e with accent
    let truncated = truncate(utf8, 6);
    assert!(truncated.len() <= 6);
    assert!(truncated.is_char_boundary(truncated.len()));
}
