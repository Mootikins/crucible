//! Factory for creating text generation providers from configuration

use super::*;
use crate::embeddings::error::{EmbeddingError, EmbeddingResult};
use crucible_config::{EffectiveLlmConfig, LlmProviderConfig, LlmProviderType};

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

/// Create a text generation provider from named provider config
pub async fn from_provider_config(
    config: &LlmProviderConfig,
) -> EmbeddingResult<Box<dyn TextGenerationProvider>> {
    match config.provider_type {
        LlmProviderType::Ollama => {
            let provider_config = TextProviderConfig::Ollama(OllamaConfig {
                base_url: config.endpoint(),
                default_model: Some(config.model()),
                timeout_secs: Some(config.timeout_secs()),
                temperature: Some(config.temperature()),
            });
            create_text_provider(provider_config).await
        }
        LlmProviderType::OpenAI => {
            let api_key = config.api_key().ok_or_else(|| {
                EmbeddingError::ConfigError(
                    "API key not set. Configure api_key in provider config or set OPENAI_API_KEY".to_string(),
                )
            })?;

            let provider_config = TextProviderConfig::OpenAI(OpenAIConfig {
                api_key,
                base_url: Some(config.endpoint()),
                default_model: Some(config.model()),
                timeout_secs: Some(config.timeout_secs()),
                temperature: Some(config.temperature()),
                organization: None,
            });
            create_text_provider(provider_config).await
        }
        LlmProviderType::Anthropic => Err(EmbeddingError::ConfigError(
            "Anthropic provider not yet implemented".to_string(),
        )),
    }
}

/// Create a text generation provider from effective LLM config
/// This is the recommended way to get a provider from Config
pub async fn from_effective_config(
    config: &EffectiveLlmConfig,
) -> EmbeddingResult<Box<dyn TextGenerationProvider>> {
    match config.provider_type {
        LlmProviderType::Ollama => {
            let provider_config = TextProviderConfig::Ollama(OllamaConfig {
                base_url: config.endpoint.clone(),
                default_model: Some(config.model.clone()),
                timeout_secs: Some(config.timeout_secs),
                temperature: Some(config.temperature),
            });
            create_text_provider(provider_config).await
        }
        LlmProviderType::OpenAI => {
            let api_key = config
                .api_key
                .clone()
                .ok_or_else(|| EmbeddingError::ConfigError("OpenAI API key not set".to_string()))?;

            let provider_config = TextProviderConfig::OpenAI(OpenAIConfig {
                api_key,
                base_url: Some(config.endpoint.clone()),
                default_model: Some(config.model.clone()),
                timeout_secs: Some(config.timeout_secs),
                temperature: Some(config.temperature),
                organization: None,
            });
            create_text_provider(provider_config).await
        }
        LlmProviderType::Anthropic => Err(EmbeddingError::ConfigError(
            "Anthropic provider not yet implemented".to_string(),
        )),
    }
}

/// Create a text generation provider from app config using named providers
/// Falls back to ChatConfig if no named providers are configured
pub async fn from_config(
    config: &crucible_config::Config,
) -> EmbeddingResult<Box<dyn TextGenerationProvider>> {
    // First try the new named provider system
    match config.effective_llm_provider() {
        Ok(effective) => from_effective_config(&effective).await,
        Err(_) => {
            // Fall back to legacy ChatConfig
            from_app_config(config).await
        }
    }
}

/// Create a text generation provider by name from config
pub async fn from_config_by_name(
    config: &crucible_config::Config,
    name: &str,
) -> EmbeddingResult<Box<dyn TextGenerationProvider>> {
    let llm_config = config
        .llm_config()
        .ok_or_else(|| EmbeddingError::ConfigError("No LLM providers configured".to_string()))?;

    let provider_config = llm_config.get_provider(name).ok_or_else(|| {
        EmbeddingError::ConfigError(format!("Provider '{}' not found in config", name))
    })?;

    from_provider_config(provider_config).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_config::{LlmConfig, LlmProviderConfig, LlmProviderType};
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_from_provider_config_ollama() {
        let config = LlmProviderConfig {
            provider_type: LlmProviderType::Ollama,
            endpoint: Some("http://localhost:11434".to_string()),
            default_model: Some("llama3.2".to_string()),
            temperature: Some(0.7),
            max_tokens: Some(4096),
            timeout_secs: Some(120),
            api_key: None,
        };

        let provider = from_provider_config(&config).await;
        assert!(provider.is_ok());

        let provider = provider.unwrap();
        assert_eq!(provider.provider_name(), "Ollama");
        assert_eq!(provider.default_model(), "llama3.2");
    }

    #[tokio::test]
    async fn test_from_provider_config_openai_missing_api_key() {
        let config = LlmProviderConfig {
            provider_type: LlmProviderType::OpenAI,
            endpoint: Some("https://api.openai.com/v1".to_string()),
            default_model: Some("gpt-4o".to_string()),
            temperature: Some(0.7),
            max_tokens: Some(4096),
            timeout_secs: Some(120),
            api_key: None,
        };

        let provider = from_provider_config(&config).await;
        assert!(provider.is_err());
        match provider {
            Err(EmbeddingError::ConfigError(msg)) => {
                assert!(msg.contains("API key not set"));
            }
            _ => panic!("Expected ConfigError for missing API key"),
        }
    }

    #[tokio::test]
    async fn test_from_provider_config_openai_with_api_key() {
        // Set up environment variable
        std::env::set_var("TEST_OPENAI_KEY", "test-key-123");

        let config = LlmProviderConfig {
            provider_type: LlmProviderType::OpenAI,
            endpoint: Some("https://api.openai.com/v1".to_string()),
            default_model: Some("gpt-4o".to_string()),
            temperature: Some(0.7),
            max_tokens: Some(4096),
            timeout_secs: Some(120),
            api_key: Some("TEST_OPENAI_KEY".to_string()),
        };

        let provider = from_provider_config(&config).await;
        assert!(provider.is_ok());

        let provider = provider.unwrap();
        assert_eq!(provider.provider_name(), "OpenAI");
        assert_eq!(provider.default_model(), "gpt-4o");

        // Clean up
        std::env::remove_var("TEST_OPENAI_KEY");
    }

    #[tokio::test]
    async fn test_from_provider_config_anthropic_not_implemented() {
        let config = LlmProviderConfig {
            provider_type: LlmProviderType::Anthropic,
            endpoint: None,
            default_model: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
        };

        let provider = from_provider_config(&config).await;
        assert!(provider.is_err());
        match provider {
            Err(EmbeddingError::ConfigError(msg)) => {
                assert!(msg.contains("Anthropic provider not yet implemented"));
            }
            _ => panic!("Expected ConfigError for Anthropic provider"),
        }
    }

    #[tokio::test]
    async fn test_from_effective_config_ollama() {
        let effective = EffectiveLlmConfig {
            key: "local".to_string(),
            provider_type: LlmProviderType::Ollama,
            endpoint: "http://localhost:11434".to_string(),
            model: "llama3.2".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            timeout_secs: 120,
            api_key: None,
        };

        let provider = from_effective_config(&effective).await;
        assert!(provider.is_ok());

        let provider = provider.unwrap();
        assert_eq!(provider.provider_name(), "Ollama");
        assert_eq!(provider.default_model(), "llama3.2");
    }

    #[tokio::test]
    async fn test_from_effective_config_openai_with_key() {
        let effective = EffectiveLlmConfig {
            key: "cloud".to_string(),
            provider_type: LlmProviderType::OpenAI,
            endpoint: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            timeout_secs: 120,
            api_key: Some("test-key-456".to_string()),
        };

        let provider = from_effective_config(&effective).await;
        assert!(provider.is_ok());

        let provider = provider.unwrap();
        assert_eq!(provider.provider_name(), "OpenAI");
        assert_eq!(provider.default_model(), "gpt-4o");
    }

    #[tokio::test]
    async fn test_from_effective_config_openai_missing_key() {
        let effective = EffectiveLlmConfig {
            key: "cloud".to_string(),
            provider_type: LlmProviderType::OpenAI,
            endpoint: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o".to_string(),
            temperature: 0.7,
            max_tokens: 4096,
            timeout_secs: 120,
            api_key: None,
        };

        let provider = from_effective_config(&effective).await;
        assert!(provider.is_err());
        match provider {
            Err(EmbeddingError::ConfigError(msg)) => {
                assert!(msg.contains("OpenAI API key not set"));
            }
            _ => panic!("Expected ConfigError for missing API key"),
        }
    }

    #[tokio::test]
    async fn test_from_config_by_name_success() {
        let mut providers = HashMap::new();
        providers.insert(
            "local".to_string(),
            LlmProviderConfig {
                provider_type: LlmProviderType::Ollama,
                endpoint: Some("http://localhost:11434".to_string()),
                default_model: Some("llama3.2".to_string()),
                temperature: Some(0.7),
                max_tokens: Some(4096),
                timeout_secs: Some(120),
                api_key: None,
            },
        );

        let llm_config = LlmConfig {
            default: Some("local".to_string()),
            providers,
        };

        let config = crucible_config::Config {
            llm: Some(llm_config),
            ..Default::default()
        };

        let provider = from_config_by_name(&config, "local").await;
        assert!(provider.is_ok());

        let provider = provider.unwrap();
        assert_eq!(provider.provider_name(), "Ollama");
        assert_eq!(provider.default_model(), "llama3.2");
    }

    #[tokio::test]
    async fn test_from_config_by_name_not_found() {
        let providers = HashMap::new();

        let llm_config = LlmConfig {
            default: None,
            providers,
        };

        let config = crucible_config::Config {
            llm: Some(llm_config),
            ..Default::default()
        };

        let provider = from_config_by_name(&config, "nonexistent").await;
        assert!(provider.is_err());
        match provider {
            Err(EmbeddingError::ConfigError(msg)) => {
                assert!(msg.contains("Provider 'nonexistent' not found"));
            }
            _ => panic!("Expected ConfigError for provider not found"),
        }
    }

    #[tokio::test]
    async fn test_from_config_by_name_no_llm_config() {
        let config = crucible_config::Config {
            llm: None,
            ..Default::default()
        };

        let provider = from_config_by_name(&config, "local").await;
        assert!(provider.is_err());
        match provider {
            Err(EmbeddingError::ConfigError(msg)) => {
                assert!(msg.contains("No LLM providers configured"));
            }
            _ => panic!("Expected ConfigError for no LLM config"),
        }
    }
}
