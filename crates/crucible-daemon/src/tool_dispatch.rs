use async_trait::async_trait;
use crucible_core::traits::tools::{
    ExecutionContext, ToolDefinition, ToolError, ToolExecutor, ToolResult,
};
use crucible_core::types::{ToolRef, ToolSource};
use futures::FutureExt;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{RawContent, Tool};
use serde::de::DeserializeOwned;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::RwLock;

use crate::tools::mcp_server::CrucibleMcpServer;
use crate::tools::mcp_server::{
    BashToolParams, EditFileToolParams, GlobToolParams, GrepToolParams, ReadFileToolParams,
    WriteFileToolParams,
};
use crate::tools::mcp_server::{
    CancelJobParams, DelegateSessionParams, GetJobResultParams, ListJobsParams, SkillViewParams,
};
use crate::tools::notes::{
    CreateNoteParams, DeleteNoteParams, ListNotesParams, ReadMetadataParams, ReadNoteParams,
    UpdateNoteParams,
};
use crate::tools::search::{PropertySearchParams, SemanticSearchParams, TextSearchParams};
use crate::tools::tool_discovery::{DiscoverToolsParams, GetToolSchemaParams, ToolDiscovery};

/// Names of the progressive-disclosure discovery tools handled directly by
/// the dispatcher (not routed to a provider). `invoke_tool` is intentionally
/// absent: it is unwrapped to its inner tool upstream in
/// `handle_tool_call_in_stream` and never reaches dispatch.
const DISCOVERY_TOOL_NAMES: &[&str] = &["discover_tools", "get_tool_schema"];

/// Flatten an rmcp `CallToolResult` into a JSON value: parse the joined text
/// content as JSON when possible, otherwise return it as a string. Errors map
/// to `Err` so callers surface them as tool errors.
fn call_tool_result_to_value(
    result: rmcp::model::CallToolResult,
) -> Result<serde_json::Value, String> {
    let text = result
        .content
        .into_iter()
        .filter_map(|c| match c.raw {
            RawContent::Text(t) => Some(t.text),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    if result.is_error.unwrap_or(false) {
        return Err(text);
    }
    Ok(serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)))
}

#[async_trait]
pub trait ToolDispatcher: Send + Sync {
    async fn dispatch_tool(
        &self,
        name: &str,
        args: serde_json::Value,
        env_vars: std::collections::HashMap<String, String>,
    ) -> Result<serde_json::Value, String>;
    fn has_tool(&self, name: &str) -> bool;
    fn get_tool_ref(&self, name: &str) -> Option<ToolRef>;
}

pub struct DaemonToolDispatcher {
    providers: Vec<Arc<dyn ToolExecutor>>,
    tool_names: RwLock<HashSet<String>>,
    tool_names_hydrated: AtomicBool,
    tool_refs: RwLock<HashMap<String, ToolRef>>,
    tool_refs_hydrated: AtomicBool,
}

impl DaemonToolDispatcher {
    pub fn new(providers: Vec<Arc<dyn ToolExecutor>>) -> Self {
        let mut tool_names = HashSet::new();
        let mut tool_refs = HashMap::new();
        for provider in &providers {
            if let Some(Ok(defs)) = provider.list_tools().now_or_never() {
                for def in defs {
                    tool_names.insert(def.name.clone());
                    let tool_ref = Self::tool_ref_from_definition(&def);
                    tool_refs.entry(def.name).or_insert(tool_ref);
                }
            }
        }

        Self {
            providers,
            tool_names: RwLock::new(tool_names),
            tool_names_hydrated: AtomicBool::new(false),
            tool_refs: RwLock::new(tool_refs),
            tool_refs_hydrated: AtomicBool::new(false),
        }
    }

    fn is_core_tool_name(name: &str) -> bool {
        matches!(
            name,
            "read_file" | "edit_file" | "write_file" | "bash" | "glob" | "grep"
        )
    }

    fn tool_ref_from_definition(def: &ToolDefinition) -> ToolRef {
        let schema = def
            .parameters
            .clone()
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        let description = if def.description.is_empty() {
            "No description".to_string()
        } else {
            def.description.clone()
        };
        let tool = Tool::new(def.name.clone(), description, Arc::new(schema));
        let source = if Self::is_core_tool_name(&def.name) {
            ToolSource::Core
        } else {
            ToolSource::Crucible
        };

        ToolRef {
            name: def.name.clone(),
            source,
            definition: tool,
            tags: Vec::new(),
            always_available: true,
        }
    }

    async fn hydrate_tool_names(&self) {
        if self.tool_names_hydrated.load(Ordering::Acquire) {
            return;
        }

        let mut discovered_names = HashSet::new();
        let mut discovered_refs = HashMap::new();
        for provider in &self.providers {
            if let Ok(defs) = provider.list_tools().await {
                for def in defs {
                    discovered_names.insert(def.name.clone());
                    let tool_ref = Self::tool_ref_from_definition(&def);
                    discovered_refs.entry(def.name).or_insert(tool_ref);
                }
            }
        }

        if discovered_names.is_empty() {
            self.tool_names_hydrated.store(true, Ordering::Release);
            return;
        }

        self.tool_names
            .write()
            .expect("tool_names lock poisoned")
            .extend(discovered_names);
        self.tool_refs
            .write()
            .expect("tool_refs lock poisoned")
            .extend(discovered_refs);
        self.tool_names_hydrated.store(true, Ordering::Release);
        self.tool_refs_hydrated.store(true, Ordering::Release);
    }

    fn hydrate_tool_names_blocking(&self) {
        if self.tool_names_hydrated.load(Ordering::Acquire) {
            return;
        }

        // NOTE(crucible): spawns a throwaway current-thread runtime and joins it
        // so a sync caller (has_tool/get_tool_ref) can drive async list_tools
        // without blocking an existing runtime's worker. Only runs once per
        // dispatcher (guarded by tool_names_hydrated); the async hydrate path is
        // preferred wherever an await is available.
        let providers = self.providers.clone();
        let (discovered_names, discovered_refs) = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            let mut names = HashSet::new();
            let mut refs = HashMap::new();

            if let Ok(runtime) = runtime {
                runtime.block_on(async {
                    for provider in &providers {
                        if let Ok(defs) = provider.list_tools().await {
                            for def in defs {
                                names.insert(def.name.clone());
                                let tool_ref = DaemonToolDispatcher::tool_ref_from_definition(&def);
                                refs.entry(def.name).or_insert(tool_ref);
                            }
                        }
                    }
                });
            }

            (names, refs)
        })
        .join()
        .unwrap_or_default();

        if !discovered_names.is_empty() {
            self.tool_names
                .write()
                .expect("tool_names lock poisoned")
                .extend(discovered_names);
        }

        if !discovered_refs.is_empty() {
            self.tool_refs
                .write()
                .expect("tool_refs lock poisoned")
                .extend(discovered_refs);
        }

        self.tool_names_hydrated.store(true, Ordering::Release);
        self.tool_refs_hydrated.store(true, Ordering::Release);
    }

    fn hydrate_tool_refs_blocking(&self) {
        if self.tool_refs_hydrated.load(Ordering::Acquire) {
            return;
        }
        self.hydrate_tool_names_blocking();
    }

    /// Aggregate every provider's tools into a `ToolDiscovery` so the
    /// `discover_tools`/`get_tool_schema` bridge can search and inspect the
    /// full catalog — including deferred (gateway) tools that were dropped
    /// from the request's attached schemas.
    ///
    /// NOTE(crucible): re-lists providers on every bridge call. That's cheap
    /// today (a handful of providers, cached upstream tool lists) and keeps the
    /// catalog fresh if a gateway reconnects mid-session; revisit with a cached
    /// snapshot if provider `list_tools` ever becomes expensive.
    async fn build_tool_discovery(&self) -> ToolDiscovery {
        let mut tools: Vec<Tool> = Vec::new();
        for provider in &self.providers {
            if let Ok(defs) = provider.list_tools().await {
                for def in defs {
                    let schema = def
                        .parameters
                        .and_then(|v| v.as_object().cloned())
                        .unwrap_or_default();
                    let description = if def.description.is_empty() {
                        "No description".to_string()
                    } else {
                        def.description
                    };
                    tools.push(Tool::new(def.name, description, Arc::new(schema)));
                }
            }
        }
        ToolDiscovery::new(tools)
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
            "skill_view" => {
                self.server
                    .skill_view(Self::parse_params::<SkillViewParams>(params)?)
                    .await
            }
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
            "read_file" => {
                self.server
                    .read_file(Self::parse_params::<ReadFileToolParams>(params)?)
                    .await
            }
            "edit_file" => {
                self.server
                    .edit_file(Self::parse_params::<EditFileToolParams>(params)?)
                    .await
            }
            "write_file" => {
                self.server
                    .write_file(Self::parse_params::<WriteFileToolParams>(params)?)
                    .await
            }
            "bash" => {
                self.server
                    .bash(Self::parse_params::<BashToolParams>(params)?)
                    .await
            }
            "glob" => {
                self.server
                    .glob(Self::parse_params::<GlobToolParams>(params)?)
                    .await
            }
            "grep" => {
                self.server
                    .grep(Self::parse_params::<GrepToolParams>(params)?)
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

/// Dispatches gateway (user MCP) tools through the shared `McpGatewayManager`,
/// scoped to the session agent's configured upstream servers. Registering this
/// as a dispatcher provider makes deferred gateway tools reachable via the
/// progressive-disclosure bridge (`discover_tools` → `invoke_tool`).
pub struct GatewayToolExecutor {
    gateway: Arc<tokio::sync::RwLock<crate::tools::mcp_gateway::McpGatewayManager>>,
    allowed_servers: HashSet<String>,
}

impl GatewayToolExecutor {
    pub fn new(
        gateway: Arc<tokio::sync::RwLock<crate::tools::mcp_gateway::McpGatewayManager>>,
        allowed_servers: Vec<String>,
    ) -> Self {
        Self {
            gateway,
            allowed_servers: allowed_servers.into_iter().collect(),
        }
    }
}

#[async_trait]
impl ToolExecutor for GatewayToolExecutor {
    async fn execute_tool(
        &self,
        name: &str,
        params: serde_json::Value,
        _context: &ExecutionContext,
    ) -> ToolResult<serde_json::Value> {
        let gateway = self.gateway.read().await;
        // Only dispatch tools belonging to the agent's configured servers;
        // anything else falls through the provider chain as NotFound.
        match gateway.find_upstream(name) {
            Some(upstream) if self.allowed_servers.contains(upstream) => {}
            _ => return Err(ToolError::NotFound(name.to_string())),
        }
        match gateway.call_tool(name, params).await {
            Ok(result) => {
                let text = result
                    .content
                    .iter()
                    .filter_map(|c| c.as_text().map(str::to_string))
                    .collect::<Vec<_>>()
                    .join("\n");
                if result.is_error {
                    Err(ToolError::ExecutionFailed(text))
                } else {
                    Ok(serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)))
                }
            }
            Err(err) => Err(ToolError::ExecutionFailed(err.to_string())),
        }
    }

    async fn list_tools(&self) -> ToolResult<Vec<ToolDefinition>> {
        let gateway = self.gateway.read().await;
        Ok(gateway
            .all_tools()
            .into_iter()
            .filter(|t| self.allowed_servers.contains(&t.upstream))
            .map(|t| ToolDefinition {
                name: t.prefixed_name,
                description: t.description.unwrap_or_default(),
                category: Some("mcp".to_string()),
                parameters: Some(t.input_schema),
                returns: None,
                examples: vec![],
                required_permissions: vec![],
            })
            .collect())
    }
}

#[async_trait]
impl ToolDispatcher for DaemonToolDispatcher {
    async fn dispatch_tool(
        &self,
        name: &str,
        args: serde_json::Value,
        env_vars: std::collections::HashMap<String, String>,
    ) -> Result<serde_json::Value, String> {
        self.hydrate_tool_names().await;

        // Progressive-disclosure bridge: search/inspect the full tool catalog.
        // Handled here rather than by a provider so it spans every provider.
        match name {
            "discover_tools" => {
                let params: DiscoverToolsParams = if args.is_null() {
                    DiscoverToolsParams::default()
                } else {
                    serde_json::from_value(args)
                        .map_err(|e| format!("invalid discover_tools params: {e}"))?
                };
                return self
                    .build_tool_discovery()
                    .await
                    .discover_tools(&params)
                    .map_err(|e| e.to_string())
                    .and_then(call_tool_result_to_value);
            }
            "get_tool_schema" => {
                let params: GetToolSchemaParams = serde_json::from_value(args)
                    .map_err(|e| format!("invalid get_tool_schema params: {e}"))?;
                return self
                    .build_tool_discovery()
                    .await
                    .get_tool_schema(&params)
                    .map_err(|e| e.to_string())
                    .and_then(call_tool_result_to_value);
            }
            _ => {}
        }

        let ctx = ExecutionContext {
            env_vars,
            ..ExecutionContext::default()
        };

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
        if DISCOVERY_TOOL_NAMES.contains(&name) {
            return true;
        }

        if !self.tool_names_hydrated.load(Ordering::Acquire) {
            self.hydrate_tool_names_blocking();
        }

        self.tool_names
            .read()
            .expect("tool_names lock poisoned")
            .contains(name)
    }

    fn get_tool_ref(&self, name: &str) -> Option<ToolRef> {
        if !self.tool_refs_hydrated.load(Ordering::Acquire) {
            self.hydrate_tool_refs_blocking();
        }

        self.tool_refs
            .read()
            .expect("tool_refs lock poisoned")
            .get(name)
            .cloned()
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
        Arc::new(WorkspaceTools::new(std::path::PathBuf::from("/tmp")))
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
        let result = dispatcher
            .dispatch_tool("get_kiln_info", json!({}), Default::default())
            .await;

        assert!(
            !matches!(result, Err(ref err) if err.contains("Unknown tool")),
            "get_kiln_info should not route to Unknown tool error: {result:?}"
        );
    }

    #[tokio::test]
    async fn red_dispatch_list_notes_is_not_unknown_tool() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher
            .dispatch_tool("list_notes", json!({}), Default::default())
            .await;

        assert!(
            !matches!(result, Err(ref err) if err.contains("Unknown tool")),
            "list_notes should not route to Unknown tool error: {result:?}"
        );
    }

    #[tokio::test]
    async fn red_dispatch_read_note_is_not_unknown_tool() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher
            .dispatch_tool(
                "read_note",
                json!({ "path": "test.md" }),
                Default::default(),
            )
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
            .dispatch_tool(
                "text_search",
                json!({ "query": "test" }),
                Default::default(),
            )
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
            .dispatch_tool("glob", json!({ "pattern": "**/*.md" }), Default::default())
            .await;

        assert!(
            result.is_ok(),
            "workspace tool should still dispatch: {result:?}"
        );
    }

    #[tokio::test]
    async fn red_dispatch_discover_tools_is_not_unknown_tool() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher
            .dispatch_tool("discover_tools", json!({}), Default::default())
            .await;

        assert!(
            !matches!(result, Err(ref err) if err.contains("Unknown tool")),
            "discover_tools should be handled by the bridge, not Unknown: {result:?}"
        );
        let value = result.expect("discover_tools should succeed");
        let rendered = value.to_string();
        assert!(
            rendered.contains("read_file") || rendered.contains("get_kiln_info"),
            "discovery output should list provider tools: {rendered}"
        );
    }

    #[tokio::test]
    async fn dispatch_get_tool_schema_returns_schema_for_known_tool() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher
            .dispatch_tool(
                "get_tool_schema",
                json!({ "name": "read_file" }),
                Default::default(),
            )
            .await
            .expect("get_tool_schema should succeed for a known tool");

        assert!(
            result.to_string().contains("read_file"),
            "schema output should name the tool: {result}"
        );
    }

    #[tokio::test]
    async fn dispatch_get_tool_schema_errors_for_unknown_tool() {
        let (_temp, dispatcher) = test_dispatcher_with_mcp();
        let result = dispatcher
            .dispatch_tool(
                "get_tool_schema",
                json!({ "name": "does_not_exist" }),
                Default::default(),
            )
            .await;

        assert!(result.is_err(), "unknown tool schema lookup should error");
    }

    #[test]
    fn has_tool_reports_discovery_bridge() {
        let dispatcher = test_dispatcher();
        assert!(dispatcher.has_tool("discover_tools"));
        assert!(dispatcher.has_tool("get_tool_schema"));
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
