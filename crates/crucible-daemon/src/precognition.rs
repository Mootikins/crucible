use anyhow::Result;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use std::sync::Arc;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrecognitionResult {
    pub enriched_prompt: String,
    pub notes_count: usize,
}

pub struct DaemonPrecognition {
    knowledge_repo: Arc<dyn KnowledgeRepository>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
}

impl DaemonPrecognition {
    pub fn new(
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        Self {
            knowledge_repo,
            embedding_provider,
        }
    }

    pub async fn enrich(&self, query: &str, top_k: usize) -> Result<PrecognitionResult> {
        debug!(top_k, "Enriching daemon query with semantic context");

        let query_embedding = match self.embedding_provider.embed(query).await {
            Ok(embedding) => embedding,
            Err(error) => {
                warn!(error = %error, "Precognition embedding failed, using original query");
                return Ok(PrecognitionResult {
                    enriched_prompt: query.to_string(),
                    notes_count: 0,
                });
            }
        };

        let results = match self.knowledge_repo.search_vectors(query_embedding).await {
            Ok(results) => results,
            Err(error) => {
                warn!(error = %error, "Precognition semantic search failed, using original query");
                return Ok(PrecognitionResult {
                    enriched_prompt: query.to_string(),
                    notes_count: 0,
                });
            }
        };

        let top_results = results.into_iter().take(top_k).collect::<Vec<_>>();
        if top_results.is_empty() {
            info!("No semantic context found for daemon query");
            return Ok(PrecognitionResult {
                enriched_prompt: query.to_string(),
                notes_count: 0,
            });
        }

        let context = top_results
            .iter()
            .enumerate()
            .map(|(i, result)| {
                let title = result
                    .document_id
                    .0
                    .split('/')
                    .next_back()
                    .unwrap_or(&result.document_id.0)
                    .trim_end_matches(".md");

                format!(
                    "## Context #{}: {} (similarity: {:.2})\n\n{}\n",
                    i + 1,
                    title,
                    result.score,
                    result.snippet.clone().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let enriched_prompt = format!(
            "# Context from Knowledge Base\n\n{}\n\n---\n\n# User Query\n\n{}",
            context, query
        );

        info!(
            notes_count = top_results.len(),
            "Daemon query enriched with semantic context"
        );

        Ok(PrecognitionResult {
            enriched_prompt,
            notes_count: top_results.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use crucible_core::parser::ParsedNote;
    use crucible_core::traits::knowledge::NoteInfo;
    use crucible_core::types::{DocumentId, SearchResult};

    struct MockKnowledgeRepository {
        results: Vec<SearchResult>,
    }

    #[async_trait]
    impl KnowledgeRepository for MockKnowledgeRepository {
        async fn get_note_by_name(&self, _name: &str) -> crucible_core::Result<Option<ParsedNote>> {
            Ok(None)
        }

        async fn list_notes(&self, _path: Option<&str>) -> crucible_core::Result<Vec<NoteInfo>> {
            Ok(vec![])
        }

        async fn search_vectors(
            &self,
            _vector: Vec<f32>,
        ) -> crucible_core::Result<Vec<SearchResult>> {
            Ok(self.results.clone())
        }
    }

    struct MockEmbeddingProvider;

    #[async_trait]
    impl EmbeddingProvider for MockEmbeddingProvider {
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![0.1, 0.2, 0.3])
        }

        async fn embed_batch(&self, _texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(vec![vec![0.1, 0.2, 0.3]])
        }

        fn model_name(&self) -> &str {
            "mock-model"
        }

        fn dimensions(&self) -> usize {
            3
        }

        fn provider_name(&self) -> &str {
            "mock"
        }

        async fn list_models(&self) -> anyhow::Result<Vec<String>> {
            Ok(vec!["mock-model".to_string()])
        }
    }

    #[tokio::test]
    async fn enrich_with_results_returns_formatted_prompt() {
        let knowledge_repo = Arc::new(MockKnowledgeRepository {
            results: vec![
                SearchResult {
                    document_id: DocumentId("docs/alpha.md".to_string()),
                    score: 0.93,
                    highlights: None,
                    snippet: Some("Alpha snippet".to_string()),
                },
                SearchResult {
                    document_id: DocumentId("docs/beta.md".to_string()),
                    score: 0.79,
                    highlights: None,
                    snippet: Some("Beta snippet".to_string()),
                },
                SearchResult {
                    document_id: DocumentId("docs/gamma.md".to_string()),
                    score: 0.66,
                    highlights: None,
                    snippet: Some("Gamma snippet".to_string()),
                },
            ],
        });

        let provider = Arc::new(MockEmbeddingProvider);
        let service = DaemonPrecognition::new(knowledge_repo, provider);
        let result = service.enrich("What changed recently?", 3).await.unwrap();

        assert_eq!(result.notes_count, 3);
        assert!(result
            .enriched_prompt
            .starts_with("# Context from Knowledge Base\n\n"));
        assert!(result
            .enriched_prompt
            .contains("## Context #1: alpha (similarity: 0.93)\n\nAlpha snippet\n"));
        assert!(result
            .enriched_prompt
            .contains("## Context #2: beta (similarity: 0.79)\n\nBeta snippet\n"));
        assert!(result
            .enriched_prompt
            .contains("## Context #3: gamma (similarity: 0.66)\n\nGamma snippet\n"));
        assert!(result
            .enriched_prompt
            .ends_with("\n\n---\n\n# User Query\n\nWhat changed recently?"));
    }

    #[tokio::test]
    async fn enrich_with_empty_results_returns_original() {
        let knowledge_repo = Arc::new(MockKnowledgeRepository { results: vec![] });
        let provider = Arc::new(MockEmbeddingProvider);
        let service = DaemonPrecognition::new(knowledge_repo, provider);

        let result = service.enrich("No matches please", 5).await.unwrap();

        assert_eq!(result.notes_count, 0);
        assert_eq!(result.enriched_prompt, "No matches please");
    }

    #[test]
    fn daemon_precognition_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DaemonPrecognition>();
    }
}
