//! Integration tests for TextGenerationProvider implementations
//!
//! These tests define the contract that all TextGenerationProvider implementations must satisfy.

use async_trait::async_trait;
use chrono::Utc;
use crucible_core::traits::{
    ChatCompletionChoice, ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse,
    ChatMessageDelta, CompletionChoice, CompletionChunk, CompletionRequest, CompletionResponse,
    LlmError, LlmMessage, LlmRequest, LlmResult, LlmToolDefinition, MessageRole, ModelCapability,
    ModelStatus, ProviderCapabilities, TextGenerationProvider, TextModelInfo, TokenUsage, ToolCall,
};
use futures::stream::{self, BoxStream};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Mock text generation provider for integration tests
struct MockTextProvider {
    model_name: String,
    chat_responses: Arc<Mutex<HashMap<String, String>>>,
    default_response: String,
}

impl MockTextProvider {
    fn new() -> Self {
        Self {
            model_name: "mock-llm".to_string(),
            chat_responses: Arc::new(Mutex::new(HashMap::new())),
            default_response: "This is a mock response.".to_string(),
        }
    }

    fn set_chat_response(&self, last_user_message: &str, response: &str) {
        let mut responses = self.chat_responses.lock().unwrap();
        responses.insert(last_user_message.to_string(), response.to_string());
    }

    fn get_chat_response(&self, messages: &[LlmMessage]) -> String {
        let last_user_message = messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
            .map(|m| m.content.as_str())
            .unwrap_or("");

        let responses = self.chat_responses.lock().unwrap();
        responses
            .get(last_user_message)
            .cloned()
            .unwrap_or_else(|| self.default_response.clone())
    }
}

#[async_trait]
impl TextGenerationProvider for MockTextProvider {
    async fn generate_completion(
        &self,
        request: CompletionRequest,
    ) -> LlmResult<CompletionResponse> {
        Ok(CompletionResponse {
            choices: vec![CompletionChoice {
                text: self.default_response.clone(),
                index: 0,
                logprobs: None,
                finish_reason: Some("stop".to_string()),
            }],
            model: self.model_name.clone(),
            usage: TokenUsage {
                prompt_tokens: request.prompt.split_whitespace().count() as u32,
                completion_tokens: 10,
                total_tokens: request.prompt.split_whitespace().count() as u32 + 10,
            },
            id: "mock-completion-id".to_string(),
            object: "text_completion".to_string(),
            created: Utc::now(),
            system_fingerprint: Some("mock-fp".to_string()),
        })
    }

    fn generate_completion_stream<'a>(
        &'a self,
        _request: CompletionRequest,
    ) -> BoxStream<'a, LlmResult<CompletionChunk>> {
        Box::pin(stream::iter(vec![Ok(CompletionChunk {
            text: self.default_response.clone(),
            index: 0,
            finish_reason: Some("stop".to_string()),
            logprobs: None,
        })]))
    }

    async fn generate_chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> LlmResult<ChatCompletionResponse> {
        let response_text = self.get_chat_response(&request.messages);

        Ok(ChatCompletionResponse {
            choices: vec![ChatCompletionChoice {
                index: 0,
                message: LlmMessage {
                    role: MessageRole::Assistant,
                    content: response_text.clone(),
                    function_call: None,
                    tool_calls: None,
                    name: None,
                    tool_call_id: None,
                },
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            }],
            model: self.model_name.clone(),
            usage: TokenUsage {
                prompt_tokens: request.messages.len() as u32 * 10,
                completion_tokens: response_text.split_whitespace().count() as u32,
                total_tokens: (request.messages.len() as u32 * 10)
                    + response_text.split_whitespace().count() as u32,
            },
            id: "mock-chat-id".to_string(),
            object: "chat.completion".to_string(),
            created: Utc::now(),
            system_fingerprint: Some("mock-fp".to_string()),
        })
    }

    fn generate_chat_completion_stream<'a>(
        &'a self,
        _request: ChatCompletionRequest,
    ) -> BoxStream<'a, LlmResult<ChatCompletionChunk>> {
        Box::pin(stream::iter(vec![Ok(ChatCompletionChunk {
            index: 0,
            delta: ChatMessageDelta {
                role: Some(MessageRole::Assistant),
                content: Some(self.default_response.clone()),
                function_call: None,
                tool_calls: None,
            },
            finish_reason: Some("stop".to_string()),
            logprobs: None,
        })]))
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    fn default_model(&self) -> &str {
        &self.model_name
    }

    async fn list_models(&self) -> LlmResult<Vec<TextModelInfo>> {
        Ok(vec![TextModelInfo {
            id: "mock-llm".to_string(),
            name: "Mock LLM".to_string(),
            owner: Some("Test".to_string()),
            capabilities: vec![
                ModelCapability::TextCompletion,
                ModelCapability::ChatCompletion,
                ModelCapability::Streaming,
            ],
            max_context_length: Some(4096),
            max_output_tokens: Some(2048),
            input_price: None,
            output_price: None,
            created: Some(Utc::now()),
            status: ModelStatus::Available,
        }])
    }

    async fn health_check(&self) -> LlmResult<bool> {
        Ok(true)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            text_completion: true,
            chat_completion: true,
            streaming: true,
            function_calling: false,
            tool_use: false,
            vision: false,
            audio: false,
            max_batch_size: Some(1),
            input_formats: vec!["text".to_string()],
            output_formats: vec!["text".to_string()],
        }
    }
}

#[tokio::test]
async fn test_simple_chat_completion() {
    // Given: A provider and a simple user message
    let provider = MockTextProvider::new();
    provider.set_chat_response("Hello", "Hello! How can I help you?");

    let request =
        ChatCompletionRequest::new("mock-model".to_string(), vec![LlmMessage::user("Hello")]);

    // When: We request a chat completion
    let response = provider.generate_chat_completion(request).await.unwrap();

    // Then: We get a valid assistant response
    let message = &response.choices[0].message;
    assert_eq!(message.role, MessageRole::Assistant);
    assert_eq!(message.content, "Hello! How can I help you?");
    assert!(message.tool_calls.is_none());
}

#[tokio::test]
async fn test_multi_turn_conversation() {
    // Given: A multi-turn conversation
    let provider = MockTextProvider::new();
    provider.set_chat_response(
        "Tell me more about its safety features.",
        "Based on our previous discussion, Rust provides memory safety guarantees.",
    );

    let messages = vec![
        LlmMessage::user("What is Rust?"),
        LlmMessage::assistant("Rust is a systems programming language."),
        LlmMessage::user("Tell me more about its safety features."),
    ];

    let request = ChatCompletionRequest::new("mock-model".to_string(), messages);

    // When: We continue the conversation
    let response = provider.generate_chat_completion(request).await.unwrap();

    // Then: We get a contextual response
    let message = &response.choices[0].message;
    assert_eq!(message.role, MessageRole::Assistant);
    assert!(message.content.contains("safety"));
}

#[tokio::test]
async fn test_provider_capabilities() {
    // Given: A chat provider
    let provider = MockTextProvider::new();

    // When: We query provider capabilities
    let capabilities = provider.capabilities();

    // Then: We get valid capabilities
    assert!(capabilities.chat_completion);
    assert!(capabilities.streaming);
}

#[tokio::test]
async fn test_token_usage_tracking() {
    // Given: A provider and a request
    let provider = MockTextProvider::new();

    let request =
        ChatCompletionRequest::new("mock-model".to_string(), vec![LlmMessage::user("Hello")]);

    // When: We get a response
    let response = provider.generate_chat_completion(request).await.unwrap();

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
    let request = LlmRequest::new("mock-model".to_string(), vec![LlmMessage::user("Hello")])
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
    let tool_result = LlmMessage::tool(
        "call_123",
        json!({"results": ["note1", "note2"]}).to_string(),
    );

    // Then: It has the correct structure
    assert_eq!(tool_result.role, MessageRole::Tool);
    assert_eq!(tool_result.tool_call_id, Some("call_123".to_string()));
    assert!(tool_result.content.contains("note1"));
}

#[tokio::test]
async fn test_health_check() {
    // Given: A chat provider
    let provider = MockTextProvider::new();

    // When: We perform a health check
    let healthy = provider.health_check().await.unwrap();

    // Then: The provider is healthy
    assert!(healthy);
}

#[tokio::test]
async fn test_list_models() {
    // Given: A chat provider
    let provider = MockTextProvider::new();

    // When: We list models
    let models = provider.list_models().await.unwrap();

    // Then: We get model info
    assert!(!models.is_empty());
    assert_eq!(models[0].id, "mock-llm");
}

#[tokio::test]
async fn test_provider_metadata() {
    // Given: A chat provider
    let provider = MockTextProvider::new();

    // When: We query provider metadata
    let name = provider.provider_name();
    let model = provider.default_model();

    // Then: We get valid metadata
    assert_eq!(name, "mock");
    assert_eq!(model, "mock-llm");
}

#[tokio::test]
async fn test_tool_definition_creation() {
    // Given: Tool definitions
    let tool = LlmToolDefinition::new(
        "search_notes",
        "Search through notes",
        json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"}
            }
        }),
    );

    // Then: Tool definition is correct
    assert_eq!(tool.function.name, "search_notes");
    assert_eq!(tool.function.description, "Search through notes");
}

#[tokio::test]
async fn test_tool_call_creation() {
    // Given: Creating a tool call
    let tool_call = ToolCall::new(
        "call_1",
        "search_notes",
        json!({"query": "rust programming"}).to_string(),
    );

    // Then: Tool call is correct
    assert_eq!(tool_call.id, "call_1");
    assert_eq!(tool_call.function.name, "search_notes");
    assert!(tool_call.function.arguments.contains("rust programming"));
}
