//! Factory functions for creating unified providers
//!
//! These factory functions create providers from the new unified `ProviderConfig`
//! and `ProvidersConfig` types, wrapping them with the appropriate adapters.

use anyhow::{anyhow, Result};
use crucible_config::{BackendType, ProviderConfig, ProvidersConfig};
use crucible_core::traits::{CanChat, CanEmbed, Provider};
use std::sync::Arc;

use super::adapters::{ChatProviderAdapter, EmbeddingProviderAdapter, UnifiedProvider};
use crate::embeddings::{create_provider as create_embedding_provider_legacy, EmbeddingConfig};
use crate::text_generation::{
    create_text_provider, OllamaConfig as TextOllamaConfig, OpenAIConfig as TextOpenAIConfig,
    TextProviderConfig,
};

/// Create a unified provider from configuration
///
/// This creates a provider that implements the base `Provider` trait.
/// Use `create_embedding_provider_unified` or `create_chat_provider_unified`
/// if you need specific capabilities.
pub async fn create_unified_provider(
    name: &str,
    config: &ProviderConfig,
) -> Result<Arc<dyn Provider>> {
    // Determine what capabilities we can provide
    let supports_embed = config.supports_embeddings();
    let supports_chat = config.supports_chat();

    match (supports_embed, supports_chat) {
        (true, true) => {
            // Create both providers and wrap in UnifiedProvider
            let embed_provider = create_embedding_provider_from_config(config).await?;
            let chat_provider = create_chat_provider_from_config(config).await?;
            Ok(Arc::new(UnifiedProvider::new(
                name,
                config.backend.clone(),
                config.endpoint.clone(),
                embed_provider,
                chat_provider,
            )))
        }
        (true, false) => {
            // Embedding only
            let provider = create_embedding_provider_from_config(config).await?;
            Ok(Arc::new(EmbeddingProviderAdapter::new(
                name,
                config.backend.clone(),
                config.endpoint.clone(),
                provider,
            )))
        }
        (false, true) => {
            // Chat only
            let provider = create_chat_provider_from_config(config).await?;
            Ok(Arc::new(ChatProviderAdapter::new(
                name,
                config.backend.clone(),
                config.endpoint.clone(),
                provider,
            )))
        }
        (false, false) => Err(anyhow!(
            "Backend {:?} supports neither embeddings nor chat",
            config.backend
        )),
    }
}

/// Create an embedding provider from configuration
pub async fn create_embedding_provider_unified(
    name: &str,
    config: &ProviderConfig,
) -> Result<Arc<dyn CanEmbed>> {
    if !config.supports_embeddings() {
        return Err(anyhow!(
            "Backend {:?} does not support embeddings",
            config.backend
        ));
    }

    let provider = create_embedding_provider_from_config(config).await?;
    Ok(Arc::new(EmbeddingProviderAdapter::new(
        name,
        config.backend.clone(),
        config.endpoint.clone(),
        provider,
    )))
}

/// Create a chat provider from configuration
pub async fn create_chat_provider_unified(
    name: &str,
    config: &ProviderConfig,
) -> Result<Arc<dyn CanChat>> {
    if !config.supports_chat() {
        return Err(anyhow!(
            "Backend {:?} does not support chat",
            config.backend
        ));
    }

    let provider = create_chat_provider_from_config(config).await?;
    Ok(Arc::new(ChatProviderAdapter::new(
        name,
        config.backend.clone(),
        config.endpoint.clone(),
        provider,
    )))
}

/// Create a provider by name from a ProvidersConfig
pub async fn create_provider_by_name(
    providers: &ProvidersConfig,
    name: &str,
) -> Result<Arc<dyn Provider>> {
    let config = providers
        .get(name)
        .ok_or_else(|| anyhow!("Provider '{}' not found in configuration", name))?;

    create_unified_provider(name, config).await
}

// === Internal helper functions ===

/// Create a legacy embedding provider from unified config
async fn create_embedding_provider_from_config(
    config: &ProviderConfig,
) -> Result<Arc<dyn crate::embeddings::EmbeddingProvider>> {
    let legacy_config = convert_to_embedding_config(config)?;
    create_embedding_provider_legacy(legacy_config)
        .await
        .map_err(|e| anyhow!("Failed to create embedding provider: {}", e))
}

/// Create a legacy text generation provider from unified config
async fn create_chat_provider_from_config(
    config: &ProviderConfig,
) -> Result<Arc<dyn crate::text_generation::TextGenerationProvider>> {
    let legacy_config = convert_to_text_provider_config(config)?;
    let boxed = create_text_provider(legacy_config)
        .await
        .map_err(|e| anyhow!("Failed to create chat provider: {}", e))?;
    // Convert Box to Arc
    Ok(Arc::from(boxed))
}

/// Convert unified ProviderConfig to legacy EmbeddingConfig
fn convert_to_embedding_config(config: &ProviderConfig) -> Result<EmbeddingConfig> {
    let model = config
        .embedding_model()
        .ok_or_else(|| anyhow!("No embedding model specified"))?;
    let endpoint = config.endpoint();

    match config.backend {
        BackendType::Ollama => Ok(EmbeddingConfig::ollama(endpoint, Some(model))),
        BackendType::OpenAI => {
            let api_key = config
                .api_key()
                .ok_or_else(|| anyhow!("OpenAI requires an API key for embeddings"))?;
            Ok(EmbeddingConfig::openai(api_key, Some(model)))
        }
        BackendType::FastEmbed => Ok(EmbeddingConfig::fastembed(Some(model), None, None)),
        BackendType::Cohere => {
            // Cohere embedding not yet supported via convenience function
            Err(anyhow!(
                "Cohere embedding not yet supported in unified factory"
            ))
        }
        BackendType::Mock => Ok(EmbeddingConfig::mock(None)),
        _ => Err(anyhow!(
            "Backend {:?} not yet supported for embeddings in unified factory",
            config.backend
        )),
    }
}

/// Convert unified ProviderConfig to legacy TextProviderConfig
fn convert_to_text_provider_config(config: &ProviderConfig) -> Result<TextProviderConfig> {
    let model = config.chat_model();
    let endpoint = config.endpoint().unwrap_or_default();

    match config.backend {
        BackendType::Ollama => Ok(TextProviderConfig::Ollama(TextOllamaConfig {
            base_url: endpoint,
            default_model: model,
            timeout_secs: Some(config.timeout_secs),
            temperature: config.temperature,
        })),
        BackendType::OpenAI => {
            let api_key = config
                .api_key()
                .ok_or_else(|| anyhow!("OpenAI requires an API key"))?;
            Ok(TextProviderConfig::OpenAI(TextOpenAIConfig {
                api_key,
                base_url: Some(endpoint),
                default_model: model,
                timeout_secs: Some(config.timeout_secs),
                temperature: config.temperature,
                organization: None,
            }))
        }
        BackendType::Anthropic => {
            // Anthropic not yet supported in text_generation - would need new variant
            Err(anyhow!(
                "Anthropic text generation not yet implemented in unified factory"
            ))
        }
        _ => Err(anyhow!(
            "Backend {:?} not yet supported for chat in unified factory",
            config.backend
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_ollama_embedding_config() {
        let config = ProviderConfig::new(BackendType::Ollama)
            .with_endpoint("http://localhost:11434")
            .with_embedding_model("nomic-embed-text");

        let legacy = convert_to_embedding_config(&config).unwrap();
        assert!(matches!(legacy, EmbeddingConfig::Ollama(_)));
    }

    #[test]
    fn test_convert_fastembed_config() {
        let config = ProviderConfig::new(BackendType::FastEmbed)
            .with_embedding_model("BAAI/bge-small-en-v1.5");

        let legacy = convert_to_embedding_config(&config).unwrap();
        assert!(matches!(legacy, EmbeddingConfig::FastEmbed(_)));
    }

    #[test]
    fn test_convert_ollama_chat_config() {
        let config = ProviderConfig::new(BackendType::Ollama)
            .with_endpoint("http://localhost:11434")
            .with_chat_model("llama3.2")
            .with_timeout(120);

        let legacy = convert_to_text_provider_config(&config).unwrap();
        assert!(matches!(legacy, TextProviderConfig::Ollama(_)));
    }

    #[test]
    fn test_unsupported_backend_error() {
        let config = ProviderConfig::new(BackendType::Anthropic);
        let result = convert_to_embedding_config(&config);
        assert!(result.is_err());
    }
}
