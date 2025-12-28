//! # Text Generation Providers
//!
//! This module provides configuration types and factory functions for text generation
//! providers. The actual provider implementations live in the `chat` module.
//!
//! ## Architecture Note
//!
//! Previously this module contained duplicate provider implementations with many
//! unimplemented `todo!()` methods. These have been removed in favor of the proper
//! implementations in `crate::chat::{OllamaChatProvider, OpenAIChatProvider}`.

use serde::{Deserialize, Serialize};

use crate::chat::{OllamaChatProvider, OpenAIChatProvider};
use crate::embeddings::error::EmbeddingResult;

// Re-export all types from crucible-core for backwards compatibility
pub use crucible_core::traits::{
    ChatCompletionChoice, ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse,
    ChatMessageDelta, CompletionChoice, CompletionChunk, CompletionRequest, CompletionResponse,
    FunctionCall, FunctionCallBehavior, FunctionCallDelta, FunctionDefinition, LlmError,
    LlmMessage, LlmResult, LlmToolDefinition, LogProbs, MessageRole, ModelCapability, ModelStatus,
    ProviderCapabilities, ResponseFormat, TextGenerationProvider, TextModelInfo, TokenUsage,
    ToolCall, ToolCallDelta, ToolChoice,
};

/// OpenAI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    /// API key
    pub api_key: String,
    /// Base URL
    pub base_url: Option<String>,
    /// Default model
    pub default_model: Option<String>,
    /// Timeout in seconds
    pub timeout_secs: Option<u64>,
    /// Default temperature
    pub temperature: Option<f32>,
    /// Organization ID
    pub organization: Option<String>,
}

/// Ollama configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Base URL for Ollama API
    pub base_url: String,
    /// Default model
    pub default_model: Option<String>,
    /// Timeout in seconds
    pub timeout_secs: Option<u64>,
    /// Default temperature
    pub temperature: Option<f32>,
}

/// Factory function to create a text generation provider from configuration.
///
/// This delegates to the proper implementations in `crate::chat`.
pub async fn create_text_provider(
    config: TextProviderConfig,
) -> EmbeddingResult<Box<dyn TextGenerationProvider>> {
    match config {
        TextProviderConfig::OpenAI(openai_config) => {
            let provider = OpenAIChatProvider::new(
                openai_config.api_key,
                openai_config.base_url,
                openai_config
                    .default_model
                    .unwrap_or_else(|| "gpt-3.5-turbo".to_string()),
                openai_config.timeout_secs.unwrap_or(60),
            );
            Ok(Box::new(provider))
        }
        TextProviderConfig::Ollama(ollama_config) => {
            let provider = OllamaChatProvider::new(
                ollama_config.base_url,
                ollama_config
                    .default_model
                    .unwrap_or_else(|| "llama2".to_string()),
                ollama_config.timeout_secs.unwrap_or(120),
            );
            Ok(Box::new(provider))
        }
    }
}

/// Text provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider")]
pub enum TextProviderConfig {
    /// OpenAI configuration
    OpenAI(OpenAIConfig),
    /// Ollama configuration
    Ollama(OllamaConfig),
}

impl TextProviderConfig {
    /// Create OpenAI configuration
    pub fn openai(api_key: String) -> Self {
        Self::OpenAI(OpenAIConfig {
            api_key,
            base_url: None,
            default_model: None,
            timeout_secs: None,
            temperature: None,
            organization: None,
        })
    }

    /// Create Ollama configuration
    pub fn ollama(base_url: String) -> Self {
        Self::Ollama(OllamaConfig {
            base_url,
            default_model: None,
            timeout_secs: None,
            temperature: None,
        })
    }
}
