//! Tests for the live `SessionEvent` variants: names, identifiers,
//! categories, summaries, payloads and the serde round trip.

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
fn identifier_names_the_subject() {
    assert_eq!(message().identifier(), "message:user");
    assert_eq!(interaction().identifier(), "interaction:permission:req-1");
    assert_eq!(custom().identifier(), "tool_called");
    assert_eq!(
        note_modified().identifier(),
        test_path("note.md").display().to_string()
    );
    assert_eq!(precognition().identifier(), "precognition:complete");
}

#[test]
fn category_groups_by_concern() {
    assert_eq!(message().category(), EventCategory::Message);
    assert_eq!(interaction().category(), EventCategory::Interaction);
    assert_eq!(custom().category(), EventCategory::Custom);
    assert_eq!(note_modified().category(), EventCategory::Note);
    assert_eq!(
        SessionEvent::internal(InternalSessionEvent::FileDeleted {
            path: test_path("a.md"),
        })
        .category(),
        EventCategory::File
    );
    assert_eq!(precognition().category(), EventCategory::Other);
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
fn payload_is_the_main_content() {
    assert_eq!(message().payload(100), Some("Hello, world!".to_string()));
    assert_eq!(message().payload(5), Some("Hello".to_string()));
    assert_eq!(interaction().payload(100), None);
    assert_eq!(
        custom().payload(100),
        Some(r#"{"tool":"search"}"#.to_string())
    );
    assert_eq!(
        precognition().payload(100),
        Some("notes=3, query=rust".to_string())
    );
}

#[test]
fn estimate_tokens_adds_overhead() {
    assert_eq!(message().estimate_tokens(), 13 / 4 + 10);
    assert_eq!(interaction().estimate_tokens(), 100 / 4 + 10);
    assert_eq!(note_modified().estimate_tokens(), 50 / 4 + 10);
}

#[test]
fn every_live_event_round_trips_through_json() {
    for event in every_live_event() {
        let json = serde_json::to_string(&event).unwrap();
        let back: SessionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event, "{json}");
    }
}

#[test]
fn json_tag_is_the_event_type() {
    for event in every_live_event() {
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["type"], event.event_type(), "{value}");
    }
}

#[test]
fn an_unknown_tag_is_an_error() {
    let err = serde_json::from_str::<SessionEvent>(r#"{"type":"tool_called","name":"x"}"#)
        .unwrap_err()
        .to_string();
    assert!(err.contains("unknown event type 'tool_called'"), "{err}");
}
