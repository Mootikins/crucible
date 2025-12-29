//! Internal Agent Handle
//!
//! Implements `AgentHandle` using direct LLM API calls via `TextGenerationProvider`.

use async_trait::async_trait;
use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatError, ChatResult};
use crucible_core::traits::context::ContextManager;
use crucible_core::traits::llm::{
    ChatCompletionRequest, LlmMessage, TextGenerationProvider, ToolCall as LlmToolCall,
};
use crucible_core::traits::tools::{ExecutionContext, ToolExecutor};
use crucible_core::types::acp::schema::SessionModeState;
use crucible_core::types::mode::default_internal_modes;
use futures::stream::{BoxStream, StreamExt};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::prompt::LayeredPromptBuilder;
use crate::token::TokenBudget;

/// Default maximum number of tool execution iterations
const DEFAULT_MAX_TOOL_ITERATIONS: usize = 10;

/// Internal agent handle that uses direct LLM API calls
///
/// This handle wraps a `TextGenerationProvider` and provides conversation
/// management with sliding window context and layered prompts.
pub struct InternalAgentHandle {
    /// The LLM provider for generating completions (wrapped in Arc for 'static streams)
    provider: Arc<Box<dyn TextGenerationProvider>>,

    /// Context manager for conversation history (wrapped in Arc<Mutex<>> for shared mutation)
    context: Arc<Mutex<Box<dyn ContextManager>>>,

    /// Optional tool executor (wrapped in Arc for 'static streams)
    tools: Option<Arc<Box<dyn ToolExecutor>>>,

    /// Prompt builder for layered system prompts (reserved for future use)
    #[allow(dead_code)]
    prompt_builder: LayeredPromptBuilder,

    /// Token budget tracker (wrapped in Arc<Mutex<>> for shared mutation)
    token_budget: Arc<Mutex<TokenBudget>>,

    /// Mode state advertised by this agent (Plan/Act/Auto)
    mode_state: SessionModeState,

    /// Current mode ID
    current_mode_id: String,

    /// Model identifier
    model: String,

    /// Unique agent ID (reserved for future use)
    #[allow(dead_code)]
    agent_id: String,

    /// Maximum number of tool execution iterations to prevent infinite loops
    max_tool_iterations: usize,
}

impl InternalAgentHandle {
    /// Create a new internal agent handle
    ///
    /// # Arguments
    ///
    /// * `provider` - LLM provider for text generation
    /// * `context` - Context manager for conversation history
    /// * `tools` - Optional tool executor
    /// * `prompt_builder` - Builder for layered prompts
    /// * `model` - Model identifier
    /// * `max_context_tokens` - Maximum context window size
    pub fn new(
        provider: Box<dyn TextGenerationProvider>,
        context: Box<dyn ContextManager>,
        tools: Option<Box<dyn ToolExecutor>>,
        prompt_builder: LayeredPromptBuilder,
        model: String,
        max_context_tokens: usize,
    ) -> Self {
        let agent_id = Uuid::new_v4().to_string();
        let token_budget = TokenBudget::new(max_context_tokens);

        // Set system prompt from prompt builder
        let mut new_context = context;
        let system_prompt = prompt_builder.build();
        new_context.set_system_prompt(system_prompt);

        let mode_state = default_internal_modes();
        let current_mode_id = mode_state.current_mode_id.0.to_string();

        Self {
            provider: Arc::new(provider),
            context: Arc::new(Mutex::new(new_context)),
            tools: tools.map(Arc::new),
            prompt_builder,
            token_budget: Arc::new(Mutex::new(token_budget)),
            mode_state,
            current_mode_id,
            model,
            agent_id,
            max_tool_iterations: DEFAULT_MAX_TOOL_ITERATIONS,
        }
    }

    /// Set the maximum number of tool execution iterations
    ///
    /// This prevents infinite loops when the LLM keeps requesting tool calls.
    /// Default is 10 iterations.
    pub fn with_max_tool_iterations(mut self, max: usize) -> Self {
        self.max_tool_iterations = max;
        self
    }

    /// Execute tool calls and add results to context
    async fn execute_tool_calls(
        context: &Arc<Mutex<Box<dyn ContextManager>>>,
        tools: &Arc<Box<dyn ToolExecutor>>,
        tool_calls: &[LlmToolCall],
    ) -> ChatResult<()> {
        let execution_context = ExecutionContext::new();

        for tool_call in tool_calls {
            // Parse arguments
            let arguments: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)
                .map_err(|e| {
                    ChatError::Internal(format!("Failed to parse tool arguments: {}", e))
                })?;

            // Execute tool
            let result = tools
                .execute_tool(&tool_call.function.name, arguments, &execution_context)
                .await
                .map_err(|e| ChatError::Internal(format!("Tool execution failed: {}", e)))?;

            // Add tool result to context
            let result_str = serde_json::to_string(&result)
                .unwrap_or_else(|_| "Error serializing tool result".to_string());
            context
                .lock()
                .unwrap()
                .add_message(LlmMessage::tool(tool_call.id.clone(), result_str));
        }

        Ok(())
    }
}

#[async_trait]
impl AgentHandle for InternalAgentHandle {
    fn send_message_stream(
        &mut self,
        message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        // Clone Arc references for the 'static stream
        let context = Arc::clone(&self.context);
        let provider = Arc::clone(&self.provider);
        let tools = self.tools.as_ref().map(Arc::clone);
        let model = self.model.clone();
        let max_tool_iterations = self.max_tool_iterations;
        let token_budget = Arc::clone(&self.token_budget);

        Box::pin(async_stream::stream! {
            // Debug to file since TUI eats stderr
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/crucible-debug.log")
            {
                use std::io::Write;
                let _ = writeln!(f, "[DEBUG] send_message_stream called: {}", &message);
            }

            // Add user message to context
            context.lock().unwrap().add_message(LlmMessage::user(&message));

            // Get tool definitions from executor (if available)
            let tool_definitions: Option<Vec<crucible_core::traits::llm::LlmToolDefinition>> =
                if let Some(ref tool_executor) = tools {
                    match tool_executor.list_tools().await {
                        Ok(defs) => {
                            let llm_defs: Vec<_> = defs.into_iter().map(Into::into).collect();
                            tracing::debug!(tool_count = llm_defs.len(), "Loaded tool definitions");
                            if llm_defs.is_empty() {
                                None
                            } else {
                                Some(llm_defs)
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to list tools: {}", e);
                            None
                        }
                    }
                } else {
                    None
                };

            // Tool execution loop - continue until no more tool calls
            let mut tool_iteration = 0;
            loop {
                // Check for max iterations to prevent infinite loops
                if tool_iteration >= max_tool_iterations {
                    yield Err(ChatError::Internal(format!(
                        "Maximum tool iterations ({}) exceeded - possible infinite loop",
                        max_tool_iterations
                    )));
                    return;
                }
                tool_iteration += 1;
                // Trim context to budget
                let remaining = token_budget.lock().unwrap().remaining();
                context.lock().unwrap().trim_to_budget(remaining);

                // Build request from context
                let messages = context.lock().unwrap().get_messages();

                // Debug: log message sequence being sent
                let msg_roles: Vec<_> = messages.iter().map(|m| {
                    let role = format!("{:?}", m.role);
                    if m.tool_calls.is_some() {
                        format!("{}(tool_calls)", role)
                    } else if m.tool_call_id.is_some() {
                        format!("{}(tool_result)", role)
                    } else {
                        role
                    }
                }).collect();

                // Log to file
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/crucible-debug.log")
                {
                    use std::io::Write;
                    let _ = writeln!(f, "[DEBUG] iteration {}: {:?}", tool_iteration, msg_roles);
                }

                // Check for consecutive assistant messages at end
                let last_two: Vec<_> = messages.iter().rev().take(2).collect();
                if last_two.len() >= 2 {
                    use crucible_core::traits::llm::MessageRole;
                    if matches!(last_two[0].role, MessageRole::Assistant)
                        && matches!(last_two[1].role, MessageRole::Assistant) {
                        if let Ok(mut f) = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open("/tmp/crucible-debug.log")
                        {
                            use std::io::Write;
                            let _ = writeln!(f, "BUG! Consecutive assistant messages: {:?}", msg_roles);
                        }
                    }
                }

                tracing::debug!(
                    iteration = tool_iteration,
                    messages = ?msg_roles,
                    "Sending request to LLM"
                );

                let mut request = ChatCompletionRequest::new(model.clone(), messages);
                request.tools = tool_definitions.clone();

                // Stream completion from provider
                // We need to collect chunks into a vec to avoid borrowing issues
                let stream = provider.generate_chat_completion_stream(request);
                let chunks: Vec<_> = stream.collect().await;

                let mut content = String::new();
                let mut accumulated_tool_calls: Vec<LlmToolCall> = Vec::new();
                let mut finish_reason: Option<String> = None;

                // Process collected chunks
                for chunk_result in chunks {
                    match chunk_result {
                        Ok(chunk) => {
                            // Accumulate content
                            if let Some(delta_content) = &chunk.delta.content {
                                content.push_str(delta_content);

                                // Yield content chunk
                                yield Ok(ChatChunk {
                                    delta: delta_content.clone(),
                                    done: false,
                                    tool_calls: None,
                                });
                            }

                            // Accumulate tool calls
                            if let Some(ref tool_call_deltas) = chunk.delta.tool_calls {
                                for delta in tool_call_deltas {
                                    let index = delta.index as usize;

                                    // Ensure we have enough slots
                                    while accumulated_tool_calls.len() <= index {
                                        accumulated_tool_calls.push(LlmToolCall {
                                            id: String::new(),
                                            r#type: "function".to_string(),
                                            function: crucible_core::traits::llm::FunctionCall {
                                                name: String::new(),
                                                arguments: String::new(),
                                            },
                                        });
                                    }

                                    // Accumulate deltas
                                    if let Some(ref id) = delta.id {
                                        accumulated_tool_calls[index].id.push_str(id);
                                    }
                                    if let Some(ref func) = delta.function {
                                        if let Some(ref name) = func.name {
                                            accumulated_tool_calls[index].function.name.push_str(name);
                                        }
                                        if let Some(ref args) = func.arguments {
                                            accumulated_tool_calls[index].function.arguments.push_str(args);
                                        }
                                    }
                                }
                            }

                            // Capture finish reason
                            if chunk.finish_reason.is_some() {
                                finish_reason = chunk.finish_reason;
                            }
                        }
                        Err(e) => {
                            yield Err(ChatError::Communication(format!("LLM error: {}", e)));
                            return;
                        }
                    }
                }

                // Debug: log response details
                tracing::debug!(
                    iteration = tool_iteration,
                    finish_reason = ?finish_reason,
                    tool_call_count = accumulated_tool_calls.len(),
                    content_len = content.len(),
                    "Received LLM response"
                );

                // Add assistant response to context
                if !accumulated_tool_calls.is_empty() {
                    context.lock().unwrap().add_message(LlmMessage::assistant_with_tools(
                        content.clone(),
                        accumulated_tool_calls.clone(),
                    ));
                } else {
                    context.lock().unwrap().add_message(LlmMessage::assistant(content.clone()));
                }

                // Check if we need to execute tools
                // Note: Some APIs use "tool_calls", others use "stop" or other values
                // We execute tools if we have tool calls, regardless of finish_reason
                if !accumulated_tool_calls.is_empty() {
                    // Execute tools (only if tool executor available)
                    if let Some(ref tool_executor) = tools {
                        if let Err(e) = Self::execute_tool_calls(&context, tool_executor, &accumulated_tool_calls).await {
                            yield Err(e);
                            return;
                        }
                    } else {
                        yield Err(ChatError::Internal(
                            "Tool calls requested but no tool executor available".to_string()
                        ));
                        return;
                    }
                    // Continue loop to get next response
                } else {
                    // No tool calls, we're done
                    yield Ok(ChatChunk {
                        delta: String::new(),
                        done: true,
                        tool_calls: None,
                    });
                    break;
                }
            }
        })
    }

    fn get_modes(&self) -> Option<&SessionModeState> {
        Some(&self.mode_state)
    }

    fn get_mode_id(&self) -> &str {
        &self.current_mode_id
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        // Validate mode exists in our advertised modes
        let exists = self
            .mode_state
            .available_modes
            .iter()
            .any(|m| m.id.0.as_ref() == mode_id);

        if !exists {
            return Err(ChatError::InvalidMode(format!(
                "Unknown mode '{}'. Available: {:?}",
                mode_id,
                self.mode_state
                    .available_modes
                    .iter()
                    .map(|m| m.id.0.as_ref())
                    .collect::<Vec<_>>()
            )));
        }

        self.current_mode_id = mode_id.to_string();
        Ok(())
    }

    fn is_connected(&self) -> bool {
        // Internal agents are always connected
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::SlidingWindowContext;
    use crucible_core::traits::llm::{
        ChatCompletionChunk, ChatCompletionResponse, ChatMessageDelta, CompletionRequest,
        CompletionResponse, LlmResult, MessageRole, ProviderCapabilities, TextModelInfo,
    };
    use futures::stream;

    // Mock provider for testing
    struct MockProvider {
        responses: Vec<Vec<ChatCompletionChunk>>,
        response_index: std::sync::Arc<std::sync::Mutex<usize>>,
    }

    impl MockProvider {
        fn new(responses: Vec<Vec<ChatCompletionChunk>>) -> Self {
            Self {
                responses,
                response_index: std::sync::Arc::new(std::sync::Mutex::new(0)),
            }
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
            let mut index = self.response_index.lock().unwrap();
            let current = *index;
            *index = (*index + 1) % self.responses.len();
            drop(index);

            let chunks = self.responses.get(current).cloned().unwrap_or_default();
            Box::pin(stream::iter(chunks.into_iter().map(Ok)))
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
                function_calling: false,
                tool_use: false,
                vision: false,
                audio: false,
                max_batch_size: None,
                input_formats: vec![],
                output_formats: vec![],
            }
        }
    }

    #[tokio::test]
    async fn test_handle_creation() {
        let provider = Box::new(MockProvider::new(vec![]));
        let context = Box::new(SlidingWindowContext::new(1000));
        let prompt_builder = LayeredPromptBuilder::new();

        let handle = InternalAgentHandle::new(
            provider,
            context,
            None,
            prompt_builder,
            "test-model".to_string(),
            1000,
        );

        assert!(handle.is_connected());
        assert_eq!(handle.get_mode_id(), "plan");
        assert!(handle.get_modes().is_some());
        assert_eq!(handle.get_modes().unwrap().available_modes.len(), 3);
    }

    #[tokio::test]
    async fn test_send_message_stream_basic() {
        let chunks = vec![
            ChatCompletionChunk {
                index: 0,
                delta: ChatMessageDelta {
                    role: Some(MessageRole::Assistant),
                    content: Some("Hello, ".to_string()),
                    function_call: None,
                    tool_calls: None,
                },
                finish_reason: None,
                logprobs: None,
            },
            ChatCompletionChunk {
                index: 0,
                delta: ChatMessageDelta {
                    role: None,
                    content: Some("world!".to_string()),
                    function_call: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            },
        ];

        let provider = Box::new(MockProvider::new(vec![chunks]));
        let context = Box::new(SlidingWindowContext::new(1000));
        let prompt_builder = LayeredPromptBuilder::new();

        let mut handle = InternalAgentHandle::new(
            provider,
            context,
            None,
            prompt_builder,
            "test-model".to_string(),
            1000,
        );

        let mut stream = handle.send_message_stream("Hi".to_string());
        let mut collected = Vec::new();

        while let Some(result) = stream.next().await {
            collected.push(result.unwrap());
        }

        // Should have content chunks + done chunk
        assert!(collected.len() >= 2);

        // Last chunk should be done
        assert!(collected.last().unwrap().done);

        // Content should be accumulated
        let content: String = collected
            .iter()
            .filter(|c| !c.done)
            .map(|c| c.delta.as_str())
            .collect();
        assert_eq!(content, "Hello, world!");
    }

    #[tokio::test]
    async fn test_set_mode() {
        let provider = Box::new(MockProvider::new(vec![]));
        let context = Box::new(SlidingWindowContext::new(1000));
        let prompt_builder = LayeredPromptBuilder::new();

        let mut handle = InternalAgentHandle::new(
            provider,
            context,
            None,
            prompt_builder,
            "test-model".to_string(),
            1000,
        );

        assert_eq!(handle.get_mode_id(), "plan");

        handle.set_mode_str("act").await.unwrap();
        assert_eq!(handle.get_mode_id(), "act");

        handle.set_mode_str("auto").await.unwrap();
        assert_eq!(handle.get_mode_id(), "auto");

        // Test invalid mode returns error
        let result = handle.set_mode_str("invalid").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_context_trimming() {
        let chunks = vec![ChatCompletionChunk {
            index: 0,
            delta: ChatMessageDelta {
                role: Some(MessageRole::Assistant),
                content: Some("Response".to_string()),
                function_call: None,
                tool_calls: None,
            },
            finish_reason: Some("stop".to_string()),
            logprobs: None,
        }];

        let provider = Box::new(MockProvider::new(vec![chunks]));
        let context = Box::new(SlidingWindowContext::new(100));
        let prompt_builder = LayeredPromptBuilder::new();

        let mut handle = InternalAgentHandle::new(
            provider,
            context,
            None,
            prompt_builder,
            "test-model".to_string(),
            100, // Budget that will force trimming
        );

        // Send message - should trim context automatically
        {
            let mut stream = handle.send_message_stream("Test message".to_string());
            while (stream.next().await).is_some() {
                // Consume stream
            }
            // Stream is dropped here
        }

        // Context should have been trimmed to fit budget (including system prompt)
        // System prompt "You are a helpful assistant." is ~7 tokens
        // Response is ~2 tokens
        // Total should be well under 100 tokens
        assert!(handle.context.lock().unwrap().token_estimate() <= 100);
    }

    // Mock provider that captures requests for inspection
    struct CapturingMockProvider {
        responses: Vec<Vec<ChatCompletionChunk>>,
        response_index: std::sync::Arc<std::sync::Mutex<usize>>,
        captured_requests: std::sync::Arc<std::sync::Mutex<Vec<Vec<LlmMessage>>>>,
    }

    impl CapturingMockProvider {
        fn new(responses: Vec<Vec<ChatCompletionChunk>>) -> Self {
            Self {
                responses,
                response_index: std::sync::Arc::new(std::sync::Mutex::new(0)),
                captured_requests: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }

        fn get_captured_requests(&self) -> Vec<Vec<LlmMessage>> {
            self.captured_requests.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl TextGenerationProvider for CapturingMockProvider {
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
            request: ChatCompletionRequest,
        ) -> BoxStream<'a, LlmResult<ChatCompletionChunk>> {
            // Capture the request
            self.captured_requests.lock().unwrap().push(request.messages.clone());

            let mut index = self.response_index.lock().unwrap();
            let current = *index;
            *index = (*index + 1) % self.responses.len();
            drop(index);

            let chunks = self.responses.get(current).cloned().unwrap_or_default();
            Box::pin(stream::iter(chunks.into_iter().map(Ok)))
        }

        fn provider_name(&self) -> &str {
            "capturing-mock"
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

    // Mock tool executor
    struct MockToolExecutor;

    #[async_trait]
    impl crucible_core::traits::tools::ToolExecutor for MockToolExecutor {
        async fn execute_tool(
            &self,
            _name: &str,
            _params: serde_json::Value,
            _context: &crucible_core::traits::tools::ExecutionContext,
        ) -> crucible_core::traits::tools::ToolResult<serde_json::Value> {
            Ok(serde_json::json!({"result": "mock tool result"}))
        }

        async fn list_tools(
            &self,
        ) -> crucible_core::traits::tools::ToolResult<Vec<crucible_core::traits::tools::ToolDefinition>>
        {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_tool_call_flow_no_consecutive_assistant_messages() {
        use crucible_core::traits::llm::{FunctionCallDelta, ToolCallDelta};

        // Response 1: Assistant makes a tool call
        let tool_call_response = vec![
            ChatCompletionChunk {
                index: 0,
                delta: ChatMessageDelta {
                    role: Some(MessageRole::Assistant),
                    content: Some("Let me check that.".to_string()),
                    function_call: None,
                    tool_calls: Some(vec![ToolCallDelta {
                        index: 0,
                        id: Some("call_123".to_string()),
                        function: Some(FunctionCallDelta {
                            name: Some("glob".to_string()),
                            arguments: Some(r#"{"pattern": "*"}"#.to_string()),
                        }),
                    }]),
                },
                finish_reason: Some("tool_calls".to_string()),
                logprobs: None,
            },
        ];

        // Response 2: Final response after tool execution
        let final_response = vec![ChatCompletionChunk {
            index: 0,
            delta: ChatMessageDelta {
                role: Some(MessageRole::Assistant),
                content: Some("Here are the files.".to_string()),
                function_call: None,
                tool_calls: None,
            },
            finish_reason: Some("stop".to_string()),
            logprobs: None,
        }];

        let captured_requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let provider = CapturingMockProvider {
            responses: vec![tool_call_response, final_response],
            response_index: std::sync::Arc::new(std::sync::Mutex::new(0)),
            captured_requests: captured_requests.clone(),
        };
        let context = Box::new(SlidingWindowContext::new(10000));
        let prompt_builder = LayeredPromptBuilder::new();
        let tool_executor: Box<dyn crucible_core::traits::tools::ToolExecutor> =
            Box::new(MockToolExecutor);

        let mut handle = InternalAgentHandle::new(
            Box::new(provider),
            context,
            Some(tool_executor),
            prompt_builder,
            "test-model".to_string(),
            10000,
        );

        // Send a message that triggers tool use
        let mut stream = handle.send_message_stream("What files are here?".to_string());
        while let Some(result) = stream.next().await {
            let _ = result.expect("Stream should not error");
        }

        // Check the captured requests
        let requests = captured_requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 2, "Should have made 2 API calls");

        // The second request should NOT have consecutive assistant messages at the end
        let second_request = &requests[1];

        // Check for consecutive assistant messages
        for (i, window) in second_request.windows(2).enumerate() {
            let both_assistant =
                window[0].role == MessageRole::Assistant && window[1].role == MessageRole::Assistant;
            assert!(
                !both_assistant,
                "Found consecutive assistant messages at positions {} and {} in second request.\nMessages: {:?}",
                i,
                i + 1,
                second_request.iter().map(|m| format!("{:?}: {}", m.role, &m.content[..m.content.len().min(50)])).collect::<Vec<_>>()
            );
        }

        // Verify the message sequence is correct:
        // System, User, Assistant (with tool_calls), Tool, ...
        // The last message before second API call should be a Tool message, not Assistant
        let last_msg = second_request.last().expect("Should have messages");
        assert_eq!(
            last_msg.role,
            MessageRole::Tool,
            "Last message before second API call should be Tool result, got {:?}",
            last_msg.role
        );
    }

    #[tokio::test]
    async fn test_multiple_tool_calls_no_consecutive_assistant_messages() {
        use crucible_core::traits::llm::{FunctionCallDelta, ToolCallDelta};

        // Response 1: Assistant makes TWO tool calls (like the user observed)
        let tool_call_response = vec![
            ChatCompletionChunk {
                index: 0,
                delta: ChatMessageDelta {
                    role: Some(MessageRole::Assistant),
                    content: Some("Let me check that.".to_string()),
                    function_call: None,
                    tool_calls: Some(vec![
                        ToolCallDelta {
                            index: 0,
                            id: Some("call_1".to_string()),
                            function: Some(FunctionCallDelta {
                                name: Some("glob".to_string()),
                                arguments: Some(r#"{"pattern": "*.rs"}"#.to_string()),
                            }),
                        },
                        ToolCallDelta {
                            index: 1,
                            id: Some("call_2".to_string()),
                            function: Some(FunctionCallDelta {
                                name: Some("glob".to_string()),
                                arguments: Some(r#"{"pattern": "*.md"}"#.to_string()),
                            }),
                        },
                    ]),
                },
                finish_reason: Some("tool_calls".to_string()),
                logprobs: None,
            },
        ];

        // Response 2: Final response after tool execution
        let final_response = vec![ChatCompletionChunk {
            index: 0,
            delta: ChatMessageDelta {
                role: Some(MessageRole::Assistant),
                content: Some("Found Rust and Markdown files.".to_string()),
                function_call: None,
                tool_calls: None,
            },
            finish_reason: Some("stop".to_string()),
            logprobs: None,
        }];

        let captured_requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let provider = CapturingMockProvider {
            responses: vec![tool_call_response, final_response],
            response_index: std::sync::Arc::new(std::sync::Mutex::new(0)),
            captured_requests: captured_requests.clone(),
        };
        let context = Box::new(SlidingWindowContext::new(10000));
        let prompt_builder = LayeredPromptBuilder::new();
        let tool_executor: Box<dyn crucible_core::traits::tools::ToolExecutor> =
            Box::new(MockToolExecutor);

        let mut handle = InternalAgentHandle::new(
            Box::new(provider),
            context,
            Some(tool_executor),
            prompt_builder,
            "test-model".to_string(),
            10000,
        );

        let mut stream = handle.send_message_stream("What files are here?".to_string());
        while let Some(result) = stream.next().await {
            let _ = result.expect("Stream should not error");
        }

        let requests = captured_requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 2, "Should have made 2 API calls");

        let second_request = &requests[1];

        // Should have TWO tool messages (one per tool call)
        let tool_count = second_request
            .iter()
            .filter(|m| m.role == MessageRole::Tool)
            .count();
        assert_eq!(tool_count, 2, "Should have 2 tool result messages");

        // Check for consecutive assistant messages
        for (i, window) in second_request.windows(2).enumerate() {
            let both_assistant =
                window[0].role == MessageRole::Assistant && window[1].role == MessageRole::Assistant;
            assert!(
                !both_assistant,
                "Found consecutive assistant messages at positions {} and {} in second request.\nMessages: {:?}",
                i,
                i + 1,
                second_request.iter().map(|m| format!("{:?}", m.role)).collect::<Vec<_>>()
            );
        }

        // Print the message sequence for debugging
        eprintln!(
            "Message sequence: {:?}",
            second_request
                .iter()
                .map(|m| format!("{:?}", m.role))
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_incremental_tool_call_streaming() {
        use crucible_core::traits::llm::{FunctionCallDelta, ToolCallDelta};

        // Simulate real streaming: tool calls come in multiple chunks
        let tool_call_response = vec![
            // Chunk 1: Start of response, first tool call ID
            ChatCompletionChunk {
                index: 0,
                delta: ChatMessageDelta {
                    role: Some(MessageRole::Assistant),
                    content: None,
                    function_call: None,
                    tool_calls: Some(vec![ToolCallDelta {
                        index: 0,
                        id: Some("call_abc".to_string()),
                        function: Some(FunctionCallDelta {
                            name: Some("glob".to_string()),
                            arguments: None,
                        }),
                    }]),
                },
                finish_reason: None,
                logprobs: None,
            },
            // Chunk 2: First tool call arguments (partial)
            ChatCompletionChunk {
                index: 0,
                delta: ChatMessageDelta {
                    role: None,
                    content: None,
                    function_call: None,
                    tool_calls: Some(vec![ToolCallDelta {
                        index: 0,
                        id: None,
                        function: Some(FunctionCallDelta {
                            name: None,
                            arguments: Some(r#"{"pat"#.to_string()),
                        }),
                    }]),
                },
                finish_reason: None,
                logprobs: None,
            },
            // Chunk 3: First tool call arguments (rest)
            ChatCompletionChunk {
                index: 0,
                delta: ChatMessageDelta {
                    role: None,
                    content: None,
                    function_call: None,
                    tool_calls: Some(vec![ToolCallDelta {
                        index: 0,
                        id: None,
                        function: Some(FunctionCallDelta {
                            name: None,
                            arguments: Some(r#"tern": "*"}"#.to_string()),
                        }),
                    }]),
                },
                finish_reason: None,
                logprobs: None,
            },
            // Chunk 4: Done with tool_calls finish reason
            ChatCompletionChunk {
                index: 0,
                delta: ChatMessageDelta {
                    role: None,
                    content: None,
                    function_call: None,
                    tool_calls: None,
                },
                finish_reason: Some("tool_calls".to_string()),
                logprobs: None,
            },
        ];

        let final_response = vec![ChatCompletionChunk {
            index: 0,
            delta: ChatMessageDelta {
                role: Some(MessageRole::Assistant),
                content: Some("Here are the files.".to_string()),
                function_call: None,
                tool_calls: None,
            },
            finish_reason: Some("stop".to_string()),
            logprobs: None,
        }];

        let captured_requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let provider = CapturingMockProvider {
            responses: vec![tool_call_response, final_response],
            response_index: std::sync::Arc::new(std::sync::Mutex::new(0)),
            captured_requests: captured_requests.clone(),
        };
        let context = Box::new(SlidingWindowContext::new(10000));
        let prompt_builder = LayeredPromptBuilder::new();
        let tool_executor: Box<dyn crucible_core::traits::tools::ToolExecutor> =
            Box::new(MockToolExecutor);

        let mut handle = InternalAgentHandle::new(
            Box::new(provider),
            context,
            Some(tool_executor),
            prompt_builder,
            "test-model".to_string(),
            10000,
        );

        let mut stream = handle.send_message_stream("List files".to_string());
        while let Some(result) = stream.next().await {
            let _ = result.expect("Stream should not error");
        }

        let requests = captured_requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 2, "Should have made 2 API calls");

        let second_request = &requests[1];
        eprintln!(
            "Incremental streaming - Message sequence: {:?}",
            second_request
                .iter()
                .map(|m| format!("{:?}", m.role))
                .collect::<Vec<_>>()
        );

        // Check no consecutive assistant messages
        for (i, window) in second_request.windows(2).enumerate() {
            let both_assistant =
                window[0].role == MessageRole::Assistant && window[1].role == MessageRole::Assistant;
            assert!(
                !both_assistant,
                "Found consecutive assistant messages at positions {} and {}",
                i,
                i + 1
            );
        }
    }

    /// Test that tool calls are executed even when finish_reason is "stop" (not "tool_calls")
    /// This simulates the behavior of some OpenAI-compatible APIs like vLLM and Qwen
    #[tokio::test]
    async fn test_tool_calls_execute_with_finish_reason_stop() {
        use crucible_core::traits::llm::{FunctionCallDelta, ToolCallDelta};

        // Response has tool calls but finish_reason is "stop" instead of "tool_calls"
        let tool_call_response = vec![ChatCompletionChunk {
            index: 0,
            delta: ChatMessageDelta {
                role: Some(MessageRole::Assistant),
                content: Some("Let me check that for you.".to_string()),
                function_call: None,
                tool_calls: Some(vec![ToolCallDelta {
                    index: 0,
                    id: Some("call_qwen".to_string()),
                    function: Some(FunctionCallDelta {
                        name: Some("list_files".to_string()),
                        arguments: Some(r#"{"path": "."}"#.to_string()),
                    }),
                }]),
            },
            // Qwen and some other APIs return "stop" instead of "tool_calls"
            finish_reason: Some("stop".to_string()),
            logprobs: None,
        }];

        let final_response = vec![ChatCompletionChunk {
            index: 0,
            delta: ChatMessageDelta {
                role: Some(MessageRole::Assistant),
                content: Some("Found these files.".to_string()),
                function_call: None,
                tool_calls: None,
            },
            finish_reason: Some("stop".to_string()),
            logprobs: None,
        }];

        let captured_requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let provider = CapturingMockProvider {
            responses: vec![tool_call_response, final_response],
            response_index: std::sync::Arc::new(std::sync::Mutex::new(0)),
            captured_requests: captured_requests.clone(),
        };
        let context = Box::new(SlidingWindowContext::new(10000));
        let prompt_builder = LayeredPromptBuilder::new();
        let tool_executor: Box<dyn crucible_core::traits::tools::ToolExecutor> =
            Box::new(MockToolExecutor);

        let mut handle = InternalAgentHandle::new(
            Box::new(provider),
            context,
            Some(tool_executor),
            prompt_builder,
            "test-model".to_string(),
            10000,
        );

        let mut stream = handle.send_message_stream("List files".to_string());
        while let Some(result) = stream.next().await {
            let _ = result.expect("Stream should not error");
        }

        let requests = captured_requests.lock().unwrap().clone();

        // Should have made 2 API calls - tool calls SHOULD be executed despite finish_reason="stop"
        assert_eq!(
            requests.len(), 2,
            "Should have made 2 API calls (tool execution should happen even with finish_reason='stop')"
        );

        let second_request = &requests[1];
        eprintln!(
            "finish_reason=stop test - Message sequence: {:?}",
            second_request
                .iter()
                .map(|m| format!("{:?}", m.role))
                .collect::<Vec<_>>()
        );

        // Verify tool result is present
        let tool_count = second_request
            .iter()
            .filter(|m| m.role == MessageRole::Tool)
            .count();
        assert_eq!(tool_count, 1, "Should have 1 tool result message");

        // Check no consecutive assistant messages
        for (i, window) in second_request.windows(2).enumerate() {
            let both_assistant =
                window[0].role == MessageRole::Assistant && window[1].role == MessageRole::Assistant;
            assert!(
                !both_assistant,
                "Found consecutive assistant messages at positions {} and {}",
                i,
                i + 1
            );
        }
    }

    /// Test that multiple user turns don't create consecutive assistant messages
    ///
    /// This reproduces the bug where:
    /// 1. User sends first message
    /// 2. LLM responds with tool call
    /// 3. Tool executes
    /// 4. LLM responds with final content
    /// 5. User sends SECOND message
    /// 6. BUG: Context might have consecutive assistant messages
    #[tokio::test]
    async fn test_multi_turn_no_consecutive_assistant_messages() {
        use crucible_core::traits::llm::{FunctionCallDelta, ToolCallDelta};

        // Turn 1: Tool call + final response (2 LLM calls)
        let tool_call_response = vec![ChatCompletionChunk {
            index: 0,
            delta: ChatMessageDelta {
                role: Some(MessageRole::Assistant),
                content: None,
                function_call: None,
                tool_calls: Some(vec![ToolCallDelta {
                    index: 0,
                    id: Some("call_1".to_string()),
                    function: Some(FunctionCallDelta {
                        name: Some("glob".to_string()),
                        arguments: Some(r#"{"pattern": "*.rs"}"#.to_string()),
                    }),
                }]),
            },
            finish_reason: Some("stop".to_string()), // Qwen uses "stop" not "tool_calls"
            logprobs: None,
        }];

        let first_final_response = vec![ChatCompletionChunk {
            index: 0,
            delta: ChatMessageDelta {
                role: Some(MessageRole::Assistant),
                content: Some("Found these files.".to_string()),
                function_call: None,
                tool_calls: None,
            },
            finish_reason: Some("stop".to_string()),
            logprobs: None,
        }];

        // Turn 2: Simple response (1 LLM call)
        let second_response = vec![ChatCompletionChunk {
            index: 0,
            delta: ChatMessageDelta {
                role: Some(MessageRole::Assistant),
                content: Some("Sure, I can help with that.".to_string()),
                function_call: None,
                tool_calls: None,
            },
            finish_reason: Some("stop".to_string()),
            logprobs: None,
        }];

        let captured_requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let provider = CapturingMockProvider {
            responses: vec![tool_call_response, first_final_response, second_response],
            response_index: std::sync::Arc::new(std::sync::Mutex::new(0)),
            captured_requests: captured_requests.clone(),
        };
        let context = Box::new(SlidingWindowContext::new(10000));
        let prompt_builder = LayeredPromptBuilder::new();
        let tool_executor: Box<dyn crucible_core::traits::tools::ToolExecutor> =
            Box::new(MockToolExecutor);

        let mut handle = InternalAgentHandle::new(
            Box::new(provider),
            context,
            Some(tool_executor),
            prompt_builder,
            "test-model".to_string(),
            10000,
        );

        // Turn 1: User asks to list files
        let mut stream = handle.send_message_stream("List files".to_string());
        while let Some(result) = stream.next().await {
            let _ = result.expect("Stream should not error in turn 1");
        }

        // Turn 2: User asks another question
        let mut stream = handle.send_message_stream("What else can you do?".to_string());
        while let Some(result) = stream.next().await {
            let _ = result.expect("Stream should not error in turn 2");
        }

        let requests = captured_requests.lock().unwrap().clone();
        eprintln!("Multi-turn test - Total API calls: {}", requests.len());

        // Should have 3 API calls: tool call, post-tool, second user turn
        assert_eq!(requests.len(), 3, "Should have made 3 API calls");

        // Check THIRD request (second user turn) for consecutive assistant messages
        let third_request = &requests[2];
        eprintln!(
            "Multi-turn test - Third request message sequence: {:?}",
            third_request
                .iter()
                .map(|m| format!("{:?}", m.role))
                .collect::<Vec<_>>()
        );

        // Check for consecutive assistant messages anywhere in the request
        for (i, window) in third_request.windows(2).enumerate() {
            let both_assistant =
                window[0].role == MessageRole::Assistant && window[1].role == MessageRole::Assistant;
            assert!(
                !both_assistant,
                "Found consecutive assistant messages at positions {} and {} in third request.\n\
                 Message sequence: {:?}",
                i,
                i + 1,
                third_request
                    .iter()
                    .map(|m| format!("{:?}", m.role))
                    .collect::<Vec<_>>()
            );
        }

        // Verify expected message structure:
        // System, User1, Assistant(tools), Tool, Assistant, User2
        let roles: Vec<_> = third_request.iter().map(|m| m.role.clone()).collect();
        assert_eq!(roles[0], MessageRole::System);
        assert_eq!(roles[1], MessageRole::User);
        assert_eq!(roles[2], MessageRole::Assistant); // Has tool_calls
        assert_eq!(roles[3], MessageRole::Tool);
        assert_eq!(roles[4], MessageRole::Assistant); // Final response turn 1
        assert_eq!(roles[5], MessageRole::User); // Second user message
    }
}
