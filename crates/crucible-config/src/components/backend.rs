//! Unified backend type for all providers
//!
//! This module defines the `BackendType` enum that represents all supported
//! provider backends for both embeddings and chat. This unifies the previously
//! separate `EmbeddingProviderType` and `LlmProviderType` enums.

use serde::{Deserialize, Serialize};

/// Unified backend type for all providers.
///
/// Backends are the underlying services that provide AI capabilities.
/// Some backends support only embeddings, some only chat, and some support both.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum BackendType {
    // === Multi-capability backends (embeddings + chat) ===
    /// Ollama - local or remote, supports both embeddings and chat
    Ollama,
    /// OpenAI API - supports both embeddings and chat
    #[serde(rename = "openai")]
    OpenAI,
    /// Anthropic API - chat only (no embedding support)
    Anthropic,
    /// Cohere API - supports both embeddings and chat
    Cohere,
    /// Google Vertex AI - supports both embeddings and chat
    #[serde(rename = "vertexai")]
    VertexAI,

    // === Embedding-only backends ===
    /// FastEmbed - local CPU-based embeddings
    #[serde(rename = "fastembed")]
    FastEmbed,
    /// Burn - local GPU-accelerated embeddings via Burn ML framework
    Burn,
    /// LlamaCpp - local GPU-accelerated embeddings via llama.cpp
    #[serde(rename = "llamacpp")]
    LlamaCpp,

    // === Utility backends ===
    /// Custom HTTP-based provider
    Custom,
    /// Mock provider for testing
    Mock,
}

impl BackendType {
    /// Whether this backend supports embeddings
    pub fn supports_embeddings(&self) -> bool {
        !matches!(self, Self::Anthropic)
    }

    /// Whether this backend supports chat
    pub fn supports_chat(&self) -> bool {
        matches!(
            self,
            Self::Ollama
                | Self::OpenAI
                | Self::Anthropic
                | Self::Cohere
                | Self::VertexAI
                | Self::Custom
        )
    }

    /// Whether this backend is local (no remote API calls)
    pub fn is_local(&self) -> bool {
        matches!(
            self,
            Self::FastEmbed | Self::Burn | Self::LlamaCpp | Self::Mock
        )
    }

    /// Whether this backend requires an API key
    pub fn requires_api_key(&self) -> bool {
        matches!(
            self,
            Self::OpenAI | Self::Anthropic | Self::Cohere | Self::VertexAI
        )
    }

    /// Get the backend type as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
            Self::Cohere => "cohere",
            Self::VertexAI => "vertexai",
            Self::FastEmbed => "fastembed",
            Self::Burn => "burn",
            Self::LlamaCpp => "llamacpp",
            Self::Custom => "custom",
            Self::Mock => "mock",
        }
    }

    /// Get the default endpoint for this backend
    pub fn default_endpoint(&self) -> Option<&'static str> {
        match self {
            Self::Ollama => Some("http://localhost:11434"),
            Self::OpenAI => Some("https://api.openai.com/v1"),
            Self::Anthropic => Some("https://api.anthropic.com/v1"),
            Self::Cohere => Some("https://api.cohere.ai/v1"),
            Self::VertexAI => Some("https://aiplatform.googleapis.com"),
            Self::FastEmbed => None, // Local, no endpoint
            Self::Burn => None,      // Local, no endpoint
            Self::LlamaCpp => None,  // Local, no endpoint
            Self::Custom => None,    // User must specify
            Self::Mock => None,      // Local, no endpoint
        }
    }

    /// Get default embedding model for this backend (if supported)
    pub fn default_embedding_model(&self) -> Option<&'static str> {
        match self {
            Self::Ollama => Some("nomic-embed-text"),
            Self::OpenAI => Some("text-embedding-3-small"),
            Self::Cohere => Some("embed-english-v3.0"),
            Self::VertexAI => Some("textembedding-gecko@003"),
            Self::FastEmbed => Some("BAAI/bge-small-en-v1.5"),
            Self::Burn => Some("nomic-embed-text"),
            Self::LlamaCpp => Some("nomic-embed-text-v1.5.Q8_0.gguf"),
            Self::Custom => None, // User must specify
            Self::Mock => Some("mock-embed-model"),
            Self::Anthropic => None, // No embedding support
        }
    }

    /// Get default chat model for this backend (if supported)
    pub fn default_chat_model(&self) -> Option<&'static str> {
        match self {
            Self::Ollama => Some("llama3.2"),
            Self::OpenAI => Some("gpt-4o"),
            Self::Anthropic => Some("claude-3-5-sonnet-20241022"),
            Self::Cohere => Some("command-r-plus"),
            Self::VertexAI => Some("gemini-1.5-pro"),
            Self::Custom => None,    // User must specify
            Self::FastEmbed => None, // No chat support
            Self::Burn => None,      // No chat support
            Self::LlamaCpp => None,  // No chat support (embedding-only)
            Self::Mock => Some("mock-chat-model"),
        }
    }

    /// Get default max concurrent requests for this backend
    pub fn default_max_concurrent(&self) -> usize {
        match self {
            Self::Ollama => 1,                               // Single GPU, sequential
            Self::Burn => 1,                                 // GPU-bound
            Self::LlamaCpp => 1,                             // GPU-bound
            Self::FastEmbed => (num_cpus::get() / 2).max(1), // CPU-bound
            Self::OpenAI | Self::Anthropic | Self::Cohere | Self::VertexAI => 8, // Rate-limited
            Self::Mock => 16,                                // Testing
            Self::Custom => 4,                               // Conservative
        }
    }

    /// Get default environment variable name for API key
    pub fn default_api_key_env(&self) -> Option<&'static str> {
        match self {
            Self::OpenAI => Some("OPENAI_API_KEY"),
            Self::Anthropic => Some("ANTHROPIC_API_KEY"),
            Self::Cohere => Some("COHERE_API_KEY"),
            Self::VertexAI => Some("GOOGLE_API_KEY"),
            _ => None,
        }
    }
}

impl Default for BackendType {
    fn default() -> Self {
        Self::FastEmbed
    }
}

impl std::fmt::Display for BackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_detection() {
        // Ollama supports both
        assert!(BackendType::Ollama.supports_embeddings());
        assert!(BackendType::Ollama.supports_chat());

        // Anthropic is chat-only
        assert!(!BackendType::Anthropic.supports_embeddings());
        assert!(BackendType::Anthropic.supports_chat());

        // FastEmbed is embedding-only
        assert!(BackendType::FastEmbed.supports_embeddings());
        assert!(!BackendType::FastEmbed.supports_chat());
    }

    #[test]
    fn test_local_detection() {
        assert!(BackendType::FastEmbed.is_local());
        assert!(BackendType::Burn.is_local());
        assert!(BackendType::LlamaCpp.is_local());
        assert!(BackendType::Mock.is_local());

        assert!(!BackendType::OpenAI.is_local());
        assert!(!BackendType::Ollama.is_local()); // Ollama can be remote
    }

    #[test]
    fn test_api_key_requirements() {
        assert!(BackendType::OpenAI.requires_api_key());
        assert!(BackendType::Anthropic.requires_api_key());
        assert!(!BackendType::Ollama.requires_api_key());
        assert!(!BackendType::FastEmbed.requires_api_key());
    }

    #[test]
    fn test_serde_roundtrip() {
        let backend = BackendType::OpenAI;
        let json = serde_json::to_string(&backend).unwrap();
        assert_eq!(json, r#""openai""#);

        let parsed: BackendType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, BackendType::OpenAI);
    }
}
