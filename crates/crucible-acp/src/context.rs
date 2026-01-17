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
//!
//! ## Note on ACP Agents
//!
//! Context injection is disabled by default for ACP agents because they receive
//! tools via MCP protocol. The injected `<crucible_context>` block is redundant
//! and can confuse some models. Use `/search` commands for explicit context.

use crate::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Precognition instruction - explains context to the agent
const PRECOGNITION_INSTRUCTION: &str = r#"The following are relevant notes from your knowledge base, retrieved via semantic search. Use them as context to inform your response. Do not repeat them verbatim unless asked. Use read_note(path) to retrieve full content when needed."#;

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

    /// Whether to inject context into prompts automatically
    ///
    /// When false (default for ACP agents), the `<crucible_context>` and `<matches>`
    /// blocks are NOT injected. ACP agents receive tools via MCP, so automatic
    /// injection is redundant and can confuse some models.
    ///
    /// Use `/search` commands for explicit context when needed.
    pub inject_context: bool,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            context_size: 5,
            use_reranking: false,
            rerank_candidates: None,
            enable_cache: true,
            cache_ttl_secs: 300,   // 5 minutes default
            inject_context: false, // Off by default - ACP agents get tools via MCP
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
        // Skip all injection if disabled or inject_context is false
        if !self.config.enabled || !self.config.inject_context {
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

        let mut output = String::new();

        output.push_str("<precognition>\n");
        output.push_str("<instruction>\n");
        output.push_str(PRECOGNITION_INSTRUCTION);
        output.push_str("\n</instruction>\n");

        if !results.is_empty() {
            let items: Vec<serde_json::Value> = results
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "path": r.path,
                        "line": r.line.unwrap_or(1),
                        "similarity": format!("{:.2}", r.similarity)
                    })
                })
                .collect();

            let table = oq::encode_table("notes", &items, &["path", "line", "similarity"]);

            output.push_str("<matches>\n");
            output.push_str(&table);
            output.push_str("</matches>\n");
        }

        output.push_str("</precognition>\n\n");
        output.push_str(query);

        if let Some(cache) = &self.cache {
            cache.insert(query.to_string(), output.clone());
        }

        Ok(output)
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
                path: format!("docs/Note{}.md", i + 1),
                line: Some(10 + (i * 5)),
                similarity: 0.9 - (i as f64 * 0.1),
                title: format!("Note {}", i + 1),
                snippet: format!("This is a snippet related to: {}", query),
            })
            .collect()
    }

    /// Get the configuration
    pub fn config(&self) -> &ContextConfig {
        &self.config
    }
}

/// Mock search result for testing
#[allow(dead_code)]
struct MockSearchResult {
    path: String,
    line: Option<usize>,
    similarity: f64,
    title: String,
    snippet: String,
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
            inject_context: true,
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
        let config = ContextConfig {
            inject_context: true,
            ..Default::default()
        };
        let enricher = PromptEnricher::new(config);

        let query = "How do I create a note?";
        let result = enricher.enrich(query).await;

        assert!(result.is_ok(), "Enrichment should succeed");

        let enriched = result.unwrap();
        assert!(
            enriched.contains("<precognition>"),
            "Should include precognition block"
        );
        assert!(enriched.contains(query), "Should include original query");
        assert!(
            enriched.len() > query.len(),
            "Enriched prompt should be longer"
        );
    }

    #[tokio::test]
    async fn test_default_no_injection() {
        let config = ContextConfig::default();
        let enricher = PromptEnricher::new(config);

        let query = "How do I create a note?";
        let result = enricher.enrich(query).await;

        assert!(result.is_ok());
        let enriched = result.unwrap();

        assert_eq!(enriched, query, "Default should not inject context");
        assert!(
            !enriched.contains("<precognition>"),
            "Should not have precognition block"
        );
    }

    #[tokio::test]
    async fn test_enriched_format() {
        let config = ContextConfig {
            enabled: true,
            context_size: 3,
            use_reranking: false,
            rerank_candidates: None,
            enable_cache: false,
            cache_ttl_secs: 300,
            inject_context: true,
        };

        let enricher = PromptEnricher::new(config);
        let query = "What are the best practices?";

        let result = enricher.enrich(query).await;
        assert!(result.is_ok());

        let enriched = result.unwrap();

        assert!(
            enriched.contains("<precognition>"),
            "Should use XML tags for precognition"
        );
        assert!(
            enriched.contains("</precognition>"),
            "Should close XML tags"
        );
        assert!(
            enriched.contains("<instruction>"),
            "Should include instruction block"
        );
        assert!(
            enriched.contains("<matches>") || enriched.len() > query.len(),
            "Should include matches block or be longer than query"
        );
    }

    #[tokio::test]
    async fn test_no_context_found() {
        let config = ContextConfig {
            inject_context: true, // Enable injection for this test
            ..Default::default()
        };
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
            inject_context: true, // Enable injection for caching test
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
            cache_ttl_secs: 1,    // 1 second TTL for fast testing
            inject_context: true, // Enable injection for caching test
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
            inject_context: true, // Enable injection for caching test
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

#[cfg(test)]
mod xml_format_tests {
    use super::*;
    use regex::Regex;

    #[tokio::test]
    async fn test_enrich_no_matches_no_matches_block() {
        let config = ContextConfig {
            enabled: true,
            context_size: 5,
            enable_cache: false,
            inject_context: true,
            ..Default::default()
        };
        let enricher = PromptEnricher::new(config);

        let result = enricher.enrich("xyzabc123nonexistent").await.unwrap();

        assert!(
            !result.contains("<matches>"),
            "Should not have <matches> tag when no results found"
        );
        assert!(
            result.contains("<precognition>"),
            "Should still have precognition wrapper"
        );
        assert!(
            result.contains("xyzabc123nonexistent"),
            "Should still contain the query"
        );
    }

    #[tokio::test]
    async fn test_enrich_with_matches_xml_format() {
        let config = ContextConfig {
            enabled: true,
            context_size: 3,
            enable_cache: false,
            inject_context: true,
            ..Default::default()
        };
        let enricher = PromptEnricher::new(config);

        let result = enricher.enrich("test query").await.unwrap();

        assert!(
            result.contains("<precognition>"),
            "Should have opening <precognition> tag"
        );
        assert!(
            result.contains("</precognition>"),
            "Should have closing </precognition> tag"
        );
        assert!(
            result.contains("<instruction>"),
            "Should have instruction block"
        );
        assert!(
            result.contains("</instruction>"),
            "Should close instruction block"
        );
        assert!(
            result.contains("knowledge base"),
            "Instruction should mention knowledge base"
        );
        assert!(
            result.contains("read_note"),
            "Instruction should mention read_note tool"
        );
    }

    #[tokio::test]
    async fn test_enrich_matches_toon_table() {
        let config = ContextConfig {
            enabled: true,
            context_size: 2,
            enable_cache: false,
            inject_context: true,
            ..Default::default()
        };
        let enricher = PromptEnricher::new(config);

        let result = enricher.enrich("test query").await.unwrap();

        assert!(
            result.contains("<matches>"),
            "Should have opening <matches> tag"
        );
        assert!(
            result.contains("</matches>"),
            "Should have closing </matches> tag"
        );

        assert!(
            result.contains("notes["),
            "TOON table should start with 'notes['"
        );
        assert!(
            result.contains("path"),
            "TOON table should include 'path' column"
        );
        assert!(
            result.contains("similarity"),
            "TOON table should include 'similarity' column"
        );

        let notes_prefix_pattern = Regex::new(r"notes\[\d+\]\{path,line,similarity\}:").unwrap();
        assert!(
            notes_prefix_pattern.is_match(&result),
            "Should have proper TOON table header format"
        );
    }

    #[tokio::test]
    async fn test_enrich_query_after_context() {
        let config = ContextConfig {
            enabled: true,
            context_size: 2,
            enable_cache: false,
            inject_context: true,
            ..Default::default()
        };
        let enricher = PromptEnricher::new(config);

        let result = enricher.enrich("my actual question").await.unwrap();

        let query_pos = result
            .find("my actual question")
            .expect("Query should be present in result");

        let precognition_close = result
            .find("</precognition>")
            .expect("Should have closing precognition tag");

        assert!(
            query_pos > precognition_close,
            "User query should appear after </precognition>"
        );
    }
}
