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
fn test_estimate_messages_tokens_sums_metadata() {
    let msgs = vec![
        ContextMessage::user("abcd"),       // 4 / 4 = 1
        ContextMessage::assistant("abcde"), // ceil(5/4) = 2
        ContextMessage::system("12345678"), // 8 / 4 = 2
    ];
    // Sum reflects per-message metadata.token_estimate values
    assert_eq!(estimate_messages_tokens(&msgs), 5);
}

#[test]
fn test_estimate_messages_tokens_empty() {
    assert_eq!(estimate_messages_tokens(&[]), 0);
}
