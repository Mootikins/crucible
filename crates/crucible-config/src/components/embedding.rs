//! Simple embedding configuration with sensible defaults

use super::BackendType;
use serde::{Deserialize, Serialize};
use std::default::Default;

/// Embedding configuration - pragmatic settings for performance and cost
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Embedding provider type (fastembed, openai, anthropic)
    #[serde(default = "default_provider")]
    pub provider: Option<BackendType>,
    /// Model name (defaults to provider-appropriate optimal model)
    pub model: Option<String>,
    /// Custom API endpoint (only for remote providers)
    pub api_url: Option<String>,
    /// Batch size for processing (important for performance)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Maximum concurrent embedding jobs (provider-specific defaults apply if not set)
    /// - Ollama: 1 (single GPU, sequential processing)
    /// - FastEmbed: num_cpus/2 (CPU-bound, parallel OK)
    /// - Remote APIs: 8 (rate-limited, moderate concurrency)
    pub max_concurrent: Option<usize>,
}

fn default_provider() -> Option<BackendType> {
    Some(BackendType::FastEmbed)
}

fn default_batch_size() -> usize {
    16
} // Conservative default for CPU-friendly performance

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: Some(BackendType::FastEmbed),
            model: None, // Will use provider default
            api_url: None,
            batch_size: default_batch_size(),
            max_concurrent: None, // Will use provider-specific default
        }
    }
}

impl EmbeddingConfig {
    /// Get the actual model name to use
    pub fn get_model(&self) -> &str {
        self.model.as_deref().unwrap_or_else(|| {
            self.provider
                .and_then(|p| p.default_embedding_model())
                .unwrap_or("")
        })
    }

    /// Get API URL for remote providers
    pub fn get_api_url(&self) -> Option<&str> {
        self.api_url
            .as_deref()
            .or_else(|| self.provider.and_then(|p| p.default_endpoint()))
    }

    /// Check if provider is local (no API calls needed)
    pub fn is_local(&self) -> bool {
        matches!(
            self.provider,
            Some(BackendType::FastEmbed)
                | Some(BackendType::Ollama)
                | Some(BackendType::Mock)
                | Some(BackendType::Burn)
                | None
        )
    }

    /// Get the effective max concurrent embedding jobs.
    ///
    /// Returns user-configured value if set, otherwise provider-specific defaults:
    /// - Ollama: 1 (single GPU, sequential processing to avoid OOM/rate limits)
    /// - FastEmbed: num_cpus/2 (CPU-bound, parallel OK but avoid oversubscription)
    /// - Burn: 1 (GPU-bound, sequential to avoid VRAM exhaustion)
    /// - Remote APIs (OpenAI, Anthropic, Cohere, VertexAI): 8 (rate-limited, moderate concurrency)
    /// - Mock: 16 (testing, high concurrency OK)
    /// - Custom: 4 (conservative default)
    pub fn get_max_concurrent(&self) -> usize {
        self.max_concurrent.unwrap_or_else(|| {
            self.provider
                .map(|p| p.default_max_concurrent())
                .unwrap_or(1)
        })
    }

    /// Convert to EmbeddingProviderConfig for use with LLM crate
    pub fn to_provider_config(&self) -> crate::enrichment::EmbeddingProviderConfig {
        match self.provider {
            Some(BackendType::FastEmbed) => {
                crate::enrichment::EmbeddingProviderConfig::FastEmbed(
                    crate::enrichment::FastEmbedConfig {
                        model: self
                            .model
                            .clone()
                            .unwrap_or_else(|| "BAAI/bge-small-en-v1.5".to_string()),
                        cache_dir: Some(default_embedding_model_cache_dir()),
                        batch_size: self.batch_size as u32,
                        num_threads: None,
                        dimensions: 0, // Use 0 to indicate default dimensions
                    },
                )
            }
            Some(BackendType::OpenAI) => {
                crate::enrichment::EmbeddingProviderConfig::OpenAI(
                    crate::enrichment::OpenAIConfig {
                        api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
                        model: self
                            .model
                            .clone()
                            .unwrap_or_else(|| "text-embedding-3-small".to_string()),
                        base_url: self
                            .get_api_url()
                            .unwrap_or("https://api.openai.com/v1")
                            .to_string(),
                        timeout_seconds: 30,
                        retry_attempts: 3,
                        dimensions: 0, // Use 0 to indicate default dimensions
                        headers: std::collections::HashMap::new(),
                    },
                )
            }
            Some(BackendType::Ollama) => {
                crate::enrichment::EmbeddingProviderConfig::Ollama(
                    crate::enrichment::OllamaConfig {
                        model: self
                            .model
                            .clone()
                            .unwrap_or_else(|| "nomic-embed-text".to_string()),
                        base_url: self
                            .get_api_url()
                            .unwrap_or("http://localhost:11434")
                            .to_string(),
                        timeout_seconds: 30,
                        retry_attempts: 3,
                        dimensions: 0,  // Use 0 to indicate default dimensions
                        batch_size: 50, // Default batch size for ~7x speedup
                    },
                )
            }
            Some(BackendType::Mock) => {
                crate::enrichment::EmbeddingProviderConfig::Mock(crate::enrichment::MockConfig {
                    model: "mock-test-model".to_string(),
                    dimensions: 768,
                    simulated_latency_ms: 0,
                })
            }
            Some(BackendType::Burn) => {
                crate::enrichment::EmbeddingProviderConfig::Burn(
                    crate::enrichment::BurnEmbedConfig {
                        model: self
                            .model
                            .clone()
                            .unwrap_or_else(|| "nomic-embed-text".to_string()),
                        backend: crate::enrichment::BurnBackendConfig::Auto,
                        model_dir: crate::enrichment::BurnEmbedConfig::default_model_dir(),
                        model_search_paths: Vec::new(), // Will use defaults
                        dimensions: 0,                  // Auto-detect
                    },
                )
            }
            _ => {
                // For other providers, create a default FastEmbed config as fallback
                crate::enrichment::EmbeddingProviderConfig::FastEmbed(
                    crate::enrichment::FastEmbedConfig {
                        model: self
                            .model
                            .clone()
                            .unwrap_or_else(|| "BAAI/bge-small-en-v1.5".to_string()),
                        cache_dir: Some(default_embedding_model_cache_dir()),
                        batch_size: self.batch_size as u32,
                        num_threads: None,
                        dimensions: 0, // Use 0 to indicate default dimensions
                    },
                )
            }
        }
    }
}
/// Get the default cache directory for embedding models following OS conventions
///
/// Returns platform-specific directories:
/// - Linux: ~/.local/share/crucible/embedding-models (XDG data directory)
/// - macOS: ~/Library/Application Support/crucible/embedding-models
/// - Windows: %LOCALAPPDATA%/crucible/embedding-models (Local AppData, non-roaming)
fn default_embedding_model_cache_dir() -> String {
    // Use platform-appropriate data directory (cache/data, not config)
    // On Windows: %LOCALAPPDATA% (Local AppData, non-roaming)
    // On Linux: ~/.local/share (XDG data directory)
    // On macOS: ~/Library/Application Support
    if let Some(data_dir) = dirs::data_dir() {
        return data_dir
            .join("crucible")
            .join("embedding-models")
            .to_string_lossy()
            .to_string();
    }

    // Fallback: Use home directory with platform-specific subdirectories
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));

    #[cfg(target_os = "linux")]
    {
        home.join(".local")
            .join("share")
            .join("crucible")
            .join("embedding-models")
            .to_string_lossy()
            .to_string()
    }

    #[cfg(target_os = "macos")]
    {
        home.join("Library")
            .join("Application Support")
            .join("crucible")
            .join("embedding-models")
            .to_string_lossy()
            .to_string()
    }

    #[cfg(target_os = "windows")]
    {
        home.join("AppData")
            .join("Local")
            .join("crucible")
            .join("embedding-models")
            .to_string_lossy()
            .to_string()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        // Fallback for other platforms - use home directory
        home.join(".crucible")
            .join("embedding-models")
            .to_string_lossy()
            .to_string()
    }
}
