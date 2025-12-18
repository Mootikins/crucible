//! Model cache with TTL for provider model listings
//!
//! This module provides a time-based cache for model listings from providers,
//! reducing redundant API calls while ensuring data freshness.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;

use crucible_core::traits::provider::UnifiedModelInfo;

/// Cached model listing with timestamp
#[derive(Debug, Clone)]
pub struct CachedModels {
    /// The cached model list
    pub models: Vec<UnifiedModelInfo>,
    /// When this was cached
    pub cached_at: SystemTime,
}

impl CachedModels {
    /// Check if this cache entry has expired
    pub fn is_expired(&self, ttl: Duration) -> bool {
        SystemTime::now()
            .duration_since(self.cached_at)
            .map(|age| age > ttl)
            .unwrap_or(true) // If time went backwards, consider it expired
    }
}

/// Model cache with TTL per provider
///
/// ## Example
/// ```no_run
/// use std::time::Duration;
/// use crucible_llm::unified::ModelCache;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
/// let cache = ModelCache::new(Duration::from_secs(300));
///
/// // First call fetches from provider
/// let models = cache.get_or_fetch("ollama", || async {
///     // fetch from provider
///     Ok(vec![])
/// }).await?;
///
/// // Second call uses cached result (within TTL)
/// let models = cache.get_or_fetch("ollama", || async {
///     // This won't be called
///     Ok(vec![])
/// }).await?;
/// # Ok(())
/// # }
/// ```
pub struct ModelCache {
    cache: Arc<RwLock<HashMap<String, CachedModels>>>,
    default_ttl: Duration,
}

impl ModelCache {
    /// Create a new model cache with the given default TTL
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            default_ttl,
        }
    }

    /// Get cached models or fetch them if expired/missing
    ///
    /// ## Arguments
    /// * `provider_name` - Unique identifier for the provider (e.g., "ollama", "openai")
    /// * `fetch_fn` - Async function to fetch models if cache miss or expired
    ///
    /// ## Returns
    /// Cached or freshly fetched model list
    pub async fn get_or_fetch<F, Fut>(
        &self,
        provider_name: &str,
        fetch_fn: F,
    ) -> Result<Vec<UnifiedModelInfo>, Box<dyn std::error::Error + Send + Sync>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<
            Output = Result<Vec<UnifiedModelInfo>, Box<dyn std::error::Error + Send + Sync>>,
        >,
    {
        // Check cache first (read lock)
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(provider_name) {
                if !cached.is_expired(self.default_ttl) {
                    return Ok(cached.models.clone());
                }
            }
        }

        // Cache miss or expired - fetch new data
        let models = fetch_fn().await?;

        // Update cache (write lock)
        {
            let mut cache = self.cache.write().await;
            cache.insert(
                provider_name.to_string(),
                CachedModels {
                    models: models.clone(),
                    cached_at: SystemTime::now(),
                },
            );
        }

        Ok(models)
    }

    /// Invalidate cache for a specific provider
    pub async fn invalidate(&self, provider_name: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(provider_name);
    }

    /// Invalidate all cached models
    pub async fn invalidate_all(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_config::BackendType;
    use crucible_core::traits::provider::ModelCapability;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Helper to create a test model
    fn test_model(id: &str) -> UnifiedModelInfo {
        UnifiedModelInfo::new(id, BackendType::Ollama)
            .with_capabilities(vec![ModelCapability::Chat])
    }

    #[tokio::test]
    async fn test_cache_returns_cached_within_ttl() {
        let cache = ModelCache::new(Duration::from_secs(300));
        let call_count = Arc::new(AtomicUsize::new(0));

        // First call - should fetch
        let count1 = Arc::clone(&call_count);
        let models1 = cache
            .get_or_fetch("ollama", || async move {
                count1.fetch_add(1, Ordering::SeqCst);
                Ok(vec![test_model("model1")])
            })
            .await
            .unwrap();

        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "First call should fetch"
        );
        assert_eq!(models1.len(), 1);
        assert_eq!(models1[0].id, "model1");

        // Second call - should use cache (call_count still 1)
        let count2 = Arc::clone(&call_count);
        let models2 = cache
            .get_or_fetch("ollama", || async move {
                count2.fetch_add(1, Ordering::SeqCst);
                Ok(vec![test_model("model2")]) // Different model to prove cache was used
            })
            .await
            .unwrap();

        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "Second call should use cache, not fetch"
        );
        assert_eq!(models2.len(), 1);
        assert_eq!(
            models2[0].id, "model1",
            "Should return cached model, not new model"
        );
    }

    #[tokio::test]
    async fn test_cache_refetches_after_ttl() {
        let cache = ModelCache::new(Duration::from_millis(10)); // 10ms TTL
        let call_count = Arc::new(AtomicUsize::new(0));

        // First call - should fetch
        let count1 = Arc::clone(&call_count);
        let models1 = cache
            .get_or_fetch("ollama", || async move {
                count1.fetch_add(1, Ordering::SeqCst);
                Ok(vec![test_model("model1")])
            })
            .await
            .unwrap();

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
        assert_eq!(models1[0].id, "model1");

        // Wait for TTL to expire
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Second call - should fetch again
        let count2 = Arc::clone(&call_count);
        let models2 = cache
            .get_or_fetch("ollama", || async move {
                count2.fetch_add(1, Ordering::SeqCst);
                Ok(vec![test_model("model2")])
            })
            .await
            .unwrap();

        assert_eq!(
            call_count.load(Ordering::SeqCst),
            2,
            "Should fetch again after TTL"
        );
        assert_eq!(models2[0].id, "model2", "Should get new model");
    }

    #[tokio::test]
    async fn test_separate_caches_per_provider() {
        let cache = ModelCache::new(Duration::from_secs(300));
        let ollama_count = Arc::new(AtomicUsize::new(0));
        let openai_count = Arc::new(AtomicUsize::new(0));

        // Fetch from ollama
        let count1 = Arc::clone(&ollama_count);
        let ollama_models = cache
            .get_or_fetch("ollama", || async move {
                count1.fetch_add(1, Ordering::SeqCst);
                Ok(vec![test_model("ollama-model")])
            })
            .await
            .unwrap();

        // Fetch from openai
        let count2 = Arc::clone(&openai_count);
        let openai_models = cache
            .get_or_fetch("openai", || async move {
                count2.fetch_add(1, Ordering::SeqCst);
                Ok(vec![test_model("openai-model")])
            })
            .await
            .unwrap();

        assert_eq!(ollama_count.load(Ordering::SeqCst), 1);
        assert_eq!(openai_count.load(Ordering::SeqCst), 1);
        assert_eq!(ollama_models[0].id, "ollama-model");
        assert_eq!(openai_models[0].id, "openai-model");

        // Fetch from ollama again - should use cache
        let count3 = Arc::clone(&ollama_count);
        cache
            .get_or_fetch("ollama", || async move {
                count3.fetch_add(1, Ordering::SeqCst);
                Ok(vec![test_model("different")])
            })
            .await
            .unwrap();

        assert_eq!(
            ollama_count.load(Ordering::SeqCst),
            1,
            "Ollama cache should be separate from OpenAI"
        );
    }

    #[tokio::test]
    async fn test_invalidate_single_provider() {
        let cache = ModelCache::new(Duration::from_secs(300));
        let ollama_count = Arc::new(AtomicUsize::new(0));
        let openai_count = Arc::new(AtomicUsize::new(0));

        // Cache both providers
        let count1 = Arc::clone(&ollama_count);
        cache
            .get_or_fetch("ollama", || async move {
                count1.fetch_add(1, Ordering::SeqCst);
                Ok(vec![test_model("ollama-model")])
            })
            .await
            .unwrap();

        let count2 = Arc::clone(&openai_count);
        cache
            .get_or_fetch("openai", || async move {
                count2.fetch_add(1, Ordering::SeqCst);
                Ok(vec![test_model("openai-model")])
            })
            .await
            .unwrap();

        assert_eq!(ollama_count.load(Ordering::SeqCst), 1);
        assert_eq!(openai_count.load(Ordering::SeqCst), 1);

        // Invalidate only ollama
        cache.invalidate("ollama").await;

        // Fetch ollama again - should refetch
        let count3 = Arc::clone(&ollama_count);
        cache
            .get_or_fetch("ollama", || async move {
                count3.fetch_add(1, Ordering::SeqCst);
                Ok(vec![test_model("new-ollama")])
            })
            .await
            .unwrap();

        // Fetch openai again - should still be cached
        let count4 = Arc::clone(&openai_count);
        cache
            .get_or_fetch("openai", || async move {
                count4.fetch_add(1, Ordering::SeqCst);
                Ok(vec![test_model("different")])
            })
            .await
            .unwrap();

        assert_eq!(
            ollama_count.load(Ordering::SeqCst),
            2,
            "Ollama should be refetched after invalidation"
        );
        assert_eq!(
            openai_count.load(Ordering::SeqCst),
            1,
            "OpenAI should still be cached"
        );
    }

    #[tokio::test]
    async fn test_invalidate_all() {
        let cache = ModelCache::new(Duration::from_secs(300));
        let ollama_count = Arc::new(AtomicUsize::new(0));
        let openai_count = Arc::new(AtomicUsize::new(0));

        // Cache both providers
        let count1 = Arc::clone(&ollama_count);
        cache
            .get_or_fetch("ollama", || async move {
                count1.fetch_add(1, Ordering::SeqCst);
                Ok(vec![test_model("ollama-model")])
            })
            .await
            .unwrap();

        let count2 = Arc::clone(&openai_count);
        cache
            .get_or_fetch("openai", || async move {
                count2.fetch_add(1, Ordering::SeqCst);
                Ok(vec![test_model("openai-model")])
            })
            .await
            .unwrap();

        assert_eq!(ollama_count.load(Ordering::SeqCst), 1);
        assert_eq!(openai_count.load(Ordering::SeqCst), 1);

        // Invalidate all
        cache.invalidate_all().await;

        // Both should refetch
        let count3 = Arc::clone(&ollama_count);
        cache
            .get_or_fetch("ollama", || async move {
                count3.fetch_add(1, Ordering::SeqCst);
                Ok(vec![test_model("new-ollama")])
            })
            .await
            .unwrap();

        let count4 = Arc::clone(&openai_count);
        cache
            .get_or_fetch("openai", || async move {
                count4.fetch_add(1, Ordering::SeqCst);
                Ok(vec![test_model("new-openai")])
            })
            .await
            .unwrap();

        assert_eq!(
            ollama_count.load(Ordering::SeqCst),
            2,
            "Ollama should be refetched after invalidate_all"
        );
        assert_eq!(
            openai_count.load(Ordering::SeqCst),
            2,
            "OpenAI should be refetched after invalidate_all"
        );
    }
}
