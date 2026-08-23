//! Tests for ContextMessage - the canonical message type

use super::*;
use crate::traits::llm::ToolCall;

#[test]
fn test_user_message_construction() {
    let msg = ContextMessage::user("Hello");
    assert_eq!(msg.role, MessageRole::User);
    assert_eq!(msg.content, "Hello");
    assert!(msg.metadata.timestamp.is_some());
    assert!(msg.metadata.token_estimate > 0);
}

#[test]
fn test_assistant_message_construction() {
    let msg = ContextMessage::assistant("Hi there");
    assert_eq!(msg.role, MessageRole::Assistant);
    assert_eq!(msg.content, "Hi there");
}

#[test]
fn test_assistant_with_tool_calls() {
    let tool_call = ToolCall::new("call_1", "search", r#"{"q":"rust"}"#.to_string());
    let msg = ContextMessage::assistant_with_tools("Searching...", vec![tool_call.clone()]);

    assert_eq!(msg.role, MessageRole::Assistant);
    assert_eq!(msg.metadata.tool_calls.len(), 1);
    assert_eq!(msg.metadata.tool_calls[0].id, "call_1");
}

#[test]
fn test_system_message_construction() {
    let msg = ContextMessage::system("You are helpful");
    assert_eq!(msg.role, MessageRole::System);
}

#[test]
fn test_tool_result_construction() {
    let msg = ContextMessage::tool_result("call_1", "Result data");
    assert_eq!(msg.role, MessageRole::Tool);
    assert_eq!(msg.metadata.tool_call_id, Some("call_1".to_string()));
}

#[test]
fn test_message_metadata_chaining() {
    let msg = ContextMessage::user("test")
        .with_tag("important")
        .with_tag("urgent")
        .with_success(true);

    assert_eq!(msg.metadata.tags, vec!["important", "urgent"]);
    assert_eq!(msg.metadata.success, Some(true));
}

#[test]
fn test_message_serialization_roundtrip() {
    let original = ContextMessage::user("Hello world");
    let json = serde_json::to_string(&original).unwrap();
    let restored: ContextMessage = serde_json::from_str(&json).unwrap();

    assert_eq!(original.role, restored.role);
    assert_eq!(original.content, restored.content);
}

#[test]
fn test_estimate_tokens_chars_div_four_ceil() {
    assert_eq!(estimate_tokens(""), 0);
    assert_eq!(estimate_tokens("abcd"), 1);
    assert_eq!(estimate_tokens("abcde"), 2); // ceil(5/4) = 2
    assert_eq!(estimate_tokens("12345678"), 2);
    assert_eq!(estimate_tokens("123456789"), 3); // ceil(9/4) = 3
}

#[test]
fn range_deserializes_each_tagged_shape() {
    let parse = |v: serde_json::Value| serde_json::from_value::<Range>(v);
    assert!(matches!(
        parse(serde_json::json!({"type": "all"})).unwrap(),
        Range::All
    ));
    assert!(matches!(
        parse(serde_json::json!({"type": "last", "n": 3})).unwrap(),
        Range::Last(3)
    ));
    assert!(matches!(
        parse(serde_json::json!({"type": "first", "n": 2})).unwrap(),
        Range::First(2)
    ));
    match parse(serde_json::json!({"type": "indices", "start": 1, "end": 4})).unwrap() {
        Range::Indices(r) => assert_eq!(r, 1..4),
        _ => panic!("expected Indices"),
    }
}

#[test]
fn range_rejects_unknown_type() {
    let err = serde_json::from_value::<Range>(serde_json::json!({"type": "bogus"})).unwrap_err();
    assert!(
        err.to_string().contains("unknown variant `bogus`"),
        "got: {err}"
    );
}

#[test]
fn range_requires_n_for_last_and_first() {
    for ty in ["last", "first"] {
        let err = serde_json::from_value::<Range>(serde_json::json!({"type": ty})).unwrap_err();
        assert!(err.to_string().contains("missing field `n`"), "got: {err}");
    }
}

#[test]
fn range_requires_start_and_end_for_indices() {
    let err = serde_json::from_value::<Range>(serde_json::json!({"type": "indices", "start": 0}))
        .unwrap_err();
    assert!(
        err.to_string().contains("missing field `end`"),
        "got: {err}"
    );
}

#[test]
fn range_serializes_to_the_tagged_shape_it_reads() {
    let to = |r: Range| serde_json::to_value(r).unwrap();
    assert_eq!(to(Range::All), serde_json::json!({"type": "all"}));
    assert_eq!(
        to(Range::Last(3)),
        serde_json::json!({"type": "last", "n": 3})
    );
    assert_eq!(
        to(Range::First(2)),
        serde_json::json!({"type": "first", "n": 2})
    );
    assert_eq!(
        to(Range::Indices(1..4)),
        serde_json::json!({"type": "indices", "start": 1, "end": 4})
    );
}
