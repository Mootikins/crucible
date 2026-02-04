//! Context Enrichment for Agent Prompts
//!
//! Enriches user queries with semantic search results from the knowledge base.

use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, info};

use crate::core_facade::{KilnContext, SemanticSearchResult};

/// Result of context enrichment containing both the prompt and found notes
#[derive(Debug)]
pub struct EnrichmentResult {
    /// The enriched prompt with context prepended
    pub prompt: String,
    /// Notes that were found and used for context
    pub notes_found: Vec<SemanticSearchResult>,
}

/// Enriches prompts with knowledge base context
pub struct ContextEnricher {
    core: Arc<KilnContext>,
    context_size: usize,
}

impl ContextEnricher {
    /// Create a new context enricher
    ///
    /// # Arguments
    /// * `core` - The Crucible core facade
    /// * `context_size` - Number of semantic search results to include
    pub fn new(core: Arc<KilnContext>, context_size: Option<usize>) -> Self {
        Self {
            core,
            context_size: context_size.unwrap_or(5),
        }
    }

    /// Enrich a query with context from the knowledge base
    ///
    /// Performs semantic search and formats results as markdown context
    /// that will be included in the agent prompt.
    ///
    /// # Arguments
    /// * `query` - The user's query
    ///
    /// # Returns
    /// Enriched prompt with context prepended
    pub async fn enrich(&self, query: &str) -> Result<String> {
        let result = self.enrich_with_results(query).await?;
        Ok(result.prompt)
    }

    /// Enrich a query with context and return both the prompt and notes found
    ///
    /// Performs semantic search and formats results as markdown context
    /// that will be included in the agent prompt. Also returns the notes
    /// that were found so they can be displayed to the user.
    ///
    /// # Arguments
    /// * `query` - The user's query
    ///
    /// # Returns
    /// EnrichmentResult containing the enriched prompt and found notes
    pub async fn enrich_with_results(&self, query: &str) -> Result<EnrichmentResult> {
        self.enrich_with_results_n(query, self.context_size).await
    }

    /// Like `enrich_with_results` but with an explicit result count override.
    pub async fn enrich_with_results_n(
        &self,
        query: &str,
        top_k: usize,
    ) -> Result<EnrichmentResult> {
        debug!("Enriching query with {} context results", top_k);

        // Perform semantic search
        let results = self.core.semantic_search(query, top_k).await?;

        if results.is_empty() {
            info!("No context found for query");
            return Ok(EnrichmentResult {
                prompt: format!("# User Query\n\n{}", query),
                notes_found: Vec::new(),
            });
        }

        // Format results as markdown
        let context = results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                format!(
                    "## Context #{}: {} (similarity: {:.2})\n\n{}\n",
                    i + 1,
                    r.title,
                    r.similarity,
                    r.snippet
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        info!("Enriched query with {} context results", results.len());

        // Combine context with query
        Ok(EnrichmentResult {
            prompt: format!(
                "# Context from Knowledge Base\n\n{}\n\n---\n\n# User Query\n\n{}",
                context, query
            ),
            notes_found: results,
        })
    }

    /// Enrich with reranking for better quality results
    ///
    /// Uses a two-stage retrieval process:
    /// 1. Retrieve more candidates with vector search
    /// 2. Rerank them for better relevance
    ///
    /// # Arguments
    /// * `query` - The user's query
    /// * `candidate_count` - Number of candidates to retrieve (default: context_size * 3)
    ///
    /// # Returns
    /// Enriched prompt with reranked context
    pub async fn enrich_with_reranking(
        &self,
        query: &str,
        candidate_count: Option<usize>,
    ) -> Result<String> {
        let rerank_limit = candidate_count.unwrap_or(self.context_size * 3);

        debug!(
            "Enriching query with reranking ({} candidates -> {} results)",
            rerank_limit, self.context_size
        );

        // Perform semantic search with reranking
        let results = self
            .core
            .semantic_search_with_reranking(query, self.context_size, rerank_limit)
            .await?;

        if results.is_empty() {
            info!("No context found for query (with reranking)");
            return Ok(format!("# User Query\n\n{}", query));
        }

        // Format results as markdown
        let context = results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                format!(
                    "## Context #{}: {} (relevance: {:.2})\n\n{}\n",
                    i + 1,
                    r.title,
                    r.similarity,
                    r.snippet
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        info!(
            "Enriched query with {} reranked context results",
            results.len()
        );

        // Combine context with query
        Ok(format!(
            "# Context from Knowledge Base (Reranked)\n\n{}\n\n---\n\n# User Query\n\n{}",
            context, query
        ))
    }
}

#[cfg(test)]
mod tests {

    // Note: Real tests would require a test database setup
    // These are placeholder tests for the structure

    #[test]
    fn test_context_enricher_creation() {
        // This test just ensures the struct can be created
        // Real tests would need a proper core facade
    }
}
