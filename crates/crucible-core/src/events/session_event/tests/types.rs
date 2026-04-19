//! Tests for supporting types: NoteChangeType, FileChangeKind, ToolProvider,
//! ToolCall, SessionEventConfig, EntityType, Priority, TerminalStream, and
//! default variants.

use super::*;
use serde_json::Value as JsonValue;

#[test]
fn test_note_change_type() {
    assert_eq!(NoteChangeType::default(), NoteChangeType::Content);
    assert_eq!(format!("{}", NoteChangeType::Content), "content");
    assert_eq!(format!("{}", NoteChangeType::Frontmatter), "frontmatter");
    assert_eq!(format!("{}", NoteChangeType::Links), "links");
    assert_eq!(format!("{}", NoteChangeType::Tags), "tags");
}

#[test]
fn test_file_change_kind() {
    // Test default
    assert_eq!(FileChangeKind::default(), FileChangeKind::Modified);

    // Test Display
    assert_eq!(format!("{}", FileChangeKind::Created), "created");
    assert_eq!(format!("{}", FileChangeKind::Modified), "modified");

    // Test serialization
    let created = FileChangeKind::Created;
    let json = serde_json::to_string(&created).unwrap();
    assert_eq!(json, "\"created\"");

    let modified = FileChangeKind::Modified;
    let json = serde_json::to_string(&modified).unwrap();
    assert_eq!(json, "\"modified\"");

    // Test deserialization
    let created: FileChangeKind = serde_json::from_str("\"created\"").unwrap();
    assert_eq!(created, FileChangeKind::Created);

    let modified: FileChangeKind = serde_json::from_str("\"modified\"").unwrap();
    assert_eq!(modified, FileChangeKind::Modified);

    // Test equality and hashing
    assert_eq!(FileChangeKind::Created, FileChangeKind::Created);
    assert_ne!(FileChangeKind::Created, FileChangeKind::Modified);

    // Test Clone and Copy
    let kind = FileChangeKind::Created;
    let cloned = kind;
    let copied = kind;
    assert_eq!(kind, cloned);
    assert_eq!(kind, copied);
}

#[test]
fn test_tool_provider() {
    assert_eq!(ToolProvider::default(), ToolProvider::Builtin);
    assert_eq!(format!("{}", ToolProvider::Lua), "lua");
    assert_eq!(format!("{}", ToolProvider::Lua), "lua");
    assert_eq!(
        format!(
            "{}",
            ToolProvider::Mcp {
                server: "test".into()
            }
        ),
        "mcp:test"
    );
    assert_eq!(format!("{}", ToolProvider::Builtin), "builtin");
}

#[test]
fn test_tool_call() {
    let test_file = test_path("test.txt");
    let test_file_str = test_file.to_string_lossy();
    let call = ToolCall::new("read_file", serde_json::json!({"path": test_file_str}))
        .with_call_id("call_123");

    assert_eq!(call.name, "read_file");
    assert_eq!(call.args["path"], test_file_str.as_ref());
    assert_eq!(call.call_id, Some("call_123".to_string()));
}

#[test]
fn test_session_event_config() {
    let session_folder = test_path("session");
    let config = SessionEventConfig::new("test-session")
        .with_folder(&session_folder)
        .with_max_context_tokens(50_000)
        .with_system_prompt("You are helpful.");

    assert_eq!(config.session_id, "test-session");
    assert_eq!(config.folder, Some(session_folder));
    assert_eq!(config.max_context_tokens, 50_000);
    assert_eq!(config.system_prompt, Some("You are helpful.".to_string()));
}

#[test]
fn test_session_event_default() {
    let event = SessionEvent::default();
    match event {
        SessionEvent::Custom { name, payload } => {
            assert_eq!(name, "default");
            assert_eq!(payload, JsonValue::Null);
        }
        _ => panic!("Expected Custom variant"),
    }
}

#[test]
fn test_entity_type() {
    // Test default
    assert_eq!(EntityType::default(), EntityType::Note);

    // Test Display
    assert_eq!(format!("{}", EntityType::Note), "note");
    assert_eq!(format!("{}", EntityType::Block), "block");
    assert_eq!(format!("{}", EntityType::Tag), "tag");
    assert_eq!(format!("{}", EntityType::Task), "task");
    assert_eq!(format!("{}", EntityType::TaskFile), "task_file");

    // Test serialization
    let note = EntityType::Note;
    let json = serde_json::to_string(&note).unwrap();
    assert_eq!(json, "\"note\"");

    let task_file = EntityType::TaskFile;
    let json = serde_json::to_string(&task_file).unwrap();
    assert_eq!(json, "\"task_file\"");

    // Test deserialization
    let note: EntityType = serde_json::from_str("\"note\"").unwrap();
    assert_eq!(note, EntityType::Note);

    let task_file: EntityType = serde_json::from_str("\"task_file\"").unwrap();
    assert_eq!(task_file, EntityType::TaskFile);

    // Test equality and hashing
    assert_eq!(EntityType::Note, EntityType::Note);
    assert_ne!(EntityType::Note, EntityType::Block);

    // Test Clone and Copy
    let entity_type = EntityType::Task;
    let cloned = entity_type;
    let copied = entity_type;
    assert_eq!(entity_type, cloned);
    assert_eq!(entity_type, copied);

    // Test Hash (use in HashSet)
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(EntityType::Note);
    set.insert(EntityType::Block);
    set.insert(EntityType::Note); // duplicate
    assert_eq!(set.len(), 2);
}

#[test]
fn test_priority() {
    // Test default
    assert_eq!(Priority::default(), Priority::Normal);

    // Test Display
    assert_eq!(format!("{}", Priority::Low), "low");
    assert_eq!(format!("{}", Priority::Normal), "normal");
    assert_eq!(format!("{}", Priority::High), "high");
    assert_eq!(format!("{}", Priority::Critical), "critical");

    // Test serialization
    let low = Priority::Low;
    let json = serde_json::to_string(&low).unwrap();
    assert_eq!(json, "\"low\"");

    let critical = Priority::Critical;
    let json = serde_json::to_string(&critical).unwrap();
    assert_eq!(json, "\"critical\"");

    // Test deserialization
    let low: Priority = serde_json::from_str("\"low\"").unwrap();
    assert_eq!(low, Priority::Low);

    let critical: Priority = serde_json::from_str("\"critical\"").unwrap();
    assert_eq!(critical, Priority::Critical);

    // Test equality
    assert_eq!(Priority::Normal, Priority::Normal);
    assert_ne!(Priority::Low, Priority::High);

    // Test Clone and Copy
    let priority = Priority::High;
    let cloned = priority;
    let copied = priority;
    assert_eq!(priority, cloned);
    assert_eq!(priority, copied);

    // Test Hash (use in HashSet)
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(Priority::Low);
    set.insert(Priority::High);
    set.insert(Priority::Low); // duplicate
    assert_eq!(set.len(), 2);
}

#[test]
fn test_priority_ordering() {
    // Test that higher priority values compare greater
    assert!(Priority::Critical > Priority::High);
    assert!(Priority::High > Priority::Normal);
    assert!(Priority::Normal > Priority::Low);

    // Test min/max
    assert!(Priority::Critical >= Priority::Low);
    assert!(Priority::Low <= Priority::Critical);

    // Test sorting
    let mut priorities = vec![
        Priority::Normal,
        Priority::Critical,
        Priority::Low,
        Priority::High,
    ];
    priorities.sort();
    assert_eq!(
        priorities,
        vec![
            Priority::Low,
            Priority::Normal,
            Priority::High,
            Priority::Critical
        ]
    );
}

#[test]
fn test_session_event_priority() {
    // FileChanged(Created) → High
    let created = SessionEvent::internal(InternalSessionEvent::FileChanged {
        path: PathBuf::from("/notes/new.md"),
        kind: FileChangeKind::Created,
    });
    assert_eq!(created.priority(), Priority::High);

    // FileChanged(Modified) → Normal
    let modified = SessionEvent::internal(InternalSessionEvent::FileChanged {
        path: PathBuf::from("/notes/existing.md"),
        kind: FileChangeKind::Modified,
    });
    assert_eq!(modified.priority(), Priority::Normal);

    // FileDeleted → Low
    let deleted = SessionEvent::internal(InternalSessionEvent::FileDeleted {
        path: PathBuf::from("/notes/old.md"),
    });
    assert_eq!(deleted.priority(), Priority::Low);

    // FileMoved → Normal
    let moved = SessionEvent::internal(InternalSessionEvent::FileMoved {
        from: PathBuf::from("/notes/old.md"),
        to: PathBuf::from("/notes/new.md"),
    });
    assert_eq!(moved.priority(), Priority::Normal);

    // EmbeddingRequested → uses embedded priority
    let embedding_normal = SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
        entity_id: "test".into(),
        block_id: None,
        priority: Priority::Normal,
    });
    assert_eq!(embedding_normal.priority(), Priority::Normal);

    let embedding_high = SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
        entity_id: "test".into(),
        block_id: None,
        priority: Priority::High,
    });
    assert_eq!(embedding_high.priority(), Priority::High);

    let embedding_critical = SessionEvent::internal(InternalSessionEvent::EmbeddingRequested {
        entity_id: "test".into(),
        block_id: None,
        priority: Priority::Critical,
    });
    assert_eq!(embedding_critical.priority(), Priority::Critical);

    // Other events default to Normal
    let message = SessionEvent::MessageReceived {
        content: "hello".into(),
        participant_id: "user".into(),
    };
    assert_eq!(message.priority(), Priority::Normal);

    let tool_called = SessionEvent::ToolCalled {
        name: "search".into(),
        args: JsonValue::Null,
        description: None,
        source: None,
    };
    assert_eq!(tool_called.priority(), Priority::Normal);

    let entity_stored = SessionEvent::internal(InternalSessionEvent::EntityStored {
        entity_id: "test".into(),
        entity_type: EntityType::Note,
    });
    assert_eq!(entity_stored.priority(), Priority::Normal);

    let custom = SessionEvent::Custom {
        name: "custom".into(),
        payload: JsonValue::Null,
    };
    assert_eq!(custom.priority(), Priority::Normal);
}

#[test]
fn test_terminal_stream() {
    // Test default
    assert_eq!(TerminalStream::default(), TerminalStream::Stdout);

    // Test Display
    assert_eq!(format!("{}", TerminalStream::Stdout), "stdout");
    assert_eq!(format!("{}", TerminalStream::Stderr), "stderr");

    // Test serialization
    let stdout = TerminalStream::Stdout;
    let json = serde_json::to_string(&stdout).unwrap();
    assert_eq!(json, "\"stdout\"");

    let stderr = TerminalStream::Stderr;
    let json = serde_json::to_string(&stderr).unwrap();
    assert_eq!(json, "\"stderr\"");

    // Test deserialization
    let stdout: TerminalStream = serde_json::from_str("\"stdout\"").unwrap();
    assert_eq!(stdout, TerminalStream::Stdout);

    let stderr: TerminalStream = serde_json::from_str("\"stderr\"").unwrap();
    assert_eq!(stderr, TerminalStream::Stderr);
}
