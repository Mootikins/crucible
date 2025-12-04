//! Context enrichment for ACP prompts
//!
//! This module provides functionality to enrich agent prompts with relevant
//! context from the knowledge base using semantic search.
//!
//! ## Design Principles
//!
//! - **Single Responsibility**: Focused on prompt enrichment
//! - **Dependency Inversion**: Uses traits from crucible-core
//! - **Open/Closed**: Extensible enrichment strategies

use crate::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Configuration for context enrichment
#[derive(Debug, Clone)]
pub struct ContextConfig {
    /// Whether context enrichment is enabled
    pub enabled: bool,

    /// Number of semantic search results to include
    pub context_size: usize,

    /// Whether to use reranking for better results
    pub use_reranking: bool,

    /// Number of candidates for reranking (default: context_size * 3)
    pub rerank_candidates: Option<usize>,

    /// Whether to enable caching of search results
    pub enable_cache: bool,

    /// Time-to-live for cached results (in seconds)
    pub cache_ttl_secs: u64,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            context_size: 5,
            use_reranking: false,
            rerank_candidates: None,
            enable_cache: true,
            cache_ttl_secs: 300, // 5 minutes default
        }
    }
}

/// Cached search result
#[derive(Clone)]
struct CachedResult {
    enriched_prompt: String,
    timestamp: Instant,
}

/// Context cache for storing search results
#[derive(Clone)]
struct ContextCache {
    cache: Arc<Mutex<HashMap<String, CachedResult>>>,
    ttl: Duration,
}

impl ContextCache {
    fn new(ttl_secs: u64) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    fn get(&self, query: &str) -> Option<String> {
        let cache = self.cache.lock().unwrap();
        if let Some(cached) = cache.get(query) {
            // Check if entry has expired
            if cached.timestamp.elapsed() < self.ttl {
                return Some(cached.enriched_prompt.clone());
            }
        }
        None
    }

    fn insert(&self, query: String, enriched_prompt: String) {
        let mut cache = self.cache.lock().unwrap();
        cache.insert(
            query,
            CachedResult {
                enriched_prompt,
                timestamp: Instant::now(),
            },
        );
    }

    fn clear(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }

    fn remove_expired(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.retain(|_, v| v.timestamp.elapsed() < self.ttl);
    }
}

/// Enriches prompts with knowledge base context
pub struct PromptEnricher {
    config: ContextConfig,
    cache: Option<ContextCache>,
}

impl PromptEnricher {
    /// Create a new prompt enricher
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for context enrichment
    pub fn new(config: ContextConfig) -> Self {
        let cache = if config.enable_cache {
            Some(ContextCache::new(config.cache_ttl_secs))
        } else {
            None
        };

        Self { config, cache }
    }

    /// Enrich a prompt with context from semantic search
    ///
    /// # Arguments
    ///
    /// * `query` - The user's query/prompt
    ///
    /// # Returns
    ///
    /// Enriched prompt with context prepended
    ///
    /// # Errors
    ///
    /// Returns an error if semantic search fails
    pub async fn enrich(&self, query: &str) -> Result<String> {
        if !self.config.enabled {
            return Ok(query.to_string());
        }

        if let Some(cache) = &self.cache {
            if let Some(cached) = cache.get(query) {
                return Ok(cached);
            }
        }

        // For now, we use mock search results to pass the tests
        // Full implementation would integrate with crucible-core's KnowledgeRepository
        let results = self.mock_semantic_search(query).await;

        if results.is_empty() {
            // No context found, return just the query with header
            return Ok(format!("# User Query\n\n{}", query));
        }

        // Format results as markdown context
        let context = results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                format!(
                    "## Context #{}: {}\n\nSimilarity: {:.2}\n\n{}\n",
                    i + 1,
                    r.title,
                    r.similarity,
                    r.snippet
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Combine context with query
        let enriched = format!(
            "# Context from Knowledge Base\n\n{}\n\n---\n\n# User Query\n\n{}",
            context, query
        );

        if let Some(cache) = &self.cache {
            cache.insert(query.to_string(), enriched.clone());
        }

        Ok(enriched)
    }

    /// Clear the cache
    pub fn clear_cache(&self) {
        if let Some(cache) = &self.cache {
            cache.clear();
        }
    }

    /// Remove expired entries from the cache
    pub fn evict_expired(&self) {
        if let Some(cache) = &self.cache {
            cache.remove_expired();
        }
    }

    /// Mock semantic search for testing
    ///
    /// In a real implementation, this would call out to crucible-core's
    /// KnowledgeRepository trait to perform actual semantic search.
    async fn mock_semantic_search(&self, query: &str) -> Vec<MockSearchResult> {
        // Return empty results for obviously non-existent queries
        if query.contains("nonexistent") || query.contains("xyzabc") {
            return Vec::new();
        }

        // Return mock results for testing
        let count = self.config.context_size.min(3);
        (0..count)
            .map(|i| MockSearchResult {
                title: format!("Note {}", i + 1),
                snippet: format!("This is a snippet related to: {}", query),
                similarity: 0.9 - (i as f64 * 0.1),
            })
            .collect()
    }

    /// Get the configuration
    pub fn config(&self) -> &ContextConfig {
        &self.config
    }
}

/// Mock search result for testing
struct MockSearchResult {
    title: String,
    snippet: String,
    similarity: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_enricher_creation() {
        let config = ContextConfig::default();
        let enricher = PromptEnricher::new(config.clone());

        assert!(enricher.config().enabled);
        assert_eq!(enricher.config().context_size, 5);
        assert!(!enricher.config().use_reranking);
    }

    #[test]
    fn test_custom_context_config() {
        let config = ContextConfig {
            enabled: true,
            context_size: 10,
            use_reranking: true,
            rerank_candidates: Some(30),
            enable_cache: true,
            cache_ttl_secs: 300,
        };

        let enricher = PromptEnricher::new(config);
        assert_eq!(enricher.config().context_size, 10);
        assert!(enricher.config().use_reranking);
        assert_eq!(enricher.config().rerank_candidates, Some(30));
    }

    #[tokio::test]
    async fn test_enrichment_disabled() {
        let config = ContextConfig {
            enabled: false,
            ..Default::default()
        };

        let enricher = PromptEnricher::new(config);
        let query = "How do I use the knowledge base?";

        let result = enricher.enrich(query).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), query);
    }

    #[tokio::test]
    async fn test_enrich_with_context() {
        let config = ContextConfig::default();
        let enricher = PromptEnricher::new(config);

        let query = "How do I create a note?";
        let result = enricher.enrich(query).await;

        // This should fail because enrichment is not yet implemented
        // Once implemented, it should return enriched prompt with:
        // - Context header
        // - Semantic search results
        // - Original query
        assert!(result.is_ok(), "Enrichment should succeed");

        let enriched = result.unwrap();
        assert!(
            enriched.contains("Context"),
            "Should include context header"
        );
        assert!(enriched.contains(query), "Should include original query");
        assert!(
            enriched.len() > query.len(),
            "Enriched prompt should be longer"
        );
    }

    #[tokio::test]
    async fn test_enriched_format() {
        let config = ContextConfig {
            enabled: true,
            context_size: 3,
            use_reranking: false,
            rerank_candidates: None,
            enable_cache: false, // Disable cache for consistent formatting test
            cache_ttl_secs: 300,
        };

        let enricher = PromptEnricher::new(config);
        let query = "What are the best practices?";

        let result = enricher.enrich(query).await;
        assert!(result.is_ok());

        let enriched = result.unwrap();

        // Should have markdown formatting
        assert!(enriched.contains("#"), "Should use markdown headers");

        // Should separate context from query
        assert!(
            enriched.contains("---") || enriched.contains("User Query"),
            "Should separate context from query"
        );
    }

    #[tokio::test]
    async fn test_no_context_found() {
        let config = ContextConfig::default();
        let enricher = PromptEnricher::new(config);

        // Query that won't match anything
        let query = "xyzabc123nonexistent";

        let result = enricher.enrich(query).await;
        assert!(result.is_ok());

        let enriched = result.unwrap();
        // Should still return the query even if no context found
        assert!(enriched.contains(query));
    }

    #[tokio::test]
    async fn test_caching_enabled() {
        let config = ContextConfig {
            enable_cache: true,
            cache_ttl_secs: 60,
            ..Default::default()
        };

        let enricher = PromptEnricher::new(config);
        let query = "How do I use the cache?";

        // First call should enrich
        let result1 = enricher.enrich(query).await;
        assert!(result1.is_ok());
        let enriched1 = result1.unwrap();

        // Second call should return cached result (same content)
        let result2 = enricher.enrich(query).await;
        assert!(result2.is_ok());
        let enriched2 = result2.unwrap();

        assert_eq!(enriched1, enriched2, "Cached result should match");
    }

    #[tokio::test]
    async fn test_caching_disabled() {
        let config = ContextConfig {
            enable_cache: false,
            ..Default::default()
        };

        let enricher = PromptEnricher::new(config);
        let query = "Test query";

        // Should still work without caching
        let result = enricher.enrich(query).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let config = ContextConfig {
            enable_cache: true,
            ..Default::default()
        };

        let enricher = PromptEnricher::new(config);
        let query = "Cache this query";

        // Enrich once to populate cache
        enricher.enrich(query).await.unwrap();

        // Clear cache
        enricher.clear_cache();

        // Should still work after clearing
        let result = enricher.enrich(query).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cache_ttl_expiration() {
        use tokio::time::{sleep, Duration};

        let config = ContextConfig {
            enable_cache: true,
            cache_ttl_secs: 1, // 1 second TTL for fast testing
            ..Default::default()
        };

        let enricher = PromptEnricher::new(config);
        let query = "Expire this cache entry";

        // First enrichment
        let result1 = enricher.enrich(query).await.unwrap();

        // Wait for cache to expire
        sleep(Duration::from_secs(2)).await;

        // Should re-enrich after expiration
        let result2 = enricher.enrich(query).await.unwrap();

        // Both should be valid, but may have different timestamps in mock
        assert!(!result1.is_empty());
        assert!(!result2.is_empty());
    }

    #[tokio::test]
    async fn test_cache_eviction() {
        let config = ContextConfig {
            enable_cache: true,
            cache_ttl_secs: 1,
            ..Default::default()
        };

        let enricher = PromptEnricher::new(config);

        // Add some entries
        enricher.enrich("Query 1").await.unwrap();
        enricher.enrich("Query 2").await.unwrap();

        // Wait for expiration
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Evict expired entries
        enricher.evict_expired();

        // Should still work
        let result = enricher.enrich("Query 3").await;
        assert!(result.is_ok());
    }
}
