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

struct BackendMetadata {
    supports_embeddings: bool,
    supports_chat: bool,
    is_local: bool,
    default_trust_level: TrustLevel,
    requires_api_key: bool,
    api_key_env_var: Option<&'static str>,
    as_str: &'static str,
    default_endpoint: Option<&'static str>,
    default_embedding_model: Option<&'static str>,
    default_chat_model: Option<&'static str>,
}

impl BackendType {
    const OLLAMA_METADATA: BackendMetadata = BackendMetadata {
        supports_embeddings: true,
        supports_chat: true,
        is_local: false,
        default_trust_level: TrustLevel::Cloud,
        requires_api_key: false,
        api_key_env_var: None,
        as_str: "ollama",
        default_endpoint: Some(super::defaults::DEFAULT_OLLAMA_ENDPOINT),
        default_embedding_model: Some("nomic-embed-text"),
        default_chat_model: Some(super::defaults::DEFAULT_CHAT_MODEL),
    };

    const OPENAI_METADATA: BackendMetadata = BackendMetadata {
        supports_embeddings: true,
        supports_chat: true,
        is_local: false,
        default_trust_level: TrustLevel::Cloud,
        requires_api_key: true,
        api_key_env_var: Some("OPENAI_API_KEY"),
        as_str: "openai",
        default_endpoint: Some(super::defaults::DEFAULT_OPENAI_ENDPOINT),
        default_embedding_model: Some("text-embedding-3-small"),
        default_chat_model: Some(super::defaults::DEFAULT_OPENAI_MODEL),
    };

    const ANTHROPIC_METADATA: BackendMetadata = BackendMetadata {
        supports_embeddings: false,
        supports_chat: true,
        is_local: false,
        default_trust_level: TrustLevel::Cloud,
        requires_api_key: true,
        api_key_env_var: Some("ANTHROPIC_API_KEY"),
        as_str: "anthropic",
        default_endpoint: Some(super::defaults::DEFAULT_ANTHROPIC_ENDPOINT),
        default_embedding_model: None,
        default_chat_model: Some(super::defaults::DEFAULT_ANTHROPIC_MODEL),
    };

    const COHERE_METADATA: BackendMetadata = BackendMetadata {
        supports_embeddings: true,
        supports_chat: true,
        is_local: false,
        default_trust_level: TrustLevel::Cloud,
        requires_api_key: true,
        api_key_env_var: Some("COHERE_API_KEY"),
        as_str: "cohere",
        default_endpoint: Some("https://api.cohere.ai/v1"),
        default_embedding_model: Some("embed-english-v3.0"),
        default_chat_model: Some("command-r-plus"),
    };

    const VERTEX_AI_METADATA: BackendMetadata = BackendMetadata {
        supports_embeddings: true,
        supports_chat: true,
        is_local: false,
        default_trust_level: TrustLevel::Cloud,
        requires_api_key: true,
        api_key_env_var: Some("GOOGLE_API_KEY"),
        as_str: "vertexai",
        default_endpoint: Some("https://aiplatform.googleapis.com"),
        default_embedding_model: Some("textembedding-gecko@003"),
        default_chat_model: Some("gemini-1.5-pro"),
    };

    const FAST_EMBED_METADATA: BackendMetadata = BackendMetadata {
        supports_embeddings: true,
        supports_chat: false,
        is_local: true,
        default_trust_level: TrustLevel::Local,
        requires_api_key: false,
        api_key_env_var: None,
        as_str: "fastembed",
        default_endpoint: None,
        default_embedding_model: Some("BAAI/bge-small-en-v1.5"),
        default_chat_model: None,
    };

    const BURN_METADATA: BackendMetadata = BackendMetadata {
        supports_embeddings: true,
        supports_chat: false,
        is_local: true,
        default_trust_level: TrustLevel::Local,
        requires_api_key: false,
        api_key_env_var: None,
        as_str: "burn",
        default_endpoint: None,
        default_embedding_model: Some("nomic-embed-text"),
        default_chat_model: None,
    };

    const GITHUB_COPILOT_METADATA: BackendMetadata = BackendMetadata {
        supports_embeddings: false,
        supports_chat: true,
        is_local: false,
        default_trust_level: TrustLevel::Cloud,
        requires_api_key: false,
        api_key_env_var: None,
        as_str: "github-copilot",
        default_endpoint: Some(super::defaults::DEFAULT_GITHUB_COPILOT_ENDPOINT),
        default_embedding_model: None,
        default_chat_model: Some(super::defaults::DEFAULT_GITHUB_COPILOT_MODEL),
    };

    const OPENROUTER_METADATA: BackendMetadata = BackendMetadata {
        supports_embeddings: false,
        supports_chat: true,
        is_local: false,
        default_trust_level: TrustLevel::Cloud,
        requires_api_key: true,
        api_key_env_var: Some("OPENROUTER_API_KEY"),
        as_str: "openrouter",
        default_endpoint: Some(super::defaults::DEFAULT_OPENROUTER_ENDPOINT),
        default_embedding_model: None,
        default_chat_model: Some(super::defaults::DEFAULT_OPENROUTER_MODEL),
    };

    const ZAI_METADATA: BackendMetadata = BackendMetadata {
        supports_embeddings: false,
        supports_chat: true,
        is_local: false,
        default_trust_level: TrustLevel::Cloud,
        requires_api_key: true,
        api_key_env_var: Some("GLM_AUTH_TOKEN"),
        as_str: "zai",
        default_endpoint: Some(super::defaults::DEFAULT_ZAI_ENDPOINT),
        default_embedding_model: None,
        default_chat_model: Some(super::defaults::DEFAULT_ZAI_MODEL),
    };

    const CUSTOM_METADATA: BackendMetadata = BackendMetadata {
        supports_embeddings: true,
        supports_chat: true,
        is_local: false,
        default_trust_level: TrustLevel::Cloud,
        requires_api_key: false,
        api_key_env_var: None,
        as_str: "custom",
        default_endpoint: None,
        default_embedding_model: None,
        default_chat_model: None,
    };

    const MOCK_METADATA: BackendMetadata = BackendMetadata {
        supports_embeddings: true,
        supports_chat: false,
        is_local: true,
        default_trust_level: TrustLevel::Local,
        requires_api_key: false,
        api_key_env_var: None,
        as_str: "mock",
        default_endpoint: None,
        default_embedding_model: Some("mock-embed-model"),
        default_chat_model: Some("mock-chat-model"),
    };

    fn metadata(&self) -> &'static BackendMetadata {
        match self {
            Self::Ollama => &Self::OLLAMA_METADATA,
            Self::OpenAI => &Self::OPENAI_METADATA,
            Self::Anthropic => &Self::ANTHROPIC_METADATA,
            Self::Cohere => &Self::COHERE_METADATA,
            Self::VertexAI => &Self::VERTEX_AI_METADATA,
            Self::FastEmbed => &Self::FAST_EMBED_METADATA,
            Self::Burn => &Self::BURN_METADATA,
            Self::GitHubCopilot => &Self::GITHUB_COPILOT_METADATA,
            Self::OpenRouter => &Self::OPENROUTER_METADATA,
            Self::ZAI => &Self::ZAI_METADATA,
            Self::Custom => &Self::CUSTOM_METADATA,
            Self::Mock => &Self::MOCK_METADATA,
        }
    }

    /// Whether this backend supports embeddings
    pub fn supports_embeddings(&self) -> bool {
        self.metadata().supports_embeddings
    }

    /// Whether this backend supports chat
    pub fn supports_chat(&self) -> bool {
        self.metadata().supports_chat
    }

    /// Whether this backend is local (no remote API calls)
    pub fn is_local(&self) -> bool {
        self.metadata().is_local
    }

    /// Get the default trust level for this backend
    pub fn default_trust_level(&self) -> TrustLevel {
        self.metadata().default_trust_level
    }

    /// Whether this backend requires an API key
    pub fn requires_api_key(&self) -> bool {
        self.metadata().requires_api_key
    }

    /// Get the backend type as a string
    pub fn as_str(&self) -> &'static str {
        self.metadata().as_str
    }

    /// Get the default endpoint for this backend
    pub fn default_endpoint(&self) -> Option<&'static str> {
        self.metadata().default_endpoint
    }

    /// Get default embedding model for this backend (if supported)
    pub fn default_embedding_model(&self) -> Option<&'static str> {
        self.metadata().default_embedding_model
    }

    /// Get default chat model for this backend (if supported)
    pub fn default_chat_model(&self) -> Option<&'static str> {
        self.metadata().default_chat_model
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
        self.metadata().api_key_env_var
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
            let parsed =
                BackendType::from_str(s).unwrap_or_else(|_| panic!("Failed to parse: {}", s));
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
            let parsed = BackendType::from_str(alias)
                .unwrap_or_else(|_| panic!("Failed to parse alias: {}", alias));
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
            let parsed = BackendType::from_str(alias)
                .unwrap_or_else(|_| panic!("Failed to parse alias: {}", alias));
            assert_eq!(parsed, BackendType::OpenRouter, "Alias mismatch: {}", alias);
        }
    }

    #[test]
    fn test_from_str_zai_aliases() {
        let aliases = ["zai", "z.ai", "z_ai"];
        for alias in &aliases {
            let parsed = BackendType::from_str(alias)
                .unwrap_or_else(|_| panic!("Failed to parse alias: {}", alias));
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
                .unwrap_or_else(|_| panic!("Failed to parse roundtrip for: {:?}", variant));
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

    // ========================================================================
    // COMPREHENSIVE REGRESSION TESTS FOR ALL METHODS × ALL VARIANTS
    // ========================================================================
    // These tests lock in the exact values for all 12 variants across all 11 methods.
    // Total: 12 variants × 11 methods = 132+ assertions minimum.

    #[test]
    fn test_supports_embeddings_all_variants() {
        // Multi-capability backends (embeddings + chat)
        assert!(BackendType::Ollama.supports_embeddings());
        assert!(BackendType::OpenAI.supports_embeddings());
        assert!(BackendType::Cohere.supports_embeddings());
        assert!(BackendType::VertexAI.supports_embeddings());

        // Embedding-only backends
        assert!(BackendType::FastEmbed.supports_embeddings());
        assert!(BackendType::Burn.supports_embeddings());

        // Chat-only backends (no embeddings)
        assert!(!BackendType::Anthropic.supports_embeddings());
        assert!(!BackendType::GitHubCopilot.supports_embeddings());
        assert!(!BackendType::OpenRouter.supports_embeddings());
        assert!(!BackendType::ZAI.supports_embeddings());

        // Utility backends
        assert!(BackendType::Custom.supports_embeddings());
        assert!(BackendType::Mock.supports_embeddings());
    }

    #[test]
    fn test_supports_chat_all_variants() {
        // Multi-capability backends (embeddings + chat)
        assert!(BackendType::Ollama.supports_chat());
        assert!(BackendType::OpenAI.supports_chat());
        assert!(BackendType::Cohere.supports_chat());
        assert!(BackendType::VertexAI.supports_chat());

        // Chat-only backends
        assert!(BackendType::Anthropic.supports_chat());
        assert!(BackendType::GitHubCopilot.supports_chat());
        assert!(BackendType::OpenRouter.supports_chat());
        assert!(BackendType::ZAI.supports_chat());

        // Embedding-only backends (no chat)
        assert!(!BackendType::FastEmbed.supports_chat());
        assert!(!BackendType::Burn.supports_chat());

        // Utility backends
        assert!(BackendType::Custom.supports_chat());
        assert!(!BackendType::Mock.supports_chat());
    }

    #[test]
    fn test_is_local_all_variants() {
        // Local backends
        assert!(BackendType::FastEmbed.is_local());
        assert!(BackendType::Burn.is_local());
        assert!(BackendType::Mock.is_local());

        // Cloud backends
        assert!(!BackendType::Ollama.is_local());
        assert!(!BackendType::OpenAI.is_local());
        assert!(!BackendType::Anthropic.is_local());
        assert!(!BackendType::Cohere.is_local());
        assert!(!BackendType::VertexAI.is_local());
        assert!(!BackendType::GitHubCopilot.is_local());
        assert!(!BackendType::OpenRouter.is_local());
        assert!(!BackendType::ZAI.is_local());
        assert!(!BackendType::Custom.is_local());
    }

    #[test]
    fn test_requires_api_key_all_variants() {
        // Backends that require API key
        assert!(BackendType::OpenAI.requires_api_key());
        assert!(BackendType::Anthropic.requires_api_key());
        assert!(BackendType::Cohere.requires_api_key());
        assert!(BackendType::VertexAI.requires_api_key());
        assert!(BackendType::OpenRouter.requires_api_key());
        assert!(BackendType::ZAI.requires_api_key());

        // Backends that do NOT require API key
        assert!(!BackendType::Ollama.requires_api_key());
        assert!(!BackendType::FastEmbed.requires_api_key());
        assert!(!BackendType::Burn.requires_api_key());
        assert!(!BackendType::GitHubCopilot.requires_api_key()); // OAuth, not API key
        assert!(!BackendType::Custom.requires_api_key());
        assert!(!BackendType::Mock.requires_api_key());
    }

    #[test]
    fn test_as_str_all_variants() {
        assert_eq!(BackendType::Ollama.as_str(), "ollama");
        assert_eq!(BackendType::OpenAI.as_str(), "openai");
        assert_eq!(BackendType::Anthropic.as_str(), "anthropic");
        assert_eq!(BackendType::Cohere.as_str(), "cohere");
        assert_eq!(BackendType::VertexAI.as_str(), "vertexai");
        assert_eq!(BackendType::FastEmbed.as_str(), "fastembed");
        assert_eq!(BackendType::Burn.as_str(), "burn");
        assert_eq!(BackendType::GitHubCopilot.as_str(), "github-copilot");
        assert_eq!(BackendType::OpenRouter.as_str(), "openrouter");
        assert_eq!(BackendType::ZAI.as_str(), "zai");
        assert_eq!(BackendType::Custom.as_str(), "custom");
        assert_eq!(BackendType::Mock.as_str(), "mock");
    }

    #[test]
    fn test_default_endpoint_all_variants() {
        use crate::components::defaults;

        // Backends with default endpoints
        assert_eq!(
            BackendType::Ollama.default_endpoint(),
            Some(defaults::DEFAULT_OLLAMA_ENDPOINT)
        );
        assert_eq!(
            BackendType::OpenAI.default_endpoint(),
            Some(defaults::DEFAULT_OPENAI_ENDPOINT)
        );
        assert_eq!(
            BackendType::Anthropic.default_endpoint(),
            Some(defaults::DEFAULT_ANTHROPIC_ENDPOINT)
        );
        assert_eq!(
            BackendType::Cohere.default_endpoint(),
            Some("https://api.cohere.ai/v1")
        );
        assert_eq!(
            BackendType::VertexAI.default_endpoint(),
            Some("https://aiplatform.googleapis.com")
        );
        assert_eq!(
            BackendType::GitHubCopilot.default_endpoint(),
            Some(defaults::DEFAULT_GITHUB_COPILOT_ENDPOINT)
        );
        assert_eq!(
            BackendType::OpenRouter.default_endpoint(),
            Some(defaults::DEFAULT_OPENROUTER_ENDPOINT)
        );
        assert_eq!(
            BackendType::ZAI.default_endpoint(),
            Some(defaults::DEFAULT_ZAI_ENDPOINT)
        );

        // Backends with NO default endpoint
        assert_eq!(BackendType::FastEmbed.default_endpoint(), None);
        assert_eq!(BackendType::Burn.default_endpoint(), None);
        assert_eq!(BackendType::Custom.default_endpoint(), None);
        assert_eq!(BackendType::Mock.default_endpoint(), None);
    }

    #[test]
    fn test_default_embedding_model_all_variants() {
        // Multi-capability backends with embedding models
        assert_eq!(
            BackendType::Ollama.default_embedding_model(),
            Some("nomic-embed-text")
        );
        assert_eq!(
            BackendType::OpenAI.default_embedding_model(),
            Some("text-embedding-3-small")
        );
        assert_eq!(
            BackendType::Cohere.default_embedding_model(),
            Some("embed-english-v3.0")
        );
        assert_eq!(
            BackendType::VertexAI.default_embedding_model(),
            Some("textembedding-gecko@003")
        );

        // Embedding-only backends
        assert_eq!(
            BackendType::FastEmbed.default_embedding_model(),
            Some("BAAI/bge-small-en-v1.5")
        );
        assert_eq!(
            BackendType::Burn.default_embedding_model(),
            Some("nomic-embed-text")
        );

        // Chat-only backends (no embedding models)
        assert_eq!(BackendType::Anthropic.default_embedding_model(), None);
        assert_eq!(BackendType::GitHubCopilot.default_embedding_model(), None);
        assert_eq!(BackendType::OpenRouter.default_embedding_model(), None);
        assert_eq!(BackendType::ZAI.default_embedding_model(), None);

        // Utility backends
        assert_eq!(BackendType::Custom.default_embedding_model(), None);
        assert_eq!(
            BackendType::Mock.default_embedding_model(),
            Some("mock-embed-model")
        );
    }

    #[test]
    fn test_default_chat_model_all_variants() {
        use crate::components::defaults;

        // Multi-capability backends with chat models
        assert_eq!(
            BackendType::Ollama.default_chat_model(),
            Some(defaults::DEFAULT_CHAT_MODEL)
        );
        assert_eq!(
            BackendType::OpenAI.default_chat_model(),
            Some(defaults::DEFAULT_OPENAI_MODEL)
        );
        assert_eq!(
            BackendType::Cohere.default_chat_model(),
            Some("command-r-plus")
        );
        assert_eq!(
            BackendType::VertexAI.default_chat_model(),
            Some("gemini-1.5-pro")
        );

        // Chat-only backends
        assert_eq!(
            BackendType::Anthropic.default_chat_model(),
            Some(defaults::DEFAULT_ANTHROPIC_MODEL)
        );
        assert_eq!(
            BackendType::GitHubCopilot.default_chat_model(),
            Some(defaults::DEFAULT_GITHUB_COPILOT_MODEL)
        );
        assert_eq!(
            BackendType::OpenRouter.default_chat_model(),
            Some(defaults::DEFAULT_OPENROUTER_MODEL)
        );
        assert_eq!(
            BackendType::ZAI.default_chat_model(),
            Some(defaults::DEFAULT_ZAI_MODEL)
        );

        // Embedding-only backends (no chat models)
        assert_eq!(BackendType::FastEmbed.default_chat_model(), None);
        assert_eq!(BackendType::Burn.default_chat_model(), None);

        // Utility backends
        assert_eq!(BackendType::Custom.default_chat_model(), None);
        assert_eq!(
            BackendType::Mock.default_chat_model(),
            Some("mock-chat-model")
        );
    }

    #[test]
    fn test_default_max_concurrent_all_variants() {
        // FastEmbed uses runtime num_cpus::get() — NOT hardcoded
        let expected_fastembed = (num_cpus::get() / 2).max(1);
        assert_eq!(
            BackendType::FastEmbed.default_max_concurrent(),
            expected_fastembed,
            "FastEmbed should use (num_cpus::get() / 2).max(1)"
        );

        // GPU-bound backends (sequential)
        assert_eq!(BackendType::Ollama.default_max_concurrent(), 1);
        assert_eq!(BackendType::Burn.default_max_concurrent(), 1);

        // Rate-limited cloud backends
        assert_eq!(BackendType::OpenAI.default_max_concurrent(), 8);
        assert_eq!(BackendType::Anthropic.default_max_concurrent(), 8);
        assert_eq!(BackendType::Cohere.default_max_concurrent(), 8);
        assert_eq!(BackendType::VertexAI.default_max_concurrent(), 8);
        assert_eq!(BackendType::GitHubCopilot.default_max_concurrent(), 8);
        assert_eq!(BackendType::OpenRouter.default_max_concurrent(), 8);
        assert_eq!(BackendType::ZAI.default_max_concurrent(), 8);

        // Testing backend
        assert_eq!(BackendType::Mock.default_max_concurrent(), 16);

        // Conservative custom backend
        assert_eq!(BackendType::Custom.default_max_concurrent(), 4);
    }

    #[test]
    fn test_api_key_env_var_all_variants() {
        // Backends with API key environment variables
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

        // Backends with NO API key environment variable
        assert_eq!(BackendType::Ollama.api_key_env_var(), None);
        assert_eq!(BackendType::FastEmbed.api_key_env_var(), None);
        assert_eq!(BackendType::Burn.api_key_env_var(), None);
        assert_eq!(BackendType::GitHubCopilot.api_key_env_var(), None);
        assert_eq!(BackendType::Custom.api_key_env_var(), None);
        assert_eq!(BackendType::Mock.api_key_env_var(), None);
    }

    #[test]
    fn test_default_trust_level_all_variants() {
        use crate::components::trust::TrustLevel;

        // Local backends → Local trust level
        assert_eq!(
            BackendType::FastEmbed.default_trust_level(),
            TrustLevel::Local
        );
        assert_eq!(BackendType::Burn.default_trust_level(), TrustLevel::Local);
        assert_eq!(BackendType::Mock.default_trust_level(), TrustLevel::Local);

        // Cloud backends → Cloud trust level
        assert_eq!(BackendType::Ollama.default_trust_level(), TrustLevel::Cloud);
        assert_eq!(BackendType::OpenAI.default_trust_level(), TrustLevel::Cloud);
        assert_eq!(
            BackendType::Anthropic.default_trust_level(),
            TrustLevel::Cloud
        );
        assert_eq!(BackendType::Cohere.default_trust_level(), TrustLevel::Cloud);
        assert_eq!(
            BackendType::VertexAI.default_trust_level(),
            TrustLevel::Cloud
        );
        assert_eq!(
            BackendType::GitHubCopilot.default_trust_level(),
            TrustLevel::Cloud
        );
        assert_eq!(
            BackendType::OpenRouter.default_trust_level(),
            TrustLevel::Cloud
        );
        assert_eq!(BackendType::ZAI.default_trust_level(), TrustLevel::Cloud);
        assert_eq!(BackendType::Custom.default_trust_level(), TrustLevel::Cloud);
    }

    #[test]
    fn test_display_impl_delegates_to_as_str() {
        // Display impl should delegate to as_str()
        assert_eq!(format!("{}", BackendType::Ollama), "ollama");
        assert_eq!(format!("{}", BackendType::OpenAI), "openai");
        assert_eq!(format!("{}", BackendType::Anthropic), "anthropic");
        assert_eq!(format!("{}", BackendType::Cohere), "cohere");
        assert_eq!(format!("{}", BackendType::VertexAI), "vertexai");
        assert_eq!(format!("{}", BackendType::FastEmbed), "fastembed");
        assert_eq!(format!("{}", BackendType::Burn), "burn");
        assert_eq!(format!("{}", BackendType::GitHubCopilot), "github-copilot");
        assert_eq!(format!("{}", BackendType::OpenRouter), "openrouter");
        assert_eq!(format!("{}", BackendType::ZAI), "zai");
        assert_eq!(format!("{}", BackendType::Custom), "custom");
        assert_eq!(format!("{}", BackendType::Mock), "mock");
    }

    #[test]
    fn test_custom_variant_optional_fields_are_none() {
        // Custom variant should have None for all optional fields
        assert_eq!(BackendType::Custom.default_endpoint(), None);
        assert_eq!(BackendType::Custom.default_embedding_model(), None);
        assert_eq!(BackendType::Custom.default_chat_model(), None);
        assert_eq!(BackendType::Custom.api_key_env_var(), None);
        // But it should support both embeddings and chat
        assert!(BackendType::Custom.supports_embeddings());
        assert!(BackendType::Custom.supports_chat());
    }

    #[test]
    fn test_mock_variant_test_values() {
        // Mock variant should have test infrastructure values
        assert_eq!(
            BackendType::Mock.default_embedding_model(),
            Some("mock-embed-model")
        );
        assert_eq!(
            BackendType::Mock.default_chat_model(),
            Some("mock-chat-model")
        );
        assert_eq!(BackendType::Mock.default_endpoint(), None);
        assert_eq!(BackendType::Mock.api_key_env_var(), None);
        assert!(BackendType::Mock.supports_embeddings());
        assert!(!BackendType::Mock.supports_chat());
        assert!(BackendType::Mock.is_local());
        assert_eq!(BackendType::Mock.default_max_concurrent(), 16);
    }

    #[test]
    fn test_github_copilot_no_api_key_oauth_only() {
        use crate::components::defaults;

        // GitHubCopilot uses OAuth, not API key
        assert!(!BackendType::GitHubCopilot.requires_api_key());
        assert_eq!(BackendType::GitHubCopilot.api_key_env_var(), None);
        // But it should have endpoint and model
        assert_eq!(
            BackendType::GitHubCopilot.default_endpoint(),
            Some(defaults::DEFAULT_GITHUB_COPILOT_ENDPOINT)
        );
        assert_eq!(
            BackendType::GitHubCopilot.default_chat_model(),
            Some(defaults::DEFAULT_GITHUB_COPILOT_MODEL)
        );
        // And it's not local
        assert!(!BackendType::GitHubCopilot.is_local());
    }

    #[test]
    fn test_all_variants_have_non_empty_as_str() {
        // Every variant must have a non-empty as_str() value
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
            assert!(!s.is_empty(), "{:?} has empty as_str()", variant);
            assert!(
                !s.contains(' '),
                "{:?} as_str() contains spaces: '{}'",
                variant,
                s
            );
        }
    }
}
