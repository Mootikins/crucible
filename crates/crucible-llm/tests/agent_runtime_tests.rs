//! Integration tests for Agent Runtime
//!
//! The agent runtime coordinates between LlmProvider and ToolExecutor
//! to enable autonomous agent behavior with tool calling.

use async_trait::async_trait;
use crucible_core::traits::{
    LlmMessage, LlmProvider, LlmRequest, LlmResponse, ExecutionContext, LlmError, LlmResult,
    ToolCall, ToolDefinition, ToolError, ToolExecutor, ToolResult, TokenUsage,
};
use serde_json::json;

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
                ToolDefinition::new("search_notes", "Search through notes")
                    .with_parameters(json!({
                        "type": "object",
                        "properties": {
                            "query": {"type": "string"}
                        },
                        "required": ["query"]
                    })),
                ToolDefinition::new("get_weather", "Get weather for a location")
                    .with_parameters(json!({
                        "type": "object",
                        "properties": {
                            "location": {"type": "string"}
                        },
                        "required": ["location"]
                    })),
            ],
        }
    }
}

/// Mock chat provider that responds with tool calls
struct MockChatProviderWithTools {
    call_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl MockChatProviderWithTools {
    fn new() -> Self {
        Self {
            call_count: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl LlmProvider for MockChatProviderWithTools {
    async fn chat(&self, request: LlmRequest) -> LlmResult<LlmResponse> {
        let count = self
            .call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // First call: make a tool call
        if count == 0 {
            let message = LlmMessage::assistant_with_tools(
                "I'll search for that information.".to_string(),
                vec![ToolCall::new(
                    "call_1",
                    "search_notes",
                    json!({"query": "rust programming"}),
                )],
            );

            return Ok(LlmResponse {
                message,
                usage: TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                },
                model: "mock".to_string(),
            });
        }

        // Second call: respond with the tool results
        let last_message = request.messages.last().unwrap();
        if last_message.role == crucible_core::traits::MessageRole::Tool {
            let message = LlmMessage::assistant(
                "Based on the search results, I found information about Rust programming.".to_string(),
            );

            return Ok(LlmResponse {
                message,
                usage: TokenUsage {
                    prompt_tokens: 50,
                    completion_tokens: 30,
                    total_tokens: 80,
                },
                model: "mock".to_string(),
            });
        }

        // Default response
        Ok(LlmResponse {
            message: LlmMessage::assistant("I don't understand.".to_string()),
            usage: TokenUsage {
                prompt_tokens: 5,
                completion_tokens: 5,
                total_tokens: 10,
            },
            model: "mock".to_string(),
        })
    }

    fn provider_name(&self) -> &str {
        "MockWithTools"
    }

    fn default_model(&self) -> &str {
        "mock-model"
    }

    async fn health_check(&self) -> LlmResult<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn test_agent_runtime_basic_flow() {
    // Given: An agent runtime with chat provider and tool executor
    use crucible_llm::AgentRuntime;

    let provider = Box::new(MockChatProviderWithTools::new());
    let executor = Box::new(MockToolExecutor::new());

    let mut runtime = AgentRuntime::new(provider, executor);

    // When: We send a message that triggers tool use
    let messages = vec![LlmMessage::user("Find information about Rust programming")];

    let response = runtime.run_conversation(messages).await.unwrap();

    // Then: The agent should have:
    // 1. Called the LLM
    // 2. Executed the tool
    // 3. Called the LLM again with results
    // 4. Returned final response

    assert!(response
        .message
        .content
        .contains("Based on the search results"));
    assert!(response.usage.total_tokens > 0);
}

#[tokio::test]
async fn test_agent_runtime_tool_execution() {
    use crucible_llm::AgentRuntime;

    let provider = Box::new(MockChatProviderWithTools::new());
    let executor = Box::new(MockToolExecutor::new());

    let mut runtime = AgentRuntime::new(provider, executor);

    // When: The agent uses a tool
    let messages = vec![LlmMessage::user("What's the weather?")];

    let response = runtime.run_conversation(messages).await.unwrap();

    // Then: We get a response that used the tool
    assert!(!response.message.content.is_empty());
}

#[tokio::test]
async fn test_agent_runtime_conversation_history() {
    use crucible_llm::AgentRuntime;

    let provider = Box::new(MockChatProviderWithTools::new());
    let executor = Box::new(MockToolExecutor::new());

    let mut runtime = AgentRuntime::new(provider, executor);

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
    use crucible_llm::AgentRuntime;

    let provider = Box::new(MockChatProviderWithTools::new());
    let executor = Box::new(MockToolExecutor::new());

    let mut runtime = AgentRuntime::new(provider, executor).with_max_iterations(5);

    // When: We run a conversation
    let messages = vec![LlmMessage::user("Test message")];

    let response = runtime.run_conversation(messages).await.unwrap();

    // Then: It should complete within max iterations
    assert!(!response.message.content.is_empty());
}

#[tokio::test]
async fn test_tool_executor_list_tools() {
    // Given: A tool executor
    let executor = MockToolExecutor::new();
    let context = ExecutionContext::new();

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
        .execute_tool(
            "search_notes",
            json!({"query": "rust"}),
            &context,
        )
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
