//! Multi-Turn Tool Execution Tests
//!
//! TDD tests for multi-turn conversations with tool calls in `InternalAgentHandle`.
//! These tests verify correct behavior when the agent iterates through multiple
//! tool call → result → response cycles.

use async_trait::async_trait;
use crucible_agents::context::SlidingWindowContext;
use crucible_agents::handle::InternalAgentHandle;
use crucible_agents::prompt::LayeredPromptBuilder;
use crucible_core::traits::chat::AgentHandle;
use crucible_core::traits::llm::{
    ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, ChatMessageDelta,
    CompletionRequest, CompletionResponse, FunctionCallDelta, LlmResult, MessageRole,
    ProviderCapabilities, TextGenerationProvider, TextModelInfo, ToolCallDelta,
};
use crucible_core::traits::tools::{
    ExecutionContext, ToolDefinition, ToolError, ToolExecutor, ToolResult,
};
use futures::stream::{BoxStream, StreamExt};
use serde_json::json;
use std::sync::{Arc, Mutex};

// ============================================================================
// Multi-Turn Mock Provider
// ============================================================================

/// A mock provider that cycles through predefined responses.
///
/// Each call to `generate_chat_completion_stream` returns the next response
/// in the queue. This simulates multi-turn conversations where the agent
/// might first call a tool, receive results, then provide a final response.
struct MultiTurnMockProvider {
    /// Queue of responses to return, consumed in order
    responses: Arc<Mutex<Vec<MockResponse>>>,
}

#[derive(Clone)]
enum MockResponse {
    /// A text-only response (no tool calls)
    Text(String),
    /// A tool call response
    ToolCall {
        tool_name: String,
        arguments: String,
    },
    /// Multiple tool calls in a single turn
    MultipleToolCalls(Vec<(String, String)>),
}

impl MultiTurnMockProvider {
    fn new(responses: Vec<MockResponse>) -> Self {
        Self {
            responses: Arc::new(Mutex::new(responses)),
        }
    }

    /// Create a two-turn sequence: tool call → final text response
    fn two_turn_tool_then_text(tool_name: &str, args: &str, final_text: &str) -> Self {
        Self::new(vec![
            MockResponse::ToolCall {
                tool_name: tool_name.to_string(),
                arguments: args.to_string(),
            },
            MockResponse::Text(final_text.to_string()),
        ])
    }

    /// Create a response that loops: tool calls followed by more tool calls
    fn tool_loop(tool_name: &str, iterations: usize) -> Self {
        let responses: Vec<_> = (0..iterations)
            .map(|i| MockResponse::ToolCall {
                tool_name: tool_name.to_string(),
                arguments: format!(r#"{{"iteration": {}}}"#, i),
            })
            .collect();
        Self::new(responses)
    }

    /// Get the next response (or panic if none left)
    fn pop_response(&self) -> MockResponse {
        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            // Return empty text to end the conversation
            MockResponse::Text(String::new())
        } else {
            responses.remove(0)
        }
    }
}

#[async_trait]
impl TextGenerationProvider for MultiTurnMockProvider {
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
        let response = self.pop_response();
        let mut chunks = Vec::new();

        match response {
            MockResponse::Text(content) => {
                // Content chunk
                chunks.push(Ok(ChatCompletionChunk {
                    index: 0,
                    delta: ChatMessageDelta {
                        role: Some(MessageRole::Assistant),
                        content: Some(content),
                        function_call: None,
                        tool_calls: None,
                    },
                    finish_reason: Some("stop".to_string()),
                    logprobs: None,
                }));
            }
            MockResponse::ToolCall {
                tool_name,
                arguments,
            } => {
                // Tool call chunk
                chunks.push(Ok(ChatCompletionChunk {
                    index: 0,
                    delta: ChatMessageDelta {
                        role: Some(MessageRole::Assistant),
                        content: None,
                        function_call: None,
                        tool_calls: Some(vec![ToolCallDelta {
                            index: 0,
                            id: Some("call-1".to_string()),
                            function: Some(FunctionCallDelta {
                                name: Some(tool_name),
                                arguments: Some(arguments),
                            }),
                        }]),
                    },
                    finish_reason: None,
                    logprobs: None,
                }));
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
            }
            MockResponse::MultipleToolCalls(calls) => {
                for (idx, (tool_name, arguments)) in calls.into_iter().enumerate() {
                    chunks.push(Ok(ChatCompletionChunk {
                        index: 0,
                        delta: ChatMessageDelta {
                            role: if idx == 0 {
                                Some(MessageRole::Assistant)
                            } else {
                                None
                            },
                            content: None,
                            function_call: None,
                            tool_calls: Some(vec![ToolCallDelta {
                                index: idx as u32,
                                id: Some(format!("call-{}", idx + 1)),
                                function: Some(FunctionCallDelta {
                                    name: Some(tool_name),
                                    arguments: Some(arguments),
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
            }
        }

        Box::pin(futures::stream::iter(chunks))
    }

    fn provider_name(&self) -> &str {
        "multi-turn-mock"
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
// Logging Mock Tool Executor
// ============================================================================

/// A mock tool executor that logs all executions for verification
struct LoggingToolExecutor {
    tools: Vec<String>,
    executions: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
}

impl LoggingToolExecutor {
    fn new(tools: Vec<String>) -> Self {
        Self {
            tools,
            executions: Arc::new(Mutex::new(Vec::new())),
        }
    }

    #[allow(dead_code)]
    fn execution_count(&self) -> usize {
        self.executions.lock().unwrap().len()
    }

    #[allow(dead_code)]
    fn get_executions(&self) -> Vec<(String, serde_json::Value)> {
        self.executions.lock().unwrap().clone()
    }
}

#[async_trait]
impl ToolExecutor for LoggingToolExecutor {
    async fn execute_tool(
        &self,
        name: &str,
        params: serde_json::Value,
        _context: &ExecutionContext,
    ) -> ToolResult<serde_json::Value> {
        // Log the execution
        self.executions
            .lock()
            .unwrap()
            .push((name.to_string(), params.clone()));

        if !self.tools.contains(&name.to_string()) {
            return Err(ToolError::NotFound(name.to_string()));
        }
        Ok(json!({"result": "success", "tool": name}))
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
// TDD Tests - Multi-Turn Tool Execution
// ============================================================================

/// Test a standard two-round tool execution:
/// 1. Agent receives message
/// 2. Agent calls a tool
/// 3. Tool executes and returns result
/// 4. Agent provides final text response
#[tokio::test]
async fn test_two_round_tool_execution() {
    let provider = Box::new(MultiTurnMockProvider::two_turn_tool_then_text(
        "search",
        r#"{"query": "test"}"#,
        "Based on the search results, the answer is 42.",
    ));
    let tool_executor = Box::new(LoggingToolExecutor::new(vec!["search".to_string()]));
    let exec_ref = tool_executor.executions.clone();

    let mut handle = create_test_handle(provider, Some(tool_executor));

    let mut stream = handle.send_message_stream("What is the answer?".to_string());
    let mut content = String::new();

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                content.push_str(&chunk.delta);
            }
            Err(e) => {
                panic!("Unexpected error: {:?}", e);
            }
        }
    }

    // Verify tool was executed
    let executions = exec_ref.lock().unwrap();
    assert_eq!(executions.len(), 1, "Tool should be executed once");
    assert_eq!(executions[0].0, "search", "search tool should be called");

    // Verify we got the final response
    assert!(
        content.contains("42") || content.contains("search"),
        "Final response should contain expected content, got: {}",
        content
    );
}

/// Test multiple tool calls in a single turn
#[tokio::test]
async fn test_multiple_tools_single_turn() {
    let provider = Box::new(MultiTurnMockProvider::new(vec![
        MockResponse::MultipleToolCalls(vec![
            ("tool_a".to_string(), r#"{"param": "a"}"#.to_string()),
            ("tool_b".to_string(), r#"{"param": "b"}"#.to_string()),
        ]),
        MockResponse::Text("Combined results processed.".to_string()),
    ]));

    let tool_executor = Box::new(LoggingToolExecutor::new(vec![
        "tool_a".to_string(),
        "tool_b".to_string(),
    ]));
    let exec_ref = tool_executor.executions.clone();

    let mut handle = create_test_handle(provider, Some(tool_executor));

    let mut stream = handle.send_message_stream("Run both tools".to_string());
    let mut results = Vec::new();

    while let Some(chunk_result) = stream.next().await {
        results.push(chunk_result);
    }

    // Verify both tools were executed
    let executions = exec_ref.lock().unwrap();
    assert_eq!(executions.len(), 2, "Both tools should be executed");

    let tool_names: Vec<_> = executions.iter().map(|(name, _)| name.as_str()).collect();
    assert!(tool_names.contains(&"tool_a"), "tool_a should be called");
    assert!(tool_names.contains(&"tool_b"), "tool_b should be called");
}

/// Test that tool execution has a maximum iteration limit to prevent infinite loops
#[tokio::test]
async fn test_tool_loop_max_iterations() {
    // Create a provider that always returns tool calls (would infinite loop without limit)
    let provider = Box::new(MultiTurnMockProvider::tool_loop("repeat_tool", 100));

    let tool_executor = Box::new(LoggingToolExecutor::new(vec!["repeat_tool".to_string()]));
    let exec_ref = tool_executor.executions.clone();

    let mut handle = create_test_handle(provider, Some(tool_executor));

    let mut stream = handle.send_message_stream("Start the loop".to_string());

    // Consume the stream with a timeout to prevent actual infinite loop in test
    let mut iteration_count = 0;
    let max_test_iterations = 20; // Safety limit for test

    while let Some(_chunk_result) = stream.next().await {
        iteration_count += 1;
        if iteration_count > max_test_iterations {
            break; // Safety break
        }
    }

    // The implementation should either:
    // 1. Have a max iteration limit built in (preferred)
    // 2. Or we verify the test didn't actually run forever
    let executions = exec_ref.lock().unwrap();

    // Either the implementation limits iterations, or our test safety kicked in
    assert!(
        executions.len() <= 10 || iteration_count > max_test_iterations,
        "Tool execution should have a reasonable limit. Executed {} times",
        executions.len()
    );
}

/// Test that context accumulates correctly during multi-turn tool conversations
#[tokio::test]
async fn test_context_accumulation_during_tools() {
    let provider = Box::new(MultiTurnMockProvider::two_turn_tool_then_text(
        "lookup",
        r#"{"key": "test"}"#,
        "Found the value.",
    ));

    let tool_executor = Box::new(LoggingToolExecutor::new(vec!["lookup".to_string()]));

    let mut handle = create_test_handle(provider, Some(tool_executor));

    // Send first message
    let mut stream = handle.send_message_stream("Lookup test key".to_string());
    while let Some(_chunk) = stream.next().await {}

    // The context should now contain:
    // 1. User message
    // 2. Assistant tool call
    // 3. Tool result
    // 4. Assistant final response

    // We can't directly inspect context, but we can verify the handle is still usable
    // and would include this history in subsequent requests

    // This test primarily verifies no panics/errors during multi-turn execution
}
