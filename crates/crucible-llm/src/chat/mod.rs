//! Chat provider implementations

pub mod ollama;
pub mod openai;

// Re-export providers
pub use ollama::OllamaChatProvider;
pub use openai::OpenAIChatProvider;

// Factory function to create providers from config
use crucible_config::{ChatConfig, LlmProvider};
use crucible_core::traits::{ChatProvider, LlmError, LlmResult};

/// Create a chat provider from configuration
pub async fn create_chat_provider(
    config: &ChatConfig,
) -> LlmResult<Box<dyn ChatProvider>> {
    match config.provider {
        LlmProvider::Ollama => {
            let provider = OllamaChatProvider::new(
                config.llm_endpoint(),
                config.chat_model(),
                config.timeout_secs(),
            );
            Ok(Box::new(provider))
        }
        LlmProvider::OpenAI => {
            let api_key = std::env::var("OPENAI_API_KEY")
                .map_err(|_| LlmError::ConfigError("OPENAI_API_KEY not set".to_string()))?;

            let provider = OpenAIChatProvider::new(
                api_key,
                config.endpoint.clone(),
                config.chat_model(),
                config.timeout_secs(),
            );
            Ok(Box::new(provider))
        }
        LlmProvider::Anthropic => Err(LlmError::ConfigError(
            "Anthropic provider not yet implemented".to_string(),
        )),
    }
}

/// Create a chat provider from app config
pub async fn create_from_app_config(
    config: &crucible_config::Config,
) -> LlmResult<Box<dyn ChatProvider>> {
    let default_chat = ChatConfig::default();
    let chat_config = config.chat.as_ref().unwrap_or(&default_chat);
    create_chat_provider(chat_config).await
}
