//! Tests for pre-event interception points (PreToolCall, PreParse, PreLlmCall).

use super::*;
use serde_json::Value as JsonValue;

#[test]
fn test_pre_tool_call_event_type() {
    let event = SessionEvent::internal(InternalSessionEvent::PreToolCall {
        name: "search".into(),
        args: serde_json::json!({"q": "rust"}),
    });
    assert_eq!(event.event_type(), "pre_tool_call");
    assert!(event.is_pre_event());
    assert!(!event.is_tool_event()); // Pre-events are separate from tool events
}

#[test]
fn test_pre_parse_event_type() {
    let event = SessionEvent::internal(InternalSessionEvent::PreParse {
        path: PathBuf::from("/notes/test.md"),
    });
    assert_eq!(event.event_type(), "pre_parse");
    assert!(event.is_pre_event());
    assert!(!event.is_note_event()); // Pre-events are separate from note events
}

#[test]
fn test_pre_llm_call_event_type() {
    let event = SessionEvent::internal(InternalSessionEvent::PreLlmCall {
        prompt: "Hello".into(),
        model: "gpt-4".into(),
    });
    assert_eq!(event.event_type(), "pre_llm_call");
    assert!(event.is_pre_event());
}

#[test]
fn test_pre_event_identifiers() {
    let tool_event = SessionEvent::internal(InternalSessionEvent::PreToolCall {
        name: "bash".into(),
        args: serde_json::json!({"cmd": "ls"}),
    });
    assert_eq!(tool_event.identifier(), "pre:tool:bash");

    let parse_event = SessionEvent::internal(InternalSessionEvent::PreParse {
        path: PathBuf::from("/notes/test.md"),
    });
    assert_eq!(parse_event.identifier(), "pre:parse:/notes/test.md");

    let llm_event = SessionEvent::internal(InternalSessionEvent::PreLlmCall {
        prompt: "Hello".into(),
        model: "gpt-4".into(),
    });
    assert_eq!(llm_event.identifier(), "pre:llm:gpt-4");
}

#[test]
fn test_pre_event_serialization() {
    let event = SessionEvent::internal(InternalSessionEvent::PreToolCall {
        name: "bash".into(),
        args: serde_json::json!({"cmd": "ls"}),
    });

    let json = serde_json::to_string(&event).unwrap();
    let restored: SessionEvent = serde_json::from_str(&json).unwrap();

    assert_eq!(event, restored);
}

#[test]
fn test_all_pre_events_serialize() {
    let events = vec![
        SessionEvent::internal(InternalSessionEvent::PreToolCall {
            name: "search".into(),
            args: serde_json::json!({"q": "test"}),
        }),
        SessionEvent::internal(InternalSessionEvent::PreParse {
            path: PathBuf::from("/notes/test.md"),
        }),
        SessionEvent::internal(InternalSessionEvent::PreLlmCall {
            prompt: "Hello".into(),
            model: "claude-3".into(),
        }),
    ];

    for event in events {
        let json = serde_json::to_string(&event).unwrap();
        let parsed: SessionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, parsed);
    }
}

#[test]
fn test_pre_event_priority() {
    // Pre-events default to Normal priority
    let pre_tool = SessionEvent::internal(InternalSessionEvent::PreToolCall {
        name: "search".into(),
        args: JsonValue::Null,
    });
    assert_eq!(pre_tool.priority(), Priority::Normal);

    let pre_parse = SessionEvent::internal(InternalSessionEvent::PreParse {
        path: PathBuf::from("/notes/test.md"),
    });
    assert_eq!(pre_parse.priority(), Priority::Normal);

    let pre_llm = SessionEvent::internal(InternalSessionEvent::PreLlmCall {
        prompt: "test".into(),
        model: "gpt-4".into(),
    });
    assert_eq!(pre_llm.priority(), Priority::Normal);
}
