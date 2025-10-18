//! Provider configuration for embedding and AI services.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Errors related to provider configuration.
#[derive(Error, Debug)]
pub enum ProviderError {
    /// Unsupported provider type.
    #[error("Unsupported provider type: {provider_type}")]
    UnsupportedType { provider_type: String },

    /// Missing required configuration field.
    #[error("Missing required field: {field}")]
    MissingField { field: String },

    /// Invalid API key format.
    #[error("Invalid API key format")]
    InvalidApiKey,

    /// Invalid model name.
    #[error("Invalid model name: {model}")]
    InvalidModel { model: String },
}

/// Configuration for embedding providers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbeddingProviderConfig {
    /// Provider type.
    #[serde(rename = "type")]
    pub provider_type: EmbeddingProviderType,

    /// API configuration.
    pub api: ApiConfig,

    /// Model configuration.
    pub model: ModelConfig,

    /// Additional provider-specific options.
    #[serde(flatten)]
    pub options: HashMap<String, serde_json::Value>,
}

impl EmbeddingProviderConfig {
    /// Create a new OpenAI embedding provider configuration.
    pub fn openai(api_key: String, model: Option<String>) -> Self {
        Self {
            provider_type: EmbeddingProviderType::OpenAI,
            api: ApiConfig {
                key: Some(api_key),
                base_url: None,
                timeout_seconds: Some(30),
                retry_attempts: Some(3),
                headers: HashMap::new(),
            },
            model: ModelConfig {
                name: model.unwrap_or_else(|| "text-embedding-3-small".to_string()),
                dimensions: None,
                max_tokens: Some(8192),
            },
            options: HashMap::new(),
        }
    }

    /// Create a new Ollama embedding provider configuration.
    pub fn ollama(base_url: String, model: String) -> Self {
        Self {
            provider_type: EmbeddingProviderType::Ollama,
            api: ApiConfig {
                key: None,
                base_url: Some(base_url),
                timeout_seconds: Some(60),
                retry_attempts: Some(2),
                headers: HashMap::new(),
            },
            model: ModelConfig {
                name: model,
                dimensions: None,
                max_tokens: Some(2048),
            },
            options: HashMap::new(),
        }
    }

    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), ProviderError> {
        // Validate API key for providers that require it
        if self.provider_type.requires_api_key() && self.api.key.is_none() {
            return Err(ProviderError::MissingField {
                field: "api.key".to_string(),
            });
        }

        // Validate model name
        if self.model.name.is_empty() {
            return Err(ProviderError::InvalidModel {
                model: self.model.name.clone(),
            });
        }

        Ok(())
    }
}

/// Supported embedding provider types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingProviderType {
    /// OpenAI embeddings.
    OpenAI,
    /// Ollama local embeddings.
    Ollama,
    /// Cohere embeddings.
    Cohere,
    /// Google Vertex AI embeddings.
    VertexAI,
    /// Custom embedding provider.
    Custom(String),
}

impl EmbeddingProviderType {
    /// Check if this provider type requires an API key.
    pub fn requires_api_key(&self) -> bool {
        !matches!(self, Self::Ollama)
    }

    /// Get the default base URL for the provider.
    pub fn default_base_url(&self) -> Option<String> {
        match self {
            Self::OpenAI => Some("https://api.openai.com/v1".to_string()),
            Self::Ollama => Some("http://localhost:11434".to_string()),
            Self::Cohere => Some("https://api.cohere.ai/v1".to_string()),
            Self::VertexAI => Some("https://aiplatform.googleapis.com/v1".to_string()),
            Self::Custom(_) => None,
        }
    }

    /// Get the default model for the provider.
    pub fn default_model(&self) -> Option<String> {
        match self {
            Self::OpenAI => Some("text-embedding-3-small".to_string()),
            Self::Ollama => Some("nomic-embed-text".to_string()),
            Self::Cohere => Some("embed-english-v3.0".to_string()),
            Self::VertexAI => Some("textembedding-gecko@003".to_string()),
            Self::Custom(_) => None,
        }
    }
}

/// API configuration for providers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiConfig {
    /// API key (if required).
    pub key: Option<String>,

    /// Custom base URL.
    pub base_url: Option<String>,

    /// Request timeout in seconds.
    pub timeout_seconds: Option<u64>,

    /// Number of retry attempts.
    pub retry_attempts: Option<u32>,

    /// Additional HTTP headers.
    pub headers: HashMap<String, String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            key: None,
            base_url: None,
            timeout_seconds: Some(30),
            retry_attempts: Some(3),
            headers: HashMap::new(),
        }
    }
}

/// Model configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelConfig {
    /// Model name.
    pub name: String,

    /// Embedding dimensions (if configurable).
    pub dimensions: Option<u32>,

    /// Maximum tokens per request.
    pub max_tokens: Option<u32>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            dimensions: None,
            max_tokens: Some(8192),
        }
    }
}

/// Configuration for AI chat providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatProviderConfig {
    /// Provider type.
    #[serde(rename = "type")]
    pub provider_type: ChatProviderType,

    /// API configuration.
    pub api: ApiConfig,

    /// Model configuration.
    pub model: ModelConfig,

    /// Generation parameters.
    pub generation: GenerationConfig,

    /// Additional provider-specific options.
    #[serde(flatten)]
    pub options: HashMap<String, serde_json::Value>,
}

impl ChatProviderConfig {
    /// Create a new OpenAI chat provider configuration.
    pub fn openai(api_key: String, model: Option<String>) -> Self {
        Self {
            provider_type: ChatProviderType::OpenAI,
            api: ApiConfig {
                key: Some(api_key),
                base_url: None,
                timeout_seconds: Some(60),
                retry_attempts: Some(3),
                headers: HashMap::new(),
            },
            model: ModelConfig {
                name: model.unwrap_or_else(|| "gpt-4".to_string()),
                dimensions: None,
                max_tokens: Some(4096),
            },
            generation: GenerationConfig::default(),
            options: HashMap::new(),
        }
    }

    /// Create a new Ollama chat provider configuration.
    pub fn ollama(base_url: String, model: String) -> Self {
        Self {
            provider_type: ChatProviderType::Ollama,
            api: ApiConfig {
                key: None,
                base_url: Some(base_url),
                timeout_seconds: Some(120),
                retry_attempts: Some(2),
                headers: HashMap::new(),
            },
            model: ModelConfig {
                name: model,
                dimensions: None,
                max_tokens: Some(2048),
            },
            generation: GenerationConfig::default(),
            options: HashMap::new(),
        }
    }
}

/// Supported chat provider types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatProviderType {
    /// OpenAI chat models.
    OpenAI,
    /// Ollama local models.
    Ollama,
    /// Anthropic Claude models.
    Anthropic,
    /// Google Gemini models.
    Gemini,
    /// Custom chat provider.
    Custom(String),
}

/// Generation configuration for chat models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    /// Temperature for generation (0.0 to 2.0).
    pub temperature: Option<f32>,

    /// Maximum tokens to generate.
    pub max_tokens: Option<u32>,

    /// Top-p sampling parameter.
    pub top_p: Option<f32>,

    /// Top-k sampling parameter.
    pub top_k: Option<u32>,

    /// Frequency penalty.
    pub frequency_penalty: Option<f32>,

    /// Presence penalty.
    pub presence_penalty: Option<f32>,

    /// Stop sequences.
    pub stop: Option<Vec<String>>,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            temperature: Some(0.7),
            max_tokens: None,
            top_p: Some(1.0),
            top_k: None,
            frequency_penalty: Some(0.0),
            presence_penalty: Some(0.0),
            stop: None,
        }
    }
}