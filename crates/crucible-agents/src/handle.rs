//! Internal Agent Handle
//!
//! Implements `AgentHandle` using direct LLM API calls via `TextGenerationProvider`.

use async_trait::async_trait;
use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatError, ChatMode, ChatResult};
use crucible_core::traits::context::ContextManager;
use crucible_core::traits::llm::{
    ChatCompletionRequest, LlmMessage, TextGenerationProvider, ToolCall as LlmToolCall,
};
use crucible_core::traits::tools::{ExecutionContext, ToolExecutor};
use futures::stream::{BoxStream, StreamExt};
use uuid::Uuid;

use crate::prompt::LayeredPromptBuilder;
use crate::token::TokenBudget;

/// Internal agent handle that uses direct LLM API calls
///
/// This handle wraps a `TextGenerationProvider` and provides conversation
/// management with sliding window context and layered prompts.
pub struct InternalAgentHandle {
    /// The LLM provider for generating completions
    provider: Box<dyn TextGenerationProvider>,

    /// Context manager for conversation history
    context: Box<dyn ContextManager>,

    /// Optional tool executor
    tools: Option<Box<dyn ToolExecutor>>,

    /// Prompt builder for layered system prompts (reserved for future use)
    #[allow(dead_code)]
    prompt_builder: LayeredPromptBuilder,

    /// Token budget tracker
    token_budget: TokenBudget,

    /// Current chat mode
    mode: ChatMode,

    /// Model identifier
    model: String,

    /// Unique agent ID (reserved for future use)
    #[allow(dead_code)]
    agent_id: String,
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

        Self {
            provider,
            context: new_context,
            tools,
            prompt_builder,
            token_budget,
            mode: ChatMode::Plan,
            model,
            agent_id,
        }
    }

    /// Helper to convert LLM tool calls to chat tool calls
    fn convert_tool_calls(llm_calls: &[LlmToolCall]) -> Vec<crucible_core::traits::chat::ToolCall> {
        llm_calls
            .iter()
            .map(|tc| crucible_core::traits::chat::ToolCall {
                name: tc.function.name.clone(),
                arguments: serde_json::from_str(&tc.function.arguments).ok(),
                id: Some(tc.id.clone()),
            })
            .collect()
    }

    /// Execute tool calls and add results to context
    async fn execute_tool_calls(&mut self, tool_calls: &[LlmToolCall]) -> ChatResult<()> {
        let tool_executor = match &self.tools {
            Some(executor) => executor,
            None => {
                return Err(ChatError::Internal(
                    "Tool calls requested but no tool executor available".to_string(),
                ))
            }
        };

        let execution_context = ExecutionContext::new();

        for tool_call in tool_calls {
            // Parse arguments
            let arguments: serde_json::Value = serde_json::from_str(&tool_call.function.arguments)
                .map_err(|e| {
                    ChatError::Internal(format!("Failed to parse tool arguments: {}", e))
                })?;

            // Execute tool
            let result = tool_executor
                .execute_tool(&tool_call.function.name, arguments, &execution_context)
                .await
                .map_err(|e| ChatError::Internal(format!("Tool execution failed: {}", e)))?;

            // Add tool result to context
            let result_str = serde_json::to_string(&result)
                .unwrap_or_else(|_| "Error serializing tool result".to_string());
            self.context
                .add_message(LlmMessage::tool(tool_call.id.clone(), result_str));
        }

        Ok(())
    }
}

#[async_trait]
impl AgentHandle for InternalAgentHandle {
    fn send_message_stream<'a>(
        &'a mut self,
        message: &'a str,
    ) -> BoxStream<'a, ChatResult<ChatChunk>> {
        Box::pin(async_stream::stream! {
            // Add user message to context
            self.context.add_message(LlmMessage::user(message));

            // Tool execution loop - continue until no more tool calls
            loop {
                // Trim context to budget
                self.context.trim_to_budget(self.token_budget.remaining());

                // Build request from context
                let messages = self.context.get_messages();
                let request = ChatCompletionRequest::new(self.model.clone(), messages);

                // Stream completion from provider
                // We need to collect chunks into a vec to avoid borrowing issues
                let stream = self.provider.generate_chat_completion_stream(request);
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

                // Add assistant response to context
                if !accumulated_tool_calls.is_empty() {
                    self.context.add_message(LlmMessage::assistant_with_tools(
                        content.clone(),
                        accumulated_tool_calls.clone(),
                    ));
                } else {
                    self.context.add_message(LlmMessage::assistant(content.clone()));
                }

                // Check if we need to execute tools
                if !accumulated_tool_calls.is_empty() && finish_reason.as_deref() == Some("tool_calls") {
                    // Execute tools
                    if let Err(e) = self.execute_tool_calls(&accumulated_tool_calls).await {
                        yield Err(e);
                        return;
                    }
                    // Continue loop to get next response
                } else {
                    // No more tool calls, we're done
                    let chat_tool_calls = if !accumulated_tool_calls.is_empty() {
                        Some(Self::convert_tool_calls(&accumulated_tool_calls))
                    } else {
                        None
                    };

                    yield Ok(ChatChunk {
                        delta: String::new(),
                        done: true,
                        tool_calls: chat_tool_calls,
                    });
                    break;
                }
            }
        })
    }

    async fn set_mode(&mut self, mode: ChatMode) -> ChatResult<()> {
        self.mode = mode;
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
        assert_eq!(handle.mode, ChatMode::Plan);
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

        let mut stream = handle.send_message_stream("Hi");
        let mut collected = Vec::new();

        while let Some(result) = stream.next().await {
            collected.push(result.unwrap());
        }

        // Should have content chunks + done chunk
        assert!(collected.len() >= 2);

        // Last chunk should be done
        assert!(collected.last().unwrap().done);

        // Content should be accumulated
        let content: String = collected.iter()
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

        assert_eq!(handle.mode, ChatMode::Plan);

        handle.set_mode(ChatMode::Act).await.unwrap();
        assert_eq!(handle.mode, ChatMode::Act);

        handle.set_mode(ChatMode::AutoApprove).await.unwrap();
        assert_eq!(handle.mode, ChatMode::AutoApprove);
    }

    #[tokio::test]
    async fn test_context_trimming() {
        let chunks = vec![
            ChatCompletionChunk {
                index: 0,
                delta: ChatMessageDelta {
                    role: Some(MessageRole::Assistant),
                    content: Some("Response".to_string()),
                    function_call: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
                logprobs: None,
            },
        ];

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
            let mut stream = handle.send_message_stream("Test message");
            while let Some(_) = stream.next().await {
                // Consume stream
            }
            // Stream is dropped here
        }

        // Context should have been trimmed to fit budget (including system prompt)
        // System prompt "You are a helpful assistant." is ~7 tokens
        // Response is ~2 tokens
        // Total should be well under 100 tokens
        assert!(handle.context.token_estimate() <= 100);
    }
}
