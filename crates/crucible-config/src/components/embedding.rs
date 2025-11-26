//! Simple embedding configuration with sensible defaults

use serde::{Deserialize, Serialize};
use std::default::Default;

/// Embedding provider type - enum for TOML serialization
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingProviderType {
    /// Local FastEmbed provider (default, CPU-friendly)
    #[default]
    FastEmbed,
    /// OpenAI API provider
    OpenAI,
    /// Anthropic API provider
    Anthropic,
    /// Ollama provider (local or remote)
    Ollama,
    /// Cohere API provider
    Cohere,
    /// Google Vertex AI provider
    VertexAI,
    /// Custom HTTP-based provider
    Custom,
    /// Mock provider for testing
    Mock,
}

/// Embedding configuration - pragmatic settings for performance and cost
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Embedding provider type (fastembed, openai, anthropic)
    #[serde(default)]
    pub provider: EmbeddingProviderType,
    /// Model name (defaults to provider-appropriate optimal model)
    pub model: Option<String>,
    /// Custom API endpoint (only for remote providers)
    pub api_url: Option<String>,
    /// Batch size for processing (important for performance)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
}

fn default_batch_size() -> usize { 16 }  // Conservative default for CPU-friendly performance

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: EmbeddingProviderType::FastEmbed,
            model: None, // Will use provider default
            api_url: None,
            batch_size: default_batch_size(),
        }
    }
}

impl EmbeddingProviderType {
    /// Get the provider type from a full configuration
    pub fn from_config(config: &crate::enrichment::EmbeddingProviderConfig) -> Self {
        match config {
            crate::enrichment::EmbeddingProviderConfig::OpenAI(_) => Self::OpenAI,
            crate::enrichment::EmbeddingProviderConfig::Ollama(_) => Self::Ollama,
            crate::enrichment::EmbeddingProviderConfig::FastEmbed(_) => Self::FastEmbed,
            crate::enrichment::EmbeddingProviderConfig::Cohere(_) => Self::Cohere,
            crate::enrichment::EmbeddingProviderConfig::VertexAI(_) => Self::VertexAI,
            crate::enrichment::EmbeddingProviderConfig::Custom(_) => Self::Custom,
            crate::enrichment::EmbeddingProviderConfig::Mock(_) => Self::Mock,
            // Note: Anthropic is not in the legacy config, so it's handled separately
        }
    }

    /// Get the type name as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenAI => "openai",
            Self::Ollama => "ollama",
            Self::FastEmbed => "fastembed",
            Self::Cohere => "cohere",
            Self::VertexAI => "vertexai",
            Self::Custom => "custom",
            Self::Mock => "mock",
            Self::Anthropic => "anthropic",
        }
    }
}

impl EmbeddingConfig {
    /// Get the actual model name to use
    pub fn get_model(&self) -> &str {
        self.model.as_deref().unwrap_or(match self.provider {
            EmbeddingProviderType::FastEmbed => "BAAI/bge-small-en-v1.5",
            EmbeddingProviderType::OpenAI => "text-embedding-3-small",
            EmbeddingProviderType::Anthropic => "claude-3-haiku-20240307",
            EmbeddingProviderType::Ollama => "nomic-embed-text",
            EmbeddingProviderType::Cohere => "embed-english-v3.0",
            EmbeddingProviderType::VertexAI => "textembedding-gecko@003",
            EmbeddingProviderType::Custom => "custom-model",
            EmbeddingProviderType::Mock => "mock-test-model",
        })
    }

    /// Get API URL for remote providers
    pub fn get_api_url(&self) -> Option<&str> {
        match self.provider {
            EmbeddingProviderType::FastEmbed => None, // Local provider
            EmbeddingProviderType::OpenAI => self.api_url.as_deref().or(Some("https://api.openai.com/v1")),
            EmbeddingProviderType::Anthropic => self.api_url.as_deref().or(Some("https://api.anthropic.com")),
            EmbeddingProviderType::Ollama => self.api_url.as_deref().or(Some("http://localhost:11434")),
            EmbeddingProviderType::Cohere => self.api_url.as_deref().or(Some("https://api.cohere.ai/v1")),
            EmbeddingProviderType::VertexAI => self.api_url.as_deref().or(Some("https://aiplatform.googleapis.com")),
            EmbeddingProviderType::Custom => self.api_url.as_deref(), // User must specify
            EmbeddingProviderType::Mock => None, // Mock provider
        }
    }

    /// Check if provider is local (no API calls needed)
    pub fn is_local(&self) -> bool {
        matches!(self.provider, EmbeddingProviderType::FastEmbed | EmbeddingProviderType::Ollama | EmbeddingProviderType::Mock)
    }

    /// Convert to EmbeddingProviderConfig for use with LLM crate
    pub fn to_provider_config(&self) -> crate::enrichment::EmbeddingProviderConfig {
        match self.provider {
            EmbeddingProviderType::FastEmbed => {
                crate::enrichment::EmbeddingProviderConfig::FastEmbed(
                    crate::enrichment::FastEmbedConfig {
                        model: self.model.clone().unwrap_or_else(|| "BAAI/bge-small-en-v1.5".to_string()),
                        cache_dir: Some(default_embedding_model_cache_dir()),
                        batch_size: self.batch_size as u32,
                        num_threads: None,
                        dimensions: 0, // Use 0 to indicate default dimensions
                    }
                )
            }
            EmbeddingProviderType::OpenAI => {
                crate::enrichment::EmbeddingProviderConfig::OpenAI(
                    crate::enrichment::OpenAIConfig {
                        api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
                        model: self.model.clone().unwrap_or_else(|| "text-embedding-3-small".to_string()),
                        base_url: self.get_api_url().unwrap_or("https://api.openai.com/v1").to_string(),
                        timeout_seconds: 30,
                        retry_attempts: 3,
                        dimensions: 0, // Use 0 to indicate default dimensions
                        headers: std::collections::HashMap::new(),
                    }
                )
            }
            EmbeddingProviderType::Ollama => {
                crate::enrichment::EmbeddingProviderConfig::Ollama(
                    crate::enrichment::OllamaConfig {
                        model: self.model.clone().unwrap_or_else(|| "nomic-embed-text".to_string()),
                        base_url: self.get_api_url().unwrap_or("http://localhost:11434").to_string(),
                        timeout_seconds: 30,
                        retry_attempts: 3,
                        dimensions: 0, // Use 0 to indicate default dimensions
                    }
                )
            }
            EmbeddingProviderType::Mock => {
                crate::enrichment::EmbeddingProviderConfig::Mock(
                    crate::enrichment::MockConfig {
                        model: "mock-test-model".to_string(),
                        dimensions: 768,
                        simulated_latency_ms: 0,
                    }
                )
            }
            _ => {
                // For other providers, create a default FastEmbed config as fallback
                crate::enrichment::EmbeddingProviderConfig::FastEmbed(
                    crate::enrichment::FastEmbedConfig {
                        model: self.model.clone().unwrap_or_else(|| "BAAI/bge-small-en-v1.5".to_string()),
                        cache_dir: Some(default_embedding_model_cache_dir()),
                        batch_size: self.batch_size as u32,
                        num_threads: None,
                        dimensions: 0, // Use 0 to indicate default dimensions
                    }
                )
            }
        }
    }
}
/// Get the default cache directory for embedding models following OS conventions
///
/// Returns platform-specific directories:
/// - Linux: ~/.local/share/crucible/embedding-models
/// - macOS: ~/Library/Application Support/crucible/embedding-models
/// - Windows: %LOCALAPPDATA%/crucible/embedding-models
fn default_embedding_model_cache_dir() -> String {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));

    #[cfg(target_os = "linux")]
    {
        home.join(".local").join("share").join("crucible").join("embedding-models")
            .to_string_lossy()
            .to_string()
    }

    #[cfg(target_os = "macos")]
    {
        home.join("Library").join("Application Support").join("crucible").join("embedding-models")
            .to_string_lossy()
            .to_string()
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(app_data) = std::env::var_os("LOCALAPPDATA") {
            std::path::PathBuf::from(app_data)
                .join("crucible")
                .join("embedding-models")
                .to_string_lossy()
                .to_string()
        } else {
            home.join("AppData").join("Local").join("crucible").join("embedding-models")
                .to_string_lossy()
                .to_string()
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        // Fallback for other platforms - use home directory
        home.join(".crucible").join("embedding-models")
            .to_string_lossy()
            .to_string()
    }
}
