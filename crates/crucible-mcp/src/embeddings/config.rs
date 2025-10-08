//\! Configuration for embedding providers

use serde::{Deserialize, Serialize};
use std::env;

use super::error::{EmbeddingError, EmbeddingResult};

/// Type of embedding provider
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    /// Ollama local/remote embedding service
    Ollama,
    /// OpenAI embedding API
    OpenAI,
}

impl ProviderType {
    /// Parse provider type from string
    pub fn from_str(s: &str) -> EmbeddingResult<Self> {
        match s.to_lowercase().as_str() {
            "ollama" => Ok(ProviderType::Ollama),
            "openai" => Ok(ProviderType::OpenAI),
            _ => Err(EmbeddingError::ConfigError(format!(
                "Unknown provider type: {}. Valid options: ollama, openai",
                s
            ))),
        }
    }

    /// Get default endpoint for this provider
    pub fn default_endpoint(&self) -> &'static str {
        match self {
            ProviderType::Ollama => "https://llama.terminal.krohnos.io",
            ProviderType::OpenAI => "https://api.openai.com/v1",
        }
    }

    /// Get default model for this provider
    pub fn default_model(&self) -> &'static str {
        match self {
            ProviderType::Ollama => "nomic-embed-text",
            ProviderType::OpenAI => "text-embedding-3-small",
        }
    }

    /// Get expected embedding dimensions for this provider's default model
    pub fn default_dimensions(&self) -> usize {
        match self {
            ProviderType::Ollama => 768,  // nomic-embed-text
            ProviderType::OpenAI => 1536, // text-embedding-3-small
        }
    }

    /// Whether this provider requires an API key
    pub fn requires_api_key(&self) -> bool {
        match self {
            ProviderType::Ollama => false,
            ProviderType::OpenAI => true,
        }
    }
}

/// Configuration for embedding provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Provider type (ollama, openai)
    pub provider: ProviderType,
    
    /// API endpoint URL
    pub endpoint: String,
    
    /// API key (optional, required for some providers)
    pub api_key: Option<String>,
    
    /// Model name to use for embeddings
    pub model: String,
    
    /// Request timeout in seconds
    pub timeout_secs: u64,
    
    /// Maximum number of retry attempts
    pub max_retries: u32,
    
    /// Batch size for bulk embedding operations
    pub batch_size: usize,
}

impl EmbeddingConfig {
    /// Create configuration from environment variables
    pub fn from_env() -> EmbeddingResult<Self> {
        let provider_str = env::var("EMBEDDING_PROVIDER")
            .unwrap_or_else(|_| "ollama".to_string());
        
        let provider = ProviderType::from_str(&provider_str)?;
        
        let endpoint = env::var("EMBEDDING_ENDPOINT")
            .unwrap_or_else(|_| provider.default_endpoint().to_string());
        
        let api_key = env::var("EMBEDDING_API_KEY").ok();
        
        let model = env::var("EMBEDDING_MODEL")
            .unwrap_or_else(|_| provider.default_model().to_string());
        
        let timeout_secs = env::var("EMBEDDING_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);
        
        let max_retries = env::var("EMBEDDING_MAX_RETRIES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);
        
        let batch_size = env::var("EMBEDDING_BATCH_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);
        
        let config = Self {
            provider,
            endpoint,
            api_key,
            model,
            timeout_secs,
            max_retries,
            batch_size,
        };
        
        config.validate()?;
        Ok(config)
    }

    /// Create configuration for Ollama provider
    pub fn ollama(endpoint: Option<String>, model: Option<String>) -> Self {
        Self {
            provider: ProviderType::Ollama,
            endpoint: endpoint.unwrap_or_else(|| "https://llama.terminal.krohnos.io".to_string()),
            api_key: None,
            model: model.unwrap_or_else(|| "nomic-embed-text".to_string()),
            timeout_secs: 30,
            max_retries: 3,
            batch_size: 10,
        }
    }

    /// Create configuration for OpenAI provider
    pub fn openai(api_key: String, model: Option<String>) -> Self {
        Self {
            provider: ProviderType::OpenAI,
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: Some(api_key),
            model: model.unwrap_or_else(|| "text-embedding-3-small".to_string()),
            timeout_secs: 30,
            max_retries: 3,
            batch_size: 10,
        }
    }

    /// Validate configuration
    pub fn validate(&self) -> EmbeddingResult<()> {
        // Check API key requirement
        if self.provider.requires_api_key() && self.api_key.is_none() {
            return Err(EmbeddingError::ConfigError(format!(
                "Provider {} requires an API key (set EMBEDDING_API_KEY environment variable)",
                match self.provider {
                    ProviderType::OpenAI => "OpenAI",
                    ProviderType::Ollama => "Ollama",
                }
            )));
        }

        // Validate endpoint URL
        if self.endpoint.is_empty() {
            return Err(EmbeddingError::ConfigError(
                "Endpoint URL cannot be empty".to_string()
            ));
        }

        // Validate model name
        if self.model.is_empty() {
            return Err(EmbeddingError::ConfigError(
                "Model name cannot be empty".to_string()
            ));
        }

        // Validate timeout
        if self.timeout_secs == 0 {
            return Err(EmbeddingError::ConfigError(
                "Timeout must be greater than 0".to_string()
            ));
        }

        // Validate batch size
        if self.batch_size == 0 {
            return Err(EmbeddingError::ConfigError(
                "Batch size must be greater than 0".to_string()
            ));
        }

        Ok(())
    }

    /// Get expected embedding dimensions based on provider and model
    pub fn expected_dimensions(&self) -> usize {
        // This is a simplified version - in production, we'd have a more
        // comprehensive model dimension mapping
        match (&self.provider, self.model.as_str()) {
            (ProviderType::Ollama, "nomic-embed-text") => 768,
            (ProviderType::OpenAI, "text-embedding-3-small") => 1536,
            (ProviderType::OpenAI, "text-embedding-3-large") => 3072,
            (ProviderType::OpenAI, "text-embedding-ada-002") => 1536,
            // Default to provider defaults for unknown models
            (ProviderType::Ollama, _) => 768,
            (ProviderType::OpenAI, _) => 1536,
        }
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self::ollama(None, None)
    }
}
