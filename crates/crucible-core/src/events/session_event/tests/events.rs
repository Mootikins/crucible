//! Tests for the live `SessionEvent` variants: names, summaries and the
//! JSON tag.

use super::*;
use crate::interaction::{InteractionRequest, PermRequest};
use serde_json::json;

fn message() -> SessionEvent {
    SessionEvent::MessageReceived {
        content: "Hello, world!".into(),
        participant_id: "user".into(),
    }
}

fn interaction() -> SessionEvent {
    SessionEvent::InteractionRequested {
        request_id: "req-1".into(),
        request: InteractionRequest::Permission(PermRequest::bash(["true"])),
    }
}

fn custom() -> SessionEvent {
    SessionEvent::Custom {
        name: "tool_called".into(),
        payload: json!({"tool": "search"}),
    }
}

fn note_modified() -> SessionEvent {
    SessionEvent::internal(InternalSessionEvent::NoteModified {
        path: test_path("note.md"),
        change_type: NoteChangeType::Content,
    })
}

fn precognition() -> SessionEvent {
    SessionEvent::internal(InternalSessionEvent::PrecognitionComplete {
        notes_count: 3,
        query_summary: "rust".into(),
        kilns_searched: 2,
        kilns_filtered: 0,
        kilns_failed: 1,
    })
}

fn every_live_event() -> Vec<SessionEvent> {
    vec![
        message(),
        interaction(),
        custom(),
        SessionEvent::internal(InternalSessionEvent::FileChanged {
            path: test_path("a.md"),
            kind: FileChangeKind::Created,
        }),
        SessionEvent::internal(InternalSessionEvent::FileDeleted {
            path: test_path("a.md"),
        }),
        SessionEvent::internal(InternalSessionEvent::FileMoved {
            from: test_path("a.md"),
            to: test_path("b.md"),
        }),
        SessionEvent::internal(InternalSessionEvent::NoteCreated {
            path: test_path("a.md"),
            title: Some("A".into()),
        }),
        note_modified(),
        SessionEvent::internal(InternalSessionEvent::NoteDeleted {
            path: test_path("a.md"),
            existed: true,
        }),
        precognition(),
    ]
}

#[test]
fn event_type_is_the_snake_case_name() {
    assert_eq!(message().event_type(), "message_received");
    assert_eq!(interaction().event_type(), "interaction_requested");
    assert_eq!(custom().event_type(), "custom");
    assert_eq!(note_modified().event_type(), "note_modified");
    assert_eq!(precognition().event_type(), "precognition_complete");
}

#[test]
fn type_name_is_the_variant_name() {
    assert_eq!(message().type_name(), "MessageReceived");
    assert_eq!(interaction().type_name(), "InteractionRequested");
    assert_eq!(custom().type_name(), "Custom");
    assert_eq!(note_modified().type_name(), "NoteModified");
    assert_eq!(precognition().type_name(), "PrecognitionComplete");
}

#[test]
fn summary_names_the_key_fields() {
    assert_eq!(message().summary(100), "from=user, content_len=13");
    assert!(interaction().summary(100).starts_with("id=req-1, kind="));
    assert!(custom()
        .summary(100)
        .starts_with("name=tool_called, payload_size="));
    assert!(note_modified().summary(100).contains("change=Content"));
    assert_eq!(
        precognition().summary(100),
        "notes=3, query=rust, searched=2, filtered=0, failed=1"
    );
}

#[test]
fn summary_cuts_free_text_to_max_len() {
    let event = SessionEvent::Custom {
        name: "a".repeat(50),
        payload: json!(null),
    };
    assert!(event
        .summary(10)
        .starts_with(&format!("name={}, ", "a".repeat(10))));
}

#[test]
fn json_tag_is_the_event_type() {
    for event in every_live_event() {
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["type"], event.event_type(), "{value}");
    }
}
