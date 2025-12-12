//! Semantic search functionality
//!
//! Vector similarity search with optional reranking.

use crate::SurrealClient;
use anyhow::{anyhow, Result};
use serde_json::json;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tracing::{debug, warn};

use super::document_storage::fetch_document_by_id;
use super::embeddings::{MTREE_INDEX_DIMENSIONS, MTREE_INDEX_ENSURED};
use super::utils::normalize_document_id;

/// Semantic search using vector similarity
pub async fn semantic_search(
    client: &SurrealClient,
    query: &str,
    limit: usize,
    embedding_provider: Arc<dyn crucible_llm::embeddings::EmbeddingProvider>,
) -> Result<Vec<(String, f64)>> {
    debug!(
        "Performing semantic search for query: '{}', limit: {}",
        query, limit
    );

    if query.trim().is_empty() {
        warn!("Empty query provided for semantic search");
        return Ok(Vec::new());
    }

    let response = embedding_provider
        .embed(query)
        .await
        .map_err(|e| anyhow!("Failed to generate query embedding: {}", e))?;

    let query_embedding = response.embedding;

    debug!(
        "Generated query embedding with {} dimensions using provider: {}",
        query_embedding.len(),
        embedding_provider.provider_name()
    );

    let query_dims = query_embedding.len();
    let index_dims = MTREE_INDEX_DIMENSIONS.load(Ordering::Relaxed);
    let use_knn = MTREE_INDEX_ENSURED.load(Ordering::Relaxed) && index_dims == query_dims;

    let result = if use_knn {
        let knn_sql = format!(
            r#"
            SELECT
                entity_id,
                vector::similarity::cosine(embedding, $vector) AS score
            FROM embeddings
            WHERE embedding <|{limit}|> $vector
            ORDER BY score DESC
            "#,
            limit = limit
        );
        debug!("Executing KNN search with MTREE index");

        match client
            .query(&knn_sql, &[json!({ "vector": query_embedding })])
            .await
        {
            Ok(result) => result,
            Err(e) => {
                warn!("KNN search failed, falling back to ORDER BY: {}", e);
                let fallback_sql = format!(
                    r#"
                    SELECT
                        entity_id,
                        vector::similarity::cosine(embedding, $vector) AS score
                    FROM embeddings
                    ORDER BY score DESC
                    LIMIT {limit}
                    "#,
                    limit = limit
                );
                client
                    .query(&fallback_sql, &[json!({ "vector": query_embedding })])
                    .await
                    .map_err(|e| anyhow!("Semantic search query failed: {}", e))?
            }
        }
    } else {
        let sql = format!(
            r#"
            SELECT
                entity_id,
                vector::similarity::cosine(embedding, $vector) AS score
            FROM embeddings
            ORDER BY score DESC
            LIMIT {limit}
            "#,
            limit = limit
        );
        debug!("Executing semantic search with ORDER BY (no MTREE index)");

        client
            .query(&sql, &[json!({ "vector": query_embedding })])
            .await
            .map_err(|e| anyhow!("Semantic search query failed: {}", e))?
    };

    debug!("Semantic search returned {} records", result.records.len());

    let similarity_threshold = 0.5;
    let mut filtered_results: Vec<(String, f64)> = Vec::new();

    for record in result.records {
        let score = record
            .data
            .get("score")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let entity_id = if let Some(id_val) = record.data.get("entity_id") {
            if let Some(s) = id_val.as_str() {
                s.to_string()
            } else if let Some(obj) = id_val.as_object() {
                if let Some(id_inner) = obj.get("id") {
                    if let Some(s) = id_inner.as_str() {
                        s.to_string()
                    } else if let Some(inner_obj) = id_inner.as_object() {
                        inner_obj
                            .get("String")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .unwrap_or_default()
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            } else {
                continue;
            }
        } else {
            continue;
        };

        if entity_id.is_empty() {
            continue;
        }

        if score >= similarity_threshold {
            filtered_results.push((entity_id, score));
        } else if filtered_results.is_empty() {
            filtered_results.push((entity_id, score));
        }
    }

    debug!(
        "Returning {} results after filtering",
        filtered_results.len()
    );

    Ok(filtered_results)
}

/// Perform semantic search with optional reranking for improved relevance.
pub async fn semantic_search_with_reranking(
    client: &SurrealClient,
    query: &str,
    initial_limit: usize,
    reranker: Option<Arc<dyn crucible_llm::Reranker>>,
    final_limit: usize,
    embedding_provider: Arc<dyn crucible_llm::embeddings::EmbeddingProvider>,
) -> Result<Vec<(String, f64)>> {
    eprintln!(
        "DEBUG RERANK: semantic_search_with_reranking called: query='{}', initial_limit={}, final_limit={}, reranker={}",
        query,
        initial_limit,
        final_limit,
        if reranker.is_some() { "Some" } else { "None" }
    );

    let initial_results = semantic_search(client, query, initial_limit, embedding_provider).await?;

    eprintln!(
        "DEBUG RERANK: Stage 1 vector search returned {} results",
        initial_results.len()
    );

    if initial_results.is_empty() {
        warn!("Stage 1 vector search returned no results");
        return Ok(Vec::new());
    }

    if let Some(reranker) = reranker {
        eprintln!(
            "DEBUG RERANK: Reranking {} initial results to top {} with model: {}",
            initial_results.len(),
            final_limit,
            reranker.model_info().name
        );

        let mut documents = Vec::new();
        let mut failed_retrievals = 0;
        eprintln!(
            "DEBUG RERANK: Starting optimized note retrieval for {} results",
            initial_results.len()
        );

        for (document_id, vec_score) in &initial_results {
            eprintln!("DEBUG RERANK: Fetching document_id: {}", document_id);

            let normalized_id = normalize_document_id(document_id);
            match fetch_document_by_id(client, &normalized_id).await {
                Ok(Some(doc)) => {
                    let text = doc.content.plain_text.clone();
                    eprintln!(
                        "DEBUG RERANK: Retrieved note with {} chars of text",
                        text.len()
                    );
                    documents.push((normalized_id, text, *vec_score));
                }
                Ok(None) => {
                    eprintln!(
                        "DEBUG RERANK: Note not found for document_id: {}",
                        document_id
                    );
                    failed_retrievals += 1;
                }
                Err(e) => {
                    eprintln!("DEBUG RERANK: Failed to fetch note {}: {}", document_id, e);
                    failed_retrievals += 1;
                }
            }
        }

        eprintln!(
            "DEBUG RERANK: Retrieved {}/{} documents for reranking ({} failed)",
            documents.len(),
            initial_results.len(),
            failed_retrievals
        );

        if documents.is_empty() {
            eprintln!("DEBUG RERANK: No documents could be retrieved for reranking, returning empty results");
            return Ok(Vec::new());
        }

        let reranked = reranker
            .rerank(query, documents, Some(final_limit))
            .await
            .map_err(|e| anyhow!("Reranking failed: {}", e))?;

        debug!("Reranking complete, returning {} results", reranked.len());

        Ok(reranked
            .into_iter()
            .map(|r| (r.document_id, r.score))
            .collect())
    } else {
        Ok(initial_results.into_iter().take(final_limit).collect())
    }
}

/// Calculate mock similarity score for testing
#[allow(dead_code)]
fn calculate_mock_similarity(query: &str, content: &str) -> f64 {
    let query_lower = query.to_lowercase();
    let content_lower = content.to_lowercase();

    let query_words: Vec<&str> = query_lower.split_whitespace().collect();
    let content_words: Vec<&str> = content_lower.split_whitespace().collect();

    if query_words.is_empty() {
        return 0.0;
    }

    let mut matches = 0;
    for query_word in &query_words {
        if content_words.contains(query_word) {
            matches += 1;
        }
    }

    let base_score = matches as f64 / query_words.len() as f64;
    let random_factor = 0.1 + (query.len() % 100) as f64 / 1000.0;

    (base_score + random_factor).min(1.0)
}

/// Generate mock semantic search results for testing
#[allow(dead_code)]
fn generate_mock_semantic_results(query: &str, _limit: usize) -> Vec<(String, f64)> {
    let _query_lower = query.to_lowercase();
    let mut results = Vec::new();

    let mock_docs = vec![
        (
            "rust-doc",
            "Rust programming language systems programming memory safety",
        ),
        (
            "ai-doc",
            "Artificial intelligence machine learning neural networks",
        ),
        ("db-doc", "Database systems SQL NoSQL vector embeddings"),
        (
            "web-doc",
            "Web development HTML CSS JavaScript frontend backend",
        ),
        (
            "devops-doc",
            "DevOps CI/CD Docker Kubernetes deployment automation",
        ),
    ];

    for (doc_id, content) in mock_docs {
        let score = calculate_mock_similarity(query, content);
        if score > 0.1 {
            results.push((format!("/notes/{}.md", doc_id), score));
        }
    }

    if results.is_empty() {
        results.push(("/notes/welcome.md".to_string(), 0.5));
    }

    results
}
