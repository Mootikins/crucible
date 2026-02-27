use std::collections::HashSet;
use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Tool as McpTool};

use crucible_core::error_utils::strip_tool_error_prefix;
use crate::mcp_server::{
    CancelJobParams, CrucibleMcpServer, DelegateSessionParams, GetJobResultParams, ListJobsParams,
};
use crate::notes::{
    CreateNoteParams, DeleteNoteParams, ListNotesParams, ReadMetadataParams, ReadNoteParams,
    UpdateNoteParams,
};
use crate::search::{PropertySearchParams, SemanticSearchParams, TextSearchParams};
use crate::tool_modes::PLAN_TOOL_NAMES;


#[derive(Clone)]
/// Adapter for running MCP tools in-process without stdio transport.
/// Wraps a `CrucibleMcpServer` and exposes its tools as Rig-compatible tools.
pub struct InProcessMcpAdapter {
    server: Arc<CrucibleMcpServer>,
}

impl InProcessMcpAdapter {
    /// Creates a new adapter wrapping the given MCP server.
    #[must_use]
    pub fn new(server: Arc<CrucibleMcpServer>) -> Self {
        Self { server }
    }

    /// Lists all available tool names from the MCP server.
    #[must_use]
    pub fn list_tool_names(&self) -> Vec<String> {
        self.server
            .list_tools()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect()
    }

}

fn filter_plan_tools(all_tools: Vec<McpTool>) -> Vec<McpTool> {
    let plan_names: HashSet<&str> = PLAN_TOOL_NAMES.iter().copied().collect();
    all_tools
        .into_iter()
        .filter(|tool| plan_names.contains(tool.name.as_ref()))
        .collect()
}



fn normalize_tool_error_message(message: &str) -> String {
    let unquoted = serde_json::from_str::<String>(message).unwrap_or_else(|_| message.to_string());
    strip_tool_error_prefix(&unquoted)
}

fn first_text(result: &CallToolResult) -> Option<&str> {
    result
        .content
        .iter()
        .find_map(|content| content.as_text().map(|text| text.text.as_str()))
}

fn into_object(
    value: serde_json::Value,
) -> Result<serde_json::Map<String, serde_json::Value>, rmcp::ErrorData> {
    match value {
        serde_json::Value::Object(map) => Ok(map),
        serde_json::Value::Null => Ok(serde_json::Map::new()),
        _ => Err(rmcp::ErrorData::invalid_params(
            "tool arguments must be a JSON object",
            None,
        )),
    }
}

fn parse_params<T: serde::de::DeserializeOwned>(
    map: serde_json::Map<String, serde_json::Value>,
) -> Result<T, rmcp::ErrorData> {
    serde_json::from_value(serde_json::Value::Object(map))
        .map_err(|err| rmcp::ErrorData::invalid_params(err.to_string(), None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::enrichment::EmbeddingProvider;
    use crucible_core::parser::ParsedNote;
    use crucible_core::traits::knowledge::NoteInfo;
    use crucible_core::traits::KnowledgeRepository;
    use crucible_core::types::SearchResult;
    use std::sync::Arc;

    // Minimal mock implementations for testing
    #[derive(Clone)]
    struct MockKnowledgeRepository;

    #[async_trait::async_trait]
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
            Ok(vec![])
        }
    }

    #[derive(Clone)]
    struct MockEmbeddingProvider;

    #[async_trait::async_trait]
    impl EmbeddingProvider for MockEmbeddingProvider {
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            Ok(vec![0.0; 384])
        }

        async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
            Ok(vec![vec![0.0; 384]; texts.len()])
        }

        fn model_name(&self) -> &str {
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

    #[tokio::test]
    async fn test_in_process_tool_error_returns_ok_with_error_prefix() {
        // Create a server with a nonexistent kiln path to force tool failures
        let nonexistent_path = "/nonexistent/kiln/path/that/does/not/exist".to_string();
        let knowledge_repo = Arc::new(MockKnowledgeRepository);
        let embedding_provider = Arc::new(MockEmbeddingProvider);

        let server = Arc::new(CrucibleMcpServer::new(
            nonexistent_path,
            knowledge_repo,
            embedding_provider,
        ));

        // Wrap in adapter
        let adapter = InProcessMcpAdapter::new(server);

        // Get the tools
        let tools = adapter.create_rig_tools("default");

        // Find the list_notes tool
        let list_notes_tool = tools
            .iter()
            .find(|tool| tool.name() == "list_notes")
            .expect("list_notes tool should exist");

        // Call the tool with empty params - this will fail because the kiln doesn't exist
        // ToolDyn::call expects a String (JSON serialized)
        let result = list_notes_tool.call("{}".to_string()).await;

        // CURRENT BEHAVIOR (WRONG): Returns Err(ToolError)
        // DESIRED BEHAVIOR (Task 5): Returns Ok("Error: ...") with error prefix
        // This test FAILS with current code, proving the bug exists

        match result {
            Ok(error_msg) => {
                let error_msg = serde_json::from_str::<String>(&error_msg).unwrap_or(error_msg);
                // This is what we WANT (Task 5 will make this pass)
                assert!(
                    error_msg.starts_with("Error: "),
                    "Error message should start with 'Error: ', got: {}",
                    error_msg
                );
                assert!(
                    !error_msg.contains("ToolCallError:"),
                    "Error message should NOT contain 'ToolCallError:', got: {}",
                    error_msg
                );
            }
            Err(_) => {
                // This is the CURRENT (WRONG) behavior
                panic!("Tool call returned Err instead of Ok with error prefix. This proves the bug exists and Task 5 needs to fix it.");
            }
        }
    }
}
