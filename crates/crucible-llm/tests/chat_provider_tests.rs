//! Integration tests for LlmProvider implementations
//!
//! These tests define the contract that all LlmProvider implementations must satisfy.
//! Following TDD: Write tests first, then implement to make them pass.

use crucible_core::traits::{
    LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmToolDefinition, MessageRole,
    ToolCall,
};
use serde_json::json;

/// Test helper: Create a mock chat provider for testing
/// This will be replaced with real providers once implemented
#[cfg(test)]
mod test_helpers {
    use super::*;
    use async_trait::async_trait;
    use crucible_core::traits::{LlmError, LlmResult};

    pub struct MockChatProvider {
        pub response_content: String,
        pub should_call_tool: bool,
    }

    #[async_trait]
    impl LlmProvider for MockChatProvider {
        async fn chat(&self, request: LlmRequest) -> LlmResult<LlmResponse> {
            let message = if self.should_call_tool {
                LlmMessage::assistant_with_tools(
                    self.response_content.clone(),
                    vec![ToolCall::new(
                        "call_1",
                        "search_notes",
                        json!({"query": "rust programming"}),
                    )],
                )
            } else {
                LlmMessage::assistant(self.response_content.clone())
            };

            Ok(LlmResponse {
                message,
                usage: crucible_core::traits::TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                },
                model: "mock-model".to_string(),
            })
        }

        fn provider_name(&self) -> &str {
            "Mock"
        }

        fn default_model(&self) -> &str {
            "mock-model"
        }

        async fn health_check(&self) -> LlmResult<bool> {
            Ok(true)
        }
    }
}

use test_helpers::MockChatProvider;

#[tokio::test]
async fn test_simple_chat_completion() {
    // Given: A provider and a simple user message
    let provider = MockChatProvider {
        response_content: "Hello! How can I help you?".to_string(),
        should_call_tool: false,
    };

    let request = LlmRequest::new(vec![LlmMessage::user("Hello")]);

    // When: We request a chat completion
    let response = provider.chat(request).await.unwrap();

    // Then: We get a valid assistant response
    assert_eq!(response.message.role, MessageRole::Assistant);
    assert_eq!(response.message.content, "Hello! How can I help you?");
    assert!(response.message.tool_calls.is_none());
    assert_eq!(response.model, "mock-model");
}

#[tokio::test]
async fn test_chat_with_tool_calling() {
    // Given: A provider and a request with tools available
    let provider = MockChatProvider {
        response_content: "I'll search for that.".to_string(),
        should_call_tool: true,
    };

    let tools = vec![LlmToolDefinition::new(
        "search_notes",
        "Search through notes",
        json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"}
            }
        }),
    )];

    let request = LlmRequest::new(vec![LlmMessage::user("Find notes about Rust")])
        .with_tools(tools);

    // When: We request a chat completion
    let response = provider.chat(request).await.unwrap();

    // Then: The assistant makes a tool call
    assert_eq!(response.message.role, MessageRole::Assistant);
    assert!(response.message.tool_calls.is_some());

    let tool_calls = response.message.tool_calls.unwrap();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].name, "search_notes");
    assert_eq!(tool_calls[0].parameters["query"], "rust programming");
}

#[tokio::test]
async fn test_multi_turn_conversation() {
    // Given: A multi-turn conversation
    let provider = MockChatProvider {
        response_content: "Based on our previous discussion...".to_string(),
        should_call_tool: false,
    };

    let messages = vec![
        LlmMessage::user("What is Rust?"),
        LlmMessage::assistant("Rust is a systems programming language."),
        LlmMessage::user("Tell me more about its safety features."),
    ];

    let request = LlmRequest::new(messages);

    // When: We continue the conversation
    let response = provider.chat(request).await.unwrap();

    // Then: We get a contextual response
    assert_eq!(response.message.role, MessageRole::Assistant);
    assert!(response
        .message
        .content
        .contains("Based on our previous discussion"));
}

#[tokio::test]
async fn test_provider_metadata() {
    // Given: A chat provider
    let provider = MockChatProvider {
        response_content: "Test".to_string(),
        should_call_tool: false,
    };

    // When: We query provider metadata
    let name = provider.provider_name();
    let model = provider.default_model();

    // Then: We get valid metadata
    assert_eq!(name, "Mock");
    assert_eq!(model, "mock-model");
}

#[tokio::test]
async fn test_health_check() {
    // Given: A chat provider
    let provider = MockChatProvider {
        response_content: "Test".to_string(),
        should_call_tool: false,
    };

    // When: We perform a health check
    let healthy = provider.health_check().await.unwrap();

    // Then: The provider is healthy
    assert!(healthy);
}

#[tokio::test]
async fn test_token_usage_tracking() {
    // Given: A provider and a request
    let provider = MockChatProvider {
        response_content: "Response".to_string(),
        should_call_tool: false,
    };

    let request = LlmRequest::new(vec![LlmMessage::user("Hello")]);

    // When: We get a response
    let response = provider.chat(request).await.unwrap();

    // Then: Token usage is tracked
    assert!(response.usage.prompt_tokens > 0);
    assert!(response.usage.completion_tokens > 0);
    assert_eq!(
        response.usage.total_tokens,
        response.usage.prompt_tokens + response.usage.completion_tokens
    );
}

#[tokio::test]
async fn test_request_builder() {
    // Given: A chat request builder
    let request = LlmRequest::new(vec![LlmMessage::user("Hello")])
        .with_max_tokens(100)
        .with_temperature(0.7);

    // Then: Parameters are set correctly
    assert_eq!(request.max_tokens, Some(100));
    assert_eq!(request.temperature, Some(0.7));
    assert_eq!(request.messages.len(), 1);
}

#[tokio::test]
async fn test_tool_result_message() {
    // Given: A tool result message
    let tool_result = LlmMessage::tool("call_123", json!({"results": ["note1", "note2"]}).to_string());

    // Then: It has the correct structure
    assert_eq!(tool_result.role, MessageRole::Tool);
    assert_eq!(tool_result.tool_call_id, Some("call_123".to_string()));
    assert!(tool_result.content.contains("note1"));
}
