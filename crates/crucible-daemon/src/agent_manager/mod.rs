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
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Unique identifier for a pending permission request.
pub type PermissionId = String;

pub(crate) mod tool_safety;
pub use tool_safety::{believed_read_only, is_safe};

pub(crate) const MODEL_CACHE_TTL: Duration = Duration::from_secs(300);

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

pub(crate) struct PendingPermission {
    request: PermRequest,
    response_tx: oneshot::Sender<PermResponse>,
}

#[derive(Clone)]
struct StreamContext {
    session_id: String,
    message_id: String,
    event_tx: broadcast::Sender<SessionEventMessage>,
    session_state: Arc<Mutex<SessionEventState>>,
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
    /// This session's slot. Carried whole rather than as one handle per store
    /// it touches: the stream already knows its session, and the alternative
    /// was a StreamContext field per map.
    slot: Arc<slot::SessionSlot>,
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
    /// System block carrying the contents of the `@file` mentions in this
    /// turn's user message, prepended alongside the Precognition block.
    /// `None` when the message mentioned no resolvable file.
    attachment_message: Option<crucible_core::traits::ContextMessage>,
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
    /// Mid-turn knowledge attachments queued by Lua handlers. Drained after
    /// each tool result and forwarded to the agent as `ContextAttach`, so the
    /// next LLM call in this turn sees what was retrieved.
    context_attach: Arc<crucible_lua::ContextAttachRegistry>,
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
    /// Per-session state, one slot per live session. See [`slot::SessionSlot`]
    /// for why this is a `DashMap` of `Arc`s with the locks inside the slot
    /// rather than one lock over the map.
    slots: Arc<DashMap<String, Arc<slot::SessionSlot>>>,
    /// Config `runtimepath`, for resolving `runtime/defaults/init.lua`.
    /// Empty is the correct default, not a missing value: resolution falls
    /// through to `CRUCIBLE_RUNTIME`, then exe-relative, then the compiled-in
    /// copy. See [`crate::runtime_defaults`].
    runtimepath: Vec<PathBuf>,
    /// Global session defaults set from Lua (`cru.defaults.x = …`) — the
    /// values a NEW session starts from, applied in `configure_agent` to any
    /// field the agent card left unset. The per-session override is
    /// `session.x`; see `crucible_lua::session_defaults` for why this tier is
    /// not called `cru.o`.
    session_defaults: crucible_lua::SessionDefaults,
    /// Modes declared from Lua (`cru.modes.<name> = {…}`). Empty until a
    /// session VM has run its files; every read falls back to
    /// `default_internal_modes()` in that case, so a daemon whose Lua failed
    /// to load still has working modes rather than none.
    modes: crucible_lua::ModeRegistry,
    pub(crate) model_cache: Arc<DashMap<String, (Vec<String>, Instant)>>,
    kiln_manager: Arc<KilnManager>,
    session_manager: Arc<SessionManager>,
    background_manager: Arc<BackgroundJobManager>,
    /// Spawns delegated child sessions through this manager's scheduler loop.
    /// The service holds a `Weak` back-reference (bound at startup), so this
    /// strong Arc creates no cycle.
    delegation_service: Arc<DelegationService>,
    mcp_gateway: Option<Arc<tokio::sync::RwLock<crate::tools::mcp_gateway::McpGatewayManager>>>,
    llm_config: Option<crucible_core::config::LlmConfig>,
    acp_config: Option<AcpConfig>,
    /// `[context]` from the daemon config — which project rules files get
    /// loaded into an agent's system prompt. `None` uses the defaults.
    context_config: Option<crucible_core::config::ContextConfig>,
    permission_config: Option<PermissionConfig>,
    plugin_loader: Option<Arc<Mutex<Option<DaemonPluginLoader>>>>,
    tool_dispatcher: Arc<dyn ToolDispatcher>,
    /// Lua validator registry + plugin `Lua` handle. Populated once at
    /// daemon startup via [`AgentManager::set_lua_validators`] after the
    /// plugin loader has finished initializing. `OnceLock` keeps the
    /// hot validation path lock-free; tests and isolated managers leave
    /// it empty and `OutputValidation::Lua` surfaces as a validation
    /// failure with a clear reason instead of panicking.
    lua_validators: std::sync::OnceLock<(Arc<LuaValidatorRegistry>, Arc<Lua>)>,
    /// Plugin `crucible.on` handler registry + the plugin `Lua` handle.
    /// Bound at daemon startup alongside `lua_validators`. Empty in tests and
    /// isolated managers, where plugin hooks simply don't fire.
    plugin_handlers: std::sync::OnceLock<(Arc<LuaScriptHandlerRegistry>, Arc<Lua>)>,
    /// Plugin isolation claims, bound at daemon startup alongside the handlers.
    isolation: std::sync::OnceLock<crucible_lua::IsolationRegistry>,

    /// Mid-turn knowledge attachments. Written by Lua, drained by the stream
    /// loop.
    ///
    /// Created eagerly rather than bound later: it is a per-session buffer with
    /// no dependency on the plugin system, and late binding meant a session VM
    /// built before the bind silently got a nil `cru.context.attach` — for that
    /// session, permanently, because VMs are cached.
    context_attach: std::sync::Arc<crucible_lua::ContextAttachRegistry>,
    /// Statusline expression values, created eagerly for the same reason as
    /// `context_attach` — see `statusline_exprs()`.
    statusline_exprs: std::sync::Arc<crucible_lua::StatuslineExprRegistry>,
    /// Plugin tool/command registry, bound at daemon startup like the others.
    ///
    /// The loader mutex must not be the read path: `fire_session_start` holds
    /// it across plugin hook execution — which since the oci repair includes
    /// container builds — so a read that locks the loader stalls behind any
    /// session's slow start. The registry updates in place on reload, so a
    /// cached `Arc` never goes stale.
    plugin_tool_registry: std::sync::OnceLock<Arc<crate::plugin_tools::PluginRegistry>>,
    /// Sessions with a title generation currently in flight — the RPC path
    /// and the message_complete auto-trigger can race.
    titles_in_flight: Arc<DashMap<String, ()>>,
    /// Per-session, per-turn workspace snapshots indexed by the
    /// conversation-tree node id that was `current` at the moment the
    /// snapshot was captured (i.e. the parent of the soon-to-be-added
    /// User node). On undo, the tree's new cursor position is looked up
    /// here and its snapshot is replayed to revert tool-side file edits.
    pub(crate) snapshots: Arc<crate::workspace_snapshot::SnapshotMap>,
    /// Per-session review ledgers: the per-tool-call attribution beside the
    /// per-turn snapshots above. The two are independent — snapshots exist to
    /// undo a whole turn, ledgers to say which call produced which hunk — so
    /// undo consumes a snapshot without touching the ledger. It does not need
    /// to: the composed diff is derived from the worktree, so a restored turn
    /// simply stops producing hunks for its intervals to attribute.
    pub(crate) review: Arc<crate::review::ReviewLedgers>,
    /// Worktree watch behind the review ledger's external-change backstop.
    ///
    /// A `OnceLock` bound at daemon startup, like the plugin registries above
    /// and for the same reason: starting a watch registers inotify handles and
    /// is async, so it cannot happen in `new`. Absent in every test manager
    /// and in any embedding that never starts a server — attribution is
    /// unaffected, only the push notification is.
    external_watch: std::sync::OnceLock<Arc<crate::watch::external_changes::ExternalChangeWatch>>,
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
    pub context_config: Option<crucible_core::config::ContextConfig>,
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
            slots: Arc::new(DashMap::new()),
            runtimepath: Vec::new(),
            session_defaults: crucible_lua::SessionDefaults::new(),
            modes: crucible_lua::ModeRegistry::new(),
            model_cache: Arc::new(DashMap::new()),
            kiln_manager: params.kiln_manager,
            session_manager: params.session_manager,
            background_manager: params.background_manager,
            delegation_service,
            mcp_gateway: params.mcp_gateway,
            llm_config: params.llm_config,
            acp_config: params.acp_config,
            context_config: params.context_config,
            permission_config: params.permission_config,
            plugin_loader: params.plugin_loader,
            tool_dispatcher,
            lua_validators: std::sync::OnceLock::new(),
            plugin_handlers: std::sync::OnceLock::new(),
            isolation: std::sync::OnceLock::new(),
            context_attach: std::sync::Arc::new(crucible_lua::ContextAttachRegistry::default()),
            statusline_exprs: std::sync::Arc::new(crucible_lua::StatuslineExprRegistry::new()),
            plugin_tool_registry: std::sync::OnceLock::new(),
            titles_in_flight: Arc::new(DashMap::new()),
            snapshots: Arc::new(crate::workspace_snapshot::SnapshotMap::default()),
            review: Arc::new(crate::review::ReviewLedgers::default()),
            external_watch: std::sync::OnceLock::new(),
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

    /// Bind the plugin loader's `crucible.on` registry + `Lua` handle, so
    /// hooks registered by plugins reach the stream loop. Without this,
    /// plugins can register handlers that never fire. Idempotent.
    pub fn set_plugin_handlers(&self, registry: Arc<LuaScriptHandlerRegistry>, lua: Arc<Lua>) {
        let _ = self.plugin_handlers.set((registry, lua));
    }

    /// Snapshot of the plugin hook registry for the stream loop. `None` when
    /// no plugin loader has bound one.
    pub(crate) fn plugin_handlers(&self) -> Option<(Arc<LuaScriptHandlerRegistry>, Arc<Lua>)> {
        self.plugin_handlers
            .get()
            .map(|(r, l)| (Arc::clone(r), Arc::clone(l)))
    }

    /// Bind the worktree watch behind the review backstop, and hand its
    /// tracker to the ledgers so their own writes stop being reported as the
    /// user's. Idempotent, like the other startup bindings.
    pub fn set_external_watch(
        &self,
        watch: Arc<crate::watch::external_changes::ExternalChangeWatch>,
    ) {
        self.review
            .set_external_tracker(Arc::clone(watch.tracker()));
        let _ = self.external_watch.set(watch);
    }

    /// The worktree watch, or `None` when the daemon started none.
    pub(crate) fn external_watch(
        &self,
    ) -> Option<&Arc<crate::watch::external_changes::ExternalChangeWatch>> {
        self.external_watch.get()
    }

    /// Bind the plugin isolation registry. Idempotent, like the others.
    pub fn set_isolation(&self, registry: crucible_lua::IsolationRegistry) {
        let _ = self.isolation.set(registry);
    }

    /// The mid-turn attachment registry. Always present — see the field.
    pub fn context_attach(&self) -> std::sync::Arc<crucible_lua::ContextAttachRegistry> {
        self.context_attach.clone()
    }

    /// Statusline expression values. Created eagerly for the same reason as
    /// `context_attach`: session VMs are lazy and cached, so a registry bound
    /// later leaves earlier VMs holding a nil function forever.
    pub fn statusline_exprs(&self) -> std::sync::Arc<crucible_lua::StatuslineExprRegistry> {
        self.statusline_exprs.clone()
    }

    /// The global session defaults store (`cru.defaults`).
    #[cfg(test)]
    pub(crate) fn session_defaults(&self) -> &crucible_lua::SessionDefaults {
        &self.session_defaults
    }

    /// The operator's `[permissions]` rules, for gates outside the agent
    /// dispatch path (`cru.tools.call`, via `DaemonToolsBridge`).
    pub(crate) fn permission_config(&self) -> Option<PermissionConfig> {
        self.permission_config.clone()
    }

    pub(crate) fn isolation(&self) -> Option<crucible_lua::IsolationRegistry> {
        self.isolation.get().cloned()
    }

    /// Bind the plugin tool/command registry. Idempotent, like the others.
    pub fn set_plugin_tool_registry(&self, registry: Arc<crate::plugin_tools::PluginRegistry>) {
        let _ = self.plugin_tool_registry.set(registry);
    }

    /// Snapshot the prompt-cache aggregate for `session_id`. Returns
    /// `Default::default()` (all zeros) when no completion has reported
    /// cache fields yet — callers can interpret that as "no data" via
    /// `CacheStats::hit_rate()`, which returns `None`.
    pub fn get_cache_stats(&self, session_id: &str) -> cache_stats::CacheStats {
        self.slots
            .get(session_id)
            .map(|slot| slot.cache_stats())
            .unwrap_or_default()
    }

    /// Test-support: seed the session's cached agent handle, as if a previous
    /// turn had built it.
    ///
    /// A named seam rather than 30 tests reaching into a field: the field it
    /// used to write moved into [`slot::SessionSlot`], and one helper means the
    /// next move is one edit rather than 30.
    #[cfg(test)]
    pub(crate) fn install_agent_for_test(
        &self,
        session_id: String,
        agent: Arc<Mutex<BoxedAgentHandle>>,
    ) {
        self.slot(&session_id)
            .seed_build_for_test(Some(&agent), None);
    }

    /// Test-support: whether the session has a cached agent handle.
    #[cfg(test)]
    pub(crate) fn has_cached_agent(&self, session_id: &str) -> bool {
        self.slots
            .get(session_id)
            .is_some_and(|slot| slot.has_agent())
    }

    /// The session's slot, created on first use.
    ///
    /// One DashMap shard lock, held only long enough to clone the `Arc` —
    /// never across an `await`. Reading a slot is therefore not a place a
    /// session can queue behind another session.
    pub(crate) fn slot(&self, session_id: &str) -> Arc<slot::SessionSlot> {
        self.slots
            .entry(session_id.to_string())
            .or_default()
            .clone()
    }

    /// The session's slot only if it already has one.
    ///
    /// For invalidation: no slot means nothing is cached and no build is in
    /// flight (a build creates the slot before it starts), so there is nothing
    /// to invalidate — and creating one to record that would leak a slot per id
    /// nobody ever ran a turn for.
    fn existing_slot(&self, session_id: &str) -> Option<Arc<slot::SessionSlot>> {
        self.slots.get(session_id).map(|slot| Arc::clone(&slot))
    }

    /// Tool names per upstream, as the live MCP gateway currently sees them.
    ///
    /// Empty when no gateway was configured. Read-only and projected to
    /// `name -> tools` on purpose rather than handing out `&self.mcp_gateway`:
    /// the caller is `session.create`'s setup task filling in the
    /// `mcp_servers_ready` event, and it has no business holding the gateway.
    ///
    /// It exists because the daemon had this data and did not publish it, so the
    /// TUI opened its OWN connections to learn the same thing — one stdio child
    /// process per configured upstream, plus an `initialize` round-trip with
    /// each, on every `cru chat` launch, discarded immediately after counting.
    pub async fn mcp_tools_by_upstream(&self) -> HashMap<String, Vec<String>> {
        let Some(gateway) = &self.mcp_gateway else {
            return HashMap::new();
        };
        let gateway = gateway.read().await;
        let mut out: HashMap<String, Vec<String>> = HashMap::new();
        for tool in gateway.all_tools() {
            out.entry(tool.upstream).or_default().push(tool.name);
        }
        out
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
        let slot = self.slot(session_id);
        // One rebuild, not one per concurrent first caller: `get_or_init`
        // serialises them, where check-then-insert let two callers each parse
        // the JSONL and one of the resulting trees be dropped.
        slot.tree
            .get_or_init(|| self.rebuild_session_tree(session_id, jsonl_path))
            .await
            .clone()
    }

    async fn rebuild_session_tree(
        &self,
        session_id: &str,
        jsonl_path: &std::path::Path,
    ) -> Arc<tokio::sync::Mutex<crucible_core::turn::ConversationTree>> {
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

        Arc::new(tokio::sync::Mutex::new(initial))
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
        self.existing_slot(session_id)
            .and_then(|slot| slot.tree.get().cloned())
    }

    /// Access the background job manager for direct job queries.
    pub fn background_manager(&self) -> &Arc<BackgroundJobManager> {
        &self.background_manager
    }

    /// Access the session manager. Used by tests that need to drive session
    /// lifecycle (end/resume) directly against the same manager the agent
    /// manager reads from.
    #[cfg(test)]
    pub(crate) fn session_manager(&self) -> &Arc<SessionManager> {
        &self.session_manager
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
        if let Some(slot) = self.existing_slot(session_id) {
            slot.invalidate_agent();
        }
    }

    /// Set the config `runtimepath` used to resolve `runtime/defaults/init.lua`.
    ///
    /// A setter rather than a `AgentManagerParams` field: every test
    /// constructing a manager wants the fall-through behaviour (empty →
    /// exe-relative → built-in), and only the daemon has a configured path to
    /// pass. Call before the first session VM is created; later calls do not
    /// re-run defaults for VMs that already exist.
    #[must_use]
    pub fn with_runtimepath(mut self, runtimepath: Vec<PathBuf>) -> Self {
        self.runtimepath = runtimepath;
        self
    }

    /// Modes available to `session_id`, as the ACP-facing state.
    ///
    /// Built from the Lua registry; falls back to the shipped Rust definitions
    /// when nothing declared any — a broken defaults file must not leave a
    /// session with zero modes, which would make `set_mode` reject everything.
    pub fn session_modes(
        &self,
        session_id: &str,
    ) -> crucible_core::types::acp::schema::SessionModeState {
        use crucible_core::types::acp::schema::{SessionMode, SessionModeId, SessionModeState};
        use crucible_core::types::mode::default_internal_modes;
        let _vm = self.get_or_create_session_state(session_id);
        // The SESSION's mode, not registration order. `current_mode_id` was
        // previously `declared.first()`, which nothing noticed because both
        // callers read only `available_modes` — but the moment this struct
        // goes on the wire, a UI hydrating `current_mode_id` would show
        // whichever mode the defaults file happens to declare first. A
        // persisted mode that is no longer declared is ignored: reporting it
        // would advertise a mode `set_mode` now rejects.
        let persisted = self
            .session_manager
            .get_session(session_id)
            .and_then(|s| s.agent.and_then(|a| a.mode));

        let declared = self.modes.all();
        if declared.is_empty() {
            let fallback = default_internal_modes();
            // Same rule for the fallback set: the built-ins hardcode "normal"
            // as current, which would silently disagree with `get_mode`.
            let current = persisted
                .filter(|m| {
                    fallback
                        .available_modes
                        .iter()
                        .any(|d| d.id.0.as_ref() == m)
                })
                .unwrap_or_else(|| fallback.current_mode_id.0.to_string());
            return SessionModeState::new(
                SessionModeId::new(current.as_str()),
                fallback.available_modes,
            );
        }
        let current = persisted
            .filter(|m| declared.iter().any(|d| &d.name == m))
            .or_else(|| declared.first().map(|m| m.name.clone()))
            .unwrap_or_else(|| "normal".to_string());
        let available = declared
            .into_iter()
            .map(|m| {
                let title = {
                    let mut c = m.name.chars();
                    match c.next() {
                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                        None => m.name.clone(),
                    }
                };
                let mode = SessionMode::new(SessionModeId::new(m.name.as_str()), title);
                match m.description {
                    Some(d) => mode.description(d),
                    None => mode,
                }
            })
            .collect();
        SessionModeState::new(SessionModeId::new(current.as_str()), available)
    }

    /// The permission stance a mode declares, if any.
    ///
    /// Test-only accessor. The gate deliberately reads the stance from the
    /// TURN's snapshotted registry (`AgentStreamConfig::modes`), not from the
    /// live one, so a mid-turn redefinition cannot reshape a turn already in
    /// progress — routing it through here would defeat that.
    #[cfg(test)]
    pub(crate) fn mode_stance(&self, mode_id: &str) -> Option<crucible_lua::ModeStance> {
        self.modes.get(mode_id).map(|m| m.permissions.default)
    }

    pub fn invalidate_model_cache(&self) {
        self.model_cache.clear();
    }

    pub async fn get_or_create_session_dispatcher(
        &self,
        session: &crucible_core::session::Session,
    ) -> Arc<dyn ToolDispatcher> {
        let session_id = &session.id;

        // Return cached dispatcher if it exists, else note the generation this
        // build must install against — see `SessionSlot::install_dispatcher`.
        let slot = self.slot(session_id);
        let generation = match slot.dispatcher_or_generation() {
            Ok(dispatcher) => return dispatcher,
            Err(generation) => generation,
        };

        // Build new dispatcher for this session
        let dispatcher = if !session.workspace.as_os_str().is_empty() {
            use crate::empty_providers::{EmptyEmbeddingProvider, EmptyKnowledgeRepository};
            use crate::tool_dispatch::McpToolExecutor;
            use crate::tools::mcp_server::CrucibleMcpServer;

            // Resolve real knowledge repo + embedding provider from the kiln
            // when available. Falls back to empty impls only when the kiln
            // cannot be opened or no enrichment is configured — in which case
            // semantic_search will honestly report its unavailable state.
            let default_kiln = session.default_kiln().map(Path::to_path_buf);
            let (knowledge_repo, embedding_provider): (
                Arc<dyn crucible_core::traits::KnowledgeRepository>,
                Arc<dyn crucible_core::enrichment::EmbeddingProvider>,
            ) = {
                let repo: Arc<dyn crucible_core::traits::KnowledgeRepository> =
                    match default_kiln.as_deref() {
                        Some(kiln_path) => match self.kiln_manager.get_or_open(kiln_path).await {
                            Ok(storage) => storage.as_knowledge_repository(),
                            Err(_) => Arc::new(EmptyKnowledgeRepository),
                        },
                        None => Arc::new(EmptyKnowledgeRepository),
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

            // The session's kilns join semantic_search's scope through the SAME
            // source builder precognition uses (one open loop, one trust
            // policy). Left empty when a single kiln makes the fan-out
            // redundant — `SearchTools` then keeps the source it built from
            // `kiln_path` — or when no enrichment config exists to search with.
            let mut search_sources: Vec<crate::multi_kiln_search::KilnSearchSource> = Vec::new();
            if session.kilns.len() > 1 && self.kiln_manager.enrichment_config().is_some() {
                search_sources = self.collect_kiln_search_sources(&session.id, session).await;
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
                    default_kiln.as_deref(),
                )
            });
            let mcp = Arc::new(
                CrucibleMcpServer::new_with_workspace_and_delegation(
                    default_kiln
                        .as_deref()
                        .unwrap_or(Path::new(""))
                        .to_string_lossy()
                        .to_string(),
                    session.workspace.clone(),
                    knowledge_repo,
                    embedding_provider,
                    delegation_context,
                )
                .with_search_sources(search_sources),
            );

            // Security posture for the session's workspace tools: file
            // operations are contained to the workspace + kilns + this
            // session's own storage dir (spill reads), and the project's
            // `[security.shell]` policy applies to bash. Both are resolved
            // ONCE here, never re-read at call time.
            //
            // Only THIS session's storage dir is reachable, never the sessions
            // root around it: that root holds every transcript the daemon has
            // ever recorded, so granting the kiln roots alone handed the agent
            // `read_file`/`glob` over every past conversation. Denying the
            // sessions root — plus each kiln's legacy in-kiln one, which
            // migration is not guaranteed to have emptied — and re-granting
            // this session's own directory is the subtraction; see
            // `session_containment_roots`. (`bash` is not scoped by
            // `allowed_roots` at all — it answers to the shell policy instead
            // — and closing that gap is deliberately out of scope here.)
            let shell_policy = crucible_core::config::read_project_config(&session.workspace)
                .map(|c| c.security.shell);
            let sessions_root = self.session_manager.sessions_root().to_path_buf();
            let (allowed_roots, denied_roots) =
                crate::agent_manager::scope::session_containment_roots(session, &sessions_root);
            let mut providers: Vec<Arc<dyn ToolExecutor>> = vec![
                Arc::new(
                    WorkspaceTools::new(&session.workspace)
                        .with_env("CRU_SESSION", &session.id)
                        .with_env(
                            "CRU_SESSION_DIR",
                            session
                                .storage_path(&sessions_root)
                                .to_string_lossy()
                                .to_string(),
                        )
                        .with_allowed_roots(allowed_roots)
                        .with_denied_roots(denied_roots)
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
                    providers.push(Arc::new(
                        crate::tools::gateway_executor::GatewayToolExecutor::new(
                            gateway.clone(),
                            allowed,
                        ),
                    ) as Arc<dyn ToolExecutor>);
                }
            }

            // Plugin tools go LAST so a built-in always wins the dispatch walk.
            // `PluginRegistry` also refuses to register a name that collides
            // with a built-in, so this ordering is a second line of defence
            // rather than the policy itself.
            if let Some(registry) = self.plugin_registry().await {
                providers.push(
                    Arc::new(crate::plugin_tools::PluginToolExecutor::new(registry))
                        as Arc<dyn ToolExecutor>,
                );
            }

            Arc::new(DaemonToolDispatcher::new(providers))
        } else {
            self.tool_dispatcher.clone()
        };

        // Cache and return
        slot.install_dispatcher(generation, &dispatcher);
        dispatcher
    }

    /// Plugin-contributed tools and commands, or `None` when no plugin loader
    /// is attached (most tests).
    pub(crate) async fn plugin_registry(&self) -> Option<Arc<crate::plugin_tools::PluginRegistry>> {
        // Cache first: the loader mutex is held across plugin lifecycle hook
        // execution (container builds included), and per-turn reads must not
        // queue behind another session's slow start. Falls back to the loader
        // for managers the server didn't wire (tests, isolated setups).
        if let Some(registry) = self.plugin_tool_registry.get() {
            return Some(Arc::clone(registry));
        }
        let loader = self.plugin_loader.as_ref()?;
        let guard = loader.lock().await;
        guard.as_ref().map(|l| l.plugin_registry())
    }

    /// The plugin `Lua` handle, preferring the startup-bound `OnceLock` over
    /// the loader mutex.
    ///
    /// Same reasoning as [`Self::plugin_registry`], and it matters on the same
    /// paths: `fire_session_start` holds the loader across plugin hook
    /// execution — container builds included — so a cold-start turn or a title
    /// generation that locks the loader stalls behind an unrelated session's
    /// slow start. The `OnceLock` is bound at daemon startup, so the fallback
    /// only runs for managers the server never wired (tests, isolated setups),
    /// where the loader is uncontended anyway.
    pub(crate) async fn plugin_lua(&self) -> Option<Lua> {
        if let Some((_, lua)) = self.plugin_handlers() {
            return Some((*lua).clone());
        }
        let loader = self.plugin_loader.as_ref()?;
        let guard = loader.lock().await;
        guard.as_ref().map(|l| l.executor().lua().clone())
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

        if let Some((_, mut state)) = self.request_state.remove(session_id) {
            if let Some(cancel_tx) = state.cancel_tx.take() {
                let _ = cancel_tx.send(());
            }
        }

        // Free the per-session workspace-snapshot journal — up to a few MiB per
        // turn, and before this step existed it leaked for the daemon's whole
        // lifetime. Keyed `(session_id, node_id)`, which is why it is not in the
        // slot.
        self.snapshots.clear_session(session_id);
        // The ledger, its review decisions and its comments all die with the
        // session. Persisting them across a restart is `ReviewLedgers::restore`
        // on the resume path, not a survival property of this map.
        //
        // A delegated child is the exception: `delegate_session` is
        // deliberately left unbracketed — a parent bracket would overlap every
        // one of the child's and mark them all contested — so the child's own
        // intervals are the only attribution its work has, and they have to be
        // copied into the parent *before* this drops them. That is ordered and
        // journalled, hence spawned; a session with no registered parent, which
        // is every ordinary one, still tears down synchronously.
        let review_harvest_spawned = match tokio::runtime::Handle::try_current() {
            Ok(handle) if self.review.parent_of(session_id).is_some() => {
                let review = Arc::clone(&self.review);
                let session_id = session_id.to_string();
                handle.spawn(async move { review.harvest_and_clear(&session_id).await });
                true
            }
            _ => {
                self.review.clear_session(session_id);
                false
            }
        };
        // Release the session's claim on its watched roots. Best-effort and
        // spawned: an inotify handle that outlives its session costs a
        // descriptor, where blocking teardown on a watch backend would stall
        // every caller of this function.
        if let (Ok(handle), Some(watch)) =
            (tokio::runtime::Handle::try_current(), self.external_watch())
        {
            let watch = Arc::clone(watch);
            let session_id = session_id.to_string();
            handle.spawn(async move {
                if let Err(e) = watch.unwatch_session(&session_id).await {
                    debug!(session_id = %session_id, error = %e, "review watch not released");
                }
            });
        }
        // Everything the slot owns — Lua VM, conversation tree, agent handle,
        // dispatcher, deferred mode, pending prompts, cache stats — in one
        // remove. This used to be seven, and forgetting one was a leak nothing
        // caught.
        self.slots.remove(session_id);
        // Last, deliberately: see `forget_session`. The spawns above can still
        // emit, and a re-created counter is cheaper than a duplicate `seq`.
        crate::event_emitter::forget_session(session_id);

        self.debug_assert_no_residue(session_id, review_harvest_spawned);
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

        self.slot(session_id)
            .insert_permission(permission_id.clone(), pending);

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
        let pending = self
            .existing_slot(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?
            .take_permission(permission_id)
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
        self.existing_slot(session_id)
            .and_then(|slot| slot.permission_request(permission_id))
    }

    #[allow(dead_code)] // permission system API, exercised by tests
    pub fn list_pending_permissions(&self, session_id: &str) -> Vec<(PermissionId, PermRequest)> {
        self.existing_slot(session_id)
            .map(|slot| slot.list_permissions())
            .unwrap_or_default()
    }

    /// All pending permission prompts across every session. The web Inbox
    /// needs the aggregate view: a session waiting on a permission must
    /// surface even when no browser tab is subscribed to its event stream.
    pub fn list_all_pending_permissions(&self) -> Vec<(String, PermissionId, PermRequest)> {
        self.slots
            .iter()
            .flat_map(|entry| {
                let session_id = entry.key().clone();
                entry
                    .value()
                    .list_permissions()
                    .into_iter()
                    .map(move |(id, request)| (session_id.clone(), id, request))
                    .collect::<Vec<_>>()
            })
            .collect()
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
}

pub(crate) mod attachments;
pub mod autocompact;
pub mod cache_stats;
pub mod context_length;
mod iter;
mod messaging;
mod models;
pub(crate) mod precognition;
pub(crate) mod precognition_gate;
pub mod providers;
mod residue;
mod scope;
pub(crate) mod session_vm;
mod slot;
pub(crate) mod stream_config;
pub(crate) use stream_config::{AgentStreamConfig, TurnEnvironment};
pub(crate) mod title;
pub mod tool_tracking;

#[cfg(test)]
mod tests;
