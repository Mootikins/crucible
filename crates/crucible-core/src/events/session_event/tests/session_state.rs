//! Tests for session state lifecycle and terminal output events emitted by the
//! daemon protocol.

use super::*;

#[test]
fn test_session_state_changed_event() {
    use crate::session::SessionState;

    let event = SessionEvent::internal(InternalSessionEvent::SessionStateChanged {
        session_id: "chat-2025-01-08T1530-abc123".into(),
        state: SessionState::Paused,
        previous_state: Some(SessionState::Active),
    });

    assert_eq!(event.event_type(), "session_state_changed");
    assert!(event.category() == EventCategory::Lifecycle);
    assert_eq!(
        event.identifier(),
        "session:state_changed:chat-2025-01-08T1530-abc123"
    );

    // Verify serialization
    let json = serde_json::to_string(&event).unwrap();
    let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, parsed);
}

#[test]
fn test_session_paused_event() {
    let event = SessionEvent::internal(InternalSessionEvent::SessionPaused {
        session_id: "chat-2025-01-08T1530-abc123".into(),
    });

    assert_eq!(event.event_type(), "session_paused");
    assert!(event.category() == EventCategory::Lifecycle);
    assert_eq!(
        event.identifier(),
        "session:paused:chat-2025-01-08T1530-abc123"
    );

    let json = serde_json::to_string(&event).unwrap();
    let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, parsed);
}

#[test]
fn test_session_resumed_event() {
    let event = SessionEvent::internal(InternalSessionEvent::SessionResumed {
        session_id: "chat-2025-01-08T1530-abc123".into(),
    });

    assert_eq!(event.event_type(), "session_resumed");
    assert!(event.category() == EventCategory::Lifecycle);
    assert_eq!(
        event.identifier(),
        "session:resumed:chat-2025-01-08T1530-abc123"
    );

    let json = serde_json::to_string(&event).unwrap();
    let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, parsed);
}

#[test]
fn test_terminal_output_event() {
    let event = SessionEvent::internal(InternalSessionEvent::TerminalOutput {
        session_id: "chat-2025-01-08T1530-abc123".into(),
        stream: TerminalStream::Stdout,
        content_base64: "SGVsbG8gV29ybGQK".into(), // "Hello World\n"
    });

    assert_eq!(event.event_type(), "terminal_output");
    assert!(event.category() == EventCategory::Streaming);
    assert!(event.category() != EventCategory::Lifecycle);
    assert_eq!(
        event.identifier(),
        "terminal:chat-2025-01-08T1530-abc123:stdout"
    );

    let json = serde_json::to_string(&event).unwrap();
    let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, parsed);
}
