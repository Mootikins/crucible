use async_trait::async_trait;
use crucible_core::traits::tools::{
    ExecutionContext, ToolDefinition, ToolError, ToolExecutor, ToolResult,
};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::RawContent;
use serde::de::DeserializeOwned;
use std::collections::HashSet;
use std::sync::Arc;

use crate::tools::mcp_server::CrucibleMcpServer;
use crate::tools::mcp_server::{
    CancelJobParams, DelegateSessionParams, GetJobResultParams, ListJobsParams,
};
use crate::tools::notes::{
    CreateNoteParams, DeleteNoteParams, ListNotesParams, ReadMetadataParams, ReadNoteParams,
    UpdateNoteParams,
};
use crate::tools::search::{PropertySearchParams, SemanticSearchParams, TextSearchParams};

#[async_trait]
pub trait ToolDispatcher: Send + Sync {
    async fn dispatch_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
    fn has_tool(&self, name: &str) -> bool;
}

pub struct DaemonToolDispatcher {
    providers: Vec<Arc<dyn ToolExecutor>>,
    tool_names: HashSet<String>,
}

impl DaemonToolDispatcher {
    pub fn new(providers: Vec<Arc<dyn ToolExecutor>>) -> Self {
        let mut tool_names = HashSet::new();
        for provider in &providers {
            if let Ok(defs) = futures::executor::block_on(provider.list_tools()) {
                tool_names.extend(defs.into_iter().map(|def| def.name));
            }
        }

        Self {
            providers,
            tool_names,
        }
    }
}

#[derive(Clone)]
pub struct McpToolExecutor {
    server: Arc<CrucibleMcpServer>,
}

impl McpToolExecutor {
    pub fn new(server: Arc<CrucibleMcpServer>) -> Self {
        Self { server }
    }

    fn convert_call_tool_result(
        result: rmcp::model::CallToolResult,
    ) -> ToolResult<serde_json::Value> {
        let mut values = Vec::new();
        let mut text_parts = Vec::new();

        for content in result.content {
            match content.raw {
                RawContent::Text(text) => {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text.text) {
                        values.push(value);
                    } else {
                        text_parts.push(text.text);
                    }
                }
                RawContent::Image(_) => text_parts.push("[image content]".to_string()),
                RawContent::Resource(_) => text_parts.push("[resource content]".to_string()),
                RawContent::Audio(_) => text_parts.push("[audio content]".to_string()),
                RawContent::ResourceLink(link) => text_parts.push(link.uri),
            }
        }

        if !text_parts.is_empty() {
            values.push(serde_json::Value::String(text_parts.join("\n")));
        }

        let value = match values.len() {
            0 => serde_json::Value::Null,
            1 => values.into_iter().next().unwrap_or(serde_json::Value::Null),
            _ => serde_json::Value::Array(values),
        };

        if result.is_error.unwrap_or(false) {
            Err(ToolError::ExecutionFailed(value.to_string()))
        } else {
            Ok(value)
        }
    }

    fn parse_params<T: DeserializeOwned>(params: serde_json::Value) -> ToolResult<Parameters<T>> {
        serde_json::from_value(params)
            .map(Parameters)
            .map_err(|err| ToolError::InvalidParameters(err.to_string()))
    }
}

#[async_trait]
impl ToolExecutor for McpToolExecutor {
    async fn execute_tool(
        &self,
        name: &str,
        params: serde_json::Value,
        _context: &ExecutionContext,
    ) -> ToolResult<serde_json::Value> {
        let result = match name {
            "create_note" => {
                self.server
                    .create_note(Self::parse_params::<CreateNoteParams>(params)?)
                    .await
            }
            "read_note" => {
                self.server
                    .read_note(Self::parse_params::<ReadNoteParams>(params)?)
                    .await
            }
            "read_metadata" => {
                self.server
                    .read_metadata(Self::parse_params::<ReadMetadataParams>(params)?)
                    .await
            }
            "update_note" => {
                self.server
                    .update_note(Self::parse_params::<UpdateNoteParams>(params)?)
                    .await
            }
            "delete_note" => {
                self.server
                    .delete_note(Self::parse_params::<DeleteNoteParams>(params)?)
                    .await
            }
            "list_notes" => {
                self.server
                    .list_notes(Self::parse_params::<ListNotesParams>(params)?)
                    .await
            }
            "semantic_search" => {
                self.server
                    .semantic_search(Self::parse_params::<SemanticSearchParams>(params)?)
                    .await
            }
            "text_search" => {
                self.server
                    .text_search(Self::parse_params::<TextSearchParams>(params)?)
                    .await
            }
            "property_search" => {
                self.server
                    .property_search(Self::parse_params::<PropertySearchParams>(params)?)
                    .await
            }
            "get_kiln_info" => self.server.get_kiln_info().await,
            "delegate_session" => {
                self.server
                    .delegate_session(Self::parse_params::<DelegateSessionParams>(params)?)
                    .await
            }
            "list_jobs" => {
                self.server
                    .list_jobs(Self::parse_params::<ListJobsParams>(params)?)
                    .await
            }
            "get_job_result" => {
                self.server
                    .get_job_result(Self::parse_params::<GetJobResultParams>(params)?)
                    .await
            }
            "cancel_job" => {
                self.server
                    .cancel_job(Self::parse_params::<CancelJobParams>(params)?)
                    .await
            }
            _ => return Err(ToolError::NotFound(name.to_string())),
        };

        result
            .map_err(|err| ToolError::ExecutionFailed(err.message.to_string()))
            .and_then(Self::convert_call_tool_result)
    }

    async fn list_tools(&self) -> ToolResult<Vec<ToolDefinition>> {
        let tools = CrucibleMcpServer::list_tools(self.server.as_ref())
            .into_iter()
            .map(|tool| ToolDefinition {
                name: tool.name.to_string(),
                description: tool.description.map(|d| d.to_string()).unwrap_or_default(),
                category: Some("mcp".to_string()),
                parameters: Some(serde_json::Value::Object((*tool.input_schema).clone())),
                returns: None,
                examples: vec![],
                required_permissions: vec![],
            })
            .collect();

        Ok(tools)
    }
}

#[async_trait]
impl ToolDispatcher for DaemonToolDispatcher {
    async fn dispatch_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let ctx = ExecutionContext::default();

        for provider in &self.providers {
            match provider.execute_tool(name, args.clone(), &ctx).await {
                Ok(value) => return Ok(value),
                Err(ToolError::NotFound(_)) => continue,
                Err(err) => return Err(err.to_string()),
            }
        }

        Err(format!("Unknown tool: {name}"))
    }

    fn has_tool(&self, name: &str) -> bool {
        self.tool_names.contains(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::empty_providers::{EmptyEmbeddingProvider, EmptyKnowledgeRepository};
    use crate::tools::mcp_server::CrucibleMcpServer;
    use crate::tools::workspace::WorkspaceTools;
    use serde_json::json;
    use tempfile::TempDir;

    fn workspace_tools() -> Arc<WorkspaceTools> {
        Arc::new(WorkspaceTools::new(&std::path::PathBuf::from("/tmp")))
    }

    fn test_dispatcher() -> DaemonToolDispatcher {
        DaemonToolDispatcher::new(vec![workspace_tools() as Arc<dyn ToolExecutor>])
    }

    fn test_dispatcher_with_mcp() -> (TempDir, DaemonToolDispatcher) {
        let temp = TempDir::new().expect("tempdir");
        std::fs::write(temp.path().join("test.md"), "hello world\nsearch test\n")
            .expect("seed note");

        let mcp_server = Arc::new(CrucibleMcpServer::new(
            temp.path().display().to_string(),
            Arc::new(EmptyKnowledgeRepository),
            Arc::new(EmptyEmbeddingProvider),
        ));

        let providers: Vec<Arc<dyn ToolExecutor>> = vec![
            workspace_tools(),
            Arc::new(McpToolExecutor::new(mcp_server)),
        ];

        (temp, DaemonToolDispatcher::new(providers))
    }

    #[test]
    fn test_daemon_tool_dispatcher_construction() {
        let workspace_tools = workspace_tools();
        let dispatcher = DaemonToolDispatcher::new(vec![workspace_tools.clone()]);

        assert!(std::mem::size_of_val(&dispatcher) > 0);
    }

    #[test]
    fn test_daemon_tool_dispatcher_holds_workspace_tools_arc() {
        let workspace_tools = workspace_tools();
        let strong_count = Arc::strong_count(&workspace_tools);

        let _dispatcher = DaemonToolDispatcher::new(vec![workspace_tools.clone()]);

        assert_eq!(Arc::strong_count(&workspace_tools), strong_count + 1);
    }

    #[test]
    fn test_has_tool_checks_workspace_definitions() {
        let dispatcher = test_dispatcher();

        assert!(dispatcher.has_tool("read_file"));
        assert!(!dispatcher.has_tool("not_a_tool"));
    }

    #[tokio::test]
    async fn red_dispatch_get_kiln_info_is_not_unknown_tool() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher.dispatch_tool("get_kiln_info", json!({})).await;

        assert!(
            !matches!(result, Err(ref err) if err.contains("Unknown tool")),
            "get_kiln_info should not route to Unknown tool error: {result:?}"
        );
    }

    #[tokio::test]
    async fn red_dispatch_list_notes_is_not_unknown_tool() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher.dispatch_tool("list_notes", json!({})).await;

        assert!(
            !matches!(result, Err(ref err) if err.contains("Unknown tool")),
            "list_notes should not route to Unknown tool error: {result:?}"
        );
    }

    #[tokio::test]
    async fn red_dispatch_read_note_is_not_unknown_tool() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher
            .dispatch_tool("read_note", json!({ "path": "test.md" }))
            .await;

        assert!(
            !matches!(result, Err(ref err) if err.contains("Unknown tool")),
            "read_note should not route to Unknown tool error: {result:?}"
        );
    }

    #[tokio::test]
    async fn red_dispatch_text_search_is_not_unknown_tool() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher
            .dispatch_tool("text_search", json!({ "query": "test" }))
            .await;

        assert!(
            !matches!(result, Err(ref err) if err.contains("Unknown tool")),
            "text_search should not route to Unknown tool error: {result:?}"
        );
    }

    #[test]
    fn red_has_tool_reports_get_kiln_info() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();

        assert!(dispatcher.has_tool("get_kiln_info"));
    }

    #[tokio::test]
    async fn workspace_tools_still_dispatch_with_mcp_provider_present() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher
            .dispatch_tool("glob", json!({ "pattern": "**/*.md" }))
            .await;

        assert!(
            result.is_ok(),
            "workspace tool should still dispatch: {result:?}"
        );
    }

    #[tokio::test]
    async fn mcp_tool_executor_handles_all_listed_tools() {
        // This test catches the case where a #[tool] method is added to CrucibleMcpServer
        // without a corresponding match arm in execute_tool(). If a tool is listed but not dispatched,
        // execute_tool returns ToolError::NotFound, which fails this test.
        let temp = TempDir::new().expect("tempdir");
        let mcp_server = Arc::new(CrucibleMcpServer::new(
            temp.path().display().to_string(),
            Arc::new(EmptyKnowledgeRepository),
            Arc::new(EmptyEmbeddingProvider),
        ));
        let executor = McpToolExecutor::new(mcp_server);
        let ctx = ExecutionContext::default();

        // Get all tools from list_tools()
        let tools =
            futures::executor::block_on(executor.list_tools()).expect("list_tools should succeed");

        // For each tool, verify it has a match arm in execute_tool()
        for tool in tools {
            let result = futures::executor::block_on(executor.execute_tool(
                &tool.name,
                serde_json::json!({}),
                &ctx,
            ));

            // We expect either Ok or an error OTHER than NotFound.
            // NotFound means the tool was listed but not dispatched (bug).
            // InvalidParameters or ExecutionFailed are fine — they prove the match arm was hit.
            assert!(
                !matches!(result, Err(ToolError::NotFound(_))),
                "Tool '{}' is listed by list_tools() but not handled in execute_tool(): {:?}",
                tool.name,
                result
            );
        }
    }
}
