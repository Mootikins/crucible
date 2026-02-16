//! Provider capability types and model information
//!
//! This module defines types for describing provider capabilities and models.
//! The canonical embedding trait is [`EmbeddingProvider`](crate::enrichment::EmbeddingProvider)
//! in crucible-core::enrichment.
//!
//! Chat completions are handled by [`CompletionBackend`](super::CompletionBackend).

use crucible_config::BackendType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::llm::ProviderCapabilities;

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
/// This is distinct from [`crate::traits::llm::ModelFeature`] which describes
/// **features** of text/chat models (function calling, streaming, JSON mode, etc.).
///
/// # When to Use
/// - Use this enum when categorizing models by their primary function
/// - Use `llm::ModelFeature` when describing chat model feature sets
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
