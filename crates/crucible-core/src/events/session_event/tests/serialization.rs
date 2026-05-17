//! Tests for SessionEvent serialization/deserialization.

use super::*;

#[test]
fn test_session_event_serialization() {
    let event = SessionEvent::MessageReceived {
        content: "Hello".into(),
        participant_id: "user".into(),
    };

    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("message_received"));
    assert!(json.contains("Hello"));

    let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, parsed);
}

#[test]
fn tool_called_with_metadata_round_trips() {
    let event = SessionEvent::ToolCalled {
        name: "search".to_string(),
        args: serde_json::json!({"query": "rust"}),
        description: Some("Search notes".to_string()),
        source: Some("Crucible".to_string()),
    };

    let json = serde_json::to_string(&event).unwrap();
    let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, parsed);

    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["description"], "Search notes");
    assert_eq!(value["source"], "Crucible");
}

#[test]
fn tool_called_without_metadata_backward_compat() {
    let json = r#"{"type":"tool_called","name":"test","args":{}}"#;
    let parsed: SessionEvent = serde_json::from_str(json).unwrap();

    match parsed {
        SessionEvent::ToolCalled {
            name,
            args,
            description,
            source,
        } => {
            assert_eq!(name, "test");
            assert_eq!(args, serde_json::json!({}));
            assert_eq!(description, None);
            assert_eq!(source, None);
        }
        _ => panic!("Expected ToolCalled"),
    }

    let event = SessionEvent::ToolCalled {
        name: "test".to_string(),
        args: serde_json::json!({}),
        description: None,
        source: None,
    };
    let reserialized = serde_json::to_string(&event).unwrap();
    assert!(!reserialized.contains("\"description\""));
    assert!(!reserialized.contains("\"source\""));
}

#[test]
fn test_all_variants_serialize() {
    let events = vec![
        SessionEvent::MessageReceived {
            content: "test".into(),
            participant_id: "user".into(),
        },
        SessionEvent::AgentResponded {
            content: "response".into(),
            tool_calls: vec![],
        },
        SessionEvent::AgentThinking {
            thought: "thinking".into(),
        },
        SessionEvent::ToolCalled {
            name: "tool".into(),
            args: serde_json::json!({}),
            description: None,
            source: None,
        },
        SessionEvent::ToolCompleted {
            name: "tool".into(),
            result: "done".into(),
            error: None,
            terminate: false,
        },
        SessionEvent::SessionStarted {
            config: SessionEventConfig::default(),
        },
        SessionEvent::internal(InternalSessionEvent::SessionCompacted {
            summary: "summary".into(),
            new_file: test_path("new"),
        }),
        SessionEvent::SessionEnded {
            reason: "user closed".into(),
        },
        SessionEvent::internal(InternalSessionEvent::SubagentSpawned {
            id: "sub1".into(),
            prompt: "do stuff".into(),
        }),
        SessionEvent::internal(InternalSessionEvent::SubagentCompleted {
            id: "sub1".into(),
            result: "done".into(),
        }),
        SessionEvent::internal(InternalSessionEvent::SubagentFailed {
            id: "sub1".into(),
            error: "failed".into(),
        }),
        SessionEvent::TextDelta {
            delta: "chunk".into(),
            seq: 1,
        },
        // File system events
        SessionEvent::internal(InternalSessionEvent::FileChanged {
            path: PathBuf::from("/notes/test.md"),
            kind: FileChangeKind::Created,
        }),
        SessionEvent::internal(InternalSessionEvent::FileChanged {
            path: PathBuf::from("/notes/test.md"),
            kind: FileChangeKind::Modified,
        }),
        SessionEvent::internal(InternalSessionEvent::FileDeleted {
            path: PathBuf::from("/notes/deleted.md"),
        }),
        SessionEvent::internal(InternalSessionEvent::FileMoved {
            from: PathBuf::from("/notes/old.md"),
            to: PathBuf::from("/notes/new.md"),
        }),
        // Note events
        SessionEvent::internal(InternalSessionEvent::NoteParsed {
            path: PathBuf::from("/notes/test.md"),
            block_count: 5,
            payload: None,
        }),
        SessionEvent::internal(InternalSessionEvent::NoteCreated {
            path: PathBuf::from("/notes/new.md"),
            title: Some("New Note".into()),
        }),
        SessionEvent::internal(InternalSessionEvent::NoteModified {
            path: PathBuf::from("/notes/test.md"),
            change_type: NoteChangeType::Content,
        }),
        SessionEvent::internal(InternalSessionEvent::NoteDeleted {
            path: PathBuf::from("/notes/deleted.md"),
            existed: true,
        }),
        // Storage events
        SessionEvent::internal(InternalSessionEvent::EntityStored {
            entity_id: "entities:note:test".into(),
            entity_type: EntityType::Note,
        }),
        SessionEvent::internal(InternalSessionEvent::EntityDeleted {
            entity_id: "entities:note:test".into(),
            entity_type: EntityType::Note,
        }),
        SessionEvent::internal(InternalSessionEvent::BlocksUpdated {
            entity_id: "entities:note:test".into(),
            block_count: 5,
        }),
        SessionEvent::internal(InternalSessionEvent::RelationStored {
            from_id: "entities:note:source".into(),
            to_id: "entities:note:target".into(),
            relation_type: "wikilink".into(),
        }),
        SessionEvent::internal(InternalSessionEvent::RelationDeleted {
            from_id: "entities:note:source".into(),
            to_id: "entities:note:target".into(),
            relation_type: "wikilink".into(),
        }),
        SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
            entity_id: "entities:note:test".into(),
            block_id: None,
            priority: Priority::Normal,
        }),
        SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
            entity_id: "entities:note:test".into(),
            block_id: Some("block:0".into()),
            priority: Priority::High,
        }),
        SessionEvent::internal(InternalSessionEvent::EmbeddingStored {
            entity_id: "entities:note:test".into(),
            block_id: Some("block:0".into()),
            dimensions: 384,
            model: "nomic-embed-text".into(),
        }),
        SessionEvent::internal(InternalSessionEvent::EmbeddingFailed {
            entity_id: "entities:note:test".into(),
            block_id: None,
            error: "provider timeout".into(),
        }),
        SessionEvent::internal(InternalSessionEvent::EmbeddingFailed {
            entity_id: "entities:note:test".into(),
            block_id: Some("block:0".into()),
            error: "rate limited".into(),
        }),
        SessionEvent::internal(InternalSessionEvent::McpAttached {
            server: "crucible".into(),
            tool_count: 10,
        }),
        SessionEvent::internal(InternalSessionEvent::ToolDiscovered {
            name: "search".into(),
            source: ToolProvider::Mcp {
                server: "crucible".into(),
            },
            schema: Some(serde_json::json!({"type": "object"})),
        }),
        SessionEvent::Custom {
            name: "custom".into(),
            payload: serde_json::json!({}),
        },
    ];

    for event in events {
        let json = serde_json::to_string(&event).unwrap();
        let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }
}

#[test]
fn test_daemon_protocol_events_serialize() {
    use crate::session::SessionState;

    let events = vec![
        SessionEvent::internal(InternalSessionEvent::SessionStateChanged {
            session_id: "chat-test".into(),
            state: SessionState::Active,
            previous_state: None,
        }),
        SessionEvent::internal(InternalSessionEvent::SessionStateChanged {
            session_id: "chat-test".into(),
            state: SessionState::Paused,
            previous_state: Some(SessionState::Active),
        }),
        SessionEvent::internal(InternalSessionEvent::SessionStateChanged {
            session_id: "chat-test".into(),
            state: SessionState::Compacting,
            previous_state: Some(SessionState::Active),
        }),
        SessionEvent::internal(InternalSessionEvent::SessionStateChanged {
            session_id: "chat-test".into(),
            state: SessionState::Ended,
            previous_state: Some(SessionState::Active),
        }),
        SessionEvent::internal(InternalSessionEvent::SessionPaused {
            session_id: "agent-test".into(),
        }),
        SessionEvent::internal(InternalSessionEvent::SessionResumed {
            session_id: "agent-test".into(),
        }),
        SessionEvent::internal(InternalSessionEvent::TerminalOutput {
            session_id: "workflow-test".into(),
            stream: TerminalStream::Stdout,
            content_base64: "dGVzdA==".into(),
        }),
        SessionEvent::internal(InternalSessionEvent::TerminalOutput {
            session_id: "workflow-test".into(),
            stream: TerminalStream::Stderr,
            content_base64: "ZXJyb3I=".into(),
        }),
    ];

    for event in events {
        let json = serde_json::to_string(&event).unwrap();
        let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }
}

#[test]
fn test_delegation_spawned_serde() {
    // Test DelegationSpawned variant serializes/deserializes correctly
    let event = SessionEvent::DelegationSpawned {
        delegation_id: "deleg-123".into(),
        prompt: "Analyze this code".into(),
        parent_session_id: "parent-456".into(),
        target_agent: None,
    };

    // Test JSON round-trip
    let json = serde_json::to_string(&event).expect("serialize");
    let deserialized: SessionEvent = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(event, deserialized);

    // Verify JSON structure
    let json_obj: serde_json::Value = serde_json::from_str(&json).expect("parse json");
    assert_eq!(json_obj["type"], "delegation_spawned");
    assert_eq!(json_obj["delegation_id"], "deleg-123");
    assert_eq!(json_obj["parent_session_id"], "parent-456");
}

#[test]
fn test_delegation_completed_serde() {
    // Test DelegationCompleted variant serializes/deserializes correctly
    let event = SessionEvent::DelegationCompleted {
        delegation_id: "deleg-123".into(),
        result_summary: "Analysis complete: 5 issues found".into(),
        parent_session_id: "parent-456".into(),
    };

    // Test JSON round-trip
    let json = serde_json::to_string(&event).expect("serialize");
    let deserialized: SessionEvent = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(event, deserialized);

    // Verify JSON structure
    let json_obj: serde_json::Value = serde_json::from_str(&json).expect("parse json");
    assert_eq!(json_obj["type"], "delegation_completed");
    assert_eq!(json_obj["delegation_id"], "deleg-123");
    assert_eq!(json_obj["parent_session_id"], "parent-456");
}

#[test]
fn test_delegation_failed_serde() {
    // Test DelegationFailed variant serializes/deserializes correctly
    let event = SessionEvent::DelegationFailed {
        delegation_id: "deleg-123".into(),
        error: "Timeout after 30s".into(),
        parent_session_id: "parent-456".into(),
    };

    // Test JSON round-trip
    let json = serde_json::to_string(&event).expect("serialize");
    let deserialized: SessionEvent = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(event, deserialized);

    // Verify JSON structure
    let json_obj: serde_json::Value = serde_json::from_str(&json).expect("parse json");
    assert_eq!(json_obj["type"], "delegation_failed");
    assert_eq!(json_obj["delegation_id"], "deleg-123");
    assert_eq!(json_obj["parent_session_id"], "parent-456");
}

#[test]
fn test_subagent_variants_still_deserialize() {
    // Verify backwards compatibility: existing SubagentSpawned/Completed/Failed still work
    let subagent_spawned = SessionEvent::internal(InternalSessionEvent::SubagentSpawned {
        id: "sub-123".into(),
        prompt: "Do something".into(),
    });
    let json = serde_json::to_string(&subagent_spawned).expect("serialize");
    let deserialized: SessionEvent = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(subagent_spawned, deserialized);

    let subagent_completed = SessionEvent::internal(InternalSessionEvent::SubagentCompleted {
        id: "sub-123".into(),
        result: "Done".into(),
    });
    let json = serde_json::to_string(&subagent_completed).expect("serialize");
    let deserialized: SessionEvent = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(subagent_completed, deserialized);

    let subagent_failed = SessionEvent::internal(InternalSessionEvent::SubagentFailed {
        id: "sub-123".into(),
        error: "Failed".into(),
    });
    let json = serde_json::to_string(&subagent_failed).expect("serialize");
    let deserialized: SessionEvent = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(subagent_failed, deserialized);
}
