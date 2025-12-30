//! Unified provider traits with capability-based extensions
//!
//! This module defines the unified provider abstraction with extension traits
//! for specific capabilities (embeddings, chat). This follows the Interface
//! Segregation principle - providers implement only what they support.
//!
//! ## Design Pattern
//!
//! ```text
//! Provider (base trait)
//!    ├── CanEmbed (extension trait for embeddings)
//!    └── CanChat (extension trait for chat/completions)
//! ```
//!
//! This design allows:
//! - Type-safe capability discovery at compile time
//! - Providers that support only embeddings (FastEmbed, Burn)
//! - Providers that support only chat (Anthropic)
//! - Providers that support both (Ollama, OpenAI)

use async_trait::async_trait;
use crucible_config::BackendType;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::completion_backend::BackendResult;
use super::llm::{
    ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, ProviderCapabilities,
};

/// Embedding response from a provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    /// The embedding vector
    pub embedding: Vec<f32>,
    /// Number of tokens in the input
    pub token_count: Option<usize>,
    /// Model used for embedding
    pub model: String,
}

/// Provider-level model capability flags.
///
/// Indicates the **type** of model (embedding, chat, image, etc.) at the provider
/// abstraction layer. Used by [`UnifiedModelInfo`] for provider capability discovery.
///
/// This is distinct from [`crate::traits::llm::ModelCapability`] which describes
/// **features** of text/chat models (function calling, streaming, JSON mode, etc.).
///
/// # When to Use
/// - Use this enum when categorizing models by their primary function
/// - Use `llm::ModelCapability` when describing chat model feature sets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelCapability {
    /// Text embedding generation
    Embedding,
    /// Chat/conversation completion
    Chat,
    /// Text generation (completion)
    TextGeneration,
    /// Image generation
    ImageGeneration,
    /// Vision/image understanding
    Vision,
    /// Audio processing
    Audio,
    /// Code generation/completion
    Code,
}

impl ModelCapability {
    /// Check if this is an embedding capability
    pub fn is_embedding(&self) -> bool {
        matches!(self, Self::Embedding)
    }

    /// Check if this is a text generation capability
    pub fn is_text_generation(&self) -> bool {
        matches!(self, Self::Chat | Self::TextGeneration | Self::Code)
    }
}

/// Unified model information across all backends
///
/// This type provides a consistent view of models regardless of their source
/// (local files, Ollama, OpenAI, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedModelInfo {
    /// Model identifier (name or path)
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Model capabilities
    pub capabilities: Vec<ModelCapability>,
    /// Source backend
    pub backend: BackendType,
    /// Size in bytes (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Embedding dimensions (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<usize>,
    /// Additional metadata
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Default for UnifiedModelInfo {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            capabilities: Vec::new(),
            backend: BackendType::Mock,
            size_bytes: None,
            dimensions: None,
            metadata: HashMap::new(),
        }
    }
}

impl UnifiedModelInfo {
    /// Create a new model info with minimal fields
    pub fn new(id: impl Into<String>, backend: BackendType) -> Self {
        let id = id.into();
        Self {
            name: id.clone(),
            id,
            backend,
            ..Default::default()
        }
    }

    /// Add capabilities
    pub fn with_capabilities(
        mut self,
        capabilities: impl IntoIterator<Item = ModelCapability>,
    ) -> Self {
        self.capabilities = capabilities.into_iter().collect();
        self
    }

    /// Add embedding capability with dimensions
    pub fn with_embedding(mut self, dimensions: usize) -> Self {
        if !self.capabilities.contains(&ModelCapability::Embedding) {
            self.capabilities.push(ModelCapability::Embedding);
        }
        self.dimensions = Some(dimensions);
        self
    }

    /// Add chat capability
    pub fn with_chat(mut self) -> Self {
        if !self.capabilities.contains(&ModelCapability::Chat) {
            self.capabilities.push(ModelCapability::Chat);
        }
        self
    }

    /// Set size in bytes
    pub fn with_size(mut self, size_bytes: u64) -> Self {
        self.size_bytes = Some(size_bytes);
        self
    }

    /// Check if model supports embedding
    pub fn supports_embedding(&self) -> bool {
        self.capabilities.contains(&ModelCapability::Embedding)
    }

    /// Check if model supports chat
    pub fn supports_chat(&self) -> bool {
        self.capabilities.contains(&ModelCapability::Chat)
    }
}

/// Extended provider capabilities including embedding support
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtendedCapabilities {
    /// Base LLM capabilities
    #[serde(flatten)]
    pub llm: ProviderCapabilities,

    // === Embedding capabilities ===
    /// Supports text embeddings
    pub embeddings: bool,
    /// Supports batch embeddings
    pub embeddings_batch: bool,
    /// Embedding dimensions (if known)
    pub embedding_dimensions: Option<usize>,
    /// Maximum texts per batch
    pub max_batch_size: Option<usize>,
}

impl ExtendedCapabilities {
    /// Create capabilities for an embedding-only provider
    pub fn embedding_only(dimensions: usize) -> Self {
        Self {
            llm: ProviderCapabilities::default(),
            embeddings: true,
            embeddings_batch: true,
            embedding_dimensions: Some(dimensions),
            max_batch_size: Some(16),
        }
    }

    /// Create capabilities for a chat-only provider
    pub fn chat_only() -> Self {
        Self {
            llm: ProviderCapabilities {
                chat_completion: true,
                streaming: true,
                tool_use: true,
                ..Default::default()
            },
            embeddings: false,
            embeddings_batch: false,
            embedding_dimensions: None,
            max_batch_size: None,
        }
    }

    /// Create capabilities for a full provider (embeddings + chat)
    pub fn full(dimensions: usize) -> Self {
        Self {
            llm: ProviderCapabilities {
                chat_completion: true,
                streaming: true,
                tool_use: true,
                ..Default::default()
            },
            embeddings: true,
            embeddings_batch: true,
            embedding_dimensions: Some(dimensions),
            max_batch_size: Some(16),
        }
    }
}

/// Base trait for all providers
///
/// This trait defines the common interface shared by all providers,
/// regardless of their specific capabilities.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Get the provider name (e.g., "ollama-local", "openai-prod")
    fn name(&self) -> &str;

    /// Get the backend type
    fn backend_type(&self) -> BackendType;

    /// Get the API endpoint (if applicable)
    fn endpoint(&self) -> Option<&str>;

    /// Get extended capabilities including embedding support
    fn capabilities(&self) -> ExtendedCapabilities;

    /// Check if the provider is healthy/reachable
    async fn health_check(&self) -> BackendResult<bool>;
}

/// Extension trait for providers that support text embeddings
///
/// Providers implementing this trait can generate vector embeddings
/// from text input. This is used for semantic search and similarity.
#[async_trait]
pub trait CanEmbed: Provider {
    /// Generate embedding for a single text
    async fn embed(&self, text: &str) -> BackendResult<EmbeddingResponse>;

    /// Generate embeddings for multiple texts (batch operation)
    ///
    /// The default implementation calls `embed` for each text sequentially.
    /// Providers should override this for better performance.
    async fn embed_batch(&self, texts: Vec<String>) -> BackendResult<Vec<EmbeddingResponse>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(&text).await?);
        }
        Ok(results)
    }

    /// Get the embedding dimensions for this provider
    fn embedding_dimensions(&self) -> usize;

    /// Get the embedding model name
    fn embedding_model(&self) -> &str;
}

/// Extension trait for providers that support chat completion
///
/// Providers implementing this trait can generate chat completions,
/// including streaming and tool calling support.
#[async_trait]
pub trait CanChat: Provider {
    /// Generate a chat completion
    async fn chat(&self, request: ChatCompletionRequest) -> BackendResult<ChatCompletionResponse>;

    /// Generate a streaming chat completion
    fn chat_stream<'a>(
        &'a self,
        request: ChatCompletionRequest,
    ) -> BoxStream<'a, BackendResult<ChatCompletionChunk>>;

    /// Get the default chat model name
    fn chat_model(&self) -> &str;
}

/// Schema format for constrained generation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaFormat {
    /// GBNF grammar (llama.cpp)
    Gbnf,
    /// JSON Schema (OpenAI, Anthropic)
    JsonSchema,
    /// Regex pattern
    Regex,
}

/// Request for constrained generation
#[derive(Debug, Clone)]
pub struct ConstrainedRequest {
    /// The input prompt
    pub prompt: String,
    /// Schema/grammar content
    pub schema: String,
    /// Schema format
    pub format: SchemaFormat,
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// Temperature (0.0 = greedy)
    pub temperature: Option<f32>,
    /// Stop sequences
    pub stop: Option<Vec<String>>,
}

impl ConstrainedRequest {
    /// Create a new GBNF-constrained request
    pub fn gbnf(prompt: impl Into<String>, grammar: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            schema: grammar.into(),
            format: SchemaFormat::Gbnf,
            max_tokens: None,
            temperature: None,
            stop: None,
        }
    }

    /// Create a new JSON Schema-constrained request
    pub fn json_schema(prompt: impl Into<String>, schema: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            schema: schema.into(),
            format: SchemaFormat::JsonSchema,
            max_tokens: None,
            temperature: None,
            stop: None,
        }
    }

    /// Set maximum tokens
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }
}

/// Response from constrained generation
#[derive(Debug, Clone)]
pub struct ConstrainedResponse {
    /// Generated text (guaranteed to match schema)
    pub text: String,
    /// Token count
    pub tokens: u32,
    /// Whether generation was truncated
    pub truncated: bool,
}

/// Extension trait for providers that support constrained/structured generation
///
/// Providers implementing this trait can constrain output to match a grammar
/// or schema. Different backends support different formats:
/// - llama.cpp: GBNF grammars
/// - OpenAI: JSON Schema via response_format
/// - Anthropic: Tool use for structured output
#[async_trait]
pub trait CanConstrainGeneration: Provider {
    /// Get the supported schema formats
    fn supported_formats(&self) -> Vec<SchemaFormat>;

    /// Check if a specific format is supported
    fn supports_format(&self, format: SchemaFormat) -> bool {
        self.supported_formats().contains(&format)
    }

    /// Generate text constrained by a schema/grammar
    async fn generate_constrained(
        &self,
        request: ConstrainedRequest,
    ) -> BackendResult<ConstrainedResponse>;
}

/// Marker trait for providers that support both embeddings and chat
///
/// This is automatically implemented for any type that implements
/// both `CanEmbed` and `CanChat`.
pub trait FullProvider: CanEmbed + CanChat {}

// Blanket implementation: anything with both capabilities is a FullProvider
impl<T: CanEmbed + CanChat> FullProvider for T {}

/// Dynamic provider handle that can be queried for capabilities
///
/// This is useful when you have a `Box<dyn Provider>` and want to
/// check if it supports specific capabilities at runtime.
pub trait ProviderExt: Provider {
    /// Try to get this provider as an embedding provider
    fn as_embedding_provider(&self) -> Option<&dyn CanEmbed>;

    /// Try to get this provider as a chat provider
    fn as_chat_provider(&self) -> Option<&dyn CanChat>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extended_capabilities_embedding_only() {
        let caps = ExtendedCapabilities::embedding_only(768);
        assert!(caps.embeddings);
        assert!(caps.embeddings_batch);
        assert_eq!(caps.embedding_dimensions, Some(768));
        assert!(!caps.llm.chat_completion);
    }

    #[test]
    fn test_extended_capabilities_chat_only() {
        let caps = ExtendedCapabilities::chat_only();
        assert!(!caps.embeddings);
        assert!(caps.llm.chat_completion);
        assert!(caps.llm.streaming);
        assert!(caps.llm.tool_use);
    }

    #[test]
    fn test_extended_capabilities_full() {
        let caps = ExtendedCapabilities::full(1536);
        assert!(caps.embeddings);
        assert!(caps.llm.chat_completion);
        assert_eq!(caps.embedding_dimensions, Some(1536));
    }

    #[test]
    fn test_model_capability_checks() {
        assert!(ModelCapability::Embedding.is_embedding());
        assert!(!ModelCapability::Chat.is_embedding());

        assert!(ModelCapability::Chat.is_text_generation());
        assert!(ModelCapability::TextGeneration.is_text_generation());
        assert!(ModelCapability::Code.is_text_generation());
        assert!(!ModelCapability::Embedding.is_text_generation());
    }

    #[test]
    fn test_unified_model_info_builder() {
        let model = UnifiedModelInfo::new("nomic-embed-text", BackendType::Ollama)
            .with_embedding(768)
            .with_size(500_000_000);

        assert_eq!(model.id, "nomic-embed-text");
        assert_eq!(model.name, "nomic-embed-text");
        assert!(model.supports_embedding());
        assert!(!model.supports_chat());
        assert_eq!(model.dimensions, Some(768));
        assert_eq!(model.size_bytes, Some(500_000_000));
    }

    #[test]
    fn test_unified_model_info_chat() {
        let model = UnifiedModelInfo::new("llama3.2", BackendType::Ollama).with_chat();

        assert!(model.supports_chat());
        assert!(!model.supports_embedding());
    }

    #[test]
    fn test_unified_model_info_with_capabilities() {
        let model = UnifiedModelInfo::new("gpt-4o", BackendType::OpenAI)
            .with_capabilities([ModelCapability::Chat, ModelCapability::Vision]);

        assert!(model.supports_chat());
        assert!(model.capabilities.contains(&ModelCapability::Vision));
    }
}
