//! Tests for supporting types: NoteChangeType, FileChangeKind and default
//! variants.

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
