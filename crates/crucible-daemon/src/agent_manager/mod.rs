//! Agent lifecycle management for the daemon.

use crate::acp::discovery::default_agent_profiles;
use crate::agent_factory::{
    create_agent_from_session_config, AgentFactoryError, CreateAgentFromSessionConfigParams,
};
use crate::background_manager::{BackgroundJobManager, SubagentContext};
use crate::daemon_plugins::DaemonPluginLoader;
use crate::event_emitter::emit_event;
use crate::kiln_manager::KilnManager;
use crate::multi_kiln_search::{search_across_kilns, KilnSearchSource};
use crate::permission_bridge::{DaemonPermissionGate, PermissionPromptCallback};
use crate::protocol::SessionEventMessage;
use crate::provider::model_listing;
use crate::session_manager::{SessionError, SessionManager};
use crate::tool_dispatch::{DaemonToolDispatcher, ToolDispatcher};
use crate::tools::workspace::WorkspaceTools;
use crate::trust_resolution::resolve_provider_trust;
use crucible_core::config::components::permissions::{PermissionConfig, PermissionMode};
use crucible_core::config::{
    AcpConfig, AgentProfile, BackendType, DataClassification, LlmProviderConfig, PatternStore,
};
use crucible_core::discovery::DiscoveryPaths;
use crucible_core::events::{
    InternalSessionEvent, Reactor, ReactorEmitResult as EmitResult, SessionEvent,
};
use crucible_core::interaction::{InteractionRequest, PermRequest, PermResponse, PermissionScope};
use crucible_core::session::{ContextStrategy, OutputValidation, SessionAgent};
use crucible_core::traits::chat::{AgentHandle, ChatError};
use crucible_core::traits::tools::ToolExecutor;
use crucible_core::traits::PermissionGate;
use crucible_lua::{
    execute_permission_hooks, register_crucible_on_api, register_permission_hook_api,
    LuaScriptHandlerRegistry, LuaValidatorRegistry, PermissionHook, PermissionHookResult,
    PermissionRequest,
};
use dashmap::DashMap;
use mlua::Lua;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Unique identifier for a pending permission request.
pub type PermissionId = String;

pub(crate) const MODEL_CACHE_TTL: Duration = Duration::from_secs(300);

/// Check if a tool is safe to execute without requiring explicit permission.
///
/// Safe tools are read-only operations that only query data without modifying state.
/// Unsafe tools can modify files, execute commands, or change state and require permission.
///
/// # Safe Tools (is_safe=true)
///
/// **MCP Tools (10 read-only):**
/// - `semantic_search` — Search notes using semantic similarity
/// - `text_search` — Fast full-text search across notes
/// - `property_search` — Search notes by frontmatter properties (includes tags)
/// - `list_notes` — List notes in a directory
/// - `read_note` — Read note content with optional line range
/// - `read_metadata` — Read note metadata without loading full content
/// - `get_kiln_info` — Get comprehensive kiln information including root path and statistics
/// - `list_jobs` — List all background jobs (running and completed) for the current session
///
/// **Workspace Tools (Rig-native, 3 read-only):**
/// - `read_file` — Read file content
/// - `glob` — Find files matching patterns
/// - `grep` — Search file contents
///
/// # Unsafe Tools (is_safe=false)
///
/// **MCP Tools (6 mutating):**
/// - `create_note` — Create a new note in the kiln
/// - `update_note` — Update an existing note
/// - `delete_note` — Delete a note from the kiln
/// - `delegate_session` — Delegate a task to another AI agent
/// - `get_job_result` — Get the result of a background job
/// - `cancel_job` — Cancel a running background job by ID
///
/// **Workspace Tools (Rig-native, 3 mutating):**
/// - `write` — Write file content
/// - `edit` — Edit file content
/// - `bash` — Execute shell commands
///
/// # Default-Deny Policy
///
/// Only explicitly safe tools skip the permission prompt.
/// Everything unknown (including all external MCP tools) requires permission.
pub fn is_safe(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "read_file"
            | "glob"
            | "grep"
            | "semantic_search"
            | "text_search"
            | "property_search"
            | "list_notes"
            | "read_note"
            | "read_metadata"
            | "get_kiln_info"
            | "list_jobs"
    )
}

pub(crate) fn resolve_agent_profile(
    name: &str,
    configured: &HashMap<String, AgentProfile>,
    available: &HashMap<String, AgentProfile>,
) -> Option<AgentProfile> {
    let profile = configured.get(name)?;
    let base_name = profile.extends.as_deref().unwrap_or(name);

    let mut resolved = available.get(base_name).cloned().unwrap_or_default();
    resolved.extends = profile.extends.clone();

    if let Some(command) = &profile.command {
        resolved.command = Some(command.clone());
    }
    if let Some(args) = &profile.args {
        resolved.args = Some(args.clone());
    }
    if let Some(description) = &profile.description {
        resolved.description = Some(description.clone());
    }
    if let Some(capabilities) = &profile.capabilities {
        resolved.capabilities = Some(capabilities.clone());
    }
    if let Some(delegation) = &profile.delegation {
        resolved.delegation = Some(delegation.clone());
    }
    if let Some(permissions) = &profile.permissions {
        resolved.permissions = Some(permissions.clone());
    }

    resolved.env.extend(profile.env.clone());
    Some(resolved)
}

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("No agent configured for session: {0}")]
    NoAgentConfigured(String),

    #[error("Concurrent request in progress for session: {0}")]
    ConcurrentRequest(String),

    #[error("Invalid model ID: {0}")]
    InvalidModelId(String),

    #[error("Permission not found: {0}")]
    PermissionNotFound(String),

    #[error("Session error: {0}")]
    Session(#[from] SessionError),

    #[error("Agent factory error: {0}")]
    Factory(#[from] AgentFactoryError),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Operation not supported: {0}")]
    NotSupported(String),

    #[error(transparent)]
    Chat(#[from] ChatError),
}

struct RequestState {
    cancel_tx: Option<oneshot::Sender<()>>,
    task_handle: Option<JoinHandle<()>>,
    #[allow(dead_code)] // stored for timing diagnostics; written on creation, readable for metrics
    started_at: Instant,
}

pub(crate) type BoxedAgentHandle = Box<dyn AgentHandle + Send + Sync>;

use mlua::RegistryKey;
use std::sync::Mutex as StdMutex;

pub(crate) struct SessionEventState {
    lua: Lua,
    registry: LuaScriptHandlerRegistry,
    permission_hooks: Arc<StdMutex<Vec<PermissionHook>>>,
    permission_functions: Arc<StdMutex<HashMap<String, RegistryKey>>>,
    pub(crate) reactor: Reactor,
    /// Counter for spill file naming, persists across messages in a session
    pub(crate) spill_counter: std::sync::atomic::AtomicU32,
}

/// Discover Lua handler files and register them with the Reactor.
/// Logs warnings on discovery/conversion failures, returns silently on empty dirs.
fn discover_and_register_lua_handlers(
    reactor: &mut Reactor,
    kiln_path: &std::path::Path,
    session_id: &str,
) {
    let paths = DiscoveryPaths::new("handlers", Some(kiln_path));
    let existing = paths.existing_paths();
    if existing.is_empty() {
        debug!(session_id = %session_id, "No handler directories found, skipping Lua handlers");
        return;
    }

    let handler_registry = match LuaScriptHandlerRegistry::discover(&existing) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to discover Lua handlers: {}", e);
            return;
        }
    };

    let handlers = match handler_registry.to_core_handlers() {
        Ok(h) => h,
        Err(e) => {
            warn!("Failed to create core handlers from Lua: {}", e);
            return;
        }
    };

    let mut loaded_count = 0;
    for handler in handlers {
        let name = handler.name().to_string();
        if let Err(e) = reactor.register(handler) {
            warn!("Failed to register Lua handler {}: {}", name, e);
        } else {
            loaded_count += 1;
            debug!("Loaded Lua handler: {}", name);
        }
    }
    if loaded_count > 0 {
        info!(session_id = %session_id, "Loaded {} Lua handlers", loaded_count);
    }
}

fn emit_precognition_event(
    event_tx: &broadcast::Sender<SessionEventMessage>,
    session_id: &str,
    query: &str,
    notes_count: usize,
    kilns_searched: usize,
    kilns_failed: usize,
    notes: Option<Vec<crucible_core::traits::chat::PrecognitionNoteInfo>>,
) {
    let query_summary = query.chars().take(100).collect::<String>();
    let event = SessionEvent::internal(InternalSessionEvent::PrecognitionComplete {
        notes_count,
        query_summary: query_summary.clone(),
        kilns_searched,
        kilns_filtered: 0,
        kilns_failed,
    });
    let mut data = serde_json::json!({
        "notes_count": notes_count,
        "query_summary": query_summary,
    });
    if let Some(notes) = notes {
        data["notes"] = serde_json::to_value(notes).unwrap_or_default();
    }
    if !emit_event(
        event_tx,
        SessionEventMessage::new(session_id, event.event_type(), data),
    ) {
        warn!(
            session_id = %session_id,
            "No subscribers for precognition_complete event"
        );
    }
}

struct PendingPermission {
    request: PermRequest,
    response_tx: oneshot::Sender<PermResponse>,
}

#[derive(Clone)]
struct StreamContext {
    session_id: String,
    message_id: String,
    event_tx: broadcast::Sender<SessionEventMessage>,
    session_state: Arc<Mutex<SessionEventState>>,
    pending_permissions: Arc<DashMap<String, HashMap<PermissionId, PendingPermission>>>,
    workspace_path: PathBuf,
    session_dir: PathBuf,
    agent_stream_config: AgentStreamConfig,
    tool_dispatcher: Arc<dyn ToolDispatcher>,
    /// Explicit CLI-level permission override (e.g. `--permissions allow`).
    /// When `Some(Allow)` or `Some(Deny)`, the streaming-path permission
    /// handler short-circuits without invoking Lua hooks or the default
    /// prompt. `Some(Ask)` and `None` fall through to the standard flow.
    permission_override: Option<PermissionMode>,
    /// Scheduler-owned conversation tree. Populated as a shadow of
    /// events; see `AgentManager::session_trees`.
    conversation_tree: Arc<tokio::sync::Mutex<crucible_core::turn::ConversationTree>>,
    /// Per-session prompt-cache aggregate updated on every
    /// `message_complete` that carries usage data.
    cache_stats: Arc<DashMap<String, cache_stats::CacheStats>>,
    /// Session manager handle, used by the auto-compact trigger to
    /// request compaction when prompt usage exceeds the configured
    /// threshold.
    session_manager: Arc<SessionManager>,
    /// Pre-computed kiln/Precognition system message to prepend to the
    /// context vector. Computed upstream of the stream loop (because
    /// kiln search needs &AgentManager) and injected by
    /// `apply_transform_context_handlers` before Lua transform_context
    /// handlers run, so Lua plugins can further mutate it. `None` when
    /// Precognition is gated off (disabled, /search command, no kiln,
    /// or not the first user message of the session).
    precognition_message: Option<crucible_core::traits::ContextMessage>,
}

#[allow(dead_code)] // fields capture config snapshot; model used in events, others reserved for stream configuration
#[derive(Clone)]
struct AgentStreamConfig {
    model: String,
    temperature: Option<f64>,
    max_tokens: Option<u32>,
    thinking_budget: Option<i64>,
    system_prompt: String,
    max_iterations: Option<u32>,
    execution_timeout_secs: Option<u64>,
    /// Snapshot of the session's `context_budget` for auto-compaction.
    /// `None` disables auto-compaction (no budget to compare against).
    context_budget: Option<usize>,
    /// Fraction of `context_budget` that triggers auto-compaction.
    /// `None` falls back to `DEFAULT_AUTOCOMPACT_THRESHOLD`. See
    /// [`crate::agent_manager::autocompact`].
    autocompact_threshold: Option<f32>,
    /// Validation mode for assistant text responses. Drives the
    /// validate-retry loop in `execute_agent_stream`.
    output_validation: OutputValidation,
    /// Maximum retry count when output validation fails.
    validation_retries: u32,
    /// Registry of Lua-defined validators, populated when the daemon
    /// has a plugin loader. The agent stream loop dispatches
    /// `OutputValidation::Lua { name }` against this registry.
    /// `None` outside daemon contexts (tests, isolated managers) — the
    /// stream loop treats that as a validation failure with a clear reason.
    lua_validators: Option<Arc<LuaValidatorRegistry>>,
    /// Plugin runtime `Lua` handle used to call into validator functions.
    /// Paired with `lua_validators`; both are `Some` together or both `None`.
    plugin_lua: Option<Arc<Lua>>,
}

impl AgentStreamConfig {
    fn from_session_agent(
        session_agent: &SessionAgent,
        lua_validators: Option<Arc<LuaValidatorRegistry>>,
        plugin_lua: Option<Arc<Lua>>,
    ) -> Self {
        Self {
            model: session_agent.model.clone(),
            temperature: session_agent.temperature,
            max_tokens: session_agent.max_tokens,
            thinking_budget: session_agent.thinking_budget,
            system_prompt: session_agent.system_prompt.clone(),
            max_iterations: session_agent.max_iterations,
            execution_timeout_secs: session_agent.execution_timeout_secs,
            context_budget: session_agent.context_budget,
            autocompact_threshold: session_agent.autocompact_threshold,
            output_validation: session_agent.output_validation.clone(),
            validation_retries: session_agent.validation_retries,
            lua_validators,
            plugin_lua,
        }
    }
}

#[derive(Clone)]
struct AgentCache {
    inner: Arc<DashMap<String, Arc<Mutex<BoxedAgentHandle>>>>,
}

impl AgentCache {
    fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    fn get(
        &self,
        key: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, String, Arc<Mutex<BoxedAgentHandle>>>> {
        self.inner.get(key)
    }

    fn insert(&self, key: String, value: Arc<Mutex<BoxedAgentHandle>>) {
        self.inner.insert(key, value);
    }

    fn remove(&self, key: &str) -> Option<(String, Arc<Mutex<BoxedAgentHandle>>)> {
        self.inner.remove(key)
    }

    #[cfg(test)]
    fn contains_key(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }
}

#[derive(Clone)]
struct SessionStateCache {
    inner: Arc<DashMap<String, Arc<tokio::sync::Mutex<SessionEventState>>>>,
}

impl SessionStateCache {
    fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    fn get(
        &self,
        key: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, String, Arc<tokio::sync::Mutex<SessionEventState>>>>
    {
        self.inner.get(key)
    }

    fn insert(&self, key: String, value: Arc<tokio::sync::Mutex<SessionEventState>>) {
        self.inner.insert(key, value);
    }

    fn remove(&self, key: &str) -> Option<(String, Arc<tokio::sync::Mutex<SessionEventState>>)> {
        self.inner.remove(key)
    }

    #[cfg(test)]
    fn contains_key(&self, key: &str) -> bool {
        self.inner.contains_key(key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedProvider {
    /// Backend/provider type
    pub provider_type: BackendType,
    /// API endpoint, if configured
    pub endpoint: Option<String>,
    /// API key, if configured
    pub api_key: Option<String>,
    /// Which config system this was resolved from, for logging
    pub source: &'static str,
}

pub struct AgentManager {
    request_state: Arc<DashMap<String, RequestState>>,
    // TODO: invalidate agent_cache entries on kiln hot-swap (multi-kiln support)
    agent_cache: AgentCache,
    // TODO: invalidate session_dispatchers on kiln hot-swap (multi-kiln support)
    session_dispatchers: Arc<DashMap<String, Arc<dyn ToolDispatcher>>>,
    /// Scheduler-owned conversation tree per session. Populated as a
    /// shadow of the agent's conversation state — the source of truth
    /// remains the agent's internal history today, but this tree is
    /// the target for future reads (workflow fan/collect, branching,
    /// O(1) undo). See `plans/2026-04-19-crucible-simplification.md`
    /// Phase 1 Step 3.
    session_trees:
        Arc<DashMap<String, Arc<tokio::sync::Mutex<crucible_core::turn::ConversationTree>>>>,
    pub(crate) model_cache: Arc<DashMap<String, (Vec<String>, Instant)>>,
    kiln_manager: Arc<KilnManager>,
    session_manager: Arc<SessionManager>,
    background_manager: Arc<BackgroundJobManager>,
    session_states: SessionStateCache,
    pending_permissions: Arc<DashMap<String, HashMap<PermissionId, PendingPermission>>>,
    mcp_gateway: Option<Arc<tokio::sync::RwLock<crate::tools::mcp_gateway::McpGatewayManager>>>,
    llm_config: Option<crucible_core::config::LlmConfig>,
    acp_config: Option<AcpConfig>,
    permission_config: Option<PermissionConfig>,
    plugin_loader: Option<Arc<Mutex<Option<DaemonPluginLoader>>>>,
    tool_dispatcher: Arc<dyn ToolDispatcher>,
    cache_stats: Arc<DashMap<String, cache_stats::CacheStats>>,
    /// Lua validator registry + plugin `Lua` handle. Populated once at
    /// daemon startup via [`AgentManager::set_lua_validators`] after the
    /// plugin loader has finished initializing. `OnceLock` keeps the
    /// hot validation path lock-free; tests and isolated managers leave
    /// it empty and `OutputValidation::Lua` surfaces as a validation
    /// failure with a clear reason instead of panicking.
    lua_validators: std::sync::OnceLock<(Arc<LuaValidatorRegistry>, Arc<Lua>)>,
    /// Per-session, per-turn workspace snapshots indexed by the
    /// conversation-tree node id that was `current` at the moment the
    /// snapshot was captured (i.e. the parent of the soon-to-be-added
    /// User node). On undo, the tree's new cursor position is looked up
    /// here and its snapshot is replayed to revert tool-side file edits.
    pub(crate) snapshots: Arc<crate::workspace_snapshot::SnapshotMap>,
}

/// Parameters for creating an AgentManager.
pub struct AgentManagerParams {
    pub kiln_manager: Arc<KilnManager>,
    pub session_manager: Arc<SessionManager>,
    pub background_manager: Arc<BackgroundJobManager>,
    pub mcp_gateway: Option<Arc<tokio::sync::RwLock<crate::tools::mcp_gateway::McpGatewayManager>>>,
    pub llm_config: Option<crucible_core::config::LlmConfig>,
    pub acp_config: Option<AcpConfig>,
    pub permission_config: Option<PermissionConfig>,
    pub plugin_loader: Option<Arc<Mutex<Option<DaemonPluginLoader>>>>,
    pub workspace_tools: Arc<WorkspaceTools>,
}

impl AgentManager {
    pub fn new(params: AgentManagerParams) -> Self {
        let tool_dispatcher: Arc<dyn ToolDispatcher> = Arc::new(DaemonToolDispatcher::new(vec![
            params.workspace_tools as Arc<dyn ToolExecutor>,
        ]));
        Self {
            request_state: Arc::new(DashMap::new()),
            agent_cache: AgentCache::new(),
            model_cache: Arc::new(DashMap::new()),
            kiln_manager: params.kiln_manager,
            session_manager: params.session_manager,
            background_manager: params.background_manager,
            session_states: SessionStateCache::new(),
            pending_permissions: Arc::new(DashMap::new()),
            session_dispatchers: Arc::new(DashMap::new()),
            mcp_gateway: params.mcp_gateway,
            llm_config: params.llm_config,
            acp_config: params.acp_config,
            permission_config: params.permission_config,
            plugin_loader: params.plugin_loader,
            tool_dispatcher,
            session_trees: Arc::new(DashMap::new()),
            cache_stats: Arc::new(DashMap::new()),
            lua_validators: std::sync::OnceLock::new(),
            snapshots: Arc::new(crate::workspace_snapshot::SnapshotMap::default()),
        }
    }

    /// Bind the plugin loader's validator registry + `Lua` handle.
    ///
    /// Called once during daemon startup after the plugin loader has
    /// initialized. Subsequent calls are silently ignored (`OnceLock`
    /// semantics) so reload paths can re-call without panicking; the
    /// registry itself is shared by `Arc` and stays live across reloads.
    pub fn set_lua_validators(&self, registry: Arc<LuaValidatorRegistry>, lua: Arc<Lua>) {
        let _ = self.lua_validators.set((registry, lua));
    }

    /// Snapshot of `(registry, lua)` for the agent stream loop. `None`
    /// when no plugin loader has bound validators (test contexts).
    pub(crate) fn lua_validators(&self) -> Option<(Arc<LuaValidatorRegistry>, Arc<Lua>)> {
        self.lua_validators
            .get()
            .map(|(r, l)| (Arc::clone(r), Arc::clone(l)))
    }

    /// Snapshot the prompt-cache aggregate for `session_id`. Returns
    /// `Default::default()` (all zeros) when no completion has reported
    /// cache fields yet — callers can interpret that as "no data" via
    /// `CacheStats::hit_rate()`, which returns `None`.
    pub fn get_cache_stats(&self, session_id: &str) -> cache_stats::CacheStats {
        self.cache_stats
            .get(session_id)
            .map(|s| s.clone())
            .unwrap_or_default()
    }

    pub(crate) fn cache_stats_handle(&self) -> Arc<DashMap<String, cache_stats::CacheStats>> {
        self.cache_stats.clone()
    }

    /// Look up or create the scheduler-owned `ConversationTree` for a
    /// session. When the entry is first inserted (e.g. after a daemon
    /// restart resuming a persisted session, or a freshly-attached
    /// AgentManager for an existing session), rebuild its contents from
    /// the session JSONL log if one exists. Without this, the
    /// first-user-message gate (Precognition, digest, etc.) sees an
    /// empty tree post-restart and treats the next turn as "first" —
    /// re-injecting on every restart.
    ///
    /// `jsonl_path` is the session's event log; when it doesn't exist
    /// (the common case for in-progress sessions), the tree starts
    /// empty.
    pub(crate) async fn get_or_rebuild_session_tree(
        &self,
        session_id: &str,
        jsonl_path: &std::path::Path,
    ) -> Arc<tokio::sync::Mutex<crucible_core::turn::ConversationTree>> {
        if let Some(existing) = self.session_trees.get(session_id) {
            return existing.clone();
        }

        let initial = if jsonl_path.exists() {
            match crate::observe::rebuild::rebuild_tree_from_jsonl(jsonl_path).await {
                Ok(tree) => tree,
                Err(error) => {
                    tracing::warn!(
                        session_id = %session_id,
                        path = %jsonl_path.display(),
                        error = %error,
                        "Failed to rebuild conversation tree from JSONL; starting fresh"
                    );
                    crucible_core::turn::ConversationTree::new()
                }
            }
        } else {
            crucible_core::turn::ConversationTree::new()
        };

        self.session_trees
            .entry(session_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(initial)))
            .clone()
    }

    /// Look up an existing session tree without rebuilding from JSONL.
    /// Returns `None` if the session has no in-memory tree yet.
    ///
    /// Production paths use `get_or_rebuild_session_tree` so a resumed
    /// session's history loads on first access. This sibling is for
    /// tests inspecting the in-memory state without touching disk.
    #[cfg(test)]
    pub(crate) fn get_session_tree(
        &self,
        session_id: &str,
    ) -> Option<Arc<tokio::sync::Mutex<crucible_core::turn::ConversationTree>>> {
        self.session_trees.get(session_id).map(|t| t.clone())
    }

    /// Access the background job manager for direct job queries.
    pub fn background_manager(&self) -> &Arc<BackgroundJobManager> {
        &self.background_manager
    }

    pub fn get_session_with_agent(
        &self,
        session_id: &str,
    ) -> Result<(crucible_core::session::Session, SessionAgent), AgentError> {
        let session = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        let agent = session
            .agent
            .clone()
            .ok_or_else(|| AgentError::NoAgentConfigured(session_id.to_string()))?;

        Ok((session, agent))
    }

    #[allow(dead_code)] // Future: concurrency guard for multi-client scenarios
    pub fn get_session_if_idle(
        &self,
        session_id: &str,
    ) -> Result<crucible_core::session::Session, AgentError> {
        if self.request_state.contains_key(session_id) {
            return Err(AgentError::ConcurrentRequest(session_id.to_string()));
        }
        self.session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))
    }

    pub fn invalidate_agent_cache(&self, session_id: &str) {
        self.agent_cache.remove(session_id);
    }

    pub fn invalidate_model_cache(&self) {
        self.model_cache.clear();
    }

    pub async fn get_or_create_session_dispatcher(
        &self,
        session: &crucible_core::session::Session,
    ) -> Arc<dyn ToolDispatcher> {
        let session_id = &session.id;

        // Return cached dispatcher if it exists
        if let Some(dispatcher) = self.session_dispatchers.get(session_id) {
            return dispatcher.clone();
        }

        // Build new dispatcher for this session
        let dispatcher = if !session.workspace.as_os_str().is_empty() {
            use crate::empty_providers::{EmptyEmbeddingProvider, EmptyKnowledgeRepository};
            use crate::tool_dispatch::McpToolExecutor;
            use crate::tools::mcp_server::CrucibleMcpServer;

            // Resolve real knowledge repo + embedding provider from the kiln
            // when available. Falls back to empty impls only when the kiln
            // cannot be opened or no enrichment is configured — in which case
            // semantic_search will honestly report its unavailable state.
            let (knowledge_repo, embedding_provider): (
                Arc<dyn crucible_core::traits::KnowledgeRepository>,
                Arc<dyn crucible_core::enrichment::EmbeddingProvider>,
            ) = {
                let kiln_path = session.kiln.as_path();
                let repo: Arc<dyn crucible_core::traits::KnowledgeRepository> =
                    match self.kiln_manager.get_or_open(kiln_path).await {
                        Ok(storage) => storage.as_knowledge_repository(),
                        Err(_) => Arc::new(EmptyKnowledgeRepository),
                    };
                let embed: Arc<dyn crucible_core::enrichment::EmbeddingProvider> =
                    if let Some(config) = self.kiln_manager.enrichment_config().cloned() {
                        match crate::embedding::get_or_create_embedding_provider(&config).await {
                            Ok(provider) => provider,
                            Err(e) => {
                                tracing::warn!(
                                    error = %e,
                                    "Failed to create embedding provider for session dispatcher; \
                                     semantic_search will report unavailable"
                                );
                                Arc::new(EmptyEmbeddingProvider)
                            }
                        }
                    } else {
                        Arc::new(EmptyEmbeddingProvider)
                    };
                (repo, embed)
            };

            let mcp = Arc::new(CrucibleMcpServer::new(
                session.kiln.to_string_lossy().to_string(),
                knowledge_repo,
                embedding_provider,
            ));

            Arc::new(DaemonToolDispatcher::new(vec![
                Arc::new(
                    WorkspaceTools::new(&session.workspace)
                        .with_env("CRU_SESSION", &session.id)
                        .with_env(
                            "CRU_SESSION_DIR",
                            session.storage_path().to_string_lossy().to_string(),
                        ),
                ) as Arc<dyn ToolExecutor>,
                Arc::new(McpToolExecutor::new(mcp)),
            ]))
        } else {
            self.tool_dispatcher.clone()
        };

        // Cache and return
        self.session_dispatchers
            .insert(session_id.clone(), dispatcher.clone());
        dispatcher
    }

    pub fn cleanup_session(&self, session_id: &str) {
        if self.session_states.remove(session_id).is_some() {
            debug!(session_id = %session_id, "Cleaned up Lua state for session");
        }

        self.agent_cache.remove(session_id);
        self.session_dispatchers.remove(session_id);
        // Drop the in-memory conversation tree so a re-attach to this
        // session rebuilds from on-disk JSONL rather than reusing stale
        // pointers (and frees memory for ended sessions).
        self.session_trees.remove(session_id);

        if let Some((_, mut state)) = self.request_state.remove(session_id) {
            if let Some(cancel_tx) = state.cancel_tx.take() {
                let _ = cancel_tx.send(());
            }
        }

        if self.pending_permissions.remove(session_id).is_some() {
            debug!(session_id = %session_id, "Cleaned up pending permissions for session");
        }
    }

    #[allow(dead_code)] // permission system API, exercised by tests
    pub fn await_permission(
        &self,
        session_id: &str,
        request: PermRequest,
    ) -> (PermissionId, oneshot::Receiver<PermResponse>) {
        let permission_id = format!("perm-{}", uuid::Uuid::new_v4());
        let (response_tx, response_rx) = oneshot::channel();

        let pending = PendingPermission {
            request,
            response_tx,
        };

        self.pending_permissions
            .entry(session_id.to_string())
            .or_default()
            .insert(permission_id.clone(), pending);

        debug!(
            session_id = %session_id,
            permission_id = %permission_id,
            "Created pending permission request"
        );

        (permission_id, response_rx)
    }

    pub fn respond_to_permission(
        &self,
        session_id: &str,
        permission_id: &str,
        response: PermResponse,
    ) -> Result<(), AgentError> {
        let mut session_permissions = self
            .pending_permissions
            .get_mut(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        let pending = session_permissions
            .remove(permission_id)
            .ok_or_else(|| AgentError::PermissionNotFound(permission_id.to_string()))?;

        let _ = pending.response_tx.send(response);

        debug!(
            session_id = %session_id,
            permission_id = %permission_id,
            "Responded to permission request"
        );

        Ok(())
    }

    #[allow(dead_code)] // permission system API, exercised by tests
    pub fn get_pending_permission(
        &self,
        session_id: &str,
        permission_id: &str,
    ) -> Option<PermRequest> {
        self.pending_permissions
            .get(session_id)
            .and_then(|perms| perms.get(permission_id).map(|p| p.request.clone()))
    }

    #[allow(dead_code)] // permission system API, exercised by tests
    pub fn list_pending_permissions(&self, session_id: &str) -> Vec<(PermissionId, PermRequest)> {
        self.pending_permissions
            .get(session_id)
            .map(|perms| {
                perms
                    .iter()
                    .map(|(id, p)| (id.clone(), p.request.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_or_create_session_state(&self, session_id: &str) -> Arc<Mutex<SessionEventState>> {
        if let Some(state) = self.session_states.get(session_id) {
            return state.clone();
        }

        let lua = Lua::new();
        let registry = LuaScriptHandlerRegistry::new();
        let permission_hooks = Arc::new(StdMutex::new(Vec::new()));
        let permission_functions = Arc::new(StdMutex::new(HashMap::new()));

        if let Err(e) = register_crucible_on_api(
            &lua,
            registry.runtime_handlers(),
            registry.handler_functions(),
        ) {
            error!(session_id = %session_id, error = %e, "Failed to register crucible.on API");
        }

        if let Err(e) = register_permission_hook_api(
            &lua,
            permission_hooks.clone(),
            permission_functions.clone(),
        ) {
            error!(session_id = %session_id, error = %e, "Failed to register crucible.permissions API");
        }

        if let Err(e) = lua.load(crucible_lua::BUILTIN_INIT_LUA).exec() {
            warn!(session_id = %session_id, error = %e, "Failed to load built-in init.lua (fail-open)");
        }

        let mut reactor = Reactor::new();
        if let Some(session) = self.session_manager.get_session(session_id) {
            let user_init = session.workspace.join(".crucible/lua/init.lua");
            if user_init.exists() {
                match std::fs::read_to_string(&user_init) {
                    Ok(source) => {
                        if let Err(e) = lua.load(&source).set_name("user init.lua").exec() {
                            warn!(
                                session_id = %session_id,
                                path = %user_init.display(),
                                error = %e,
                                "Failed to load user init.lua (fail-open)"
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            session_id = %session_id,
                            path = %user_init.display(),
                            error = %e,
                            "Failed to read user init.lua (fail-open)"
                        );
                    }
                }
            }

            discover_and_register_lua_handlers(&mut reactor, &session.kiln, session_id);
        }

        let state = Arc::new(Mutex::new(SessionEventState {
            lua,
            registry,
            permission_hooks,
            permission_functions,
            reactor,
            spill_counter: std::sync::atomic::AtomicU32::new(1),
        }));
        self.session_states
            .insert(session_id.to_string(), state.clone());
        state
    }

    fn get_session(&self, session_id: &str) -> Result<crucible_core::session::Session, AgentError> {
        self.session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))
    }

    pub fn build_available_agents(&self) -> HashMap<String, AgentProfile> {
        let mut available = default_agent_profiles();
        if let Some(config) = &self.acp_config {
            for name in config.agents.keys() {
                if let Some(resolved) = resolve_agent_profile(name, &config.agents, &available) {
                    available.insert(name.clone(), resolved);
                }
            }
        }
        available
    }

    pub async fn configure_agent(
        &self,
        session_id: &str,
        agent: SessionAgent,
    ) -> Result<(), AgentError> {
        let mut session = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        session.agent = Some(agent.clone());

        self.session_manager
            .update_session(&session)
            .await
            .map_err(AgentError::Session)?;

        info!(
            session_id = %session_id,
            model = %agent.model,
            provider = %agent.provider,
            "Agent configured for session"
        );

        Ok(())
    }
}

pub mod autocompact;
pub mod cache_stats;
pub mod context_length;
mod iter;
mod messaging;
mod models;
mod precognition;
pub mod providers;
pub mod tool_tracking;

#[cfg(test)]
mod tests;
