//! Agent lifecycle management for the daemon.

use crate::acp::discovery::default_agent_profiles;
use crate::agent_factory::{
    create_agent_from_session_config, AgentFactoryError, CreateAgentFromSessionConfigParams,
};
use crate::background_manager::BackgroundJobManager;
use crate::daemon_plugins::DaemonPluginLoader;
use crate::delegation::DelegationService;
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
            // Progressive-disclosure bridge lookups are read-only.
            | "discover_tools"
            | "get_tool_schema"
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

/// Terminal status of a `send_message` turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnStatus {
    Completed,
    Cancelled,
    TimedOut,
    Failed,
}

/// Terminal outcome of a turn, delivered through the completion channel of
/// [`AgentManager::send_message_notified`]. This is the reliable completion
/// signal: the event bus emits no terminal event on *successful* turns, so
/// callers that must await a turn (delegation) use this channel instead.
#[derive(Debug, Clone)]
pub struct TurnOutcome {
    pub status: TurnStatus,
    /// The accumulated assistant text (possibly partial on cancel/timeout).
    pub final_text: String,
    pub error: Option<String>,
}

/// Internal result of `execute_agent_stream`: distinguishes a turn that ran
/// to a normal end from one that bailed on an error path. Every early-return
/// site maps to `Failed` with the same reason string it emitted as an
/// `ended` event.
#[derive(Debug, Clone)]
pub(crate) enum StreamOutcome {
    Completed,
    Failed(String),
}

/// RAII claim on a session's `request_state` slot — the single mutual-exclusion
/// point shared by `send_message` and the scope mutations. Acquiring inserts a
/// marker `RequestState`; dropping removes it, so the slot is released on every
/// exit path including early returns, `?`, and panics.
///
/// `send_message` does *not* use this guard: its claim must outlive the call
/// (the spawned stream task releases the slot when the turn ends), so it manages
/// the entry by hand. The synchronous scope mutations, whose claim lasts exactly
/// one function body, use the guard instead. Both go through the same
/// `Entry::Occupied`/`Vacant` gate, so a send and a mutation exclude each other
/// in both directions.
struct RequestSlotGuard {
    request_state: Arc<DashMap<String, RequestState>>,
    session_id: String,
}

impl RequestSlotGuard {
    /// Atomically claim the slot, or return `ConcurrentRequest` if a send or
    /// another mutation already holds it.
    fn acquire(
        request_state: Arc<DashMap<String, RequestState>>,
        session_id: &str,
    ) -> Result<Self, AgentError> {
        use dashmap::mapref::entry::Entry;
        match request_state.entry(session_id.to_string()) {
            Entry::Occupied(_) => {
                return Err(AgentError::ConcurrentRequest(session_id.to_string()))
            }
            Entry::Vacant(e) => {
                e.insert(RequestState {
                    cancel_tx: None,
                    task_handle: None,
                    started_at: Instant::now(),
                });
            }
        }
        Ok(Self {
            request_state,
            session_id: session_id.to_string(),
        })
    }
}

impl Drop for RequestSlotGuard {
    fn drop(&mut self) {
        // Remove only our own marker (cancel_tx/task_handle both None). If a
        // cancel raced the mutation and a send has since claimed the slot,
        // that entry carries Some(cancel_tx) — an unconditional remove would
        // release a slot a live turn owns.
        self.request_state.remove_if(&self.session_id, |_, state| {
            state.cancel_tx.is_none() && state.task_handle.is_none()
        });
    }
}

pub(crate) type BoxedAgentHandle = Box<dyn AgentHandle + Send + Sync>;

/// Test-support seam: replaces `create_agent_from_session_config` so tests
/// (including delegation tests, where the child session id doesn't exist
/// until spawn time) can inject scripted agents. Never set in production.
pub type AgentFactoryOverride = Box<
    dyn Fn(
            &SessionAgent,
            &std::path::Path,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<BoxedAgentHandle, String>> + Send>,
        > + Send
        + Sync,
>;

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
    /// The agent's mode ("auto"/"plan") captured at request start. Used to
    /// enforce plan-mode restrictions on the inner tool when an `invoke_tool`
    /// bridge call is unwrapped — the agent handle that owns the canonical
    /// mode is not reachable from the tool-dispatch path.
    session_mode: String,
    /// Whether a user can answer permission prompts for this turn. Delegated
    /// child sessions run non-interactive: a tool call that would prompt is
    /// denied immediately instead of hanging on a prompt nobody sees.
    is_interactive: bool,
    /// The daemon's `[permissions]` config compiled for this turn. Internal
    /// agents consult it before hooks/patterns/prompt: config deny is
    /// absolute, config allow short-circuits the gate. `None` = no config.
    permission_engine:
        Option<Arc<crucible_core::config::components::permissions::PermissionEngine>>,
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
    /// From the session's `delegation_config.timeout_secs`; sizes the
    /// tool-dispatch timeout for `delegate_session` (a blocking delegation
    /// legitimately outlives the standard 30 s tool timeout).
    delegation_timeout_secs: Option<u64>,
    /// Per-tool policy from the session's agent card: Deny blocks execution,
    /// Ask forces a prompt (even for safe tools), Allow skips the gate.
    tool_policy: Option<crucible_core::agent::ToolPolicyMap>,
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
            delegation_timeout_secs: session_agent
                .delegation_config
                .as_ref()
                .map(|c| c.timeout_secs),
            tool_policy: session_agent.tool_policy.clone(),
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
    agent_cache: AgentCache,
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
    /// Spawns delegated child sessions through this manager's scheduler loop.
    /// The service holds a `Weak` back-reference (bound at startup), so this
    /// strong Arc creates no cycle.
    delegation_service: Arc<DelegationService>,
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
    /// Sessions with a title generation currently in flight — the RPC path
    /// and the message_complete auto-trigger can race.
    titles_in_flight: Arc<DashMap<String, ()>>,
    /// Per-session, per-turn workspace snapshots indexed by the
    /// conversation-tree node id that was `current` at the moment the
    /// snapshot was captured (i.e. the parent of the soon-to-be-added
    /// User node). On undo, the tree's new cursor position is looked up
    /// here and its snapshot is replayed to revert tool-side file edits.
    pub(crate) snapshots: Arc<crate::workspace_snapshot::SnapshotMap>,
    /// Test-support: when set, agent handles are built through this instead
    /// of the real factory. See [`AgentFactoryOverride`].
    agent_factory_override: std::sync::OnceLock<Arc<AgentFactoryOverride>>,
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
    /// Construct with a default, unbound `DelegationService`. Delegation
    /// spawns through it fail with a clear "not bound" error — appropriate
    /// for contexts that never delegate (most tests). Production and
    /// delegation tests use [`AgentManager::new_with_delegation`] and bind.
    pub fn new(params: AgentManagerParams) -> Self {
        let delegation_service =
            DelegationService::new(params.session_manager.clone(), broadcast::channel(16).0);
        Self::new_with_delegation(params, delegation_service)
    }

    /// Construct with an explicit delegation service. The caller must call
    /// `delegation_service.bind_agent_manager(&arc_manager)` after Arc-ing
    /// the returned manager, or delegation spawns will fail.
    pub fn new_with_delegation(
        params: AgentManagerParams,
        delegation_service: Arc<DelegationService>,
    ) -> Self {
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
            delegation_service,
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
            titles_in_flight: Arc::new(DashMap::new()),
            snapshots: Arc::new(crate::workspace_snapshot::SnapshotMap::default()),
            agent_factory_override: std::sync::OnceLock::new(),
        }
    }

    /// Test-support: install an agent-factory override (first call wins).
    /// Production never calls this; it exists so tests can script the agents
    /// that `send_message` (and thus delegation) builds.
    pub fn set_agent_factory_override(&self, factory: AgentFactoryOverride) {
        let _ = self.agent_factory_override.set(Arc::new(factory));
    }

    pub(crate) fn agent_factory_override(&self) -> Option<Arc<AgentFactoryOverride>> {
        self.agent_factory_override.get().cloned()
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

            // Connected kilns join semantic_search's scope through the SAME
            // source builder precognition uses (one open loop, one
            // model-mismatch filter policy). Falls back to primary-only when
            // there's nothing to fan out to or no enrichment config.
            let mut search_sources: Vec<crate::multi_kiln_search::KilnSearchSource> = Vec::new();
            if !session.connected_kilns.is_empty() {
                if let (Ok(handle), Some(config)) = (
                    self.kiln_manager.get_or_open(session.kiln.as_path()).await,
                    self.kiln_manager.enrichment_config().cloned(),
                ) {
                    search_sources = self
                        .collect_kiln_search_sources(&session.id, session, &handle, &config)
                        .await;
                }
            }

            // Pass the real workspace (not the kiln) so `skill_view` discovers
            // workspace-scoped skills under the same root the prompt catalog used.
            //
            // The delegation context here MUST match the one used when the
            // agent's tool definitions were built (agent_factory) — the old
            // split (defs built with a context, execution built with `None`)
            // advertised a `delegate_session` tool the executor then refused.
            // Both sites now call the same builder with the same inputs.
            let delegation_context = session.agent.as_ref().and_then(|agent_config| {
                crate::agent_factory::build_internal_delegation_context(
                    agent_config,
                    Some(&session.id),
                    Some(self.background_manager.clone()),
                    Some(self.delegation_service.clone()),
                    &session.workspace,
                    Some(session.kiln.as_path()),
                )
            });
            let mcp = Arc::new(
                CrucibleMcpServer::new_with_workspace_and_delegation(
                    session.kiln.to_string_lossy().to_string(),
                    session.workspace.clone(),
                    knowledge_repo,
                    embedding_provider,
                    delegation_context,
                )
                .with_search_sources(search_sources),
            );

            // Security posture for the session's workspace tools: file
            // operations are contained to the workspace + kilns + session
            // dir (spill reads), and the project's `[security.shell]`
            // policy applies to bash. Both are resolved ONCE here, never
            // re-read at call time.
            let shell_policy = crucible_core::config::read_project_config(&session.workspace)
                .map(|c| c.security.shell);
            let mut allowed_roots = vec![session.kiln.clone()];
            allowed_roots.extend(session.connected_kilns.iter().cloned());
            allowed_roots.push(session.storage_path());
            let mut providers: Vec<Arc<dyn ToolExecutor>> = vec![
                Arc::new(
                    WorkspaceTools::new(&session.workspace)
                        .with_env("CRU_SESSION", &session.id)
                        .with_env(
                            "CRU_SESSION_DIR",
                            session.storage_path().to_string_lossy().to_string(),
                        )
                        .with_allowed_roots(allowed_roots)
                        .with_shell_policy(shell_policy),
                ) as Arc<dyn ToolExecutor>,
                Arc::new(McpToolExecutor::new(mcp)),
            ];

            // Register the agent's configured gateway (user MCP) servers as a
            // provider so those tools are dispatchable directly and reachable
            // via the progressive-disclosure bridge when deferred.
            if let Some(gateway) = &self.mcp_gateway {
                let allowed = session
                    .agent
                    .as_ref()
                    .map(|a| a.mcp_servers.clone())
                    .unwrap_or_default();
                if !allowed.is_empty() {
                    providers.push(Arc::new(crate::tool_dispatch::GatewayToolExecutor::new(
                        gateway.clone(),
                        allowed,
                    )) as Arc<dyn ToolExecutor>);
                }
            }

            Arc::new(DaemonToolDispatcher::new(providers))
        } else {
            self.tool_dispatcher.clone()
        };

        // Cache and return
        self.session_dispatchers
            .insert(session_id.clone(), dispatcher.clone());
        dispatcher
    }

    /// Access the delegation service (child-session spawning).
    pub fn delegation_service(&self) -> &Arc<DelegationService> {
        &self.delegation_service
    }

    /// The `[llm.models]` specialty → model table for agent-card resolution.
    pub(crate) fn specialty_models(&self) -> Option<&HashMap<String, String>> {
        self.llm_config.as_ref().map(|c| &c.models)
    }

    /// Wait for a mixed set of job ids — delegations (child session ids) and
    /// background bash jobs — to reach terminal state. One JSON object per
    /// id with `id`, `status`, and (when finished) `output`/`error`/
    /// `exit_code`; still-running ids get `"timeout"`, unknown `"not_found"`.
    ///
    /// This is the fan-in primitive behind `cru.sessions.collect_subagents`
    /// and the `jobs.collect` RPC; it must span BOTH registries because
    /// delegations no longer live in the background-job manager.
    pub async fn collect_jobs(
        &self,
        job_ids: &[String],
        timeout: Duration,
    ) -> Vec<serde_json::Value> {
        use crate::delegation::DelegationSpawner as _;
        let deadline = tokio::time::Instant::now() + timeout;
        let mut results: Vec<Option<serde_json::Value>> = vec![None; job_ids.len()];

        loop {
            let mut all_done = true;
            for (i, job_id) in job_ids.iter().enumerate() {
                if results[i].is_some() {
                    continue;
                }
                let result = self
                    .delegation_service
                    .get_delegation_result(job_id)
                    .or_else(|| self.background_manager.get_job_result(job_id));
                match result {
                    Some(jr) if jr.info.status.is_terminal() => {
                        results[i] = Some(serde_json::json!({
                            "id": job_id,
                            "status": jr.info.status.to_string(),
                            "output": jr.output,
                            "error": jr.error,
                            "exit_code": jr.exit_code,
                        }));
                    }
                    Some(_) => {
                        all_done = false;
                    }
                    None => {
                        results[i] = Some(serde_json::json!({
                            "id": job_id,
                            "status": "not_found",
                        }));
                    }
                }
            }
            if all_done || tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        results
            .into_iter()
            .enumerate()
            .map(|(i, r)| {
                r.unwrap_or_else(|| {
                    serde_json::json!({
                        "id": job_ids[i].clone(),
                        "status": "timeout",
                    })
                })
            })
            .collect()
    }

    /// Effective trust level of an agent config's provider, from the LLM
    /// config's per-provider trust settings (Cloud fallback). Used to gate
    /// delegation against the kiln's data classification.
    pub(crate) fn resolve_agent_trust(
        &self,
        agent: &SessionAgent,
    ) -> crucible_core::config::TrustLevel {
        resolve_provider_trust(agent, self.llm_config.as_ref())
    }

    pub fn cleanup_session(&self, session_id: &str) {
        if self.session_states.remove(session_id).is_some() {
            debug!(session_id = %session_id, "Cleaned up Lua state for session");
        }

        // Cascade: a parent going away must not leave running children.
        // Spawned (cleanup_session is sync); cancellation resolves each
        // child's completion channel, then the records are dropped.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let service = self.delegation_service.clone();
            let session_id = session_id.to_string();
            handle.spawn(async move {
                let cancelled = service.cancel_children_of(&session_id).await;
                if cancelled > 0 {
                    info!(
                        session_id = %session_id,
                        cancelled,
                        "Cancelled delegated children of cleaned-up session"
                    );
                }
                service.forget_parent(&session_id);
            });
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

        // Free the per-session workspace-snapshot journal (up to a few MiB per
        // turn) and the cache-stats entry. Both grow per turn and, before this,
        // were never released — snapshots leaked for the daemon's whole lifetime.
        self.snapshots.clear_session(session_id);
        self.cache_stats.remove(session_id);
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

    /// All pending permission prompts across every session. The web Inbox
    /// needs the aggregate view: a session waiting on a permission must
    /// surface even when no browser tab is subscribed to its event stream.
    pub fn list_all_pending_permissions(&self) -> Vec<(String, PermissionId, PermRequest)> {
        self.pending_permissions
            .iter()
            .flat_map(|entry| {
                let session_id = entry.key().clone();
                entry
                    .value()
                    .iter()
                    .map(|(id, p)| (session_id.clone(), id.clone(), p.request.clone()))
                    .collect::<Vec<_>>()
            })
            .collect()
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
pub(crate) mod precognition;
pub mod providers;
mod scope;
pub(crate) mod title;
pub mod tool_tracking;

#[cfg(test)]
mod tests;
