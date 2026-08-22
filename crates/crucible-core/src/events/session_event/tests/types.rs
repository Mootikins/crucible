//! Tests for supporting types: NoteChangeType, FileChangeKind, Priority, and
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

    // Other events default to Normal
    let message = SessionEvent::MessageReceived {
        content: "hello".into(),
        participant_id: "user".into(),
    };
    assert_eq!(message.priority(), Priority::Normal);

    let custom = SessionEvent::Custom {
        name: "custom".into(),
        payload: JsonValue::Null,
    };
    assert_eq!(custom.priority(), Priority::Normal);
}
