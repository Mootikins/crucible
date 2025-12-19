//! Adapters that bridge legacy providers to new unified traits
//!
//! These adapters wrap existing `EmbeddingProvider` and `TextGenerationProvider`
//! implementations and expose them through the new `Provider`, `CanEmbed`, and
//! `CanChat` traits.

use async_trait::async_trait;
use crucible_config::BackendType;
use crucible_core::traits::llm::{
    ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, LlmResult,
};
use crucible_core::traits::{
    CanChat, CanEmbed, EmbeddingResponse as CoreEmbeddingResponse, ExtendedCapabilities, Provider,
};
use futures::stream::BoxStream;
use std::sync::Arc;

use crate::embeddings::EmbeddingProvider;
use crate::text_generation::TextGenerationProvider;

/// Adapter for embedding-only providers
///
/// Wraps a legacy `EmbeddingProvider` and implements the new unified traits.
pub struct EmbeddingProviderAdapter {
    name: String,
    backend: BackendType,
    endpoint: Option<String>,
    provider: Arc<dyn EmbeddingProvider>,
}

impl EmbeddingProviderAdapter {
    /// Create a new embedding provider adapter
    pub fn new(
        name: impl Into<String>,
        backend: BackendType,
        endpoint: Option<String>,
        provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        Self {
            name: name.into(),
            backend,
            endpoint,
            provider,
        }
    }
}

#[async_trait]
impl Provider for EmbeddingProviderAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn backend_type(&self) -> BackendType {
        self.backend.clone()
    }

    fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }

    fn capabilities(&self) -> ExtendedCapabilities {
        ExtendedCapabilities::embedding_only(self.provider.dimensions())
    }

    async fn health_check(&self) -> LlmResult<bool> {
        // Attempt a test embedding to check health
        match self.provider.embed("test").await {
            Ok(_) => Ok(true),
            Err(e) => Err(crucible_core::traits::llm::LlmError::ProviderError {
                provider: self.name.clone(),
                message: format!("Health check failed: {}", e),
            }),
        }
    }
}

#[async_trait]
impl CanEmbed for EmbeddingProviderAdapter {
    async fn embed(&self, text: &str) -> LlmResult<CoreEmbeddingResponse> {
        let response = self.provider.embed(text).await.map_err(|e| {
            crucible_core::traits::llm::LlmError::ProviderError {
                provider: self.name.clone(),
                message: e.to_string(),
            }
        })?;

        Ok(CoreEmbeddingResponse {
            embedding: response.embedding,
            token_count: response.tokens,
            model: response.model,
        })
    }

    async fn embed_batch(&self, texts: Vec<String>) -> LlmResult<Vec<CoreEmbeddingResponse>> {
        let responses = self.provider.embed_batch(texts).await.map_err(|e| {
            crucible_core::traits::llm::LlmError::ProviderError {
                provider: self.name.clone(),
                message: e.to_string(),
            }
        })?;

        Ok(responses
            .into_iter()
            .map(|r| CoreEmbeddingResponse {
                embedding: r.embedding,
                token_count: r.tokens,
                model: r.model,
            })
            .collect())
    }

    fn embedding_dimensions(&self) -> usize {
        self.provider.dimensions()
    }

    fn embedding_model(&self) -> &str {
        self.provider.model_name()
    }
}

/// Adapter for chat-only providers
///
/// Wraps a legacy `TextGenerationProvider` and implements the new unified traits.
pub struct ChatProviderAdapter {
    name: String,
    backend: BackendType,
    endpoint: Option<String>,
    provider: Arc<dyn TextGenerationProvider>,
}

impl ChatProviderAdapter {
    /// Create a new chat provider adapter
    pub fn new(
        name: impl Into<String>,
        backend: BackendType,
        endpoint: Option<String>,
        provider: Arc<dyn TextGenerationProvider>,
    ) -> Self {
        Self {
            name: name.into(),
            backend,
            endpoint,
            provider,
        }
    }
}

#[async_trait]
impl Provider for ChatProviderAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn backend_type(&self) -> BackendType {
        self.backend.clone()
    }

    fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }

    fn capabilities(&self) -> ExtendedCapabilities {
        let caps = self.provider.capabilities();
        ExtendedCapabilities {
            llm: caps,
            embeddings: false,
            embeddings_batch: false,
            embedding_dimensions: None,
            max_batch_size: None,
        }
    }

    async fn health_check(&self) -> LlmResult<bool> {
        self.provider.health_check().await
    }
}

#[async_trait]
impl CanChat for ChatProviderAdapter {
    async fn chat(&self, request: ChatCompletionRequest) -> LlmResult<ChatCompletionResponse> {
        self.provider.generate_chat_completion(request).await
    }

    fn chat_stream<'a>(
        &'a self,
        request: ChatCompletionRequest,
    ) -> BoxStream<'a, LlmResult<ChatCompletionChunk>> {
        self.provider.generate_chat_completion_stream(request)
    }

    fn chat_model(&self) -> &str {
        self.provider.default_model()
    }
}

/// Unified provider that supports both embeddings and chat
///
/// Wraps both an `EmbeddingProvider` and a `TextGenerationProvider` to provide
/// a single provider that implements all capabilities.
pub struct UnifiedProvider {
    name: String,
    backend: BackendType,
    endpoint: Option<String>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
    chat_provider: Arc<dyn TextGenerationProvider>,
}

impl UnifiedProvider {
    /// Create a new unified provider
    pub fn new(
        name: impl Into<String>,
        backend: BackendType,
        endpoint: Option<String>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        chat_provider: Arc<dyn TextGenerationProvider>,
    ) -> Self {
        Self {
            name: name.into(),
            backend,
            endpoint,
            embedding_provider,
            chat_provider,
        }
    }
}

#[async_trait]
impl Provider for UnifiedProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn backend_type(&self) -> BackendType {
        self.backend.clone()
    }

    fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }

    fn capabilities(&self) -> ExtendedCapabilities {
        let llm_caps = self.chat_provider.capabilities();
        ExtendedCapabilities {
            llm: llm_caps,
            embeddings: true,
            embeddings_batch: true,
            embedding_dimensions: Some(self.embedding_provider.dimensions()),
            max_batch_size: Some(16),
        }
    }

    async fn health_check(&self) -> LlmResult<bool> {
        // Check both providers
        let chat_ok = self.chat_provider.health_check().await?;
        let embed_ok = self.embedding_provider.embed("test").await.is_ok();
        Ok(chat_ok && embed_ok)
    }
}

#[async_trait]
impl CanEmbed for UnifiedProvider {
    async fn embed(&self, text: &str) -> LlmResult<CoreEmbeddingResponse> {
        let response = self.embedding_provider.embed(text).await.map_err(|e| {
            crucible_core::traits::llm::LlmError::ProviderError {
                provider: self.name.clone(),
                message: e.to_string(),
            }
        })?;

        Ok(CoreEmbeddingResponse {
            embedding: response.embedding,
            token_count: response.tokens,
            model: response.model,
        })
    }

    async fn embed_batch(&self, texts: Vec<String>) -> LlmResult<Vec<CoreEmbeddingResponse>> {
        let responses = self
            .embedding_provider
            .embed_batch(texts)
            .await
            .map_err(|e| crucible_core::traits::llm::LlmError::ProviderError {
                provider: self.name.clone(),
                message: e.to_string(),
            })?;

        Ok(responses
            .into_iter()
            .map(|r| CoreEmbeddingResponse {
                embedding: r.embedding,
                token_count: r.tokens,
                model: r.model,
            })
            .collect())
    }

    fn embedding_dimensions(&self) -> usize {
        self.embedding_provider.dimensions()
    }

    fn embedding_model(&self) -> &str {
        self.embedding_provider.model_name()
    }
}

#[async_trait]
impl CanChat for UnifiedProvider {
    async fn chat(&self, request: ChatCompletionRequest) -> LlmResult<ChatCompletionResponse> {
        self.chat_provider.generate_chat_completion(request).await
    }

    fn chat_stream<'a>(
        &'a self,
        request: ChatCompletionRequest,
    ) -> BoxStream<'a, LlmResult<ChatCompletionChunk>> {
        self.chat_provider.generate_chat_completion_stream(request)
    }

    fn chat_model(&self) -> &str {
        self.chat_provider.default_model()
    }
}

#[cfg(test)]
mod tests {

    use crucible_config::BackendType;

    // Note: Full integration tests would require mock providers
    // These tests verify the adapter structure compiles correctly

    #[test]
    fn test_backend_type_supports() {
        assert!(BackendType::Ollama.supports_embeddings());
        assert!(BackendType::Ollama.supports_chat());
        assert!(BackendType::FastEmbed.supports_embeddings());
        assert!(!BackendType::FastEmbed.supports_chat());
        assert!(!BackendType::Anthropic.supports_embeddings());
        assert!(BackendType::Anthropic.supports_chat());
    }
}
