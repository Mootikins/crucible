//! Tool Execution Error Handling Tests
//!
//! TDD tests for the `execute_tool_calls` function in `InternalAgentHandle`.
//! These tests verify proper error handling for various failure scenarios.

use async_trait::async_trait;
use crucible_agents::context::SlidingWindowContext;
use crucible_agents::handle::InternalAgentHandle;
use crucible_agents::prompt::LayeredPromptBuilder;
use crucible_core::traits::chat::AgentHandle;
use crucible_core::traits::llm::{
    ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, ChatMessageDelta,
    CompletionRequest, CompletionResponse, FunctionCall, FunctionCallDelta, LlmResult, MessageRole,
    ProviderCapabilities, TextGenerationProvider, TextModelInfo, ToolCall as LlmToolCall,
    ToolCallDelta,
};
use crucible_core::traits::tools::{
    ExecutionContext, ToolDefinition, ToolError, ToolExecutor, ToolResult,
};
use futures::stream::{BoxStream, StreamExt};
use serde_json::json;

// ============================================================================
// Mock Provider
// ============================================================================

struct MockProvider {
    tool_calls: Vec<LlmToolCall>,
    content: String,
}

impl MockProvider {
    fn with_invalid_json_tool_call() -> Self {
        Self {
            tool_calls: vec![LlmToolCall {
                id: "call-1".to_string(),
                r#type: "function".to_string(),
                function: FunctionCall {
                    name: "test_tool".to_string(),
                    arguments: r#"{"key": "value""#.to_string(), // Invalid JSON
                },
            }],
            content: "Calling tool...".to_string(),
        }
    }

    fn with_valid_tool_call(tool_name: &str) -> Self {
        Self {
            tool_calls: vec![LlmToolCall {
                id: "call-1".to_string(),
                r#type: "function".to_string(),
                function: FunctionCall {
                    name: tool_name.to_string(),
                    arguments: r#"{"param": "value"}"#.to_string(),
                },
            }],
            content: "Calling tool...".to_string(),
        }
    }

    fn with_nonexistent_tool_call() -> Self {
        Self::with_valid_tool_call("nonexistent_tool")
    }
}

#[async_trait]
impl TextGenerationProvider for MockProvider {
    async fn generate_completion(
        &self,
        _request: CompletionRequest,
    ) -> LlmResult<CompletionResponse> {
        unimplemented!()
    }

    fn generate_completion_stream<'a>(
        &'a self,
        _request: CompletionRequest,
    ) -> BoxStream<'a, LlmResult<crucible_core::traits::llm::CompletionChunk>> {
        unimplemented!()
    }

    async fn generate_chat_completion(
        &self,
        _request: ChatCompletionRequest,
    ) -> LlmResult<ChatCompletionResponse> {
        unimplemented!()
    }

    fn generate_chat_completion_stream<'a>(
        &'a self,
        _request: ChatCompletionRequest,
    ) -> BoxStream<'a, LlmResult<ChatCompletionChunk>> {
        let mut chunks = Vec::new();

        // Content chunk
        chunks.push(Ok(ChatCompletionChunk {
            index: 0,
            delta: ChatMessageDelta {
                role: Some(MessageRole::Assistant),
                content: Some(self.content.clone()),
                function_call: None,
                tool_calls: None,
            },
            finish_reason: None,
            logprobs: None,
        }));

        // Tool call chunks
        for (idx, tool_call) in self.tool_calls.iter().enumerate() {
            chunks.push(Ok(ChatCompletionChunk {
                index: 0,
                delta: ChatMessageDelta {
                    role: None,
                    content: None,
                    function_call: None,
                    tool_calls: Some(vec![ToolCallDelta {
                        index: idx as u32,
                        id: Some(tool_call.id.clone()),
                        function: Some(FunctionCallDelta {
                            name: Some(tool_call.function.name.clone()),
                            arguments: Some(tool_call.function.arguments.clone()),
                        }),
                    }]),
                },
                finish_reason: None,
                logprobs: None,
            }));
        }

        // Final chunk
        chunks.push(Ok(ChatCompletionChunk {
            index: 0,
            delta: ChatMessageDelta {
                role: None,
                content: None,
                function_call: None,
                tool_calls: None,
            },
            finish_reason: Some("tool_calls".to_string()),
            logprobs: None,
        }));

        Box::pin(futures::stream::iter(chunks))
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    fn default_model(&self) -> &str {
        "mock-model"
    }

    async fn list_models(&self) -> LlmResult<Vec<TextModelInfo>> {
        Ok(vec![])
    }

    async fn health_check(&self) -> LlmResult<bool> {
        Ok(true)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            text_completion: false,
            chat_completion: true,
            streaming: true,
            function_calling: true,
            tool_use: true,
            vision: false,
            audio: false,
            max_batch_size: None,
            input_formats: vec![],
            output_formats: vec![],
        }
    }
}

// ============================================================================
// Mock Tool Executor
// ============================================================================

struct MockToolExecutor {
    tools: Vec<String>,
    fail_on_execute: bool,
}

impl MockToolExecutor {
    fn with_tools(tools: Vec<String>) -> Self {
        Self {
            tools,
            fail_on_execute: false,
        }
    }

    fn failing() -> Self {
        Self {
            tools: vec!["failing_tool".to_string()],
            fail_on_execute: true,
        }
    }
}

#[async_trait]
impl ToolExecutor for MockToolExecutor {
    async fn execute_tool(
        &self,
        name: &str,
        _params: serde_json::Value,
        _context: &ExecutionContext,
    ) -> ToolResult<serde_json::Value> {
        if self.fail_on_execute {
            return Err(ToolError::ExecutionFailed(
                "Mock tool execution failure".to_string(),
            ));
        }
        if !self.tools.contains(&name.to_string()) {
            return Err(ToolError::NotFound(name.to_string()));
        }
        Ok(json!({"result": "success"}))
    }

    async fn list_tools(&self) -> ToolResult<Vec<ToolDefinition>> {
        Ok(self
            .tools
            .iter()
            .map(|name| ToolDefinition::new(name.clone(), format!("Mock tool: {}", name)))
            .collect())
    }
}

// ============================================================================
// Test Helper
// ============================================================================

fn create_test_handle(
    provider: Box<dyn TextGenerationProvider>,
    tool_executor: Option<Box<dyn ToolExecutor>>,
) -> InternalAgentHandle {
    let context = Box::new(SlidingWindowContext::new(1000));
    let prompt_builder = LayeredPromptBuilder::new();

    InternalAgentHandle::new(
        provider,
        context,
        tool_executor,
        prompt_builder,
        "test-model".to_string(),
        1000,
    )
}

// ============================================================================
// TDD Tests - Expected to FAIL initially (RED phase)
// ============================================================================

#[tokio::test]
async fn test_tool_json_parse_error() {
    let provider = Box::new(MockProvider::with_invalid_json_tool_call());
    let tool_executor = Box::new(MockToolExecutor::with_tools(vec!["test_tool".to_string()]));
    let mut handle = create_test_handle(provider, Some(tool_executor));

    let mut stream = handle.send_message_stream("test message".to_string());
    let mut results = Vec::new();

    while let Some(chunk_result) = stream.next().await {
        results.push(chunk_result);
    }

    let error_found = results.iter().any(|r| r.is_err());
    assert!(
        error_found,
        "Expected error for invalid JSON, got: {:?}",
        results
    );
}

#[tokio::test]
async fn test_tool_missing_executor() {
    let provider = Box::new(MockProvider::with_valid_tool_call("any_tool"));
    let mut handle = create_test_handle(provider, None);

    let mut stream = handle.send_message_stream("test message".to_string());
    let mut results = Vec::new();

    while let Some(chunk_result) = stream.next().await {
        results.push(chunk_result);
    }

    let error_found = results.iter().any(|r| r.is_err());
    assert!(
        error_found,
        "Expected error for missing executor, got: {:?}",
        results
    );
}

#[tokio::test]
async fn test_tool_not_found_error() {
    let provider = Box::new(MockProvider::with_nonexistent_tool_call());
    let tool_executor = Box::new(MockToolExecutor::with_tools(vec!["other_tool".to_string()]));
    let mut handle = create_test_handle(provider, Some(tool_executor));

    let mut stream = handle.send_message_stream("test message".to_string());
    let mut results = Vec::new();

    while let Some(chunk_result) = stream.next().await {
        results.push(chunk_result);
    }

    let error_found = results.iter().any(|r| r.is_err());
    assert!(
        error_found,
        "Expected error for tool not found, got: {:?}",
        results
    );
}

#[tokio::test]
async fn test_tool_failure_adds_error_to_context() {
    let provider = Box::new(MockProvider::with_valid_tool_call("failing_tool"));
    let tool_executor = Box::new(MockToolExecutor::failing());
    let mut handle = create_test_handle(provider, Some(tool_executor));

    let mut stream = handle.send_message_stream("test message".to_string());
    let mut results = Vec::new();

    while let Some(chunk_result) = stream.next().await {
        results.push(chunk_result);
    }

    let error_found = results.iter().any(|r| r.is_err());
    assert!(
        error_found,
        "Expected error for tool failure, got: {:?}",
        results
    );
}
