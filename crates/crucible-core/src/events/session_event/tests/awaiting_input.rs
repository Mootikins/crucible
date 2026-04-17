//! Tests for AwaitingInput event and InputType variants.

use super::*;

#[test]
fn test_input_type_default() {
    assert_eq!(InputType::default(), InputType::Message);
}

#[test]
fn test_input_type_display() {
    assert_eq!(format!("{}", InputType::Message), "message");
    assert_eq!(format!("{}", InputType::Approval), "approval");
    assert_eq!(format!("{}", InputType::Selection), "selection");
}

#[test]
fn test_input_type_serialization() {
    let message: InputType = serde_json::from_str("\"message\"").unwrap();
    assert_eq!(message, InputType::Message);

    let approval: InputType = serde_json::from_str("\"approval\"").unwrap();
    assert_eq!(approval, InputType::Approval);

    let selection: InputType = serde_json::from_str("\"selection\"").unwrap();
    assert_eq!(selection, InputType::Selection);
}

#[test]
fn test_awaiting_input_event_type() {
    let event = SessionEvent::internal(InternalSessionEvent::AwaitingInput {
        input_type: InputType::Message,
        context: None,
    });
    assert_eq!(event.event_type(), "awaiting_input");
}

#[test]
fn test_awaiting_input_identifier() {
    let message_event = SessionEvent::internal(InternalSessionEvent::AwaitingInput {
        input_type: InputType::Message,
        context: None,
    });
    assert_eq!(message_event.identifier(), "await:message");

    let approval_event = SessionEvent::internal(InternalSessionEvent::AwaitingInput {
        input_type: InputType::Approval,
        context: Some("delete files".into()),
    });
    assert_eq!(approval_event.identifier(), "await:approval");
}

#[test]
fn test_awaiting_input_serialization() {
    let event = SessionEvent::internal(InternalSessionEvent::AwaitingInput {
        input_type: InputType::Approval,
        context: Some("Agent wants to delete files".into()),
    });

    let json = serde_json::to_string(&event).unwrap();
    let restored: SessionEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, restored);

    // Verify JSON structure
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["type"], "awaiting_input");
    assert_eq!(parsed["input_type"], "approval");
    assert_eq!(parsed["context"], "Agent wants to delete files");
}

#[test]
fn test_awaiting_input_not_pre_event() {
    // AwaitingInput is NOT a pre-event (it's a state change, not an interception point)
    let event = SessionEvent::internal(InternalSessionEvent::AwaitingInput {
        input_type: InputType::Message,
        context: None,
    });
    assert!(!event.is_pre_event());
}
