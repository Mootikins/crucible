//! Enrichment service factory - creates DefaultEnrichmentService
//! Phase 5: Uses public factory function instead of importing concrete service.
//! Includes caching for embedding providers to avoid repeated initialization.

use crate::config::CliConfig;
use anyhow::Result;
use crucible_config::{BackendType, EmbeddingProviderConfig, OllamaConfig, OpenAIConfig};
use crucible_core::enrichment::{EmbeddingProvider, EnrichmentService};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{trace, warn};

/// Cache for embedding providers keyed by configuration hash
/// This avoids recreating embedding providers (which can be expensive for
/// FastEmbed model loading or remote API connections)
static EMBEDDING_PROVIDER_CACHE: Lazy<Mutex<HashMap<String, Arc<dyn EmbeddingProvider>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Generate a cache key from embedding configuration
fn embedding_config_cache_key(config: &CliConfig) -> String {
    let ec = embedding_provider_config_from_cli(config);
    format!(
        "llm|{:?}|{}|{}",
        ec.provider_type(),
        ec.endpoint(),
        ec.model_name()
    )
}

pub fn embedding_provider_config_from_cli(config: &CliConfig) -> EmbeddingProviderConfig {
    // Check if enrichment provider is explicitly configured — use it directly
    if let Some(enrichment) = &config.enrichment {
        return enrichment.provider.clone();
    }

    // Fall back to deriving from the LLM provider config
    let effective = match config.effective_llm_provider() {
        Ok(cfg) => cfg,
        Err(err) => {
            warn!(error = %err, "No effective llm provider; using default ollama embedding config");
            return EmbeddingProviderConfig::ollama(None, None);
        }
    };

    match effective.provider_type {
        BackendType::OpenAI => {
            let cfg = OpenAIConfig {
                base_url: effective.endpoint,
                api_key: effective.api_key.unwrap_or_default(),
                model: effective.model,
                ..Default::default()
            };
            EmbeddingProviderConfig::OpenAI(cfg)
        }
        BackendType::Ollama => {
            let cfg = OllamaConfig {
                base_url: effective.endpoint,
                model: effective.model,
                ..Default::default()
            };
            EmbeddingProviderConfig::Ollama(cfg)
        }
        unsupported => {
            warn!(provider = ?unsupported, "Provider does not support embeddings; using default ollama config");
            EmbeddingProviderConfig::ollama(None, None)
        }
    }
}

/// Get or create an embedding provider (cached)
///
/// This function caches embedding providers to avoid expensive repeated
/// initialization. FastEmbed requires loading model weights, and remote
/// providers may need connection setup.
///
pub async fn get_or_create_embedding_provider(
    config: &CliConfig,
) -> Result<Arc<dyn EmbeddingProvider>> {
    let cache_key = embedding_config_cache_key(config);

    // Check cache first
    {
        let cache = EMBEDDING_PROVIDER_CACHE.lock().unwrap();
        if let Some(cached) = cache.get(&cache_key) {
            trace!("Using cached embedding provider for key: {}", cache_key);
            return Ok(cached.clone());
        }
    }

    trace!("Creating new embedding provider for key: {}", cache_key);
    let embedding_config = embedding_provider_config_from_cli(config);

    let provider: Arc<dyn EmbeddingProvider> =
        crucible_llm::embeddings::create_provider(embedding_config).await?;

    // Cache it
    {
        let mut cache = EMBEDDING_PROVIDER_CACHE.lock().unwrap();
        cache.insert(cache_key, provider.clone());
    }

    Ok(provider)
}

/// Create DefaultEnrichmentService with embedding provider
///
/// Phase 5: Uses public factory function from crucible-enrichment instead of
/// constructing DefaultEnrichmentService directly.
/// Uses cached embedding provider for faster repeated calls.
pub async fn create_default_enrichment_service(
    config: &CliConfig,
) -> Result<Arc<dyn EnrichmentService>> {
    // Use cached embedding provider
    let embedding_provider = get_or_create_embedding_provider(config).await?;

    // Use public factory function from crucible-enrichment
    crucible_enrichment::create_default_enrichment_service(Some(embedding_provider))
}

/// Clear the embedding provider cache (useful for testing)
pub fn clear_embedding_provider_cache() {
    EMBEDDING_PROVIDER_CACHE.lock().unwrap().clear();
}

#[cfg(test)]
fn cache_size() -> usize {
    EMBEDDING_PROVIDER_CACHE.lock().unwrap().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clear_embedding_provider_cache_no_panic() {
        clear_embedding_provider_cache();
        assert_eq!(cache_size(), 0);
    }

    #[test]
    fn test_clear_cache_is_idempotent() {
        clear_embedding_provider_cache();
        clear_embedding_provider_cache();
        clear_embedding_provider_cache();
        assert_eq!(cache_size(), 0);
    }

    #[test]
    fn test_embedding_config_cache_key_format() {
        let config = CliConfig::default();
        let key = embedding_config_cache_key(&config);
        assert!(!key.is_empty());
        assert!(key.contains("|"));
    }

    #[test]
    fn test_embedding_config_cache_key_deterministic() {
        let config = CliConfig::default();
        let key1 = embedding_config_cache_key(&config);
        let key2 = embedding_config_cache_key(&config);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_embedding_config_cache_key_varies_with_provider() {
        let mut config1 = CliConfig::default();
        let mut config2 = CliConfig::default();

        config1.llm.providers.insert(
            "local".to_string(),
            crucible_config::LlmProviderConfig {
                provider_type: BackendType::Ollama,
                endpoint: None,
                default_model: None,
                temperature: None,
                max_tokens: None,
                timeout_secs: None,
                api_key: None,
                available_models: None,
                trust_level: None,
            },
        );
        config2.llm.providers.insert(
            "local".to_string(),
            crucible_config::LlmProviderConfig {
                provider_type: BackendType::Ollama,
                endpoint: None,
                default_model: None,
                temperature: None,
                max_tokens: None,
                timeout_secs: None,
                api_key: None,
                available_models: None,
                trust_level: None,
            },
        );

        config1.llm.default = Some("local".to_string());
        if let Some(provider) = config1.llm.providers.get_mut("local") {
            provider.default_model = Some("model-a".to_string());
        }

        config2.llm.default = Some("local".to_string());
        if let Some(provider) = config2.llm.providers.get_mut("local") {
            provider.default_model = Some("model-b".to_string());
        }

        let key1 = embedding_config_cache_key(&config1);
        let key2 = embedding_config_cache_key(&config2);
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_embedding_config_cache_key_varies_with_url() {
        let mut config1 = CliConfig::default();
        let mut config2 = CliConfig::default();

        config1.llm.providers.insert(
            "local".to_string(),
            crucible_config::LlmProviderConfig {
                provider_type: BackendType::Ollama,
                endpoint: None,
                default_model: None,
                temperature: None,
                max_tokens: None,
                timeout_secs: None,
                api_key: None,
                available_models: None,
                trust_level: None,
            },
        );
        config2.llm.providers.insert(
            "local".to_string(),
            crucible_config::LlmProviderConfig {
                provider_type: BackendType::Ollama,
                endpoint: None,
                default_model: None,
                temperature: None,
                max_tokens: None,
                timeout_secs: None,
                api_key: None,
                available_models: None,
                trust_level: None,
            },
        );

        config1.llm.default = Some("local".to_string());
        if let Some(provider) = config1.llm.providers.get_mut("local") {
            provider.endpoint = Some("http://localhost:11434".to_string());
        }

        config2.llm.default = Some("local".to_string());
        if let Some(provider) = config2.llm.providers.get_mut("local") {
            provider.endpoint = Some("http://localhost:8080".to_string());
        }

        let key1 = embedding_config_cache_key(&config1);
        let key2 = embedding_config_cache_key(&config2);
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_embedding_config_uses_enrichment_provider_when_configured() {
        use crucible_config::EnrichmentConfig;

        let config = CliConfig {
            enrichment: Some(EnrichmentConfig::default()),
            ..Default::default()
        };

        let embedding_config = embedding_provider_config_from_cli(&config);
        assert!(
            matches!(embedding_config, EmbeddingProviderConfig::FastEmbed(_)),
            "Should use FastEmbed from enrichment config, not derive from LLM provider"
        );
    }

    #[test]
    fn test_embedding_config_falls_back_without_enrichment_config() {
        let config = CliConfig::default();
        let _ = embedding_provider_config_from_cli(&config);
    }
}
