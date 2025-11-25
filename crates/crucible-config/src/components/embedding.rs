//! Embedding component configuration
//!
//! Configuration for embedding providers and LLM services.

use serde::{Deserialize, Serialize};

/// Embedding component configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingComponentConfig {
    pub enabled: bool,
    pub default_provider: ProviderType,
    pub providers: ProviderConfigs,
    pub batch_settings: BatchConfig,
    pub cache_settings: EmbeddingCacheConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProviderType {
    FastEmbed,
    OpenAI,
    Anthropic,
    #[serde(rename = "custom")]
    Custom { name: String },
}

impl ProviderType {
    pub fn custom_name(name: String) -> Self {
        Self::Custom { name }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderType::FastEmbed => "fastembed",
            ProviderType::OpenAI => "openai",
            ProviderType::Anthropic => "anthropic",
            ProviderType::Custom { .. } => "custom",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfigs {
    pub fastembed: Option<FastEmbedProviderConfig>,
    pub openai: Option<OpenAIProviderConfig>,
    pub anthropic: Option<AnthropicProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FastEmbedProviderConfig {
    pub model: String,
    pub batch_size: usize,
    pub num_threads: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIProviderConfig {
    pub api_key: Option<String>,
    pub model: String,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicProviderConfig {
    pub api_key: Option<String>,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConfig {
    pub size: usize,
    pub timeout_seconds: u64,
    pub max_concurrent: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingCacheConfig {
    pub enabled: bool,
    pub max_size: usize,
    pub ttl_seconds: u64,
}

impl Default for EmbeddingComponentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_provider: ProviderType::FastEmbed,
            providers: ProviderConfigs::default(),
            batch_settings: BatchConfig::default(),
            cache_settings: EmbeddingCacheConfig::default(),
        }
    }
}

impl Default for ProviderConfigs {
    fn default() -> Self {
        Self {
            fastembed: Some(FastEmbedProviderConfig::default()),
            openai: None,
            anthropic: None,
        }
    }
}

impl Default for FastEmbedProviderConfig {
    fn default() -> Self {
        Self {
            model: "BAAI/bge-small-en-v1.5".to_string(),
            batch_size: 16,
            num_threads: None,
        }
    }
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            size: 8,
            timeout_seconds: 60,
            max_concurrent: 2,
        }
    }
}

impl Default for EmbeddingCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_size: 1000,
            ttl_seconds: 3600,
        }
    }
}