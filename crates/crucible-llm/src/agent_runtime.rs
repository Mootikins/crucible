//! Agent Runtime - Coordinates LLM chat with tool execution
//!
//! The AgentRuntime manages the conversation loop between a LlmProvider
//! and ToolExecutor, enabling autonomous agent behavior.

use crucible_core::traits::{
    ExecutionContext, LlmError, LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmResult,
    LlmToolDefinition, MessageRole, ToolExecutor,
};
use tracing::{debug, info, warn};

/// Agent runtime that coordinates chat and tool execution
pub struct AgentRuntime {
    /// Chat provider for LLM interactions
    provider: Box<dyn LlmProvider>,
    /// Tool executor for running tools
    executor: Box<dyn ToolExecutor>,
    /// Conversation history
    conversation: Vec<LlmMessage>,
    /// Maximum iterations to prevent infinite loops
    max_iterations: usize,
    /// Execution context for tools
    context: ExecutionContext,
}

impl AgentRuntime {
    /// Create a new agent runtime
    pub fn new(provider: Box<dyn LlmProvider>, executor: Box<dyn ToolExecutor>) -> Self {
        Self {
            provider,
            executor,
            conversation: Vec::new(),
            max_iterations: 10,
            context: ExecutionContext::new(),
        }
    }

    /// Set maximum iterations
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    /// Set execution context
    pub fn with_context(mut self, context: ExecutionContext) -> Self {
        self.context = context;
        self
    }

    /// Get conversation history
    pub fn get_conversation_history(&self) -> &[LlmMessage] {
        &self.conversation
    }

    /// Clear conversation history
    pub fn clear_history(&mut self) {
        self.conversation.clear();
    }

    /// Run a conversation with tool calling support
    ///
    /// This method implements the agent loop:
    /// 1. Send messages to LLM
    /// 2. If LLM requests tools, execute them
    /// 3. Send tool results back to LLM
    /// 4. Repeat until LLM responds without tool calls or max iterations reached
    pub async fn run_conversation(
        &mut self,
        initial_messages: Vec<LlmMessage>,
    ) -> LlmResult<LlmResponse> {
        // Add initial messages to conversation
        self.conversation.extend(initial_messages);

        // Get available tools
        let tools = self
            .executor
            .list_tools()
            .await
            .map_err(|e| LlmError::Internal(format!("Failed to list tools: {}", e)))?;

        // Convert to LLM tool definitions
        let llm_tools: Vec<LlmToolDefinition> = tools
            .iter()
            .map(|t| {
                LlmToolDefinition::new(
                    t.name.clone(),
                    t.description.clone(),
                    t.parameters.clone().unwrap_or(serde_json::json!({})),
                )
            })
            .collect();

        info!(
            "Starting agent conversation with {} available tools",
            llm_tools.len()
        );

        let mut iteration = 0;
        let mut last_response = None;

        while iteration < self.max_iterations {
            iteration += 1;
            debug!("Agent iteration {}/{}", iteration, self.max_iterations);

            // Build request with conversation history and tools
            let request = LlmRequest::new(self.conversation.clone()).with_tools(llm_tools.clone());

            // Get response from LLM
            let response = self.provider.complete(request).await?;

            // Add assistant message to conversation
            self.conversation.push(response.message.clone());

            // Check if there are tool calls
            if let Some(tool_calls) = &response.message.tool_calls {
                info!("LLM requested {} tool calls", tool_calls.len());

                // Execute each tool call
                for tool_call in tool_calls {
                    debug!(
                        "Executing tool: {} with params: {:?}",
                        tool_call.name, tool_call.parameters
                    );

                    match self
                        .executor
                        .execute_tool(&tool_call.name, tool_call.parameters.clone(), &self.context)
                        .await
                    {
                        Ok(result) => {
                            info!("Tool {} executed successfully", tool_call.name);
                            // Add tool result to conversation
                            let tool_message =
                                LlmMessage::tool(tool_call.id.clone(), result.to_string());
                            self.conversation.push(tool_message);
                        }
                        Err(e) => {
                            warn!("Tool {} failed: {}", tool_call.name, e);
                            // Add error as tool result
                            let error_message =
                                LlmMessage::tool(tool_call.id.clone(), format!("Error: {}", e));
                            self.conversation.push(error_message);
                        }
                    }
                }

                // Continue loop to get LLM's response to tool results
                last_response = Some(response);
                continue;
            }

            // No tool calls, conversation complete
            info!("Agent conversation completed in {} iterations", iteration);
            return Ok(response);
        }

        // Max iterations reached
        warn!(
            "Agent reached max iterations ({}), returning last response",
            self.max_iterations
        );
        last_response.ok_or_else(|| {
            LlmError::Internal("Max iterations reached with no response".to_string())
        })
    }

    /// Send a single message and get a response (convenience method)
    pub async fn send_message(&mut self, message: String) -> LlmResult<LlmResponse> {
        self.run_conversation(vec![LlmMessage::user(message)]).await
    }

    /// Add a system message to set agent behavior
    pub fn set_system_prompt(&mut self, prompt: String) {
        // Insert at beginning or replace existing system message
        if !self.conversation.is_empty() && self.conversation[0].role == MessageRole::System {
            self.conversation[0] = LlmMessage::system(prompt);
        } else {
            self.conversation.insert(0, LlmMessage::system(prompt));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use crucible_core::traits::{TokenUsage, ToolResult};

    struct MockProvider;

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(&self, _request: LlmRequest) -> LlmResult<LlmResponse> {
            Ok(LlmResponse {
                message: LlmMessage::assistant("Test response"),
                usage: TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 20,
                    total_tokens: 30,
                },
                model: "test".to_string(),
            })
        }

        fn provider_name(&self) -> &str {
            "Mock"
        }

        fn default_model(&self) -> &str {
            "mock"
        }

        async fn health_check(&self) -> LlmResult<bool> {
            Ok(true)
        }
    }

    struct MockExecutor;

    #[async_trait]
    impl ToolExecutor for MockExecutor {
        async fn execute_tool(
            &self,
            _name: &str,
            _params: serde_json::Value,
            _context: &ExecutionContext,
        ) -> ToolResult<serde_json::Value> {
            Ok(serde_json::json!({"result": "success"}))
        }

        async fn list_tools(&self) -> ToolResult<Vec<crucible_core::traits::ToolDefinition>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_runtime_creation() {
        let provider = Box::new(MockProvider);
        let executor = Box::new(MockExecutor);

        let runtime = AgentRuntime::new(provider, executor);

        assert_eq!(runtime.max_iterations, 10);
        assert_eq!(runtime.conversation.len(), 0);
    }

    #[tokio::test]
    async fn test_set_system_prompt() {
        let provider = Box::new(MockProvider);
        let executor = Box::new(MockExecutor);

        let mut runtime = AgentRuntime::new(provider, executor);

        runtime.set_system_prompt("You are a helpful assistant".to_string());

        assert_eq!(runtime.conversation.len(), 1);
        assert_eq!(runtime.conversation[0].role, MessageRole::System);
    }
}
