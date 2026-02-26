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

/// Integration test verifying the end-to-end tool error format fix.
///
/// After the error handling fix, tool calls should:
/// - Always return `Ok(...)`, never `Err(...)`
/// - Never return results containing `"ToolCallError:"` prefix
/// - Return `Ok("Error: ...")` for failures with actionable info
/// - Return `Ok(result)` for successes
#[tokio::test]
async fn tool_call_results_never_contain_tool_call_error_prefix() {
    let (_temp, adapter) = create_adapter();
    let tools = adapter.create_rig_tools("auto");

    // Tools to test end-to-end:
    // - list_notes: should succeed with empty kiln (returns empty list)
    // - get_kiln_info: should succeed with valid temp dir (returns kiln metadata)
    let tools_to_test = ["list_notes", "get_kiln_info"];

    for tool_name in &tools_to_test {
        let tool = tools
            .iter()
            .find(|t| t.name() == *tool_name)
            .unwrap_or_else(|| panic!("{tool_name} tool should exist"));

        let result = tool.call("{}".to_string()).await;

        // CRITICAL: Tool calls must always return Ok, never Err.
        // After the fix, InProcessRigTool::call() returns Ok("Error: ...") for failures.
        let response = result.unwrap_or_else(|err| {
            panic!(
                "{tool_name}: tool call returned Err instead of Ok: {err:?}"
            )
        });

        // Deserialize the JSON-wrapped string (ToolDyn serializes Output to JSON)
        let content = serde_json::from_str::<String>(&response).unwrap_or(response.clone());

        // CRITICAL: No result should ever contain "ToolCallError:" prefix
        assert!(
            !content.contains("ToolCallError:"),
            "{tool_name}: result contains 'ToolCallError:' which should have been stripped. Got: {content}"
        );

        // If it's an error result, verify the format
        if content.starts_with("Error: ") {
            let error_body = content.strip_prefix("Error: ").unwrap();
            // Error should contain actionable information, not be empty
            assert!(
                !error_body.is_empty(),
                "{tool_name}: error result has 'Error: ' prefix but empty body"
            );
            // Error body should not contain nested ToolCallError prefixes
            assert!(
                !error_body.contains("ToolCallError:"),
                "{tool_name}: error body contains nested 'ToolCallError:'. Got: {content}"
            );
        }
    }
}

/// Verify that list_notes returns a successful (non-error) result for an empty kiln.
#[tokio::test]
async fn list_notes_on_empty_kiln_returns_success() {
    let (_temp, adapter) = create_adapter();
    let tools = adapter.create_rig_tools("auto");
    let tool = tools
        .iter()
        .find(|t| t.name() == "list_notes")
        .expect("list_notes tool should exist");

    let response = tool.call("{}".to_string()).await.expect("list_notes should return Ok");
    let content = serde_json::from_str::<String>(&response).unwrap_or(response);

    // list_notes on empty kiln should succeed, not return an error
    assert!(
        !content.starts_with("Error: "),
        "list_notes on empty kiln should succeed, not return error. Got: {content}"
    );
}

/// Verify that get_kiln_info returns a successful result with kiln metadata.
#[tokio::test]
async fn get_kiln_info_returns_success_with_metadata() {
    let (_temp, adapter) = create_adapter();
    let tools = adapter.create_rig_tools("auto");
    let tool = tools
        .iter()
        .find(|t| t.name() == "get_kiln_info")
        .expect("get_kiln_info tool should exist");

    let response = tool.call("{}".to_string()).await.expect("get_kiln_info should return Ok");
    let content = serde_json::from_str::<String>(&response).unwrap_or(response);

    // get_kiln_info should succeed and contain kiln path info
    assert!(
        !content.starts_with("Error: "),
        "get_kiln_info should succeed, not return error. Got: {content}"
    );
    assert!(
        content.contains("root"),
        "get_kiln_info should contain kiln root path. Got: {content}"
    );
}

/// Verify that semantic_search gracefully returns an error (not a panic or Err) when
/// no embeddings are available in a temp kiln.
#[tokio::test]
async fn semantic_search_on_empty_kiln_returns_ok_with_error_prefix() {
    let (_temp, adapter) = create_adapter();
    let tools = adapter.create_rig_tools("auto");
    let tool = tools
        .iter()
        .find(|t| t.name() == "semantic_search")
        .expect("semantic_search tool should exist");

    // semantic_search with a query on an empty kiln — may fail gracefully
    let result = tool
        .call(serde_json::json!({"query": "test"}).to_string())
        .await;

    // Must be Ok, never Err
    let response = result.expect("semantic_search should return Ok, not Err");
    let content = serde_json::from_str::<String>(&response).unwrap_or(response);

    // Must not contain ToolCallError prefix regardless of success/failure
    assert!(
        !content.contains("ToolCallError:"),
        "semantic_search result should not contain 'ToolCallError:'. Got: {content}"
    );

    // If it's an error, verify the format
    if content.starts_with("Error: ") {
        let error_body = content.strip_prefix("Error: ").unwrap();
        assert!(
            !error_body.is_empty(),
            "semantic_search error should contain actionable info"
        );
    }
}
