//! Daemon-side embedding provider factory
//!
//! Creates embedding providers from `EmbeddingProviderConfig` with lazy initialization
//! and caching. Providers are expensive to create (FastEmbed loads model weights,
//! remote providers need connection setup), so we cache them keyed by config identity.
//!
//! This factory does NOT block daemon startup — providers are created on first use.

use anyhow::Result;
use crucible_config::EmbeddingProviderConfig;
use crucible_core::enrichment::EmbeddingProvider;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::trace;

/// Cache for embedding providers keyed by configuration identity.
/// Avoids recreating providers on every enrichment request.
static EMBEDDING_PROVIDER_CACHE: Lazy<Mutex<HashMap<String, Arc<dyn EmbeddingProvider>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Generate a stable cache key from an `EmbeddingProviderConfig`.
///
/// Keyed by provider type, endpoint, and model name — the triple that
/// uniquely identifies a provider instance.
fn config_cache_key(config: &EmbeddingProviderConfig) -> String {
    format!(
        "{:?}|{}|{}",
        config.provider_type(),
        config.endpoint(),
        config.model_name()
    )
}

/// Get or create an embedding provider from config (cached).
///
/// On first call for a given config, creates the provider via `crucible_llm`.
/// Subsequent calls with the same provider type + endpoint + model return
/// the cached `Arc`.
///
/// # Lazy Initialization
///
/// This function is only called when enrichment actually runs — it does
/// NOT execute during daemon startup.
///
/// # Supported Backends
///
/// - **Ollama** — local or remote Ollama server
/// - **FastEmbed** — local ONNX inference (requires `fastembed` feature on `crucible-llm`)
/// - **OpenAI** — OpenAI embedding API
/// - Any other backend supported by `crucible_llm::embeddings::create_provider`
pub async fn get_or_create_embedding_provider(
    config: &EmbeddingProviderConfig,
) -> Result<Arc<dyn EmbeddingProvider>> {
    let cache_key = config_cache_key(config);

    // Fast path: return cached provider
    {
        let cache = EMBEDDING_PROVIDER_CACHE.lock().unwrap();
        if let Some(cached) = cache.get(&cache_key) {
            trace!(key = %cache_key, "Using cached embedding provider");
            return Ok(cached.clone());
        }
    }

    trace!(key = %cache_key, "Creating new embedding provider");
    let provider: Arc<dyn EmbeddingProvider> =
        crucible_llm::embeddings::create_provider(config.clone()).await?;

    // Store in cache
    {
        let mut cache = EMBEDDING_PROVIDER_CACHE.lock().unwrap();
        cache.insert(cache_key, provider.clone());
    }

    Ok(provider)
}

/// Clear the embedding provider cache.
///
/// Useful for testing or when config changes require fresh providers.
pub fn clear_embedding_provider_cache() {
    EMBEDDING_PROVIDER_CACHE.lock().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_cache_key_deterministic() {
        let config = EmbeddingProviderConfig::ollama(None, None);
        let key1 = config_cache_key(&config);
        let key2 = config_cache_key(&config);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_config_cache_key_varies_by_model() {
        let a = EmbeddingProviderConfig::ollama(None, Some("model-a".into()));
        let b = EmbeddingProviderConfig::ollama(None, Some("model-b".into()));
        assert_ne!(config_cache_key(&a), config_cache_key(&b));
    }

    #[test]
    fn test_config_cache_key_varies_by_endpoint() {
        let a = EmbeddingProviderConfig::ollama(Some("http://localhost:11434".into()), None);
        let b = EmbeddingProviderConfig::ollama(Some("http://remote:11434".into()), None);
        assert_ne!(config_cache_key(&a), config_cache_key(&b));
    }

    #[test]
    fn test_config_cache_key_varies_by_provider_type() {
        let ollama = EmbeddingProviderConfig::ollama(None, None);
        let fastembed = EmbeddingProviderConfig::fastembed(None, None, None);
        assert_ne!(config_cache_key(&ollama), config_cache_key(&fastembed));
    }

    #[test]
    fn test_clear_cache_no_panic() {
        clear_embedding_provider_cache();
        clear_embedding_provider_cache(); // idempotent
    }
}