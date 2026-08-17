use async_trait::async_trait;
use crucible_core::traits::tools::{
    ExecutionContext, ToolDefinition, ToolError, ToolExecutor, ToolResult, ToolSurface,
};
use crucible_core::types::{ToolRef, ToolSource};
use futures::FutureExt;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ContentBlock, Tool};
use serde::de::DeserializeOwned;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::RwLock;

use crate::tools::mcp_server::CrucibleMcpServer;
use crate::tools::mcp_server::{
    CancelJobParams, DelegateSessionParams, GetJobResultParams, ListJobsParams, SkillViewParams,
};
use crate::tools::notes::{
    CreateNoteParams, DeleteNoteParams, ListNotesParams, ReadMetadataParams, ReadNoteParams,
    UpdateNoteParams,
};
use crate::tools::search::{GrepNotesParams, PropertySearchParams, SemanticSearchParams};
use crate::tools::tool_discovery::{DiscoverToolsParams, GetToolSchemaParams, ToolDiscovery};

/// Names of the progressive-disclosure discovery tools handled directly by
/// the dispatcher (not routed to a provider). `invoke_tool` is intentionally
/// absent: it is unwrapped to its inner tool upstream in
/// `handle_tool_call_in_stream` and never reaches dispatch.
pub(crate) const DISCOVERY_TOOL_NAMES: &[&str] = &["discover_tools", "get_tool_schema"];

/// How long the blocking hydration path waits for providers to list their tools.
///
/// Insurance, not a fix for anything reproducible: no provider's `list_tools`
/// awaits I/O today (the gateway's reads a cached list behind an `RwLock` that
/// has no production writer), so this budget is never reached. It exists because
/// the callers are `has_tool`/`get_tool_ref` inside the per-tool-call loop, one
/// plausible provider away from being an unbounded wait on the network — and
/// because the throwaway runtime makes a foreign-runtime lock inversion possible
/// the moment that `RwLock` gains a writer.
const BLOCKING_HYDRATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// The surface recorded for `name`: the executor's answer, but only where the
/// per-tool table agrees with it.
///
/// [`ToolExecutor::surface`] takes the tool name and an implementation is free
/// to ignore it — the exhaustiveness guarantee in [`crate::tools::surface`]
/// binds names routed through `classify`, not executors. A provider
/// advertising `dump_env` and answering `ToolSurface::Daemon` for every name
/// would hand itself a sandbox exemption for a tool nobody classified, and the
/// gate would honour it, because the gate asks the catalog and the catalog
/// took the executor at its word.
///
/// So a claim is only as good as the table's agreement. `Unknown` claims stand
/// (that is the floor, and it is what plugin and gateway executors answer for
/// everything); a claim the table does not corroborate is recorded as
/// `Unknown`, which the isolation gate refuses. Absence still denies — a name
/// the table does not know cannot be talked into a classification by the code
/// that would run it.
fn corroborated_surface(claimed: ToolSurface, name: &str) -> ToolSurface {
    let table = crate::tools::surface::classify(name);
    if claimed == table {
        return claimed;
    }
    if claimed != ToolSurface::Unknown {
        tracing::warn!(
            tool = %name,
            ?claimed,
            ?table,
            "executor claimed a surface the built-in table does not give this tool; \
             recording Unknown so an isolated session refuses it"
        );
    }
    ToolSurface::Unknown
}

/// Flatten an rmcp `CallToolResult` into a JSON value: parse the joined text
/// content as JSON when possible, otherwise return it as a string. Errors map
/// to `Err` so callers surface them as tool errors.
fn call_tool_result_to_value(
    result: rmcp::model::CallToolResult,
) -> Result<serde_json::Value, String> {
    let text = result
        .content
        .into_iter()
        .filter_map(|c| match c {
            ContentBlock::Text(t) => Some(t.text),
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

    /// What running `name` would reach if nothing intercepts it.
    ///
    /// A name no provider claims answers [`ToolSurface::Unknown`]: an isolation
    /// gate asking this question must fail closed on a tool the daemon cannot
    /// place, not wave it through.
    async fn tool_surface(&self, name: &str) -> ToolSurface;
}

pub struct DaemonToolDispatcher {
    providers: Vec<Arc<dyn ToolExecutor>>,
    tool_names: RwLock<HashSet<String>>,
    tool_names_hydrated: AtomicBool,
    tool_refs: RwLock<HashMap<String, ToolRef>>,
    tool_refs_hydrated: AtomicBool,
    /// `name -> surface` of the provider that `dispatch_tool` will actually
    /// reach. Filled by the same provider walk as `tool_refs` and with the
    /// same first-provider-wins rule, so the classification can never describe
    /// a different executor than the one that runs.
    tool_surfaces: RwLock<HashMap<String, ToolSurface>>,
    /// Budget for [`Self::hydrate_tool_names_blocking`]. A field rather than
    /// only a const so a test can prove the bound without waiting it out.
    blocking_hydration_timeout: std::time::Duration,
    /// Serialises blocking hydration so concurrent callers share one attempt.
    /// `parking_lot` rather than `std` because the walk writes through
    /// `.expect("… poisoned")` locks: a panic there must not additionally
    /// poison this one and turn every later `has_tool` into a panic.
    hydration_turn: parking_lot::Mutex<()>,
    /// Bumped once per blocking hydration walk, under `hydration_turn`. A
    /// caller compares the value it read before queueing against the value it
    /// sees once it holds the turn: if they differ, someone else ran the walk
    /// it was waiting on and its outcome is the one to take.
    hydration_attempts: std::sync::atomic::AtomicU64,
}

impl DaemonToolDispatcher {
    pub fn new(providers: Vec<Arc<dyn ToolExecutor>>) -> Self {
        let mut tool_names = HashSet::new();
        let mut tool_refs = HashMap::new();
        let mut tool_surfaces = HashMap::new();
        for provider in &providers {
            if let Some(Ok(defs)) = provider.list_tools().now_or_never() {
                for def in defs {
                    tool_names.insert(def.name.clone());
                    let tool_ref = Self::tool_ref_from_definition(&def);
                    let surface = corroborated_surface(provider.surface(&def.name), &def.name);
                    tool_surfaces.entry(def.name.clone()).or_insert(surface);
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
            tool_surfaces: RwLock::new(tool_surfaces),
            blocking_hydration_timeout: BLOCKING_HYDRATION_TIMEOUT,
            hydration_turn: parking_lot::Mutex::new(()),
            hydration_attempts: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Test-support: shorten the hydration budget so a test can prove the bound
    /// exists without waiting out the production one.
    #[cfg(test)]
    pub(crate) fn with_blocking_hydration_timeout(mut self, budget: std::time::Duration) -> Self {
        self.blocking_hydration_timeout = budget;
        self
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
        let mut discovered_surfaces = HashMap::new();
        for provider in &self.providers {
            if let Ok(defs) = provider.list_tools().await {
                for def in defs {
                    discovered_names.insert(def.name.clone());
                    let tool_ref = Self::tool_ref_from_definition(&def);
                    let surface = corroborated_surface(provider.surface(&def.name), &def.name);
                    discovered_surfaces
                        .entry(def.name.clone())
                        .or_insert(surface);
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
        self.tool_surfaces
            .write()
            .expect("tool_surfaces lock poisoned")
            .extend(discovered_surfaces);
        self.tool_names_hydrated.store(true, Ordering::Release);
        self.tool_refs_hydrated.store(true, Ordering::Release);
    }

    fn hydrate_tool_names_blocking(&self) {
        if self.tool_names_hydrated.load(Ordering::Acquire) {
            return;
        }

        // `has_tool` runs once per tool call and a turn can dispatch many at
        // once, so without this the K callers that arrive before the first one
        // finishes each spawn their own OS thread and tokio runtime to redo the
        // identical walk. Take a ticket before queueing: if it has moved by the
        // time we hold the turn, another caller already ran the walk we were
        // waiting on and its outcome is ours too. A *later* call finds the
        // ticket unchanged and retries, which is what keeps a timed-out
        // hydration recoverable rather than a permanent empty catalog.
        let ticket = self.hydration_attempts.load(Ordering::Acquire);
        let _turn = self.hydration_turn.lock();
        if self.tool_names_hydrated.load(Ordering::Acquire)
            || self.hydration_attempts.load(Ordering::Acquire) != ticket
        {
            return;
        }

        /// Counts the walk as attempted on every exit path, including the
        /// timeout return below. Declared after `_turn` so it drops *before*
        /// the turn is released: a waiter therefore cannot observe the bump
        /// while it could still take the lock and redo the same walk.
        struct CountAttempt<'a>(&'a std::sync::atomic::AtomicU64);
        impl Drop for CountAttempt<'_> {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Release);
            }
        }
        let _attempt = CountAttempt(&self.hydration_attempts);

        // NOTE(crucible): spawns a throwaway current-thread runtime so a sync
        // caller (has_tool/get_tool_ref) can drive async list_tools without
        // blocking an existing runtime's worker. Only runs once per dispatcher
        // (guarded by tool_names_hydrated); the async hydrate path is preferred
        // wherever an await is available.
        //
        // The wait is bounded twice, and both bounds are needed. The inner
        // per-provider timeout is what lets the thread finish, so a provider
        // that never returns costs one abandoned listing rather than a leaked
        // thread and runtime. The outer `recv_timeout` is what unblocks the
        // caller, and it also covers the cases outside the awaits — a runtime
        // that fails to build, or a provider blocking synchronously. Neither
        // marks the dispatcher hydrated, so a later call retries.
        let providers = self.providers.clone();
        let budget = self.blocking_hydration_timeout;
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            let mut names = HashSet::new();
            let mut refs = HashMap::new();
            let mut surfaces = HashMap::new();

            if let Ok(runtime) = runtime {
                runtime.block_on(async {
                    for provider in &providers {
                        let listed = tokio::time::timeout(budget, provider.list_tools()).await;
                        let Ok(Ok(defs)) = listed else {
                            continue;
                        };
                        for def in defs {
                            names.insert(def.name.clone());
                            let tool_ref = DaemonToolDispatcher::tool_ref_from_definition(&def);
                            let surface =
                                corroborated_surface(provider.surface(&def.name), &def.name);
                            surfaces.entry(def.name.clone()).or_insert(surface);
                            refs.entry(def.name).or_insert(tool_ref);
                        }
                    }
                });
            }

            let _ = result_tx.send((names, refs, surfaces));
        });

        let (discovered_names, discovered_refs, discovered_surfaces) =
            match result_rx.recv_timeout(budget) {
                Ok(discovered) => discovered,
                Err(_) => {
                    tracing::warn!(
                        timeout_secs = budget.as_secs(),
                        "tool hydration did not finish in time; treating the catalog as \
                         incomplete so `has_tool` answers false rather than hanging the turn"
                    );
                    return;
                }
            };

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

        if !discovered_surfaces.is_empty() {
            self.tool_surfaces
                .write()
                .expect("tool_surfaces lock poisoned")
                .extend(discovered_surfaces);
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
            match content {
                ContentBlock::Text(text) => {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text.text) {
                        values.push(value);
                    } else {
                        text_parts.push(text.text);
                    }
                }
                ContentBlock::Image(_) => text_parts.push("[image content]".to_string()),
                ContentBlock::Resource(_) => text_parts.push("[resource content]".to_string()),
                ContentBlock::Audio(_) => text_parts.push("[audio content]".to_string()),
                ContentBlock::ResourceLink(link) => text_parts.push(link.uri),
                // ContentBlock is #[non_exhaustive] — new upstream variants
                // shouldn't silently drop a tool result on the floor.
                _ => text_parts.push("[unsupported content]".to_string()),
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
        // `list_tools` already hides these on a kiln-less session; this is the
        // same rule on the dispatch path, for a model that remembers a tool
        // name from before a `session.disconnect_kiln`. `NotFound` rather than
        // an execution failure: with no kiln the tool genuinely does not
        // exist, and letting it through would reach
        // `validate_path_within_kiln` with `""`.
        if !self.server.has_kiln() && crate::tools::mcp_server::KILN_BACKED_TOOLS.contains(&name) {
            return Err(ToolError::NotFound(format!(
                "{name} needs a kiln, and this session has none attached"
            )));
        }
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
            "grep_notes" => {
                self.server
                    .grep_notes(Self::parse_params::<GrepNotesParams>(params)?)
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
            // Workspace tools are not on this server; `WorkspaceTools` is its
            // own `ToolExecutor` and is the one the daemon registers. Routing
            // them here too gave the model duplicate definitions and put an
            // ungated `bash` on the external MCP surface.
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

    /// Per tool, never per executor.
    ///
    /// This method used to return a flat `ToolSurface::Daemon` for the whole
    /// catalog, justified as "all of it lands in daemon-side storage or the
    /// daemon's own managers". That was false for `create_note`, `update_note`
    /// and `delete_note`, which reach `std::fs` on the host in this process —
    /// so an isolated session refused `bash` and `write_file` while
    /// `create_note` wrote the host filesystem. It was also *inherited*: a new
    /// `#[tool]` on `CrucibleMcpServer` became `Daemon` with nobody deciding.
    ///
    /// Now a name with no classification answers `Unknown` and is refused, and
    /// `builtin_surfaces_cover_every_advertised_tool` fails the build with the
    /// tool's name in the message.
    fn surface(&self, tool: &str) -> ToolSurface {
        crate::tools::surface::classify(tool)
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

    async fn tool_surface(&self, name: &str) -> ToolSurface {
        // The progressive-disclosure bridge belongs to no provider: it is
        // answered by this dispatcher out of its own catalog and touches
        // nothing. No provider lists it, so it is absent from `tool_surfaces`
        // and would fall to `Unknown` and be refused inside every sandboxed
        // session — the agent could not even ask what tools exist. Classified
        // in the same table as everything else rather than answered `Daemon`
        // here, so the exhaustiveness check covers it too.
        if DISCOVERY_TOOL_NAMES.contains(&name) {
            return crate::tools::surface::classify(name);
        }

        self.hydrate_tool_names().await;

        self.tool_surfaces
            .read()
            .expect("tool_surfaces lock poisoned")
            .get(name)
            .copied()
            .unwrap_or(ToolSurface::Unknown)
    }
}

#[cfg(test)]
mod tests;
