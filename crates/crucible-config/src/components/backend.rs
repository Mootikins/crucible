//! Unified backend type for all providers
//!
//! This module defines the `BackendType` enum that represents all supported
//! provider backends for both embeddings and chat. This unifies the previously
//! separate embedding and LLM provider type enums (both now removed).

use super::trust::TrustLevel;
use serde::{Deserialize, Serialize};

/// Unified backend type for all providers.
///
/// Backends are the underlying services that provide AI capabilities.
/// Some backends support only embeddings, some only chat, and some support both.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
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

    /// Get the default trust level for this backend
    pub fn default_trust_level(&self) -> TrustLevel {
        match self {
            Self::FastEmbed | Self::Burn | Self::Mock => TrustLevel::Local,
            _ => TrustLevel::Cloud,
        }
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

    /// Get the environment variable name for this backend's API key
    pub fn api_key_env_var(&self) -> Option<&'static str> {
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

impl std::str::FromStr for BackendType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            // Multi-capability backends
            "ollama" => Ok(BackendType::Ollama),
            "openai" => Ok(BackendType::OpenAI),
            "anthropic" => Ok(BackendType::Anthropic),
            "cohere" => Ok(BackendType::Cohere),
            "vertexai" => Ok(BackendType::VertexAI),
            // Embedding-only backends
            "fastembed" => Ok(BackendType::FastEmbed),
            "burn" => Ok(BackendType::Burn),
            // Chat-only backends with aliases
            "github-copilot" | "github_copilot" | "copilot" | "githubcopilot" => {
                Ok(BackendType::GitHubCopilot)
            }
            "openrouter" | "open_router" | "open-router" => Ok(BackendType::OpenRouter),
            "zai" | "z.ai" | "z_ai" => Ok(BackendType::ZAI),
            // Utility backends
            "custom" => Ok(BackendType::Custom),
            "mock" => Ok(BackendType::Mock),
            other => Err(format!("Unknown backend: {}", other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_from_str_all_12_variants() {
        let variants = [
            ("ollama", BackendType::Ollama),
            ("openai", BackendType::OpenAI),
            ("anthropic", BackendType::Anthropic),
            ("cohere", BackendType::Cohere),
            ("vertexai", BackendType::VertexAI),
            ("fastembed", BackendType::FastEmbed),
            ("burn", BackendType::Burn),
            ("githubcopilot", BackendType::GitHubCopilot),
            ("openrouter", BackendType::OpenRouter),
            ("zai", BackendType::ZAI),
            ("custom", BackendType::Custom),
            ("mock", BackendType::Mock),
        ];

        for (s, expected) in &variants {
            let parsed = BackendType::from_str(s).expect(&format!("Failed to parse: {}", s));
            assert_eq!(parsed, *expected, "Mismatch for: {}", s);
        }
    }

    #[test]
    fn test_from_str_case_insensitive() {
        assert_eq!(
            BackendType::from_str("OLLAMA").unwrap(),
            BackendType::Ollama
        );
        assert_eq!(
            BackendType::from_str("OpenAI").unwrap(),
            BackendType::OpenAI
        );
        assert_eq!(
            BackendType::from_str("ANTHROPIC").unwrap(),
            BackendType::Anthropic
        );
    }

    #[test]
    fn test_from_str_github_copilot_aliases() {
        let aliases = [
            "github-copilot",
            "github_copilot",
            "copilot",
            "githubcopilot",
        ];
        for alias in &aliases {
            let parsed =
                BackendType::from_str(alias).expect(&format!("Failed to parse alias: {}", alias));
            assert_eq!(
                parsed,
                BackendType::GitHubCopilot,
                "Alias mismatch: {}",
                alias
            );
        }
    }

    #[test]
    fn test_from_str_openrouter_aliases() {
        let aliases = ["openrouter", "open_router", "open-router"];
        for alias in &aliases {
            let parsed =
                BackendType::from_str(alias).expect(&format!("Failed to parse alias: {}", alias));
            assert_eq!(parsed, BackendType::OpenRouter, "Alias mismatch: {}", alias);
        }
    }

    #[test]
    fn test_from_str_zai_aliases() {
        let aliases = ["zai", "z.ai", "z_ai"];
        for alias in &aliases {
            let parsed =
                BackendType::from_str(alias).expect(&format!("Failed to parse alias: {}", alias));
            assert_eq!(parsed, BackendType::ZAI, "Alias mismatch: {}", alias);
        }
    }

    #[test]
    fn test_from_str_unknown_returns_error() {
        assert!(BackendType::from_str("unknown").is_err());
        assert!(BackendType::from_str("invalid").is_err());
        assert!(BackendType::from_str("").is_err());
    }

    #[test]
    fn test_from_str_roundtrip_all_variants() {
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

        for variant in &all_variants {
            let s = variant.as_str();
            let parsed = BackendType::from_str(s)
                .expect(&format!("Failed to parse roundtrip for: {:?}", variant));
            assert_eq!(parsed, *variant, "Roundtrip failed for {:?}", variant);
        }
    }

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
    fn test_api_key_env_var() {
        assert_eq!(
            BackendType::OpenAI.api_key_env_var(),
            Some("OPENAI_API_KEY")
        );
        assert_eq!(
            BackendType::Anthropic.api_key_env_var(),
            Some("ANTHROPIC_API_KEY")
        );
        assert_eq!(
            BackendType::OpenRouter.api_key_env_var(),
            Some("OPENROUTER_API_KEY")
        );
        assert_eq!(BackendType::ZAI.api_key_env_var(), Some("GLM_AUTH_TOKEN"));

        assert_eq!(BackendType::GitHubCopilot.api_key_env_var(), None);
        assert_eq!(BackendType::Ollama.api_key_env_var(), None);
        assert_eq!(BackendType::FastEmbed.api_key_env_var(), None);
    }

    #[test]
    fn test_api_key_env_var_method() {
        assert_eq!(
            BackendType::OpenAI.api_key_env_var(),
            Some("OPENAI_API_KEY")
        );
        assert_eq!(
            BackendType::Anthropic.api_key_env_var(),
            Some("ANTHROPIC_API_KEY")
        );
        assert_eq!(
            BackendType::Cohere.api_key_env_var(),
            Some("COHERE_API_KEY")
        );
        assert_eq!(
            BackendType::VertexAI.api_key_env_var(),
            Some("GOOGLE_API_KEY")
        );
        assert_eq!(
            BackendType::OpenRouter.api_key_env_var(),
            Some("OPENROUTER_API_KEY")
        );
        assert_eq!(BackendType::ZAI.api_key_env_var(), Some("GLM_AUTH_TOKEN"));

        assert_eq!(BackendType::Ollama.api_key_env_var(), None);
        assert_eq!(BackendType::FastEmbed.api_key_env_var(), None);
        assert_eq!(BackendType::Burn.api_key_env_var(), None);
        assert_eq!(BackendType::GitHubCopilot.api_key_env_var(), None);
        assert_eq!(BackendType::Custom.api_key_env_var(), None);
        assert_eq!(BackendType::Mock.api_key_env_var(), None);
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

    #[test]
    fn test_default_trust_level_local_backends() {
        // Local backends should default to Local trust level
        assert_eq!(
            BackendType::FastEmbed.default_trust_level(),
            crate::components::trust::TrustLevel::Local
        );
        assert_eq!(
            BackendType::Burn.default_trust_level(),
            crate::components::trust::TrustLevel::Local
        );
        assert_eq!(
            BackendType::Mock.default_trust_level(),
            crate::components::trust::TrustLevel::Local
        );
    }

    #[test]
    fn test_default_trust_level_cloud_backends() {
        // All other backends should default to Cloud trust level
        assert_eq!(
            BackendType::Ollama.default_trust_level(),
            crate::components::trust::TrustLevel::Cloud
        );
        assert_eq!(
            BackendType::OpenAI.default_trust_level(),
            crate::components::trust::TrustLevel::Cloud
        );
        assert_eq!(
            BackendType::Anthropic.default_trust_level(),
            crate::components::trust::TrustLevel::Cloud
        );
        assert_eq!(
            BackendType::Cohere.default_trust_level(),
            crate::components::trust::TrustLevel::Cloud
        );
        assert_eq!(
            BackendType::VertexAI.default_trust_level(),
            crate::components::trust::TrustLevel::Cloud
        );
        assert_eq!(
            BackendType::GitHubCopilot.default_trust_level(),
            crate::components::trust::TrustLevel::Cloud
        );
        assert_eq!(
            BackendType::OpenRouter.default_trust_level(),
            crate::components::trust::TrustLevel::Cloud
        );
        assert_eq!(
            BackendType::ZAI.default_trust_level(),
            crate::components::trust::TrustLevel::Cloud
        );
        assert_eq!(
            BackendType::Custom.default_trust_level(),
            crate::components::trust::TrustLevel::Cloud
        );
    }
}
