use std::collections::HashSet;
use std::sync::Arc;

use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use crucible_tools::in_process_adapter::InProcessMcpAdapter;
use crucible_tools::CrucibleMcpServer;
use tempfile::TempDir;

struct MockKnowledgeRepository;
struct MockEmbeddingProvider;

#[async_trait::async_trait]
impl KnowledgeRepository for MockKnowledgeRepository {
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

#[async_trait::async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(vec![0.1; 384])
    }

    async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(vec![vec![0.1; 384]; texts.len()])
    }

    fn model_name(&self) -> &'static str {
        "mock-model"
    }

    fn dimensions(&self) -> usize {
        384
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec!["mock-model".to_string()])
    }
}

fn create_adapter() -> (TempDir, InProcessMcpAdapter) {
    let temp = TempDir::new().expect("temp dir");
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;
    let server = CrucibleMcpServer::new(
        temp.path().to_string_lossy().to_string(),
        knowledge_repo,
        embedding_provider,
    );
    (temp, InProcessMcpAdapter::new(Arc::new(server)))
}

#[test]
fn in_process_adapter_lists_all_unprefixed_tool_names() {
    let (_temp, adapter) = create_adapter();

    let names = adapter.list_tool_names();

    assert_eq!(names.len(), 16);
    assert!(names
        .iter()
        .all(|name| !name.contains('_') || !name.starts_with("crucible_")));
    assert!(names.contains(&"semantic_search".to_string()));
    assert!(names.contains(&"create_note".to_string()));
}

#[tokio::test]
async fn in_process_adapter_create_rig_tools_filters_plan_mode() {
    let (_temp, adapter) = create_adapter();

    let tools = adapter.create_rig_tools("plan");
    let names: HashSet<String> = tools.iter().map(|tool| tool.name()).collect();

    let expected: HashSet<String> = [
        "semantic_search",
        "text_search",
        "property_search",
        "list_notes",
        "read_note",
        "read_metadata",
        "get_kiln_info",
        "get_kiln_roots",
        "get_kiln_stats",
        "list_jobs",
    ]
    .iter()
    .map(|name| (*name).to_string())
    .collect();

    assert_eq!(names, expected);
}

#[test]
fn in_process_adapter_create_rig_tools_non_plan_returns_all() {
    let (_temp, adapter) = create_adapter();

    let tools = adapter.create_rig_tools("auto");
    assert_eq!(tools.len(), 16);
}

#[tokio::test]
async fn in_process_adapter_tool_wrapper_calls_server() {
    let (_temp, adapter) = create_adapter();

    let tools = adapter.create_rig_tools("auto");
    let tool = tools
        .iter()
        .find(|tool| tool.name() == "get_kiln_info")
        .expect("get_kiln_info tool");

    let response = tool
        .call("{}".to_string())
        .await
        .expect("tool call response");
    assert!(response.contains("root"));
}
