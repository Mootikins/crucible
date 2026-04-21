//! Mock completion backend.

use async_trait::async_trait;
use std::sync::Mutex;

use crate::traits::{
    BackendCompletionChunk, BackendCompletionRequest, BackendResult, CompletionBackend,
};
use futures::stream::BoxStream;

/// State tracked by MockCompletionBackend
#[derive(Debug, Default)]
struct MockCompletionBackendState {
    /// Number of complete_stream calls
    stream_call_count: usize,
    /// Number of health_check calls
    health_check_count: usize,
    /// All requests received
    requests: Vec<BackendCompletionRequest>,
}

/// Behavior configuration for MockCompletionBackend
#[derive(Debug, Clone)]
struct MockCompletionBackendBehavior {
    /// Chunks to return (in order)
    chunks: Vec<BackendResult<BackendCompletionChunk>>,
    /// Provider name to return
    provider_name: String,
    /// Model name to return
    model_name: String,
    /// Health check result
    health_result: BackendResult<bool>,
}

impl Default for MockCompletionBackendBehavior {
    fn default() -> Self {
        Self {
            chunks: vec![
                Ok(BackendCompletionChunk::text("Hello")),
                Ok(BackendCompletionChunk::text(", world!")),
                Ok(BackendCompletionChunk::finished(None)),
            ],
            provider_name: "mock".to_string(),
            model_name: "mock-model".to_string(),
            health_result: Ok(true),
        }
    }
}

/// A mock implementation of `CompletionBackend` for testing.
///
/// Features:
/// - Configurable chunk sequences for streaming
/// - Error injection at any point in the stream
/// - Call tracking for assertions
/// - Deterministic behavior
///
/// # Example
///
/// ```rust
/// use crate::test_support::mocks::MockCompletionBackend;
/// use crate::traits::{BackendCompletionChunk, BackendError};
///
/// // Create mock with default "Hello, world!" response
/// let backend = MockCompletionBackend::new();
/// assert_eq!(backend.stream_call_count(), 0);
///
/// // Configure custom response
/// let backend = MockCompletionBackend::new()
///     .with_chunks(vec![
///         Ok(BackendCompletionChunk::text("Custom response")),
///         Ok(BackendCompletionChunk::finished(None)),
///     ]);
///
/// // Configure error injection
/// let backend = MockCompletionBackend::new()
///     .with_chunks(vec![
///         Ok(BackendCompletionChunk::text("Partial...")),
///         Err(BackendError::Provider("Connection lost".into())),
///     ]);
/// ```
pub struct MockCompletionBackend {
    state: Mutex<MockCompletionBackendState>,
    behavior: Mutex<MockCompletionBackendBehavior>,
}

impl std::fmt::Debug for MockCompletionBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.state.lock().unwrap();
        let behavior = self.behavior.lock().unwrap();
        f.debug_struct("MockCompletionBackend")
            .field("stream_call_count", &state.stream_call_count)
            .field("provider_name", &behavior.provider_name)
            .field("model_name", &behavior.model_name)
            .finish()
    }
}

impl Default for MockCompletionBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MockCompletionBackend {
    /// Create a new mock with default "Hello, world!" response
    pub fn new() -> Self {
        Self {
            state: Mutex::new(MockCompletionBackendState::default()),
            behavior: Mutex::new(MockCompletionBackendBehavior::default()),
        }
    }

    /// Set the chunks to return from complete_stream
    pub fn with_chunks(self, chunks: Vec<BackendResult<BackendCompletionChunk>>) -> Self {
        self.behavior.lock().unwrap().chunks = chunks;
        self
    }

    /// Set chunks to a simple text response
    pub fn with_text_response(self, text: &str) -> Self {
        self.with_chunks(vec![
            Ok(BackendCompletionChunk::text(text)),
            Ok(BackendCompletionChunk::finished(None)),
        ])
    }

    /// Set provider name
    pub fn with_provider_name(self, name: impl Into<String>) -> Self {
        self.behavior.lock().unwrap().provider_name = name.into();
        self
    }

    /// Set model name
    pub fn with_model_name(self, name: impl Into<String>) -> Self {
        self.behavior.lock().unwrap().model_name = name.into();
        self
    }

    /// Set health check result
    pub fn with_health_result(self, result: BackendResult<bool>) -> Self {
        self.behavior.lock().unwrap().health_result = result;
        self
    }

    /// Get number of complete_stream calls
    pub fn stream_call_count(&self) -> usize {
        self.state.lock().unwrap().stream_call_count
    }

    /// Get number of health_check calls
    pub fn health_check_count(&self) -> usize {
        self.state.lock().unwrap().health_check_count
    }

    /// Get all requests received
    pub fn requests(&self) -> Vec<BackendCompletionRequest> {
        self.state.lock().unwrap().requests.clone()
    }

    /// Get the last request received
    pub fn last_request(&self) -> Option<BackendCompletionRequest> {
        self.state.lock().unwrap().requests.last().cloned()
    }

    /// Reset all state (call counts, requests)
    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        state.stream_call_count = 0;
        state.health_check_count = 0;
        state.requests.clear();
    }
}

#[async_trait]
impl CompletionBackend for MockCompletionBackend {
    fn complete_stream(
        &self,
        request: BackendCompletionRequest,
    ) -> BoxStream<'static, BackendResult<BackendCompletionChunk>> {
        // Track the call
        {
            let mut state = self.state.lock().unwrap();
            state.stream_call_count += 1;
            state.requests.push(request);
        }

        // Clone chunks for the stream
        let chunks = self.behavior.lock().unwrap().chunks.clone();

        Box::pin(futures::stream::iter(chunks))
    }

    fn provider_name(&self) -> &str {
        // This is a bit awkward but necessary for the trait signature
        // We leak a string to get a &'static str - fine for tests
        let name = self.behavior.lock().unwrap().provider_name.clone();
        Box::leak(name.into_boxed_str())
    }

    fn model_name(&self) -> &str {
        let name = self.behavior.lock().unwrap().model_name.clone();
        Box::leak(name.into_boxed_str())
    }

    async fn health_check(&self) -> BackendResult<bool> {
        self.state.lock().unwrap().health_check_count += 1;
        self.behavior.lock().unwrap().health_result.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{BackendError, ContextMessage, ToolCall};
    use futures::StreamExt;

    #[test]
    fn test_mock_completion_backend_default() {
        let backend = MockCompletionBackend::new();
        assert_eq!(backend.stream_call_count(), 0);
        assert_eq!(backend.health_check_count(), 0);
        assert!(backend.requests().is_empty());
    }

    #[tokio::test]
    async fn test_mock_completion_backend_streaming() {
        let backend = MockCompletionBackend::new();
        let request = BackendCompletionRequest::new("system", vec![ContextMessage::user("hi")]);

        let mut stream = backend.complete_stream(request);
        let mut text = String::new();

        while let Some(result) = stream.next().await {
            let chunk = result.unwrap();
            if let Some(delta) = chunk.delta {
                text.push_str(&delta);
            }
            if chunk.done {
                break;
            }
        }

        assert_eq!(text, "Hello, world!");
        assert_eq!(backend.stream_call_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_completion_backend_custom_response() {
        let backend = MockCompletionBackend::new().with_text_response("Custom!");
        let request = BackendCompletionRequest::new("system", vec![]);

        let response = backend.complete(request).await.unwrap();
        assert_eq!(response.content, "Custom!");
    }

    #[tokio::test]
    async fn test_mock_completion_backend_error_injection() {
        let backend = MockCompletionBackend::new().with_chunks(vec![
            Ok(BackendCompletionChunk::text("Partial")),
            Err(BackendError::Provider("Connection lost".into())),
        ]);

        let request = BackendCompletionRequest::new("system", vec![]);
        let mut stream = backend.complete_stream(request);

        // First chunk succeeds
        let chunk = stream.next().await.unwrap().unwrap();
        assert_eq!(chunk.delta, Some("Partial".to_string()));

        // Second chunk is an error
        let result = stream.next().await.unwrap();
        assert!(matches!(result, Err(BackendError::Provider(_))));
    }

    #[tokio::test]
    async fn test_mock_completion_backend_tool_calls() {
        use crate::traits::FunctionCall;

        let tool_call = ToolCall {
            id: "call_123".to_string(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: "search".to_string(),
                arguments: r#"{"q":"test"}"#.to_string(),
            },
        };

        let backend = MockCompletionBackend::new().with_chunks(vec![
            Ok(BackendCompletionChunk::tool_call(tool_call.clone())),
            Ok(BackendCompletionChunk::finished(None)),
        ]);

        let request = BackendCompletionRequest::new("system", vec![]);
        let response = backend.complete(request).await.unwrap();

        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "call_123");
        assert_eq!(response.tool_calls[0].function.name, "search");
    }

    #[tokio::test]
    async fn test_mock_completion_backend_request_tracking() {
        let backend = MockCompletionBackend::new();

        let req1 = BackendCompletionRequest::new("sys1", vec![ContextMessage::user("msg1")]);
        let req2 = BackendCompletionRequest::new("sys2", vec![ContextMessage::user("msg2")]);

        let _ = backend.complete(req1).await;
        let _ = backend.complete(req2).await;

        let requests = backend.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].system_prompt, "sys1");
        assert_eq!(requests[1].system_prompt, "sys2");

        let last = backend.last_request().unwrap();
        assert_eq!(last.system_prompt, "sys2");
    }

    #[tokio::test]
    async fn test_mock_completion_backend_health_check() {
        let backend = MockCompletionBackend::new();
        assert!(backend.health_check().await.unwrap());
        assert_eq!(backend.health_check_count(), 1);

        let unhealthy = MockCompletionBackend::new()
            .with_health_result(Err(BackendError::Provider("Down".into())));
        assert!(unhealthy.health_check().await.is_err());
    }

    #[test]
    fn test_mock_completion_backend_provider_model_names() {
        let backend = MockCompletionBackend::new()
            .with_provider_name("test-provider")
            .with_model_name("test-model");

        assert_eq!(backend.provider_name(), "test-provider");
        assert_eq!(backend.model_name(), "test-model");
    }

    #[tokio::test]
    async fn test_mock_completion_backend_reset() {
        let backend = MockCompletionBackend::new();

        let request = BackendCompletionRequest::new("system", vec![]);
        let _ = backend.complete(request).await;
        let _ = backend.health_check().await;

        assert_eq!(backend.stream_call_count(), 1);
        assert_eq!(backend.health_check_count(), 1);

        backend.reset();

        assert_eq!(backend.stream_call_count(), 0);
        assert_eq!(backend.health_check_count(), 0);
        assert!(backend.requests().is_empty());
    }
}
