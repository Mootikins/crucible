//! Mock text generation provider for testing
//!
//! This module provides mock implementations of the `TextGenerationProvider` trait
//! for use in unit and integration tests. It allows testing LLM-dependent code
//! without requiring real API keys or network calls.

use crate::text_generation::*;
use async_trait::async_trait;
use chrono::Utc;
use crucible_core::traits::llm::{LlmError, LlmResult};
use futures::stream::{self, BoxStream};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Mock text generation provider for testing
///
/// Returns deterministic responses based on configured templates, useful for unit tests
/// without requiring external LLM services.
///
pub struct MockTextProvider {
    model_name: String,
    /// Configured responses for specific prompts
    completion_responses: Arc<Mutex<HashMap<String, String>>>,
    /// Configured chat responses for specific message sequences
    chat_responses: Arc<Mutex<HashMap<String, String>>>,
    /// Default response when no specific response is configured
    default_response: String,
    /// Track call history for verification
    call_history: Arc<Mutex<Vec<MockCall>>>,
}

/// Record of a mock provider call
#[derive(Debug, Clone)]
pub struct MockCall {
    /// Type of call made to the mock provider
    pub call_type: MockCallType,
    /// Prompt text that was sent
    pub prompt: String,
    /// Model name that was requested
    pub model: String,
}

/// Type of mock provider call
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockCallType {
    /// Text completion call
    Completion,
    /// Streaming text completion call
    CompletionStream,
    /// Chat completion call
    ChatCompletion,
    /// Streaming chat completion call
    ChatCompletionStream,
}

impl MockTextProvider {
    /// Create a new mock text provider with default settings
    pub fn new() -> Self {
        Self {
            model_name: "mock-llm".to_string(),
            completion_responses: Arc::new(Mutex::new(HashMap::new())),
            chat_responses: Arc::new(Mutex::new(HashMap::new())),
            default_response: "This is a mock response.".to_string(),
            call_history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Create a mock provider with a custom model name
    pub fn with_model(model_name: String) -> Self {
        Self {
            model_name,
            ..Self::new()
        }
    }

    /// Set a specific response for a completion prompt
    ///
    /// Uses interior mutability, so &self is sufficient.
    ///
    /// # Example
    ///
    /// ```rust
    /// use crucible_llm::text_generation_mock::MockTextProvider;
    ///
    /// let provider = MockTextProvider::new();
    /// provider.set_completion_response("Hello", "Hi there!");
    /// ```
    pub fn set_completion_response(&self, prompt: &str, response: &str) {
        let mut responses = self.completion_responses.lock().unwrap();
        responses.insert(prompt.to_string(), response.to_string());
    }

    /// Set a specific response for a chat completion
    ///
    /// The key is the last user message content in the conversation.
    /// Uses interior mutability, so &self is sufficient.
    pub fn set_chat_response(&self, last_user_message: &str, response: &str) {
        let mut responses = self.chat_responses.lock().unwrap();
        responses.insert(last_user_message.to_string(), response.to_string());
    }

    /// Set the default response for unconfigured prompts
    ///
    /// Note: This method modifies the default_response field directly,
    /// which requires &mut self. For immutable access, use set_completion_response
    /// to configure specific responses instead.
    pub fn set_default_response(&mut self, response: &str) {
        self.default_response = response.to_string();
    }

    /// Get the call history for verification
    pub fn call_history(&self) -> Vec<MockCall> {
        self.call_history.lock().unwrap().clone()
    }

    /// Clear the call history
    ///
    /// Uses interior mutability, so &self is sufficient.
    pub fn clear_history(&self) {
        self.call_history.lock().unwrap().clear();
    }

    /// Record a call in the history
    fn record_call(&self, call_type: MockCallType, prompt: String, model: String) {
        let mut history = self.call_history.lock().unwrap();
        history.push(MockCall {
            call_type,
            prompt,
            model,
        });
    }

    /// Get response for a completion prompt
    fn get_completion_response(&self, prompt: &str) -> String {
        let responses = self.completion_responses.lock().unwrap();
        responses
            .get(prompt)
            .cloned()
            .unwrap_or_else(|| self.default_response.clone())
    }

    /// Get response for a chat completion (uses last user message as key)
    fn get_chat_response(&self, messages: &[LlmMessage]) -> String {
        // Find the last user message
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

impl Default for MockTextProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TextGenerationProvider for MockTextProvider {
    async fn generate_completion(
        &self,
        request: CompletionRequest,
    ) -> LlmResult<CompletionResponse> {
        self.record_call(
            MockCallType::Completion,
            request.prompt.clone(),
            request.model.clone(),
        );

        let text = self.get_completion_response(&request.prompt);

        Ok(CompletionResponse {
            choices: vec![CompletionChoice {
                text,
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
        request: CompletionRequest,
    ) -> BoxStream<'a, LlmResult<CompletionChunk>> {
        self.record_call(
            MockCallType::CompletionStream,
            request.prompt.clone(),
            request.model.clone(),
        );

        let text = self.get_completion_response(&request.prompt);

        // Split response into chunks
        let words: Vec<String> = text.split_whitespace().map(|s| s.to_string()).collect();
        let mut chunks = Vec::new();

        for word in words {
            chunks.push(Ok(CompletionChunk {
                text: format!("{} ", word),
                index: 0,
                finish_reason: None,
                logprobs: None,
            }));
        }

        // Add final chunk with finish_reason
        chunks.push(Ok(CompletionChunk {
            text: String::new(),
            index: 0,
            finish_reason: Some("stop".to_string()),
            logprobs: None,
        }));

        Box::pin(stream::iter(chunks))
    }

    async fn generate_chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> LlmResult<ChatCompletionResponse> {
        let last_user_msg = request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
            .map(|m| m.content.clone())
            .unwrap_or_default();

        self.record_call(
            MockCallType::ChatCompletion,
            last_user_msg,
            request.model.clone(),
        );

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
        request: ChatCompletionRequest,
    ) -> BoxStream<'a, LlmResult<ChatCompletionChunk>> {
        let last_user_msg = request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
            .map(|m| m.content.clone())
            .unwrap_or_default();

        self.record_call(
            MockCallType::ChatCompletionStream,
            last_user_msg,
            request.model.clone(),
        );

        let response_text = self.get_chat_response(&request.messages);

        // Split response into word chunks
        let words: Vec<String> = response_text
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        let mut chunks = Vec::new();

        // Send role first
        chunks.push(Ok(ChatCompletionChunk {
            index: 0,
            delta: ChatMessageDelta {
                role: Some(MessageRole::Assistant),
                content: None,
                function_call: None,
                tool_calls: None,
            },
            finish_reason: None,
            logprobs: None,
        }));

        // Send content chunks
        for word in words {
            chunks.push(Ok(ChatCompletionChunk {
                index: 0,
                delta: ChatMessageDelta {
                    role: None,
                    content: Some(format!("{} ", word)),
                    function_call: None,
                    tool_calls: None,
                },
                finish_reason: None,
                logprobs: None,
            }));
        }

        // Send final chunk
        chunks.push(Ok(ChatCompletionChunk {
            index: 0,
            delta: ChatMessageDelta {
                role: None,
                content: None,
                function_call: None,
                tool_calls: None,
            },
            finish_reason: Some("stop".to_string()),
            logprobs: None,
        }));

        Box::pin(stream::iter(chunks))
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    fn default_model(&self) -> &str {
        &self.model_name
    }

    async fn list_models(&self) -> LlmResult<Vec<TextModelInfo>> {
        Ok(vec![
            TextModelInfo {
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
            },
            TextModelInfo {
                id: "mock-llm-large".to_string(),
                name: "Mock Large LLM".to_string(),
                owner: Some("Test".to_string()),
                capabilities: vec![
                    ModelCapability::TextCompletion,
                    ModelCapability::ChatCompletion,
                    ModelCapability::Streaming,
                    ModelCapability::FunctionCalling,
                ],
                max_context_length: Some(32768),
                max_output_tokens: Some(4096),
                input_price: None,
                output_price: None,
                created: Some(Utc::now()),
                status: ModelStatus::Available,
            },
        ])
    }

    async fn health_check(&self) -> LlmResult<bool> {
        // Mock provider is always healthy
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_completion_basic() {
        let provider = MockTextProvider::new();
        let request = CompletionRequest::new("mock-model".to_string(), "Hello".to_string());

        let response = provider.generate_completion(request).await.unwrap();

        assert_eq!(response.model, "mock-llm");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].text, "This is a mock response.");
    }

    #[tokio::test]
    async fn test_mock_completion_custom_response() {
        let provider = MockTextProvider::new();
        provider
            .set_completion_response("What is Rust?", "Rust is a systems programming language.");

        let request = CompletionRequest::new("mock-model".to_string(), "What is Rust?".to_string());
        let response = provider.generate_completion(request).await.unwrap();

        assert_eq!(
            response.choices[0].text,
            "Rust is a systems programming language."
        );
    }

    #[tokio::test]
    async fn test_mock_chat_completion() {
        let provider = MockTextProvider::new();
        provider.set_chat_response("Hello", "Hi there! How can I help you?");

        let request = ChatCompletionRequest::new(
            "mock-model".to_string(),
            vec![LlmMessage::user("Hello".to_string())],
        );

        let response = provider.generate_chat_completion(request).await.unwrap();

        assert_eq!(response.choices.len(), 1);
        assert_eq!(
            response.choices[0].message.content,
            "Hi there! How can I help you?"
        );
        assert_eq!(response.choices[0].message.role, MessageRole::Assistant);
    }

    #[tokio::test]
    async fn test_mock_call_history() {
        let provider = MockTextProvider::new();

        let request1 = CompletionRequest::new("model".to_string(), "First".to_string());
        let _ = provider.generate_completion(request1).await;

        let request2 = ChatCompletionRequest::new(
            "model".to_string(),
            vec![LlmMessage::user("Second".to_string())],
        );
        let _ = provider.generate_chat_completion(request2).await;

        let history = provider.call_history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].call_type, MockCallType::Completion);
        assert_eq!(history[0].prompt, "First");
        assert_eq!(history[1].call_type, MockCallType::ChatCompletion);
        assert_eq!(history[1].prompt, "Second");
    }

    #[tokio::test]
    async fn test_mock_completion_stream() {
        use futures::StreamExt;

        let provider = MockTextProvider::new();
        provider.set_completion_response("Stream test", "Hello world from stream");

        let request = CompletionRequest::new("model".to_string(), "Stream test".to_string());
        let mut stream = provider.generate_completion_stream(request);

        let mut chunks = Vec::new();
        while let Some(result) = stream.next().await {
            chunks.push(result.unwrap());
        }

        assert!(!chunks.is_empty());
        assert_eq!(
            chunks.last().unwrap().finish_reason,
            Some("stop".to_string())
        );
    }

    #[tokio::test]
    async fn test_mock_list_models() {
        let provider = MockTextProvider::new();
        let models = provider.list_models().await.unwrap();

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "mock-llm");
        assert_eq!(models[1].id, "mock-llm-large");
    }

    #[tokio::test]
    async fn test_mock_health_check() {
        let provider = MockTextProvider::new();
        assert!(provider.health_check().await.unwrap());
    }
}
