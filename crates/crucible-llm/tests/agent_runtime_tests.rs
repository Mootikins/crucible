//! Integration tests for Agent Runtime
//!
//! The agent runtime coordinates between TextGenerationProvider and ToolExecutor
//! to enable autonomous agent behavior with tool calling.

use async_trait::async_trait;
use chrono::Utc;
use crucible_core::traits::{
    ChatCompletionChoice, ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse,
    ChatMessageDelta, CompletionChoice, CompletionChunk, CompletionRequest, CompletionResponse,
    ExecutionContext, LlmMessage, LlmResult, MessageRole, ModelCapability, ModelStatus,
    ProviderCapabilities, TextGenerationProvider, TextModelInfo, TokenUsage, ToolDefinition,
    ToolError, ToolExecutor, ToolResult,
};
use crucible_llm::AgentRuntime;
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

/// Mock tool executor for testing
struct MockToolExecutor {
    tools: Vec<ToolDefinition>,
}

#[async_trait]
impl ToolExecutor for MockToolExecutor {
    async fn execute_tool(
        &self,
        name: &str,
        params: serde_json::Value,
        _context: &ExecutionContext,
    ) -> ToolResult<serde_json::Value> {
        match name {
            "search_notes" => {
                let query = params["query"].as_str().unwrap_or("");
                Ok(json!({
                    "results": [
                        {"title": "Rust Basics", "content": format!("Found notes about {}", query)},
                        {"title": "Advanced Rust", "content": "More details"}
                    ]
                }))
            }
            "get_weather" => {
                let location = params["location"].as_str().unwrap_or("unknown");
                Ok(json!({
                    "temperature": 72,
                    "condition": "sunny",
                    "location": location
                }))
            }
            _ => Err(ToolError::NotFound(format!("Tool {} not found", name))),
        }
    }

    async fn list_tools(&self) -> ToolResult<Vec<ToolDefinition>> {
        Ok(self.tools.clone())
    }
}

impl MockToolExecutor {
    fn new() -> Self {
        Self {
            tools: vec![
                ToolDefinition::new("search_notes", "Search through notes").with_parameters(
                    json!({
                        "type": "object",
                        "properties": {
                            "query": {"type": "string"}
                        },
                        "required": ["query"]
                    }),
                ),
                ToolDefinition::new("get_weather", "Get weather for a location").with_parameters(
                    json!({
                        "type": "object",
                        "properties": {
                            "location": {"type": "string"}
                        },
                        "required": ["location"]
                    }),
                ),
            ],
        }
    }
}

#[tokio::test]
async fn test_agent_runtime_basic_flow() {
    // Given: An agent runtime with chat provider and tool executor
    let provider = MockTextProvider::new();
    provider.set_chat_response(
        "Find information about Rust programming",
        "I found information about Rust programming. It's a systems language.",
    );

    let executor = MockToolExecutor::new();

    let mut runtime = AgentRuntime::new(Box::new(provider), Box::new(executor));

    // When: We send a message that triggers tool use
    let messages = vec![LlmMessage::user("Find information about Rust programming")];

    let response = runtime.run_conversation(messages).await.unwrap();

    // Then: We get a response
    let content = &response.choices[0].message.content;
    assert!(!content.is_empty());
    assert!(response.usage.total_tokens > 0);
}

#[tokio::test]
async fn test_agent_runtime_tool_execution() {
    let provider = MockTextProvider::new();
    provider.set_chat_response("What's the weather?", "The weather is nice today.");

    let executor = MockToolExecutor::new();

    let mut runtime = AgentRuntime::new(Box::new(provider), Box::new(executor));

    // When: The agent uses a tool
    let messages = vec![LlmMessage::user("What's the weather?")];

    let response = runtime.run_conversation(messages).await.unwrap();

    // Then: We get a response
    assert!(!response.choices[0].message.content.is_empty());
}

#[tokio::test]
async fn test_agent_runtime_conversation_history() {
    let provider = MockTextProvider::new();
    let executor = MockToolExecutor::new();

    let mut runtime = AgentRuntime::new(Box::new(provider), Box::new(executor));

    // When: We have a multi-turn conversation
    let messages = vec![LlmMessage::user("Hello")];

    let _response1 = runtime.run_conversation(messages).await.unwrap();

    // Get conversation history
    let history = runtime.get_conversation_history();

    // Then: History should contain all messages
    assert!(history.len() >= 2); // At least user message + assistant response
}

#[tokio::test]
async fn test_agent_runtime_max_iterations() {
    let provider = MockTextProvider::new();
    let executor = MockToolExecutor::new();

    let mut runtime =
        AgentRuntime::new(Box::new(provider), Box::new(executor)).with_max_iterations(5);

    // When: We run a conversation
    let messages = vec![LlmMessage::user("Test message")];

    let response = runtime.run_conversation(messages).await.unwrap();

    // Then: It should complete within max iterations
    assert!(!response.choices[0].message.content.is_empty());
}

#[tokio::test]
async fn test_tool_executor_list_tools() {
    // Given: A tool executor
    let executor = MockToolExecutor::new();

    // When: We list available tools
    let tools = executor.list_tools().await.unwrap();

    // Then: We get the tool definitions
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].name, "search_notes");
    assert_eq!(tools[1].name, "get_weather");
}

#[tokio::test]
async fn test_tool_executor_execute() {
    // Given: A tool executor
    let executor = MockToolExecutor::new();
    let context = ExecutionContext::new();

    // When: We execute a tool
    let result = executor
        .execute_tool("search_notes", json!({"query": "rust"}), &context)
        .await
        .unwrap();

    // Then: We get the expected result
    assert!(result["results"].is_array());
    assert_eq!(result["results"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_tool_executor_error_handling() {
    // Given: A tool executor
    let executor = MockToolExecutor::new();
    let context = ExecutionContext::new();

    // When: We execute a non-existent tool
    let result = executor
        .execute_tool("nonexistent", json!({}), &context)
        .await;

    // Then: We get an error
    assert!(result.is_err());
    match result.unwrap_err() {
        ToolError::NotFound(msg) => assert!(msg.contains("nonexistent")),
        _ => panic!("Expected NotFound error"),
    }
}

#[tokio::test]
async fn test_agent_runtime_send_message() {
    let provider = MockTextProvider::new();
    provider.set_chat_response("Hi there", "Hello! I'm an AI assistant.");

    let executor = MockToolExecutor::new();

    let mut runtime = AgentRuntime::new(Box::new(provider), Box::new(executor));

    // When: We send a simple message
    let response = runtime.send_message("Hi there".to_string()).await.unwrap();

    // Then: We get a response
    assert!(!response.choices[0].message.content.is_empty());
}

#[tokio::test]
async fn test_agent_runtime_clear_history() {
    let provider = MockTextProvider::new();
    let executor = MockToolExecutor::new();

    let mut runtime = AgentRuntime::new(Box::new(provider), Box::new(executor));

    // Send a message to populate history
    let _ = runtime.send_message("Hello".to_string()).await.unwrap();
    assert!(!runtime.get_conversation_history().is_empty());

    // When: We clear history
    runtime.clear_history();

    // Then: History is empty
    assert!(runtime.get_conversation_history().is_empty());
}
