//! Search operations tools
//!
//! This module provides semantic and text search tools using core traits.

use crucible_core::{
    traits::KnowledgeRepository,
    enrichment::EmbeddingProvider,
};
use rmcp::{tool, tool_router, model::CallToolResult};
use rmcp::handler::server::wrapper::Parameters;
use serde::Deserialize;
use schemars::JsonSchema;
use std::sync::Arc;

/// Default value for top_k parameter
fn default_top_k() -> u32 {
    10
}

#[derive(Clone)]
pub struct SearchTools {
    knowledge_repo: Arc<dyn KnowledgeRepository>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
}

/// Parameters for semantic search
#[derive(Deserialize, JsonSchema)]
struct SemanticSearchParams {
    query: String,
    #[serde(default = "default_top_k")]
    top_k: u32,
}

/// Parameters for text search
#[derive(Deserialize, JsonSchema)]
struct TextSearchParams {
    query: String,
    #[serde(default = "default_top_k")]
    top_k: u32,
    kiln_path: String,
}

/// Parameters for metadata search
#[derive(Deserialize, JsonSchema)]
struct MetadataSearchParams {
    properties: serde_json::Value,
}

/// Parameters for tag search
#[derive(Deserialize, JsonSchema)]
struct TagSearchParams {
    tags: Vec<String>,
}

impl SearchTools {
    pub fn new(
        knowledge_repo: Arc<dyn KnowledgeRepository>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        Self {
            knowledge_repo,
            embedding_provider,
        }
    }
}

#[tool_router]
impl SearchTools {
    #[tool(description = "Search notes using semantic similarity")]
    async fn semantic_search(
        &self,
        params: Parameters<SemanticSearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let query = params.query;
        let top_k = params.top_k;

        let embedding = self.embedding_provider
            .embed(&query)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(format!("Failed to generate embedding: {e}"), None))?;

        let search_results = self.knowledge_repo
            .search_vectors(embedding)
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(format!("Search failed: {e}"), None))?;

        let results_json: Vec<serde_json::Value> = search_results
            .into_iter()
            .take(top_k as usize)
            .map(|r| serde_json::json!({
                "id": r.document_id,
                "score": r.score,
                "snippet": r.snippet,
                "highlights": r.highlights
            }))
            .collect();

        Ok(CallToolResult::success(vec![
            rmcp::model::Content::json(serde_json::json!({
                "results": results_json,
                "query": query,
                "top_k": top_k
            }))?
        ]))
    }

    #[tool(description = "Search notes by text content using ripgrep")]
    async fn text_search(
        &self,
        params: Parameters<TextSearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let query = params.query;
        let top_k = params.top_k;
        let kiln_path = params.kiln_path;

        // TODO: Implement ripgrep-based text search
        // For now, return a placeholder
        Ok(CallToolResult::success(vec![
            rmcp::model::Content::json(serde_json::json!({
                "message": "Text search not yet implemented",
                "query": query,
                "top_k": top_k,
                "kiln_path": kiln_path
            }))?
        ]))
    }

    #[tool(description = "Search notes by metadata properties")]
    async fn metadata_search(
        &self,
        params: Parameters<MetadataSearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let properties = params.properties;

        // TODO: Implement metadata search
        Ok(CallToolResult::success(vec![
            rmcp::model::Content::json(serde_json::json!({
                "message": "Metadata search not yet implemented",
                "properties": properties
            }))?
        ]))
    }

    #[tool(description = "Search notes by tags")]
    async fn tag_search(
        &self,
        params: Parameters<TagSearchParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let params = params.0;
        let tags = params.tags;

        // TODO: Implement tag search
        Ok(CallToolResult::success(vec![
            rmcp::model::Content::json(serde_json::json!({
                "message": "Tag search not yet implemented",
                "tags": tags
            }))?
        ]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock implementations for testing
    struct MockKnowledgeRepository;
    struct MockEmbeddingProvider;

    #[async_trait::async_trait]
    impl crucible_core::traits::KnowledgeRepository for MockKnowledgeRepository {
        async fn get_note_by_name(&self, _name: &str) -> crucible_core::Result<Option<crucible_core::parser::ParsedNote>> {
            Ok(None)
        }

        async fn list_notes(&self, _path: Option<&str>) -> crucible_core::Result<Vec<crucible_core::traits::knowledge::NoteMetadata>> {
            Ok(vec![])
        }

        async fn search_vectors(&self, _vector: Vec<f32>) -> crucible_core::Result<Vec<crucible_core::types::SearchResult>> {
            Ok(vec![])
        }
    }

    #[async_trait::async_trait]
    impl crucible_core::enrichment::EmbeddingProvider for MockEmbeddingProvider {
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![0.1; 384]) // Mock 384-dimensional embedding
        }

        async fn embed_batch(&self, _texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(vec![vec![0.1; 384]; _texts.len()])
        }

        fn model_name(&self) -> &str {
            "mock-model"
        }

        fn dimensions(&self) -> usize {
            384
        }
    }

    #[test]
    fn test_search_tools_creation() {
        let knowledge_repo = std::sync::Arc::new(MockKnowledgeRepository);
        let embedding_provider = std::sync::Arc::new(MockEmbeddingProvider);

        let search_tools = SearchTools::new(knowledge_repo, embedding_provider);
        // Should not panic
        assert!(true);
    }

    #[test]
    fn test_tool_router_creation() {
        let knowledge_repo = std::sync::Arc::new(MockKnowledgeRepository);
        let embedding_provider = std::sync::Arc::new(MockEmbeddingProvider);

        let search_tools = SearchTools::new(knowledge_repo, embedding_provider);

        // This should compile and not panic - the tool_router macro generates the router
        let _router = search_tools.tool_router();
    }

    #[tokio::test]
    async fn test_semantic_search_mock() {
        let knowledge_repo = std::sync::Arc::new(MockKnowledgeRepository);
        let embedding_provider = std::sync::Arc::new(MockEmbeddingProvider);

        let search_tools = SearchTools::new(knowledge_repo, embedding_provider);

        let result = search_tools.semantic_search("test query".to_string(), 5).await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        assert!(!call_result.content.is_empty());

        // Check response structure
        if let Some(content) = call_result.content.first() {
            if let Some(json_str) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
                assert_eq!(parsed["query"], "test query");
                assert_eq!(parsed["top_k"], 5);
                assert!(parsed["results"].is_array());
            }
        }
    }

    #[tokio::test]
    async fn test_text_search_stub() {
        let knowledge_repo = std::sync::Arc::new(MockKnowledgeRepository);
        let embedding_provider = std::sync::Arc::new(MockEmbeddingProvider);

        let search_tools = SearchTools::new(knowledge_repo, embedding_provider);

        let result = search_tools.text_search("test query".to_string(), 5, "/test/path".to_string()).await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(json_str) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
                assert_eq!(parsed["query"], "test query");
                assert_eq!(parsed["top_k"], 5);
                assert_eq!(parsed["kiln_path"], "/test/path");
                assert!(parsed["message"].as_str().unwrap().contains("not yet implemented"));
            }
        }
    }

    #[tokio::test]
    async fn test_metadata_search_stub() {
        let knowledge_repo = std::sync::Arc::new(MockKnowledgeRepository);
        let embedding_provider = std::sync::Arc::new(MockEmbeddingProvider);

        let search_tools = SearchTools::new(knowledge_repo, embedding_provider);

        let properties = serde_json::json!({"tags": ["test"], "status": "draft"});
        let result = search_tools.metadata_search(properties.clone()).await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(json_str) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
                assert_eq!(parsed["properties"], properties);
                assert!(parsed["message"].as_str().unwrap().contains("not yet implemented"));
            }
        }
    }

    #[tokio::test]
    async fn test_tag_search_stub() {
        let knowledge_repo = std::sync::Arc::new(MockKnowledgeRepository);
        let embedding_provider = std::sync::Arc::new(MockEmbeddingProvider);

        let search_tools = SearchTools::new(knowledge_repo, embedding_provider);

        let tags = vec!["test".to_string(), "important".to_string()];
        let result = search_tools.tag_search(tags.clone()).await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            if let Some(json_str) = content.as_text() {
                let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
                assert_eq!(parsed["tags"], serde_json::Value::Array(tags.into_iter().map(serde_json::Value::String).collect()));
                assert!(parsed["message"].as_str().unwrap().contains("not yet implemented"));
            }
        }
    }
}