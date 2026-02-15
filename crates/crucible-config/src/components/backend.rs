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
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
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
    #[default]
    #[serde(rename = "fastembed")]
    FastEmbed,
    /// Burn - local GPU-accelerated embeddings via Burn ML framework
    Burn,
    // === Chat-only backends ===
    /// GitHub Copilot (via VS Code OAuth flow)
    #[serde(alias = "github-copilot", alias = "github_copilot", alias = "copilot")]
    GitHubCopilot,
    /// OpenRouter meta-provider
    #[serde(alias = "openrouter", alias = "open_router", alias = "open-router")]
    OpenRouter,
    /// Z.AI provider (GLM Coding Plan)
    #[serde(alias = "z.ai", alias = "z_ai", alias = "zai")]
    ZAI,

    // === Utility backends ===
    /// Custom HTTP-based provider
    Custom,
    /// Mock provider for testing
    Mock,
}

impl BackendType {
    /// Whether this backend supports embeddings
    pub fn supports_embeddings(&self) -> bool {
        matches!(
            self,
            Self::Ollama
                | Self::OpenAI
                | Self::Cohere
                | Self::VertexAI
                | Self::FastEmbed
                | Self::Burn
                | Self::Custom
                | Self::Mock
        )
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
                | Self::GitHubCopilot
                | Self::OpenRouter
                | Self::ZAI
                | Self::Custom
        )
    }

    /// Whether this backend is local (no remote API calls)
    pub fn is_local(&self) -> bool {
        matches!(self, Self::FastEmbed | Self::Burn | Self::Mock)
    }

    /// Whether this backend requires an API key
    pub fn requires_api_key(&self) -> bool {
        matches!(
            self,
            Self::OpenAI
                | Self::Anthropic
                | Self::Cohere
                | Self::VertexAI
                | Self::OpenRouter
                | Self::ZAI
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
            Self::GitHubCopilot => "github-copilot",
            Self::OpenRouter => "openrouter",
            Self::ZAI => "zai",
            Self::Custom => "custom",
            Self::Mock => "mock",
        }
    }

    /// Get the default endpoint for this backend
    pub fn default_endpoint(&self) -> Option<&'static str> {
        match self {
            Self::Ollama => Some(super::defaults::DEFAULT_OLLAMA_ENDPOINT),
            Self::OpenAI => Some(super::defaults::DEFAULT_OPENAI_ENDPOINT),
            Self::Anthropic => Some(super::defaults::DEFAULT_ANTHROPIC_ENDPOINT),
            Self::Cohere => Some("https://api.cohere.ai/v1"),
            Self::VertexAI => Some("https://aiplatform.googleapis.com"),
            Self::GitHubCopilot => Some(super::defaults::DEFAULT_GITHUB_COPILOT_ENDPOINT),
            Self::OpenRouter => Some(super::defaults::DEFAULT_OPENROUTER_ENDPOINT),
            Self::ZAI => Some(super::defaults::DEFAULT_ZAI_ENDPOINT),
            Self::FastEmbed => None,
            Self::Burn => None,
            Self::Custom => None,
            Self::Mock => None,
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
            Self::Custom => None,
            Self::Mock => Some("mock-embed-model"),
            Self::Anthropic => None,
            Self::GitHubCopilot => None,
            Self::OpenRouter => None,
            Self::ZAI => None,
        }
    }

    /// Get default chat model for this backend (if supported)
    pub fn default_chat_model(&self) -> Option<&'static str> {
        match self {
            Self::Ollama => Some(super::defaults::DEFAULT_CHAT_MODEL),
            Self::OpenAI => Some(super::defaults::DEFAULT_OPENAI_MODEL),
            Self::Anthropic => Some(super::defaults::DEFAULT_ANTHROPIC_MODEL),
            Self::Cohere => Some("command-r-plus"),
            Self::VertexAI => Some("gemini-1.5-pro"),
            Self::GitHubCopilot => Some(super::defaults::DEFAULT_GITHUB_COPILOT_MODEL),
            Self::OpenRouter => Some(super::defaults::DEFAULT_OPENROUTER_MODEL),
            Self::ZAI => Some(super::defaults::DEFAULT_ZAI_MODEL),
            Self::Custom => None,    // User must specify
            Self::FastEmbed => None, // No chat support
            Self::Burn => None,      // No chat support
            Self::Mock => Some("mock-chat-model"),
        }
    }

    /// Get default max concurrent requests for this backend
    pub fn default_max_concurrent(&self) -> usize {
        match self {
            Self::Ollama => 1,                               // Single GPU, sequential
            Self::Burn => 1,                                 // GPU-bound
            Self::FastEmbed => (num_cpus::get() / 2).max(1), // CPU-bound
            Self::OpenAI
            | Self::Anthropic
            | Self::Cohere
            | Self::VertexAI
            | Self::GitHubCopilot
            | Self::OpenRouter
            | Self::ZAI => 8, // Rate-limited
            Self::Mock => 16,                                // Testing
            Self::Custom => 4,                               // Conservative
        }
    }

    /// Get default environment variable name for API key
    pub fn default_api_key(&self) -> Option<&'static str> {
        match self {
            Self::OpenAI => Some("OPENAI_API_KEY"),
            Self::Anthropic => Some("ANTHROPIC_API_KEY"),
            Self::Cohere => Some("COHERE_API_KEY"),
            Self::VertexAI => Some("GOOGLE_API_KEY"),
            Self::OpenRouter => Some("OPENROUTER_API_KEY"),
            Self::ZAI => Some("GLM_AUTH_TOKEN"),
            Self::Ollama
            | Self::FastEmbed
            | Self::Burn
            | Self::GitHubCopilot
            | Self::Custom
            | Self::Mock => None,
        }
    }
}

impl std::fmt::Display for BackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[allow(deprecated)]
impl From<super::llm::LlmProviderType> for BackendType {
    fn from(provider: super::llm::LlmProviderType) -> Self {
        match provider {
            super::llm::LlmProviderType::Ollama => BackendType::Ollama,
            super::llm::LlmProviderType::OpenAI => BackendType::OpenAI,
            super::llm::LlmProviderType::Anthropic => BackendType::Anthropic,
            super::llm::LlmProviderType::GitHubCopilot => BackendType::GitHubCopilot,
            super::llm::LlmProviderType::OpenRouter => BackendType::OpenRouter,
            super::llm::LlmProviderType::ZAI => BackendType::ZAI,
        }
    }
}

#[allow(deprecated)]
impl TryFrom<BackendType> for super::llm::LlmProviderType {
    type Error = String;

    fn try_from(backend: BackendType) -> Result<Self, Self::Error> {
        match backend {
            BackendType::Ollama => Ok(super::llm::LlmProviderType::Ollama),
            BackendType::OpenAI => Ok(super::llm::LlmProviderType::OpenAI),
            BackendType::Anthropic => Ok(super::llm::LlmProviderType::Anthropic),
            BackendType::GitHubCopilot => Ok(super::llm::LlmProviderType::GitHubCopilot),
            BackendType::OpenRouter => Ok(super::llm::LlmProviderType::OpenRouter),
            BackendType::ZAI => Ok(super::llm::LlmProviderType::ZAI),
            other => Err(format!(
                "BackendType::{} has no corresponding LlmProviderType (not a chat-capable LLM backend)",
                other.as_str()
            )),
        }
    }
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::super::llm::LlmProviderType;
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

        // New chat-only backends
        assert!(!BackendType::GitHubCopilot.supports_embeddings());
        assert!(BackendType::GitHubCopilot.supports_chat());

        assert!(!BackendType::OpenRouter.supports_embeddings());
        assert!(BackendType::OpenRouter.supports_chat());

        assert!(!BackendType::ZAI.supports_embeddings());
        assert!(BackendType::ZAI.supports_chat());
    }

    #[test]
    fn test_local_detection() {
        assert!(BackendType::FastEmbed.is_local());
        assert!(BackendType::Burn.is_local());
        assert!(BackendType::Mock.is_local());

        assert!(!BackendType::OpenAI.is_local());
        assert!(!BackendType::Ollama.is_local()); // Ollama can be remote
        assert!(!BackendType::GitHubCopilot.is_local());
        assert!(!BackendType::OpenRouter.is_local());
        assert!(!BackendType::ZAI.is_local());
    }

    #[test]
    fn test_api_key_requirements() {
        assert!(BackendType::OpenAI.requires_api_key());
        assert!(BackendType::Anthropic.requires_api_key());
        assert!(BackendType::OpenRouter.requires_api_key());
        assert!(BackendType::ZAI.requires_api_key());

        assert!(!BackendType::Ollama.requires_api_key());
        assert!(!BackendType::FastEmbed.requires_api_key());
        assert!(!BackendType::GitHubCopilot.requires_api_key());
    }

    #[test]
    fn test_default_api_key_env_vars() {
        assert_eq!(
            BackendType::OpenAI.default_api_key(),
            Some("OPENAI_API_KEY")
        );
        assert_eq!(
            BackendType::Anthropic.default_api_key(),
            Some("ANTHROPIC_API_KEY")
        );
        assert_eq!(
            BackendType::OpenRouter.default_api_key(),
            Some("OPENROUTER_API_KEY")
        );
        assert_eq!(BackendType::ZAI.default_api_key(), Some("GLM_AUTH_TOKEN"));

        assert_eq!(BackendType::GitHubCopilot.default_api_key(), None);
        assert_eq!(BackendType::Ollama.default_api_key(), None);
        assert_eq!(BackendType::FastEmbed.default_api_key(), None);
    }

    #[test]
    fn test_new_variants_default_endpoints() {
        assert_eq!(
            BackendType::GitHubCopilot.default_endpoint(),
            Some("https://api.githubcopilot.com")
        );
        assert_eq!(
            BackendType::OpenRouter.default_endpoint(),
            Some("https://openrouter.ai/api/v1")
        );
        assert_eq!(
            BackendType::ZAI.default_endpoint(),
            Some("https://api.z.ai/api/coding/paas/v4")
        );
    }

    #[test]
    fn test_new_variants_default_chat_models() {
        assert_eq!(
            BackendType::GitHubCopilot.default_chat_model(),
            Some("gpt-4o")
        );
        assert_eq!(
            BackendType::OpenRouter.default_chat_model(),
            Some("openai/gpt-4o")
        );
        assert_eq!(BackendType::ZAI.default_chat_model(), Some("GLM-4.7"));
    }

    #[test]
    fn test_new_variants_no_embedding_models() {
        assert_eq!(BackendType::GitHubCopilot.default_embedding_model(), None);
        assert_eq!(BackendType::OpenRouter.default_embedding_model(), None);
        assert_eq!(BackendType::ZAI.default_embedding_model(), None);
    }

    #[test]
    fn test_new_variants_max_concurrent() {
        assert_eq!(BackendType::GitHubCopilot.default_max_concurrent(), 8);
        assert_eq!(BackendType::OpenRouter.default_max_concurrent(), 8);
        assert_eq!(BackendType::ZAI.default_max_concurrent(), 8);
    }

    #[test]
    fn test_new_variants_as_str() {
        assert_eq!(BackendType::GitHubCopilot.as_str(), "github-copilot");
        assert_eq!(BackendType::OpenRouter.as_str(), "openrouter");
        assert_eq!(BackendType::ZAI.as_str(), "zai");
    }

    #[test]
    fn test_serde_roundtrip() {
        let backend = BackendType::OpenAI;
        let json = serde_json::to_string(&backend).unwrap();
        assert_eq!(json, r#""openai""#);

        let parsed: BackendType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, BackendType::OpenAI);
    }

    #[test]
    fn test_serde_roundtrip_all_variants() {
        let variants = [
            (BackendType::Ollama, "ollama"),
            (BackendType::OpenAI, "openai"),
            (BackendType::Anthropic, "anthropic"),
            (BackendType::Cohere, "cohere"),
            (BackendType::VertexAI, "vertexai"),
            (BackendType::FastEmbed, "fastembed"),
            (BackendType::Burn, "burn"),
            (BackendType::GitHubCopilot, "githubcopilot"),
            (BackendType::OpenRouter, "openrouter"),
            (BackendType::ZAI, "zai"),
            (BackendType::Custom, "custom"),
            (BackendType::Mock, "mock"),
        ];

        for (variant, expected_serialized) in &variants {
            let json = serde_json::to_string(variant).unwrap();
            assert_eq!(
                json,
                format!("\"{}\"", expected_serialized),
                "Serialization mismatch for {:?}",
                variant
            );

            let parsed: BackendType = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, variant, "Roundtrip failed for {:?}", variant);
        }
    }

    #[test]
    fn test_serde_github_copilot_aliases() {
        let aliases = [
            r#""githubcopilot""#,  // canonical (rename_all = lowercase)
            r#""github-copilot""#, // kebab-case alias
            r#""github_copilot""#, // snake_case alias
            r#""copilot""#,        // short alias
        ];
        for alias in &aliases {
            let parsed: BackendType = serde_json::from_str(alias).unwrap();
            assert_eq!(
                parsed,
                BackendType::GitHubCopilot,
                "Failed to parse alias: {}",
                alias
            );
        }
    }

    #[test]
    fn test_serde_openrouter_aliases() {
        let aliases = [
            r#""openrouter""#,  // canonical (also alias)
            r#""open_router""#, // snake_case alias
            r#""open-router""#, // kebab-case alias
        ];
        for alias in &aliases {
            let parsed: BackendType = serde_json::from_str(alias).unwrap();
            assert_eq!(
                parsed,
                BackendType::OpenRouter,
                "Failed to parse alias: {}",
                alias
            );
        }
    }

    #[test]
    fn test_serde_zai_aliases() {
        let aliases = [
            r#""zai""#,  // canonical (also alias)
            r#""z.ai""#, // dot notation alias
            r#""z_ai""#, // snake_case alias
        ];
        for alias in &aliases {
            let parsed: BackendType = serde_json::from_str(alias).unwrap();
            assert_eq!(parsed, BackendType::ZAI, "Failed to parse alias: {}", alias);
        }
    }

    #[test]
    fn test_from_llm_provider_type() {
        assert_eq!(
            BackendType::from(LlmProviderType::Ollama),
            BackendType::Ollama
        );
        assert_eq!(
            BackendType::from(LlmProviderType::OpenAI),
            BackendType::OpenAI
        );
        assert_eq!(
            BackendType::from(LlmProviderType::Anthropic),
            BackendType::Anthropic
        );
        assert_eq!(
            BackendType::from(LlmProviderType::GitHubCopilot),
            BackendType::GitHubCopilot
        );
        assert_eq!(
            BackendType::from(LlmProviderType::OpenRouter),
            BackendType::OpenRouter
        );
        assert_eq!(BackendType::from(LlmProviderType::ZAI), BackendType::ZAI);
    }

    #[test]
    fn test_try_from_backend_type_succeeds_for_chat_backends() {
        let chat_mappings = [
            (BackendType::Ollama, LlmProviderType::Ollama),
            (BackendType::OpenAI, LlmProviderType::OpenAI),
            (BackendType::Anthropic, LlmProviderType::Anthropic),
            (BackendType::GitHubCopilot, LlmProviderType::GitHubCopilot),
            (BackendType::OpenRouter, LlmProviderType::OpenRouter),
            (BackendType::ZAI, LlmProviderType::ZAI),
        ];

        for (backend, expected_provider) in &chat_mappings {
            let result = LlmProviderType::try_from(backend.clone());
            assert_eq!(
                result,
                Ok(*expected_provider),
                "TryFrom failed for {:?}",
                backend
            );
        }
    }

    #[test]
    fn test_try_from_backend_type_fails_for_non_chat_backends() {
        let non_chat = [
            BackendType::FastEmbed,
            BackendType::Burn,
            BackendType::Cohere,
            BackendType::VertexAI,
            BackendType::Custom,
            BackendType::Mock,
        ];

        for backend in &non_chat {
            let result = LlmProviderType::try_from(backend.clone());
            assert!(result.is_err(), "TryFrom should fail for {:?}", backend);
        }
    }

    #[test]
    fn test_llm_provider_type_roundtrip_through_backend_type() {
        // Every LlmProviderType should survive: LlmProviderType -> BackendType -> LlmProviderType
        let all_providers = [
            LlmProviderType::Ollama,
            LlmProviderType::OpenAI,
            LlmProviderType::Anthropic,
            LlmProviderType::GitHubCopilot,
            LlmProviderType::OpenRouter,
            LlmProviderType::ZAI,
        ];

        for provider in &all_providers {
            let backend = BackendType::from(*provider);
            let roundtripped = LlmProviderType::try_from(backend).unwrap();
            assert_eq!(
                &roundtripped, provider,
                "Roundtrip failed for {:?}",
                provider
            );
        }
    }

    #[test]
    fn test_backend_type_has_exactly_12_variants() {
        // Ensure all variants are accounted for in as_str (exhaustive match)
        let all_variants = [
            BackendType::Ollama,
            BackendType::OpenAI,
            BackendType::Anthropic,
            BackendType::Cohere,
            BackendType::VertexAI,
            BackendType::FastEmbed,
            BackendType::Burn,
            BackendType::GitHubCopilot,
            BackendType::OpenRouter,
            BackendType::ZAI,
            BackendType::Custom,
            BackendType::Mock,
        ];
        assert_eq!(all_variants.len(), 12);

        // Every variant should have a non-empty as_str
        for variant in &all_variants {
            assert!(
                !variant.as_str().is_empty(),
                "{:?} has empty as_str()",
                variant
            );
        }
    }
}
