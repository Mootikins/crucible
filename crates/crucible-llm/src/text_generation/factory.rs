//! Factory for creating text generation providers from configuration

use super::*;
use crate::embeddings::error::{EmbeddingError, EmbeddingResult};

/// Create a text generation provider from chat configuration
pub async fn from_chat_config(
    config: &crucible_config::ChatConfig,
) -> EmbeddingResult<Box<dyn TextGenerationProvider>> {
    use crucible_config::LlmProvider;

    match config.provider {
        LlmProvider::Ollama => {
            let provider_config = TextProviderConfig::Ollama(OllamaConfig {
                base_url: config.llm_endpoint(),
                default_model: Some(config.chat_model()),
                timeout_secs: Some(config.timeout_secs()),
                temperature: Some(config.temperature()),
            });
            create_text_provider(provider_config).await
        }
        LlmProvider::OpenAI => {
            let api_key = std::env::var("OPENAI_API_KEY")
                .map_err(|_| EmbeddingError::ConfigError("OPENAI_API_KEY not set".to_string()))?;

            let provider_config = TextProviderConfig::OpenAI(OpenAIConfig {
                api_key,
                base_url: config.endpoint.clone(),
                default_model: Some(config.chat_model()),
                timeout_secs: Some(config.timeout_secs()),
                temperature: Some(config.temperature()),
                organization: None,
            });
            create_text_provider(provider_config).await
        }
        LlmProvider::Anthropic => Err(EmbeddingError::ConfigError(
            "Anthropic provider not yet implemented".to_string(),
        )),
    }
}

/// Create a text generation provider from app config
pub async fn from_app_config(
    config: &crucible_config::Config,
) -> EmbeddingResult<Box<dyn TextGenerationProvider>> {
    let default_chat = crucible_config::ChatConfig::default();
    let chat_config = config.chat.as_ref().unwrap_or(&default_chat);
    from_chat_config(chat_config).await
}
