//! Simple chat configuration

use serde::{Deserialize, Serialize};

/// LLM provider type for chat
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    /// Ollama provider
    Ollama,
    /// OpenAI provider
    OpenAI,
    /// Anthropic provider
    Anthropic,
}

impl Default for LlmProvider {
    fn default() -> Self {
        Self::Ollama
    }
}

/// Simple chat configuration - only essential user settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConfig {
    /// Default chat model (can be overridden by agents)
    pub model: Option<String>,
    /// Enable markdown rendering
    #[serde(default = "default_true")]
    pub enable_markdown: bool,
    /// LLM provider to use
    #[serde(default)]
    pub provider: LlmProvider,
    /// LLM endpoint URL (for Ollama/compatible providers)
    pub endpoint: Option<String>,
    /// Temperature for generation (0.0-2.0)
    pub temperature: Option<f32>,
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// API timeout in seconds
    pub timeout_secs: Option<u64>,
}

fn default_true() -> bool {
    true
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            model: None, // Use agent default
            enable_markdown: true,
            provider: LlmProvider::default(),
            endpoint: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
        }
    }
}

impl ChatConfig {
    /// Get the LLM endpoint, using provider-specific default if not specified
    pub fn llm_endpoint(&self) -> String {
        self.endpoint
            .clone()
            .unwrap_or_else(|| match self.provider {
                LlmProvider::Ollama => "http://localhost:11434".to_string(),
                LlmProvider::OpenAI => "https://api.openai.com/v1".to_string(),
                LlmProvider::Anthropic => "https://api.anthropic.com/v1".to_string(),
            })
    }

    /// Get the chat model, using default if not specified
    pub fn chat_model(&self) -> String {
        self.model.clone().unwrap_or_else(|| "llama3.2".to_string())
    }

    /// Get the temperature, using default if not specified
    pub fn temperature(&self) -> f32 {
        self.temperature.unwrap_or(0.7)
    }

    /// Get max tokens, using default if not specified
    pub fn max_tokens(&self) -> u32 {
        self.max_tokens.unwrap_or(2048)
    }

    /// Get timeout in seconds, using default if not specified
    pub fn timeout_secs(&self) -> u64 {
        self.timeout_secs.unwrap_or(120)
    }
}
