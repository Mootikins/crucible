use async_trait::async_trait;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;

pub(crate) struct EmptyKnowledgeRepository;

#[async_trait]
impl KnowledgeRepository for EmptyKnowledgeRepository {
    async fn get_note_by_name(
        &self,
        _name: &str,
    ) -> crucible_core::Result<Option<crucible_core::parser::ParsedNote>> {
        Ok(None)
    }

    async fn list_notes(
        &self,
        _path: Option<&str>,
    ) -> crucible_core::Result<Vec<crucible_core::traits::knowledge::NoteInfo>> {
        Ok(vec![])
    }

    async fn search_vectors(
        &self,
        _vector: Vec<f32>,
    ) -> crucible_core::Result<Vec<crucible_core::types::SearchResult>> {
        Ok(vec![])
    }
}

pub(crate) struct EmptyEmbeddingProvider;

#[async_trait]
impl EmbeddingProvider for EmptyEmbeddingProvider {
    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        anyhow::bail!("Embedding provider unavailable for in-process MCP adapter")
    }

    async fn embed_batch(&self, _texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        anyhow::bail!("Embedding provider unavailable for in-process MCP adapter")
    }

    fn model_name(&self) -> &str {
        "unavailable"
    }

    fn dimensions(&self) -> usize {
        0
    }

    fn provider_name(&self) -> &str {
        "none"
    }

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
}
