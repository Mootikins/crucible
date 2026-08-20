//! RPC dispatch and method registration
//!
//! Provides a dispatcher that can be unit-tested without socket I/O.
//! The actual handler implementations remain in server.rs for now,
//! but this module provides the infrastructure for testable dispatch.

// The dispatch match over `RpcMethod` is the completeness gate: a new method
// does not compile until it has an arm. Both denies are needed to keep it that
// way — clippy reports a wildcard covering one remaining variant as
// `match_wildcard_for_single_variants` and only a wildcard covering two or more
// as `wildcard_enum_match_arm`, and methods are added one at a time. See
// `tools/surface.rs`, where denying only the first let a variant slip through
// with `_ =>` and the whole suite went green.
#![deny(clippy::wildcard_enum_match_arm)]
#![deny(clippy::match_wildcard_for_single_variants)]

use crate::protocol::{
    Request, RequestId, Response, RpcError, INTERNAL_ERROR, INVALID_PARAMS, METHOD_NOT_FOUND,
};
use crate::rpc::context::RpcContext;
use crate::server::plugins::OptionAction;
use crate::subscription::ClientId;
// The app-config keys that name where the daemon acts, classified once beside
// the struct whose fields they are, so the keys `config.set` refuses and the
// keys the plugin-visible config store withholds cannot drift apart.
use crucible_core::config::LOCATION_CONFIG_KEYS;
use std::sync::Arc;

pub type RpcResult<T> = Result<T, RpcError>;

/// Declare the closed set of JSON-RPC method names once.
///
/// Generates [`RpcMethod`] and [`METHODS`] from one table, so the name a client
/// sees in `daemon.capabilities` and the name the dispatcher answers to are the
/// same token. `RpcDispatcher::dispatch` then matches on the enum with no
/// wildcard arm, which is what makes rustc the completeness gate.
///
/// The gate this replaces read its own source text: it `include_str!`d this
/// file, sliced the 750-line match region between two literal markers, and
/// treated every quoted dotted-lowercase string inside it as a method name. Any
/// such literal in an arm *body* was a false positive, and a name reached
/// through anything but a bare literal was invisible. `METHODS` is what
/// `daemon.capabilities` returns, so drift there hides methods from
/// capability-detecting clients — which happened once already, with
/// `plugin.install` and `plugin.remove`.
macro_rules! rpc_methods {
    ($( $(#[$attr:meta])* $variant:ident = $name:literal ),* $(,)?) => {
        /// Every JSON-RPC method the daemon answers.
        ///
        /// Adding one is a row in [`rpc_methods!`]; the dispatch match then
        /// fails to compile until it has an arm.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[cfg_attr(test, derive(strum::EnumIter))]
        pub enum RpcMethod {
            $( $(#[$attr])* $variant, )*
        }

        impl RpcMethod {
            /// Every variant, in wire order.
            /// `every_rpc_method_variant_is_listed` proves it is complete.
            pub const ALL: &'static [Self] = &[ $( Self::$variant, )* ];

            /// The wire name.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $( Self::$variant => $name, )* }
            }

            /// The variant for a wire name, or `None` when the daemon has no
            /// such method. Derived from [`Self::ALL`], so the two directions
            /// cannot disagree.
            #[must_use]
            pub fn parse(name: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|m| m.as_str() == name)
            }
        }

        /// The names `daemon.capabilities` advertises.
        pub const METHODS: &[&str] = &[ $( $name, )* ];
    };
}

rpc_methods! {
    Ping = "ping",
    DaemonCapabilities = "daemon.capabilities",
    Shutdown = "shutdown",
    KilnOpen = "kiln.open",
    KilnClose = "kiln.close",
    KilnList = "kiln.list",
    KilnSetClassification = "kiln.set_classification",
    SearchVectors = "search_vectors",
    SearchText = "search_text",
    SearchGrep = "search_grep",
    EmbedQuery = "embed.query",
    ListNotes = "list_notes",
    GetNoteByName = "get_note_by_name",
    GetBacklinks = "get_backlinks",
    KilnGraph = "kiln.graph",
    NoteUpsert = "note.upsert",
    NoteGet = "note.get",
    NoteDelete = "note.delete",
    NoteList = "note.list",
    ProcessFile = "process_file",
    ProcessBatch = "process_batch",
    SessionCreate = "session.create",
    SessionList = "session.list",
    SessionGet = "session.get",
    SessionPause = "session.pause",
    SessionResume = "session.resume",
    SessionResumeFromStorage = "session.resume_from_storage",
    SessionEnd = "session.end",
    SessionArchive = "session.archive",
    SessionUnarchive = "session.unarchive",
    SessionDelete = "session.delete",
    SessionCompact = "session.compact",
    SessionSubscribe = "session.subscribe",
    SessionUnsubscribe = "session.unsubscribe",
    SessionConfigureAgent = "session.configure_agent",
    SessionSendMessage = "session.send_message",
    SessionCancel = "session.cancel",
    SessionSwitchModel = "session.switch_model",
    SessionConnectKiln = "session.connect_kiln",
    SessionDisconnectKiln = "session.disconnect_kiln",
    SessionSetWorkspace = "session.set_workspace",
    SessionSetMode = "session.set_mode",
    SessionGetMode = "session.get_mode",
    SessionListModels = "session.list_models",
    SessionListModes = "session.list_modes",
    SessionSetThinkingBudget = "session.set_thinking_budget",
    SessionGetThinkingBudget = "session.get_thinking_budget",
    SessionCacheStats = "session.cache_stats",
    SessionSetAutocompactThreshold = "session.set_autocompact_threshold",
    SessionGetAutocompactThreshold = "session.get_autocompact_threshold",
    SessionAddNotification = "session.add_notification",
    SessionListNotifications = "session.list_notifications",
    SessionDismissNotification = "session.dismiss_notification",
    SessionInteractionRespond = "session.interaction_respond",
    SessionPendingInteractions = "session.pending_interactions",
    SessionSetTemperature = "session.set_temperature",
    SessionGetTemperature = "session.get_temperature",
    SessionSetMaxTokens = "session.set_max_tokens",
    SessionGetMaxTokens = "session.get_max_tokens",
    SessionSetMaxIterations = "session.set_max_iterations",
    SessionGetMaxIterations = "session.get_max_iterations",
    SessionSetExecutionTimeout = "session.set_execution_timeout",
    SessionGetExecutionTimeout = "session.get_execution_timeout",
    SessionSetContextBudget = "session.set_context_budget",
    SessionGetContextBudget = "session.get_context_budget",
    SessionSetContextStrategy = "session.set_context_strategy",
    SessionGetContextStrategy = "session.get_context_strategy",
    SessionSetContextWindow = "session.set_context_window",
    SessionGetContextWindow = "session.get_context_window",
    SessionSetOutputValidation = "session.set_output_validation",
    SessionGetOutputValidation = "session.get_output_validation",
    SessionSetValidationRetries = "session.set_validation_retries",
    SessionGetValidationRetries = "session.get_validation_retries",
    SessionSetSystemPrompt = "session.set_system_prompt",
    SessionGetSystemPrompt = "session.get_system_prompt",
    SessionSetPrecognition = "session.set_precognition",
    SessionGetPrecognition = "session.get_precognition",
    SessionSetPrecognitionResults = "session.set_precognition_results",
    SessionGetPrecognitionResults = "session.get_precognition_results",
    SessionInjectContext = "session.inject_context",
    SessionTestInteraction = "session.test_interaction",
    SessionFork = "session.fork",
    SessionSetTitle = "session.set_title",
    SessionGenerateTitle = "session.generate_title",
    SessionSearch = "session.search",
    SessionLoadEvents = "session.load_events",
    SessionListPersisted = "session.list_persisted",
    SessionRenderMarkdown = "session.render_markdown",
    SessionExportToFile = "session.export_to_file",
    SessionReplay = "session.replay",
    SessionCleanup = "session.cleanup",
    SessionReindex = "session.reindex",
    SessionUndo = "session.undo",
    SessionCanUndo = "session.can_undo",
    SessionUndoDepth = "session.undo_depth",
    ReviewListHunks = "review.list_hunks",
    ReviewSetState = "review.set_state",
    ReviewComment = "review.comment",
    ReviewResolveComment = "review.resolve_comment",
    ReviewRebase = "review.rebase",
    PluginReload = "plugin.reload",
    PluginList = "plugin.list",
    PluginCommands = "plugin.commands",
    PluginPublications = "plugin.publications",
    PluginOptions = "plugin.options",
    PluginOptionGet = "plugin.option_get",
    PluginOptionSet = "plugin.option_set",
    PluginOptionExecute = "plugin.option_execute",
    SessionStatus = "session.status",
    PluginRunCommand = "plugin.run_command",
    PluginInstall = "plugin.install",
    PluginRemove = "plugin.remove",
    LuaInitSession = "lua.init_session",
    LuaShutdownSession = "lua.shutdown_session",
    LuaDiscoverPlugins = "lua.discover_plugins",
    LuaPluginHealth = "lua.plugin_health",
    LuaGenerateStubs = "lua.generate_stubs",
    LuaRunPluginTests = "lua.run_plugin_tests",
    LuaRegisterCommands = "lua.register_commands",
    LuaEval = "lua.eval",
    ConfigGet = "config.get",
    ConfigSet = "config.set",
    UiConfig = "ui.config",
    UiSetTheme = "ui.set_theme",
    ProjectRegister = "project.register",
    ProjectUnregister = "project.unregister",
    ProjectList = "project.list",
    ProjectGet = "project.get",
    ScmClone = "scm.clone",
    FsListDir = "fs.list_dir",
    FsMove = "fs.move",
    FsMkdir = "fs.mkdir",
    FsTrash = "fs.trash",
    NoteRename = "note.rename",
    NoteMove = "note.move",
    StorageVerify = "storage.verify",
    StorageCleanup = "storage.cleanup",
    StorageBackup = "storage.backup",
    StorageRestore = "storage.restore",
    McpStart = "mcp.start",
    McpStop = "mcp.stop",
    McpStatus = "mcp.status",
    SkillsList = "skills.list",
    SkillsGet = "skills.get",
    SkillsSearch = "skills.search",
    AgentsListProfiles = "agents.list_profiles",
    AgentsResolveProfile = "agents.resolve_profile",
    ModelsList = "models.list",
    ProvidersList = "providers.list",
    SubagentCollect = "subagent.collect",
    WebhookReceive = "webhook.receive",
    SuggestLinks = "suggest_links",
    WorkflowStart = "workflow.start",
    WorkflowApproveGate = "workflow.approve_gate",
    WorkflowStatus = "workflow.status",
    WorkflowCancel = "workflow.cancel",
}
fn to_response(id: Option<RequestId>, result: RpcResult<serde_json::Value>) -> Response {
    match result {
        Ok(v) => Response::success(id, v),
        Err(e) => Response {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(e),
        },
    }
}

fn map_server_resp(resp: Response) -> RpcResult<serde_json::Value> {
    match resp.error {
        Some(err) => Err(err),
        None => Ok(resp.result.unwrap_or(serde_json::Value::Null)),
    }
}

// Forward a routed method to its server handler. The `map_server_resp` round
// trip is load-bearing: a server `Response` with neither `result` nor `error`
// must stay a `null` result on the wire, which returning the `Response`
// directly would drop (`result` is `skip_serializing_if = "Option::is_none"`).
macro_rules! forward {
    ($id:expr, $call:expr) => {
        to_response($id, map_server_resp($call.await))
    };
}

// Route a filtered `session.set_*` / `session.get_*` method string to its
// server handler. Every method literal stays paired with its handler at the
// call site (greppable, wire-name-explicit); the shared call shape lives here.
macro_rules! dispatch_session_setter {
    ($req:expr, $agents:expr, $event_tx:expr, { $($method:literal => $handler:ident),+ $(,)? }) => {
        match $req.method.as_str() {
            $(
                $method => {
                    crate::server::session::$handler($req.clone(), $agents, $event_tx).await
                }
            )+
            _ => unreachable!("dispatch match already filtered to known setter methods"),
        }
    };
}

macro_rules! dispatch_session_getter {
    ($req:expr, $agents:expr, { $($method:literal => $handler:ident),+ $(,)? }) => {
        match $req.method.as_str() {
            $(
                $method => crate::server::session::$handler($req.clone(), $agents).await,
            )+
            _ => unreachable!("dispatch match already filtered to known getter methods"),
        }
    };
}

pub struct RpcDispatcher {
    /// Shared rather than owned: `Server` keeps a clone so plugin boot can hand
    /// the same context to the Lua session bridge, and both paths must see one
    /// set of managers.
    ctx: Arc<RpcContext>,
}

impl RpcDispatcher {
    pub fn new(ctx: Arc<RpcContext>) -> Self {
        Self { ctx }
    }

    pub async fn dispatch(&self, client_id: ClientId, req: Request) -> Response {
        let id = req.id.clone();
        tracing::debug!("RPC dispatch: method={:?}, id={:?}", req.method, id);

        // Parse first, so the match below is exhaustive over a closed set and
        // rustc — not a source-text scan — is what proves every advertised
        // method has an arm.
        let Some(method) = RpcMethod::parse(req.method.as_str()) else {
            return Response::error(
                id,
                METHOD_NOT_FOUND,
                format!("Method not found: '{}'", req.method),
            );
        };

        match method {
            RpcMethod::Ping => to_response(id, self.handle_ping()),
            RpcMethod::DaemonCapabilities => to_response(id, self.handle_capabilities()),
            RpcMethod::Shutdown => to_response(id, self.handle_shutdown()),

            // Subscription handlers (need client_id)
            RpcMethod::SessionSubscribe => to_response(id, self.handle_subscribe(client_id, &req)),
            RpcMethod::SessionUnsubscribe => {
                to_response(id, self.handle_unsubscribe(client_id, &req))
            }

            // Session title handler
            RpcMethod::SessionSetTitle => to_response(id, self.handle_set_title(&req).await),
            RpcMethod::SessionGenerateTitle => {
                to_response(id, self.handle_generate_title(&req).await)
            }

            // Session config get/set handlers — each pair delegates to
            // server::session::handle_session_{set,get}_<name> with uniform signatures.
            RpcMethod::SessionSetThinkingBudget
            | RpcMethod::SessionSetTemperature
            | RpcMethod::SessionSetMaxTokens
            | RpcMethod::SessionSetMaxIterations
            | RpcMethod::SessionSetExecutionTimeout
            | RpcMethod::SessionSetContextBudget
            | RpcMethod::SessionSetContextStrategy
            | RpcMethod::SessionSetContextWindow
            | RpcMethod::SessionSetOutputValidation
            | RpcMethod::SessionSetValidationRetries
            | RpcMethod::SessionSetSystemPrompt
            | RpcMethod::SessionSetPrecognition
            | RpcMethod::SessionSetPrecognitionResults
            | RpcMethod::SessionSetAutocompactThreshold => {
                to_response(id, self.dispatch_session_config_setter(&req).await)
            }
            RpcMethod::SessionGetThinkingBudget
            | RpcMethod::SessionGetTemperature
            | RpcMethod::SessionGetMode
            | RpcMethod::SessionGetMaxTokens
            | RpcMethod::SessionGetMaxIterations
            | RpcMethod::SessionGetExecutionTimeout
            | RpcMethod::SessionGetContextBudget
            | RpcMethod::SessionGetContextStrategy
            | RpcMethod::SessionGetContextWindow
            | RpcMethod::SessionGetOutputValidation
            | RpcMethod::SessionGetValidationRetries
            | RpcMethod::SessionGetSystemPrompt
            | RpcMethod::SessionGetPrecognition
            | RpcMethod::SessionGetPrecognitionResults
            | RpcMethod::SessionGetAutocompactThreshold => {
                to_response(id, self.dispatch_session_config_getter(&req).await)
            }
            RpcMethod::SessionCacheStats => forward!(
                id,
                crate::server::session::handle_session_cache_stats(req.clone(), &self.ctx.agents)
            ),
            // Kiln CRUD handlers
            RpcMethod::KilnOpen => forward!(
                id,
                crate::server::kiln::handle_kiln_open(
                    req.clone(),
                    &self.ctx.kiln,
                    &self.ctx.plugin_loader,
                    &self.ctx.event_tx
                )
            ),
            RpcMethod::KilnClose => forward!(
                id,
                crate::server::kiln::handle_kiln_close(req.clone(), &self.ctx.kiln)
            ),
            RpcMethod::KilnList => forward!(
                id,
                crate::server::kiln::handle_kiln_list(
                    req.clone(),
                    &self.ctx.kiln,
                    &self.ctx.kiln_registry,
                    &self.ctx.data_home
                )
            ),
            RpcMethod::KilnSetClassification => {
                forward!(
                    id,
                    crate::server::kiln::handle_kiln_set_classification(
                        req.clone(),
                        &self.ctx.kiln
                    )
                )
            }

            // Note search and retrieval handlers
            RpcMethod::SearchVectors => forward!(
                id,
                crate::server::kiln::handle_search_vectors(req.clone(), &self.ctx.kiln)
            ),
            RpcMethod::SearchText => forward!(
                id,
                crate::server::kiln::handle_search_text(req.clone(), &self.ctx.kiln)
            ),
            RpcMethod::SearchGrep => forward!(
                id,
                crate::server::grep::handle_search_grep(
                    req.clone(),
                    &self.ctx.project_manager,
                    &self.ctx.kiln
                )
            ),
            RpcMethod::EmbedQuery => forward!(
                id,
                crate::server::kiln::handle_embed_query(req.clone(), &self.ctx.kiln)
            ),
            RpcMethod::ListNotes => forward!(
                id,
                crate::server::kiln::handle_list_notes(req.clone(), &self.ctx.kiln)
            ),
            RpcMethod::GetNoteByName => forward!(
                id,
                crate::server::kiln::handle_get_note_by_name(req.clone(), &self.ctx.kiln)
            ),
            RpcMethod::GetBacklinks => forward!(
                id,
                crate::server::kiln::handle_get_backlinks(req.clone(), &self.ctx.kiln)
            ),
            RpcMethod::KilnGraph => forward!(
                id,
                crate::server::kiln::handle_kiln_graph(req.clone(), &self.ctx.kiln)
            ),
            RpcMethod::SuggestLinks => forward!(
                id,
                crate::server::kiln::handle_suggest_links(req.clone(), &self.ctx.kiln)
            ),

            // Note CRUD handlers
            RpcMethod::NoteUpsert => forward!(
                id,
                crate::server::kiln::handle_note_upsert(req.clone(), &self.ctx.kiln)
            ),
            RpcMethod::NoteGet => forward!(
                id,
                crate::server::kiln::handle_note_get(req.clone(), &self.ctx.kiln)
            ),
            RpcMethod::NoteDelete => forward!(
                id,
                crate::server::kiln::handle_note_delete(req.clone(), &self.ctx.kiln)
            ),
            RpcMethod::NoteList => forward!(
                id,
                crate::server::kiln::handle_note_list(req.clone(), &self.ctx.kiln)
            ),

            // Processing handlers
            RpcMethod::ProcessFile => forward!(
                id,
                crate::server::kiln::handle_process_file(req.clone(), &self.ctx.kiln)
            ),
            RpcMethod::ProcessBatch => forward!(
                id,
                crate::server::kiln::handle_process_batch(req.clone(), &self.ctx.kiln)
            ),

            // Models handler
            RpcMethod::ModelsList => forward!(
                id,
                crate::server::session::handle_models_list(req.clone(), &self.ctx.agents)
            ),
            RpcMethod::ProvidersList => forward!(
                id,
                crate::server::session::handle_providers_list(req.clone(), &self.ctx.agents)
            ),

            // Session lifecycle handlers
            RpcMethod::SessionCreate => to_response(id, self.handle_session_create(&req).await),
            RpcMethod::SessionList => forward!(
                id,
                crate::server::session::handle_session_list(
                    req.clone(),
                    &self.ctx.sessions,
                    &self.ctx.kiln,
                    &self.ctx.data_home
                )
            ),
            RpcMethod::SessionGet => forward!(
                id,
                crate::server::session::handle_session_get(req.clone(), &self.ctx.sessions)
            ),
            RpcMethod::SessionPause => to_response(id, self.handle_session_pause(&req).await),
            RpcMethod::SessionResume => to_response(id, self.handle_session_resume(&req).await),
            RpcMethod::SessionResumeFromStorage => {
                to_response(id, self.handle_session_resume_from_storage(&req).await)
            }
            RpcMethod::SessionEnd => to_response(id, self.handle_session_end(&req).await),
            RpcMethod::SessionArchive => forward!(
                id,
                crate::server::session::handle_session_archive(
                    req.clone(),
                    &self.ctx.sessions,
                    &self.ctx.agents
                )
            ),
            RpcMethod::SessionUnarchive => forward!(
                id,
                crate::server::session::handle_session_unarchive(
                    req.clone(),
                    &self.ctx.sessions,
                    &self.ctx.agents
                )
            ),
            RpcMethod::SessionDelete => forward!(
                id,
                crate::server::session::handle_session_delete(
                    req.clone(),
                    &self.ctx.sessions,
                    &self.ctx.agents
                )
            ),
            RpcMethod::SessionCompact => forward!(
                id,
                crate::server::session::handle_session_compact(req.clone(), &self.ctx.sessions)
            ),
            RpcMethod::SessionFork => to_response(id, self.handle_session_fork(&req).await),

            // Session utility handlers
            RpcMethod::SessionSearch => forward!(
                id,
                crate::server::session::handle_session_search(req.clone(), &self.ctx.sessions)
            ),
            RpcMethod::SessionLoadEvents => forward!(
                id,
                crate::server::observe::handle_session_load_events(
                    req.clone(),
                    self.ctx.sessions.sessions_root()
                )
            ),
            RpcMethod::SessionListPersisted => {
                forward!(
                    id,
                    crate::server::observe::handle_session_list_persisted(
                        req.clone(),
                        &self.ctx.sessions
                    )
                )
            }
            RpcMethod::SessionRenderMarkdown => {
                forward!(
                    id,
                    crate::server::observe::handle_session_render_markdown(
                        req.clone(),
                        self.ctx.sessions.sessions_root()
                    )
                )
            }
            RpcMethod::SessionExportToFile => {
                forward!(
                    id,
                    crate::server::observe::handle_session_export_to_file(
                        req.clone(),
                        self.ctx.sessions.sessions_root()
                    )
                )
            }
            RpcMethod::SessionCleanup => forward!(
                id,
                crate::server::observe::handle_session_cleanup(req.clone(), &self.ctx.sessions)
            ),
            // Retired rather than repointed: it indexed `{kiln}/.crucible/sessions`
            // into that kiln's NoteStore, and sessions no longer live in a kiln.
            // A flat backlog has no per-kiln session corpus to rebuild, and
            // rebuilding one would re-create the cross-session read it removed.
            RpcMethod::SessionReindex => Response::error(
                id,
                METHOD_NOT_FOUND,
                "session.reindex is retired: sessions are stored outside kilns and are no \
                 longer indexed as kiln notes. Delete any existing `sessions/*` note rows \
                 left by an earlier reindex."
                    .to_string(),
            ),

            // Agent operation handlers
            RpcMethod::SessionConfigureAgent => {
                to_response(id, self.handle_session_configure_agent(&req).await)
            }
            RpcMethod::SessionSendMessage => forward!(
                id,
                crate::server::session::handle_session_send_message(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.event_tx
                )
            ),
            RpcMethod::SessionInjectContext => {
                forward!(
                    id,
                    crate::server::session::handle_session_inject_context(
                        req.clone(),
                        &self.ctx.sessions,
                        &self.ctx.event_tx
                    )
                )
            }
            RpcMethod::SessionCancel => forward!(
                id,
                crate::server::session::handle_session_cancel(req.clone(), &self.ctx.agents)
            ),
            RpcMethod::SessionInteractionRespond => {
                forward!(
                    id,
                    crate::server::session::handle_session_interaction_respond(
                        req.clone(),
                        &self.ctx.agents,
                        &self.ctx.event_tx
                    )
                )
            }
            RpcMethod::SessionPendingInteractions => {
                forward!(
                    id,
                    crate::server::session::handle_session_pending_interactions(
                        req.clone(),
                        &self.ctx.agents
                    )
                )
            }
            RpcMethod::SessionSwitchModel => forward!(
                id,
                crate::server::session::handle_session_switch_model(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.event_tx
                )
            ),
            RpcMethod::SessionConnectKiln => forward!(
                id,
                crate::server::session::handle_session_connect_kiln(
                    req.clone(),
                    &self.ctx.sessions,
                    &self.ctx.agents,
                    &self.ctx.kiln,
                    &self.ctx.llm_config,
                    &self.ctx.event_tx
                )
            ),
            RpcMethod::SessionDisconnectKiln => {
                forward!(
                    id,
                    crate::server::session::handle_session_disconnect_kiln(
                        req.clone(),
                        &self.ctx.agents,
                        &self.ctx.event_tx
                    )
                )
            }
            RpcMethod::SessionSetWorkspace => {
                forward!(
                    id,
                    crate::server::session::handle_session_set_workspace(
                        req.clone(),
                        &self.ctx.sessions,
                        &self.ctx.agents,
                        &self.ctx.project_manager,
                        &self.ctx.llm_config,
                        &self.ctx.event_tx
                    )
                )
            }
            RpcMethod::SessionSetMode => forward!(
                id,
                crate::server::session::handle_session_set_mode(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.event_tx
                )
            ),

            // Review queue. Session-scoped like the handlers above, but
            // namespaced `review.*` rather than `session.*`: the unit they act
            // on is a composed hunk, and a delegating agent reviewing a child
            // session addresses that child's id, not its own.
            RpcMethod::ReviewListHunks => forward!(
                id,
                crate::server::session::handle_review_list_hunks(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.sessions
                )
            ),
            RpcMethod::ReviewSetState => forward!(
                id,
                crate::server::session::handle_review_set_state(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.sessions,
                    &self.ctx.event_tx
                )
            ),
            RpcMethod::ReviewComment => forward!(
                id,
                crate::server::session::handle_review_comment(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.sessions,
                    &self.ctx.event_tx
                )
            ),
            RpcMethod::ReviewResolveComment => {
                forward!(
                    id,
                    crate::server::session::handle_review_resolve_comment(
                        req.clone(),
                        &self.ctx.agents,
                        &self.ctx.sessions,
                        &self.ctx.event_tx
                    )
                )
            }
            // The release valve for the one block reviewing cannot clear: a
            // base tree gc'd out of the object store, a root that moved, a
            // journal record that would not parse. Without it, failing closed
            // on a structural failure would be an unreleasable hang.
            RpcMethod::ReviewRebase => forward!(
                id,
                crate::server::session::handle_review_rebase(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.sessions,
                    &self.ctx.event_tx
                )
            ),

            RpcMethod::SessionListModels => forward!(
                id,
                crate::server::session::handle_session_list_models(req.clone(), &self.ctx.agents)
            ),
            RpcMethod::SessionListModes => forward!(
                id,
                crate::server::session::handle_session_list_modes(req.clone(), &self.ctx.agents)
            ),
            RpcMethod::SessionAddNotification => {
                forward!(
                    id,
                    crate::server::session::handle_session_add_notification(
                        req.clone(),
                        &self.ctx.agents,
                        &self.ctx.event_tx
                    )
                )
            }
            RpcMethod::SessionListNotifications => {
                forward!(
                    id,
                    crate::server::session::handle_session_list_notifications(
                        req.clone(),
                        &self.ctx.agents
                    )
                )
            }
            RpcMethod::SessionDismissNotification => {
                forward!(
                    id,
                    crate::server::session::handle_session_dismiss_notification(
                        req.clone(),
                        &self.ctx.agents,
                        &self.ctx.event_tx
                    )
                )
            }
            RpcMethod::SessionTestInteraction => {
                forward!(
                    id,
                    crate::server::session::handle_session_test_interaction(
                        req.clone(),
                        &self.ctx.event_tx
                    )
                )
            }
            RpcMethod::SessionReplay => forward!(
                id,
                crate::server::session::handle_session_replay(
                    req.clone(),
                    &self.ctx.sessions,
                    &self.ctx.event_tx
                )
            ),

            // Undo handlers
            RpcMethod::SessionUndo => forward!(
                id,
                crate::server::session::handle_session_undo(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.event_tx
                )
            ),
            RpcMethod::SessionCanUndo => forward!(
                id,
                crate::server::session::handle_session_can_undo(req.clone(), &self.ctx.agents)
            ),
            RpcMethod::SessionUndoDepth => forward!(
                id,
                crate::server::session::handle_session_undo_depth(req.clone(), &self.ctx.agents)
            ),

            // Lua RPC handlers
            RpcMethod::LuaInitSession => forward!(
                id,
                crate::server::lua::handle_lua_init_session(
                    req.clone(),
                    &self.ctx.lua_sessions,
                    &self.ctx.plugin_loader
                )
            ),
            RpcMethod::LuaShutdownSession => forward!(
                id,
                crate::server::lua::handle_lua_shutdown_session(
                    req.clone(),
                    &self.ctx.lua_sessions
                )
            ),
            RpcMethod::LuaDiscoverPlugins => forward!(
                id,
                crate::server::lua::handle_lua_discover_plugins(req.clone())
            ),
            RpcMethod::LuaPluginHealth => forward!(
                id,
                crate::server::lua::handle_lua_plugin_health(req.clone())
            ),
            RpcMethod::LuaGenerateStubs => forward!(
                id,
                crate::server::lua::handle_lua_generate_stubs(req.clone())
            ),
            RpcMethod::LuaRunPluginTests => forward!(
                id,
                crate::server::lua_plugin_suite::handle_lua_run_plugin_tests(req.clone())
            ),
            RpcMethod::LuaRegisterCommands => {
                forward!(
                    id,
                    crate::server::lua::handle_lua_register_commands(
                        req.clone(),
                        &self.ctx.lua_sessions
                    )
                )
            }
            RpcMethod::LuaEval => to_response(id, self.handle_lua_eval(&req).await),

            // App-config store (the same store `cru.config.*` reads in Lua)
            RpcMethod::ConfigGet => to_response(id, self.handle_config_get(&req)),
            RpcMethod::ConfigSet => to_response(id, self.handle_config_set(&req)),

            // Lua-defined UI config (theme now; surfaces and bars follow).
            // Snapshot half of the handshake — see `rpc::ui`.
            RpcMethod::UiConfig => {
                to_response(id, Ok(crate::rpc::ui::handle_ui_config(&self.ctx, &req)))
            }
            RpcMethod::UiSetTheme => to_response(
                id,
                crate::rpc::ui::handle_ui_set_theme(&self.ctx, &req).map_err(|message| {
                    crate::protocol::RpcError {
                        code: crate::protocol::INVALID_PARAMS,
                        message,
                        data: None,
                    }
                }),
            ),

            // Plugin RPC handlers
            RpcMethod::PluginReload => to_response(id, self.handle_plugin_reload(&req).await),
            RpcMethod::PluginList => forward!(
                id,
                crate::server::plugins::handle_plugin_list(req.clone(), &self.ctx.plugin_loader)
            ),
            RpcMethod::PluginCommands => forward!(
                id,
                crate::server::plugins::handle_plugin_commands(
                    req.clone(),
                    &self.ctx.plugin_loader
                )
            ),
            RpcMethod::PluginPublications => forward!(
                id,
                crate::server::plugins::handle_plugin_publications(
                    req.clone(),
                    &self.ctx.plugin_loader
                )
            ),
            RpcMethod::PluginOptions => forward!(
                id,
                crate::server::plugins::handle_plugin_options(req.clone(), &self.ctx.plugin_loader)
            ),
            RpcMethod::PluginOptionGet => forward!(
                id,
                crate::server::plugins::handle_plugin_option_call(
                    req.clone(),
                    &self.ctx.plugin_loader,
                    OptionAction::Get
                )
            ),
            RpcMethod::PluginOptionSet => forward!(
                id,
                crate::server::plugins::handle_plugin_option_call(
                    req.clone(),
                    &self.ctx.plugin_loader,
                    OptionAction::Set
                )
            ),
            RpcMethod::PluginOptionExecute => forward!(
                id,
                crate::server::plugins::handle_plugin_option_call(
                    req.clone(),
                    &self.ctx.plugin_loader,
                    OptionAction::Execute
                )
            ),
            RpcMethod::SessionStatus => forward!(
                id,
                crate::server::plugins::handle_session_status(req.clone(), &self.ctx.plugin_loader)
            ),
            RpcMethod::PluginRunCommand => forward!(
                id,
                crate::server::plugins::handle_plugin_run_command(
                    req.clone(),
                    &self.ctx.plugin_loader
                )
            ),
            RpcMethod::PluginInstall => to_response(id, self.handle_plugin_install(&req).await),
            RpcMethod::PluginRemove => to_response(id, self.handle_plugin_remove(&req).await),

            // Project RPC handlers
            RpcMethod::ProjectRegister => forward!(
                id,
                crate::server::plugins::handle_project_register(
                    req.clone(),
                    &self.ctx.project_manager
                )
            ),
            RpcMethod::ProjectUnregister => forward!(
                id,
                crate::server::plugins::handle_project_unregister(
                    req.clone(),
                    &self.ctx.project_manager
                )
            ),
            RpcMethod::ProjectList => forward!(
                id,
                crate::server::plugins::handle_project_list(req.clone(), &self.ctx.project_manager)
            ),
            RpcMethod::ProjectGet => forward!(
                id,
                crate::server::plugins::handle_project_get(req.clone(), &self.ctx.project_manager)
            ),
            RpcMethod::ScmClone => to_response(id, self.handle_scm_clone(&req).await),
            RpcMethod::FsListDir => forward!(
                id,
                crate::server::fs::handle_fs_list_dir(req.clone(), &self.ctx.project_manager)
            ),
            RpcMethod::FsMove => forward!(
                id,
                crate::server::fs::handle_fs_move(
                    req.clone(),
                    &self.ctx.project_manager,
                    &self.ctx.kiln
                )
            ),
            RpcMethod::FsMkdir => forward!(
                id,
                crate::server::fs::handle_fs_mkdir(
                    req.clone(),
                    &self.ctx.project_manager,
                    &self.ctx.kiln
                )
            ),
            RpcMethod::FsTrash => forward!(
                id,
                crate::server::fs::handle_fs_trash(
                    req.clone(),
                    &self.ctx.project_manager,
                    &self.ctx.kiln
                )
            ),
            RpcMethod::NoteRename | RpcMethod::NoteMove => forward!(
                id,
                crate::server::note_refactor::handle_note_rename(req.clone(), &self.ctx.kiln)
            ),

            // Storage RPC handlers
            RpcMethod::StorageVerify => forward!(
                id,
                crate::server::storage::handle_storage_verify(req.clone())
            ),
            RpcMethod::StorageCleanup => forward!(
                id,
                crate::server::storage::handle_storage_cleanup(req.clone())
            ),
            RpcMethod::StorageBackup => forward!(
                id,
                crate::server::storage::handle_storage_backup(req.clone())
            ),
            RpcMethod::StorageRestore => forward!(
                id,
                crate::server::storage::handle_storage_restore(req.clone())
            ),

            // MCP RPC handlers
            RpcMethod::McpStart => to_response(id, self.handle_mcp_start(&req).await),
            RpcMethod::McpStop => forward!(
                id,
                crate::server::platform::handle_mcp_stop(req.clone(), &self.ctx.mcp_server_manager)
            ),
            RpcMethod::McpStatus => forward!(
                id,
                crate::server::platform::handle_mcp_status(
                    req.clone(),
                    &self.ctx.mcp_server_manager
                )
            ),

            // Skills RPC handlers
            RpcMethod::SkillsList => {
                forward!(id, crate::server::platform::handle_skills_list(req.clone()))
            }
            RpcMethod::SkillsGet => {
                forward!(id, crate::server::platform::handle_skills_get(req.clone()))
            }
            RpcMethod::SkillsSearch => forward!(
                id,
                crate::server::platform::handle_skills_search(req.clone())
            ),

            // Agents RPC handlers
            RpcMethod::AgentsListProfiles => forward!(
                id,
                crate::server::platform::handle_agents_list_profiles(req.clone(), &self.ctx.agents)
            ),
            RpcMethod::AgentsResolveProfile => {
                forward!(
                    id,
                    crate::server::platform::handle_agents_resolve_profile(
                        req.clone(),
                        &self.ctx.agents
                    )
                )
            }

            // Subagent RPC handlers
            RpcMethod::SubagentCollect => to_response(id, self.handle_subagent_collect(&req).await),

            // Webhook RPC handler
            RpcMethod::WebhookReceive => to_response(id, self.handle_webhook_receive(&req)),

            // Workflow execution (Phase 3a)
            RpcMethod::WorkflowStart => to_response(
                id,
                crate::rpc::workflow_handlers::handle_workflow_start(&self.ctx, &req).await,
            ),
            RpcMethod::WorkflowApproveGate => to_response(
                id,
                crate::rpc::workflow_handlers::handle_workflow_approve_gate(&self.ctx, &req).await,
            ),
            RpcMethod::WorkflowStatus => to_response(
                id,
                crate::rpc::workflow_handlers::handle_workflow_status(&self.ctx, &req).await,
            ),
            RpcMethod::WorkflowCancel => to_response(
                id,
                crate::rpc::workflow_handlers::handle_workflow_cancel(&self.ctx, &req).await,
            ),
        }
    }

    fn handle_ping(&self) -> RpcResult<serde_json::Value> {
        Ok(serde_json::json!("pong"))
    }

    /// Arms the shutdown rather than signalling it: the connection fires it
    /// after this confirmation has been written. See [`DeferredShutdown`].
    fn handle_shutdown(&self) -> RpcResult<serde_json::Value> {
        tracing::info!("Shutdown requested via RPC");
        self.ctx.shutdown.arm();
        Ok(serde_json::json!("shutting down"))
    }

    fn handle_capabilities(&self) -> RpcResult<serde_json::Value> {
        Ok(serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "build_sha": option_env!("CRUCIBLE_BUILD_SHA").unwrap_or("dev"),
            "protocol_version": "1.0",
            "capabilities": {
                "kilns": true,
                "sessions": true,
                "agents": true,
                "events": true,
                "thinking_budget": true,
                "model_switching": true,
            },
            "methods": METHODS,
        }))
    }

    fn handle_subscribe(&self, client_id: ClientId, req: &Request) -> RpcResult<serde_json::Value> {
        use crate::rpc::params::parse_params;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            session_ids: Vec<String>,
        }
        let p: Params = parse_params(req)?;

        for session_id in &p.session_ids {
            if session_id == "*" {
                self.ctx.subscriptions.subscribe_all(client_id);
            } else {
                self.ctx.subscriptions.subscribe(client_id, session_id);
            }
        }

        Ok(serde_json::json!({
            "subscribed": p.session_ids,
            "client_id": format!("{:?}", client_id),
        }))
    }

    fn handle_unsubscribe(
        &self,
        client_id: ClientId,
        req: &Request,
    ) -> RpcResult<serde_json::Value> {
        use crate::rpc::params::parse_params;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            session_ids: Vec<String>,
        }
        let p: Params = parse_params(req)?;

        for session_id in &p.session_ids {
            self.ctx.subscriptions.unsubscribe(client_id, session_id);
        }

        Ok(serde_json::json!({
            "unsubscribed": p.session_ids,
            "client_id": format!("{:?}", client_id),
        }))
    }

    async fn handle_set_title(&self, req: &Request) -> RpcResult<serde_json::Value> {
        // The client's own request type, not a second spelling of its two
        // fields (gate A6).
        let p = crate::rpc::params::parse_params::<crate::rpc_client::SessionSetTitleRequest>(req)?;

        self.ctx
            .sessions
            .set_title(&p.session_id, p.title.clone())
            .await
            .map_err(|e| RpcError {
                code: crate::protocol::INVALID_PARAMS,
                message: format!("Failed to set title: {}", e),
                data: None,
            })?;

        Ok(serde_json::json!({
            "session_id": p.session_id,
            "title": p.title,
        }))
    }

    async fn handle_generate_title(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let p = crate::rpc::params::parse_params::<crate::rpc_client::SessionIdRequest>(req)?;

        let title = self
            .ctx
            .agents
            .generate_session_title(&p.session_id, &self.ctx.event_tx)
            .await
            .map_err(|e| RpcError {
                code: crate::protocol::INVALID_PARAMS,
                message: format!("Failed to generate title: {}", e),
                data: None,
            })?;

        Ok(serde_json::json!({
            "session_id": p.session_id,
            "title": title,
        }))
    }

    /// Route a `session.set_*` method to the corresponding server handler.
    ///
    /// All session config setters share the signature `(Request, &AgentManager, &Sender) -> Response`.
    /// This avoids 13 near-identical one-line forwarding methods.
    async fn dispatch_session_config_setter(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = dispatch_session_setter!(req, &self.ctx.agents, &self.ctx.event_tx, {
            "session.set_thinking_budget" => handle_session_set_thinking_budget,
            "session.set_temperature" => handle_session_set_temperature,
            "session.set_max_tokens" => handle_session_set_max_tokens,
            "session.set_max_iterations" => handle_session_set_max_iterations,
            "session.set_execution_timeout" => handle_session_set_execution_timeout,
            "session.set_context_budget" => handle_session_set_context_budget,
            "session.set_context_strategy" => handle_session_set_context_strategy,
            "session.set_context_window" => handle_session_set_context_window,
            "session.set_output_validation" => handle_session_set_output_validation,
            "session.set_validation_retries" => handle_session_set_validation_retries,
            "session.set_system_prompt" => handle_session_set_system_prompt,
            "session.set_precognition" => handle_session_set_precognition,
            "session.set_precognition_results" => handle_session_set_precognition_results,
            "session.set_autocompact_threshold" => handle_session_set_autocompact_threshold,
        });
        map_server_resp(resp)
    }

    /// Route a `session.get_*` method to the corresponding server handler.
    ///
    /// All session config getters share the signature `(Request, &AgentManager) -> Response`.
    async fn dispatch_session_config_getter(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = dispatch_session_getter!(req, &self.ctx.agents, {
            "session.get_thinking_budget" => handle_session_get_thinking_budget,
            "session.get_temperature" => handle_session_get_temperature,
            "session.get_mode" => handle_session_get_mode,
            "session.get_max_tokens" => handle_session_get_max_tokens,
            "session.get_max_iterations" => handle_session_get_max_iterations,
            "session.get_execution_timeout" => handle_session_get_execution_timeout,
            "session.get_context_budget" => handle_session_get_context_budget,
            "session.get_context_strategy" => handle_session_get_context_strategy,
            "session.get_context_window" => handle_session_get_context_window,
            "session.get_output_validation" => handle_session_get_output_validation,
            "session.get_validation_retries" => handle_session_get_validation_retries,
            "session.get_system_prompt" => handle_session_get_system_prompt,
            "session.get_precognition" => handle_session_get_precognition,
            "session.get_precognition_results" => handle_session_get_precognition_results,
            "session.get_autocompact_threshold" => handle_session_get_autocompact_threshold,
        });
        map_server_resp(resp)
    }

    // ── Session lifecycle wrappers ────────────────────────────────────────────

    async fn handle_session_create(&self, req: &Request) -> RpcResult<serde_json::Value> {
        // The workspace axis, resolved before create rather than inside a
        // plugin hook: everything the path feeds — the ACP agent's working
        // directory, project registration, the persisted workspace — is decided
        // by `handle_session_create` below, and `on_session_start` fires after
        // all three. See `crate::workspace_targets`.
        let req = match self.resolve_workspace_target(req).await {
            Ok(req) => req,
            Err(e) => return Err(e),
        };
        let req = &req;

        let resp = crate::server::session::handle_session_create(req.clone(), &self.ctx).await;
        let mapped = map_server_resp(resp);

        // Plugins register their `crucible.on` handlers inside
        // `on_session_start` (oci does), so these hooks have to fire on the
        // plugin runtime — not just the per-call `lua.init_session` executor,
        // which was the only place firing them.
        //
        // Stays above `RpcContext::create_session_resolved`, not inside it: the
        // start/end hooks hold the plugin loader mutex across their Lua call,
        // and a plugin that creates a session from inside `on_session_end`
        // (reflection does) would deadlock on a create path that fired them.
        self.enforce_plugin_session_start(mapped, req).await
    }

    /// Replace `workspace` with what the requested `workspace_target` resolves
    /// to, leaving the request untouched when none was asked for.
    ///
    /// Fail-closed: a target that cannot be resolved refuses the create. A
    /// session that quietly ran against the main checkout when a worktree was
    /// asked for is the workspace-axis version of a session that quietly ran on
    /// the host when a container was asked for.
    async fn resolve_workspace_target(&self, req: &Request) -> RpcResult<Request> {
        let Some(spec) = req
            .params
            .get("workspace_target")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        else {
            return Ok(req.clone());
        };

        let workspace = req.params.get("workspace").and_then(|v| v.as_str());
        let targets = crate::workspace_targets::WorkspaceTargets::new(std::sync::Arc::clone(
            &self.ctx.plugin_loader,
        ));

        match targets.resolve(spec, workspace).await {
            Ok(path) => {
                let mut req = req.clone();
                if let Some(params) = req.params.as_object_mut() {
                    params.insert(
                        "workspace".to_string(),
                        serde_json::Value::String(path.to_string_lossy().into_owned()),
                    );
                }
                Ok(req)
            }
            Err(e) => Err(RpcError {
                code: INVALID_PARAMS,
                message: format!("workspace target '{spec}' could not be resolved: {e:#}"),
                data: None,
            }),
        }
    }

    /// RPC-shaped wrapper over [`SessionLifecycle::enforce_session_start`].
    ///
    /// Only the id extraction is RPC-specific — create returns the id in the
    /// response, resume is addressed by it. The enforcement itself is shared
    /// with `DelegationService`, because `create_child_session` bypassing it
    /// is exactly how a sandboxed parent's subagent escaped onto the host.
    async fn enforce_plugin_session_start(
        &self,
        mapped: RpcResult<serde_json::Value>,
        req: &Request,
    ) -> RpcResult<serde_json::Value> {
        let Ok(value) = &mapped else { return mapped };
        let session_id = value
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| {
                req.params
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            });
        let Some(session_id) = session_id else {
            return mapped;
        };

        match self
            .ctx
            .session_lifecycle
            .enforce_session_start(&session_id)
            .await
        {
            Ok(()) => mapped,
            Err(e) => Err(RpcError {
                code: INTERNAL_ERROR,
                message: format!("session refused: {e}"),
                data: None,
            }),
        }
    }

    /// The isolation registry, without waiting on the loader mutex.
    async fn isolation_registry(&self) -> Option<crucible_lua::IsolationRegistry> {
        self.ctx.session_lifecycle.isolation_registry().await
    }

    /// Fire plugin `on_session_end` hooks, best-effort and exactly once.
    async fn fire_plugin_session_end(&self, session_id: &str) {
        self.ctx
            .session_lifecycle
            .fire_session_end(session_id)
            .await
    }

    async fn handle_session_pause(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::session::handle_session_pause(req.clone(), &self.ctx.sessions).await;
        let mapped = map_server_resp(resp);
        // Symmetric with resume firing start hooks: without this a paused
        // session holds its container for the daemon's lifetime, and pause/
        // resume cycles would acquire one each time without releasing any.
        if mapped.is_ok() {
            if let Some(session_id) = req.params.get("session_id").and_then(|v| v.as_str()) {
                self.fire_plugin_session_end(session_id).await;
            }
        }
        mapped
    }

    async fn handle_session_resume(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::session::handle_session_resume(req.clone(), &self.ctx.sessions).await;
        // A resumed session must satisfy the same invariant as a created one.
        self.enforce_plugin_session_start(map_server_resp(resp), req)
            .await
    }

    async fn handle_session_resume_from_storage(
        &self,
        req: &Request,
    ) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_resume_from_storage(
            req.clone(),
            &self.ctx.sessions,
        )
        .await;
        self.enforce_plugin_session_start(map_server_resp(resp), req)
            .await
    }

    async fn handle_session_end(&self, req: &Request) -> RpcResult<serde_json::Value> {
        // Fire on_session_end Lua hooks before ending the session.
        // Plugins use this for cleanup (e.g., releasing resources, stopping
        // services) and for agent-learning extraction (session digest, entity
        // memory). The Session handed to hooks carries id + workspace — the
        // documented surface; richer metadata (kiln, agent, end reason) is
        // future session-API growth, not something this comment promises.
        let session_id = req
            .params
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !session_id.is_empty() {
            // Plugin runtime first — it's a separate VM from the per-session
            // `lua_sessions` executors below, and a plugin that acquired a
            // resource in `on_session_start` needs the matching teardown.
            self.fire_plugin_session_end(session_id).await;

            // Attachment state is not a plugin concern: session VMs can attach
            // with no plugin runtime bound at all, so releasing inside
            // `fire_plugin_session_end` leaked every session on a plugin-less
            // daemon. It also must NOT fire on pause — that would hand a
            // resumed session a fresh budget and a cleared dedup set.
            self.ctx.agents.context_attach().release(session_id);

            if let Some(state) = self.ctx.lua_sessions.get(session_id) {
                let state = state.value().clone();
                let mut state = state.lock().await;
                // Daemon-side idempotency: `lua.shutdown_session` also fires
                // these hooks. Whichever path reaches us first sets the flag;
                // the second is a no-op.
                if state.end_hooks_fired {
                    tracing::debug!(
                        session_id = %session_id,
                        "on_session_end hooks already fired; skipping"
                    );
                } else {
                    if let Err(e) = state.executor.sync_session_end_hooks() {
                        tracing::warn!(session_id = %session_id, error = %e, "Failed to sync session_end hooks");
                    }
                    if let Some(session) = state.executor.session_manager().get_current() {
                        if let Err(e) = state.executor.fire_session_end_hooks(&session).await {
                            tracing::warn!(session_id = %session_id, error = %e, "Failed to fire session_end hooks");
                        }
                    }
                    state.end_hooks_fired = true;
                }
            }
        }

        let resp = crate::server::session::handle_session_end(
            req.clone(),
            &self.ctx.sessions,
            &self.ctx.agents,
        )
        .await;
        map_server_resp(resp)
    }

    /// A fork is a live session on the parent's workspace with the parent's
    /// agent, so it owes the same invariant create and resume do: a live
    /// session is sandboxed, or it does not exist.
    ///
    /// It cannot use `enforce_plugin_session_start`. That helper reads the id
    /// from `session_id` — in the response, then the params — and for a fork
    /// `params.session_id` is the *parent's*, so it would re-fire hooks on the
    /// parent and leave the fork unclaimed while looking like it enforced.
    /// Fork reports its new id as `id`, so the extraction is its own.
    async fn handle_session_fork(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_fork(
            req.clone(),
            &self.ctx.sessions,
            &self.ctx.agents,
        )
        .await;
        let mapped = map_server_resp(resp)?;

        let Some(fork_id) = mapped.get("id").and_then(|v| v.as_str()) else {
            return Ok(mapped);
        };
        let fork_id = fork_id.to_string();

        match self
            .ctx
            .session_lifecycle
            .enforce_session_start(&fork_id)
            .await
        {
            Ok(()) => Ok(mapped),
            Err(e) => Err(RpcError {
                code: INTERNAL_ERROR,
                message: format!("session refused: {e}"),
                data: None,
            }),
        }
    }

    // ── Session utility wrappers ─────────────────────────────────────────────

    // ── Agent operation wrappers ─────────────────────────────────────────────

    async fn handle_session_configure_agent(&self, req: &Request) -> RpcResult<serde_json::Value> {
        // Refuse switching to an agent the session's isolation claim cannot
        // cover, BEFORE applying the config — after would leave the session
        // already reconfigured when the error returns. The rule itself is
        // `session_lifecycle::unenforceable_reason`, the same one the create
        // path applies: a second copy here drifted once already, answering
        // "no" to a switch that create answers "yes" to.
        if let Some(requested_type) = req
            .params
            .get("agent")
            .and_then(|a| a.get("agent_type"))
            .and_then(|t| t.as_str())
        {
            let session_id = req
                .params
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let claim = match self.isolation_registry().await {
                Some(registry) => registry.get(session_id),
                None => None,
            };
            if let Some(reason) = claim.as_ref().and_then(|claim| {
                crate::session_lifecycle::unenforceable_reason(claim, requested_type)
            }) {
                return Err(RpcError {
                    code: INTERNAL_ERROR,
                    message: format!("cannot switch this session to an external agent: {reason}"),
                    data: None,
                });
            }
        }
        let resp =
            crate::server::session::handle_session_configure_agent(req.clone(), &self.ctx.agents)
                .await;
        map_server_resp(resp)
    }

    // ── Undo RPC wrappers ────────────────────────────────────────────────

    // ── Lua RPC wrappers ─────────────────────────────────────────────────

    // SAFETY: lua.eval executes arbitrary code in the daemon's Lua VM.
    // This is safe because the daemon socket is protected by filesystem permissions
    // (same-user access only). If the daemon is ever exposed over TCP, this
    // endpoint MUST require authentication.
    async fn handle_lua_eval(&self, req: &Request) -> RpcResult<serde_json::Value> {
        use crate::rpc::params::parse_params;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            code: String,
        }

        let params: Params = parse_params(req)?;
        let loader_guard = self.ctx.plugin_loader.lock().await;
        match loader_guard.as_ref() {
            Some(loader) => match loader.eval(&params.code).await {
                Ok(result) => Ok(serde_json::json!({ "result": result })),
                Err(e) => Err(RpcError {
                    code: INTERNAL_ERROR,
                    message: e.to_string(),
                    data: None,
                }),
            },
            None => Err(RpcError {
                code: INTERNAL_ERROR,
                message: "Lua runtime not initialized".to_string(),
                data: None,
            }),
        }
    }

    /// Read from the app-config store — the same store `cru.config.get`
    /// exposes to Lua (seeded from TOML at daemon startup, merged by
    /// `cru.config.set` / `config.set`). With `key`: one top-level value
    /// (null if absent); without: the whole object.
    fn handle_config_get(&self, req: &Request) -> RpcResult<serde_json::Value> {
        use crate::rpc::params::parse_params;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            #[serde(default)]
            key: Option<String>,
        }

        let params: Params = parse_params(req)?;
        let config = crucible_lua::get_app_config();
        Ok(match params.key {
            Some(key) => {
                let value = config
                    .as_ref()
                    .and_then(|c| c.get(&key))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                serde_json::json!({ "value": value })
            }
            None => serde_json::json!({ "config": config }),
        })
    }

    /// Merge top-level values into the app-config store (same semantics as
    /// Lua's `cru.config.set`). Typed transport for `:set` forwarding — the
    /// TUI must never build Lua source from user input.
    ///
    /// The location-naming keys are stripped rather than merged. The socket has
    /// no authentication, and [`LOCATION_CONFIG_KEYS`] is the config's answer
    /// to *where the daemon acts*: `kilns` and `kiln_path` are what the kiln
    /// registry is built from, `projects` names workspace roots,
    /// `session_kiln` names where a CLI session's knowledge scope points, and
    /// `data_home`/`runtimepath`/`agent_directories` name the trees the daemon
    /// reads its own state and code from. A caller that can write them
    /// introduces or re-points an entry without ever handing a path to
    /// `KilnRegistry::register_path` — which is to say, without the floor
    /// seeing it. Changing where kilns live is a config-file edit
    /// (`cru kiln register`), not a socket call.
    fn handle_config_set(&self, req: &Request) -> RpcResult<serde_json::Value> {
        use crate::rpc::params::parse_params;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            values: serde_json::Map<String, serde_json::Value>,
        }

        let params: Params = parse_params(req)?;
        let mut values = params.values;
        let rejected: Vec<&str> = LOCATION_CONFIG_KEYS
            .into_iter()
            .filter(|key| values.remove(*key).is_some())
            .collect();
        if !rejected.is_empty() {
            tracing::warn!(
                keys = ?rejected,
                "config.set refused keys that name where the daemon acts; edit the config file instead"
            );
        }
        crucible_lua::merge_app_config(serde_json::Value::Object(values));
        Ok(serde_json::json!({ "ok": true, "rejected": rejected }))
    }

    // ── Plugin RPC wrappers ──────────────────────────────────────────────

    async fn handle_plugin_reload(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::plugins::handle_plugin_reload(req.clone(), &self.ctx.plugin_loader)
                .await;

        // A reload re-runs init.lua, so any `theme`/`ui`/`statusline` setup in
        // it has just changed. Tell attached clients, or hot reload would only
        // work at boot.
        crate::server::ui_broadcast::broadcast_style_changed(
            &self.ctx.event_tx,
            &self.ctx.agents,
            crate::server::ui_broadcast::GLOBAL,
        );

        map_server_resp(resp)
    }

    async fn handle_plugin_install(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::plugin_install::handle_plugin_install(
            req.clone(),
            &self.ctx.plugin_loader,
        )
        .await;

        // Install runs the new plugin's init.lua, which may set up
        // theme/ui/statusline and registers commands/tools clients cache —
        // same notification contract as reload.
        crate::server::ui_broadcast::broadcast_style_changed(
            &self.ctx.event_tx,
            &self.ctx.agents,
            crate::server::ui_broadcast::GLOBAL,
        );

        map_server_resp(resp)
    }

    async fn handle_plugin_remove(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::plugin_install::handle_plugin_remove(
            req.clone(),
            &self.ctx.plugin_loader,
        )
        .await;

        // Removal just unregistered commands/tools/status surface that
        // clients cache.
        crate::server::ui_broadcast::broadcast_style_changed(
            &self.ctx.event_tx,
            &self.ctx.agents,
            crate::server::ui_broadcast::GLOBAL,
        );

        map_server_resp(resp)
    }

    // ── Project RPC wrappers ────────────────────────────────────────────

    async fn handle_scm_clone(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let projects_dir = self
            .ctx
            .scm_config
            .as_ref()
            .and_then(|s| s.projects_dir.as_deref());
        let resp = crate::server::plugins::handle_scm_clone(
            req.clone(),
            &self.ctx.project_manager,
            projects_dir,
        )
        .await;
        map_server_resp(resp)
    }

    // ── Storage RPC wrappers ────────────────────────────────────────────

    // ── MCP RPC wrappers ────────────────────────────────────────────────

    async fn handle_mcp_start(&self, req: &Request) -> RpcResult<serde_json::Value> {
        // The same registry the internal agent dispatches through, so
        // `cru mcp` and an agent advertise one set of plugin tools.
        let plugin_tools = {
            let guard = self.ctx.plugin_loader.lock().await;
            guard.as_ref().map(|l| l.plugin_registry())
        };
        let resp = crate::server::platform::handle_mcp_start(
            req.clone(),
            &self.ctx.kiln,
            &self.ctx.mcp_server_manager,
            plugin_tools,
        )
        .await;
        map_server_resp(resp)
    }

    // ── Skills RPC wrappers ─────────────────────────────────────────────

    // ── Agents RPC wrappers ─────────────────────────────────────────────

    // ── Subagent RPC handlers ─────────────────────────────────────────────

    async fn handle_subagent_collect(&self, req: &Request) -> RpcResult<serde_json::Value> {
        use crate::rpc::params::parse_params;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            job_ids: Vec<String>,
            #[serde(default = "default_collect_timeout")]
            timeout_secs: f64,
        }

        fn default_collect_timeout() -> f64 {
            120.0
        }

        let p: Params = parse_params(req)?;
        let timeout = std::time::Duration::from_secs_f64(p.timeout_secs);
        let results = self.ctx.agents.collect_jobs(&p.job_ids, timeout).await;

        Ok(serde_json::json!({ "results": results }))
    }

    // ── Webhook RPC handler ─────────────────────────────────────────────

    /// Broadcasts an already-authenticated webhook delivery.
    ///
    /// Sender authentication happens at the HTTP edge (`crucible-web`'s
    /// `routes/webhook.rs`, using [`crate::webhook`]) because that is the only
    /// place the raw request bytes exist — by the time a body has been through
    /// JSON-RPC it is a decoded `String`, and a signature must cover what was
    /// actually sent. Re-checking here would be the same check written twice
    /// over weaker inputs. Callers of this method are on the daemon's Unix
    /// socket, which is the full control plane (`session.create`, shell tools):
    /// anyone who can call it can already do strictly more than inject an
    /// event, so there is nothing left for a signature to protect.
    fn handle_webhook_receive(&self, req: &Request) -> RpcResult<serde_json::Value> {
        use crate::rpc::params::parse_params;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            name: String,
            headers: serde_json::Map<String, serde_json::Value>,
            body: String,
        }

        let p: Params = parse_params(req)?;

        // Named by `event_map`, not spelled here: `server/file_event_hooks.rs`
        // resolves a Lua handler from that one table, so a name minted
        // independently at this end is a delivery no plugin can ever see. That
        // is exactly what happened — the ingress broadcast `webhook:received`
        // to nobody from the day it shipped.
        let event = crate::event_map::webhook_received(p.name, p.headers, p.body);

        // Best-effort broadcast — no subscribers is fine
        crate::event_emitter::emit_event(&self.ctx.event_tx, event);

        Ok(serde_json::json!({ "status": "ok" }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::RequestId;
    use crate::rpc::RpcContext;
    use crate::test_support::temp_session_manager;
    use std::sync::Arc;

    fn make_request(method: &str, params: serde_json::Value) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: method.to_string(),
            params,
        }
    }

    fn test_context() -> Arc<RpcContext> {
        test_context_with_kilns(&[])
    }

    /// A context whose kiln registry also resolves `extra`.
    ///
    /// `session.create` and the scope handlers take NAMES now, so a test that
    /// needs a kiln with a particular `kiln.toml` — a classification, say — has
    /// to bind the name to that directory here, or the handler refuses the
    /// request before the behaviour under test ever runs.
    fn test_context_with_kilns(extra: &[(&str, &std::path::Path)]) -> Arc<RpcContext> {
        use crate::agent_manager::{AgentManager, AgentManagerParams};
        use crate::background_manager::BackgroundJobManager;

        use crate::kiln_manager::KilnManager;
        use crate::project_manager::ProjectManager;
        use tokio::sync::broadcast;

        let (event_tx, _) = broadcast::channel(16);
        let kiln_manager = Arc::new(KilnManager::new());
        let session_manager = crate::test_support::temp_session_manager_with_kilns(extra);
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx.clone()));
        let agent_manager = Arc::new(AgentManager::new(AgentManagerParams {
            kiln_manager: kiln_manager.clone(),
            session_manager: session_manager.clone(),
            background_manager,
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            context_config: None,
            permission_config: None,
            plugin_loader: None,
        }));

        Arc::new(RpcContext::for_test(
            kiln_manager,
            session_manager,
            agent_manager,
            Arc::new(ProjectManager::new(std::path::PathBuf::from(
                "/tmp/projects.json",
            ))),
            event_tx,
            None,
            std::path::PathBuf::from("/tmp"),
        ))
    }

    /// Context with a real plugin loader, so tests can plant isolation claims.
    fn test_context_with_loader() -> Arc<RpcContext> {
        let ctx = test_context();
        let loader =
            crate::daemon_plugins::DaemonPluginLoader::new(std::collections::HashMap::new())
                .expect("loader");
        *ctx.plugin_loader.try_lock().expect("fresh mutex") = Some(loader);
        ctx
    }

    /// Two concurrent `session.end` requests must fire plugin `on_session_end`
    /// exactly once. Session existence was the only guard, but end hooks run
    /// BEFORE `end_session` removes the session, so both requests passed it —
    /// a check, not a claim. Plugins are promised they need not be idempotent,
    /// and a double `oci` teardown removes an already-removed container.
    ///
    /// Observed through the isolation release the teardown performs: re-plant
    /// the claim, fire again, and a short-circuited second run leaves it alone.
    #[tokio::test]
    async fn concurrent_session_end_fires_plugin_end_hooks_exactly_once() {
        use crucible_core::session::SessionType;
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let kiln_root = tempdir.path().to_path_buf();
        let ctx = test_context_with_loader();

        let session = ctx
            .sessions
            .create_session(
                SessionType::Chat,
                vec![crate::test_support::kiln_name("kiln")],
                Some(kiln_root.clone()),
                None,
            )
            .await
            .expect("create session");
        let session_id = session.id.clone();

        let dispatcher = RpcDispatcher::new(ctx);

        async fn plant(dispatcher: &RpcDispatcher, session_id: &str) {
            let guard = dispatcher.ctx.plugin_loader.lock().await;
            guard.as_ref().unwrap().isolation().claim(
                session_id,
                crucible_lua::IsolationClaim {
                    plugin: "oci".to_string(),
                    exempt: Default::default(),
                    exec: Default::default(),
                },
            );
        }
        async fn claim_present(dispatcher: &RpcDispatcher, session_id: &str) -> bool {
            let guard = dispatcher.ctx.plugin_loader.lock().await;
            guard
                .as_ref()
                .unwrap()
                .isolation()
                .get(session_id)
                .is_some()
        }

        plant(&dispatcher, &session_id).await;
        dispatcher.fire_plugin_session_end(&session_id).await;
        assert!(
            !claim_present(&dispatcher, &session_id).await,
            "first teardown must release the isolation claim"
        );

        // A second `session.end` racing the first: the session is still in the
        // manager, so the existence guard passes again.
        plant(&dispatcher, &session_id).await;
        dispatcher.fire_plugin_session_end(&session_id).await;
        assert!(
            claim_present(&dispatcher, &session_id).await,
            "second teardown must short-circuit, not fire hooks a second time"
        );
    }

    /// H2: isolation is enforceable only for internal agents — an external
    /// (ACP) agent executes tools in its own process, where pre_tool_call
    /// denials arrive after the fact. A claimed session must refuse the
    /// switch, BEFORE the config is applied.
    #[tokio::test]
    async fn switching_an_isolated_session_to_an_external_agent_is_refused() {
        let ctx = test_context_with_loader();
        {
            let guard = ctx.plugin_loader.lock().await;
            guard.as_ref().unwrap().isolation().claim(
                "iso-1",
                crucible_lua::IsolationClaim {
                    plugin: "oci".to_string(),
                    exempt: Default::default(),
                    exec: Default::default(),
                },
            );
        }
        let dispatcher = RpcDispatcher::new(ctx);

        let req = make_request(
            "session.configure_agent",
            serde_json::json!({
                "session_id": "iso-1",
                "agent": { "agent_type": "acp" },
            }),
        );
        let resp = dispatcher.dispatch(ClientId::new(), req).await;
        let err = resp.error.expect("switch must be refused");
        assert!(
            err.message.contains("cannot switch") && err.message.contains("oci"),
            "refusal must name the claiming plugin: {}",
            err.message
        );
    }

    /// The workspace axis is fail-closed at create.
    ///
    /// A session that quietly ran against the main checkout when a worktree was
    /// asked for is the workspace-axis version of one that quietly ran on the
    /// host when a container was asked for — and worse, because the agent then
    /// commits to a branch nobody expected it on. The refusal has to name the
    /// target, or the caller cannot tell this apart from an ordinary failure.
    #[tokio::test]
    async fn a_workspace_target_no_plugin_provides_refuses_the_session() {
        let dispatcher = RpcDispatcher::new(test_context_with_loader());
        let req = make_request(
            "session.create",
            serde_json::json!({
                "type": "chat",
                "workspace": "/repo",
                "workspace_target": "worktree:feat/x",
            }),
        );
        let resp = dispatcher.dispatch(ClientId::new(), req).await;
        let err = resp
            .error
            .expect("an unresolvable target must refuse the create");
        assert!(
            err.message.contains("worktree:feat/x"),
            "the refusal must name the target that could not be resolved: {}",
            err.message
        );
    }

    /// And the ordinary case is untouched: no `workspace_target`, no resolution
    /// step, no new way for create to fail.
    #[tokio::test]
    async fn a_create_without_a_workspace_target_is_not_touched_by_resolution() {
        let dispatcher = RpcDispatcher::new(test_context_with_loader());
        let req = make_request(
            "session.create",
            serde_json::json!({ "type": "chat", "workspace": "/repo" }),
        );
        let resp = dispatcher.dispatch(ClientId::new(), req).await;
        if let Some(err) = resp.error {
            assert!(
                !err.message.contains("workspace target"),
                "a create that asked for no target must not meet the resolver: {}",
                err.message
            );
        }
    }

    /// The mirror case: with no isolation claim, the external-agent switch
    /// proceeds to the normal handler (which fails on the missing session —
    /// the point is it is NOT the isolation refusal).
    #[tokio::test]
    async fn switching_an_unclaimed_session_to_an_external_agent_is_not_blocked_by_isolation() {
        let dispatcher = RpcDispatcher::new(test_context_with_loader());
        let req = make_request(
            "session.configure_agent",
            serde_json::json!({
                "session_id": "no-claim",
                "agent": { "agent_type": "acp" },
            }),
        );
        let resp = dispatcher.dispatch(ClientId::new(), req).await;
        if let Some(err) = resp.error {
            assert!(
                !err.message.contains("cannot switch"),
                "unclaimed session must not hit the isolation guard: {}",
                err.message
            );
        }
    }

    /// `session.configure_agent` is gated by the attached kilns' data
    /// classification, the same way `session.switch_model` is.
    ///
    /// Without it, create-time trust gating is bypassable in two steps: create
    /// on a provider the kiln clears, then reconfigure onto one it does not and
    /// keep the kiln. The refusal must arrive as `INVALID_PARAMS` — it is the
    /// caller's request that is wrong, not the daemon.
    #[tokio::test]
    async fn configure_agent_over_rpc_is_refused_for_an_untrusted_attached_kiln() {
        let tmp = tempfile::TempDir::new().unwrap();
        let workspace = tmp.path().join("ws");
        let kiln = workspace.join("notes");
        std::fs::create_dir_all(&kiln).unwrap();
        std::fs::create_dir_all(workspace.join(".crucible")).unwrap();
        std::fs::write(
            workspace.join(".crucible").join("project.toml"),
            "[[kilns]]\npath = \"./notes\"\ndata_classification = \"confidential\"\n",
        )
        .unwrap();

        let ctx = test_context_with_kilns(&[("notes", &kiln)]);
        let session = ctx
            .sessions
            .create_session(
                crucible_core::session::SessionType::Chat,
                vec![crate::test_support::kiln_name("notes")],
                None,
                None,
            )
            .await
            .unwrap();
        let dispatcher = RpcDispatcher::new(ctx.clone());

        // `test_context` carries no llm_config, so any provider resolves to
        // Cloud — below the Local a Confidential kiln requires.
        let req = make_request(
            "session.configure_agent",
            serde_json::json!({
                "session_id": session.id,
                "agent": {
                    "agent_type": "internal",
                    "provider": "ollama",
                    "model": "llama3.2",
                    "system_prompt": "",
                },
            }),
        );
        let resp = dispatcher.dispatch(ClientId::new(), req).await;
        let err = resp.error.expect("the configure must be refused");
        assert_eq!(err.code, INVALID_PARAMS);
        assert!(
            err.message.contains("insufficient for the attached kiln"),
            "got: {}",
            err.message
        );
        assert!(ctx
            .sessions
            .get_session(&session.id)
            .unwrap()
            .agent
            .is_none());
    }

    /// The third case, and the one that separates "external" from
    /// "unenforceable": a claim carrying an exec prefix launches the agent
    /// process inside the sandbox, so its tools are confined by where it runs.
    /// Create allows such a session, so the switch must too — one rule, one
    /// answer, whichever door the user comes through.
    #[tokio::test]
    async fn switching_to_an_external_agent_the_sandbox_can_launch_is_allowed() {
        let ctx = test_context_with_loader();
        {
            let guard = ctx.plugin_loader.lock().await;
            guard.as_ref().unwrap().isolation().claim(
                "iso-launchable",
                crucible_lua::IsolationClaim {
                    plugin: "oci".to_string(),
                    exempt: Default::default(),
                    exec: crucible_lua::SandboxExec {
                        prefix: ["podman", "exec", "-i"]
                            .iter()
                            .map(|s| s.to_string())
                            .collect(),
                        env: crucible_lua::SandboxEnv::Flag("-e".to_string()),
                        suffix: vec!["crucible-iso-launchable".to_string()],
                    },
                },
            );
        }
        let dispatcher = RpcDispatcher::new(ctx);

        let req = make_request(
            "session.configure_agent",
            serde_json::json!({
                "session_id": "iso-launchable",
                "agent": { "agent_type": "acp" },
            }),
        );
        let resp = dispatcher.dispatch(ClientId::new(), req).await;
        if let Some(err) = resp.error {
            assert!(
                !err.message.contains("cannot switch"),
                "a claim that can launch into the sandbox must not be refused: {}",
                err.message
            );
        }
    }

    /// The create-time half of the same invariant, tested through the
    /// dispatcher's own check: a claimed session whose agent is external is
    /// reported unenforceable; internal or unclaimed sessions are not.
    #[tokio::test]
    async fn isolation_claim_on_an_external_agent_session_is_unenforceable() {
        use crucible_core::session::{SessionAgent, SessionType};

        let ctx = test_context_with_loader();
        let _kiln = tempfile::tempdir().expect("kiln tempdir");
        let session = ctx
            .sessions
            .create_session(
                SessionType::Chat,
                vec![crate::test_support::kiln_name("kiln")],
                None,
                None,
            )
            .await
            .expect("create session");
        let agent: SessionAgent = serde_json::from_value(serde_json::json!({
            "agent_type": "acp",
            "provider": "ollama",
            "model": "test-model",
            "system_prompt": "",
        }))
        .expect("minimal agent config");
        ctx.agents
            .configure_agent(&session.id, agent)
            .await
            .expect("configure agent");
        {
            let guard = ctx.plugin_loader.lock().await;
            guard.as_ref().unwrap().isolation().claim(
                &session.id,
                crucible_lua::IsolationClaim {
                    plugin: "oci".to_string(),
                    exempt: Default::default(),
                    exec: Default::default(),
                },
            );
        }
        let dispatcher = RpcDispatcher::new(ctx);

        let reason = dispatcher
            .ctx
            .session_lifecycle
            .unenforceable_isolation(&session.id)
            .await
            .expect("an ACP-backed claimed session must be reported unenforceable");
        assert!(reason.contains("oci") && reason.contains("acp"), "{reason}");

        // No claim → enforceable regardless of agent type.
        assert!(
            dispatcher
                .ctx
                .session_lifecycle
                .unenforceable_isolation("other")
                .await
                .is_none(),
            "sessions without a claim must be unaffected"
        );
    }

    /// A context whose loader holds a plugin that claims isolation on start.
    ///
    /// Lets a test observe whether a code path fired plugin start hooks at all:
    /// a session that went through them has a claim, one that skipped them does
    /// not — which is exactly the difference between sandboxed and not.
    async fn test_context_claiming_isolation(dir: &std::path::Path) -> Arc<RpcContext> {
        const CLAIMS_ISOLATION: &str = r#"
crucible.on_session_start(function(session)
  crucible.require_isolation{ session = session.id, plugin = "sandbox" }
end, { required = true })
return { name = "sandbox", version = "0.1.0", description = "test isolation claimer" }
"#;
        let root = dir.join("plugins");
        let plugin = root.join("sandbox");
        std::fs::create_dir_all(&plugin).expect("plugin dir");
        std::fs::write(
            plugin.join("plugin.yaml"),
            "name: sandbox\nversion: \"0.1.0\"\ndescription: test isolation claimer\n",
        )
        .expect("plugin.yaml");
        std::fs::write(plugin.join("init.lua"), CLAIMS_ISOLATION).expect("init.lua");

        let ctx = test_context();
        let mut loader =
            crate::daemon_plugins::DaemonPluginLoader::new(std::collections::HashMap::new())
                .expect("loader");
        loader
            .load_plugins(&[(root, crucible_lua::PluginSource::EnvPath)])
            .await
            .expect("load plugins");
        *ctx.plugin_loader.try_lock().expect("fresh mutex") = Some(loader);
        ctx
    }

    /// `session.fork` produces a live session on the parent's workspace, so it
    /// owes the same invariant `create` and `resume` do: a live session is
    /// sandboxed, or it does not exist.
    ///
    /// It did not. `handle_session_fork` returned the handler's response
    /// directly, never calling `enforce_session_start`, and it does not go
    /// through `create_child_session` either — so neither the RPC path's
    /// enforcement nor `DelegationService`'s applied. Forking a sandboxed
    /// session yielded a fully unclaimed one running every tool on the host,
    /// which is the same escape delegated children had.
    #[tokio::test]
    async fn forking_a_session_fires_plugin_start_hooks_for_the_fork() {
        use crucible_core::session::SessionType;

        let tempdir = tempfile::tempdir().expect("tempdir");
        let ctx = test_context_claiming_isolation(tempdir.path()).await;
        let kiln = tempdir.path().join("kiln");
        std::fs::create_dir_all(&kiln).expect("kiln");

        let parent = ctx
            .sessions
            .create_session(
                SessionType::Chat,
                vec![crate::test_support::kiln_name("kiln")],
                None,
                None,
            )
            .await
            .expect("create parent");

        let dispatcher = RpcDispatcher::new(ctx);
        dispatcher
            .ctx
            .session_lifecycle
            .enforce_session_start(&parent.id)
            .await
            .expect("parent session start");

        let registry = dispatcher
            .ctx
            .session_lifecycle
            .isolation_registry()
            .await
            .expect("plugin isolation registry");
        assert!(
            registry.get(&parent.id).is_some(),
            "the parent must be sandboxed or this test asserts nothing"
        );

        let resp = dispatcher
            .dispatch(
                ClientId::new(),
                make_request(
                    "session.fork",
                    serde_json::json!({ "session_id": parent.id }),
                ),
            )
            .await;

        // Either outcome is safe; a live unclaimed fork is not.
        let Some(result) = resp.result else {
            return; // refused outright
        };
        // `id`, not `session_id` — and `params.session_id` is the PARENT's, so
        // the shared wrapper would enforce on the parent and leave the fork
        // unclaimed. That trap is why this path needs its own id extraction.
        let fork_id = result
            .get("id")
            .and_then(|v| v.as_str())
            .expect("a successful fork must report its session id");

        assert!(
            registry.get(fork_id).is_some(),
            "fork {fork_id} of sandboxed session {} has no isolation claim: it \
             runs on the parent's workspace with the parent's agent, so an \
             unclaimed fork is an unsandboxed one",
            parent.id
        );
    }

    #[test]
    fn methods_list_includes_core_methods() {
        assert!(METHODS.contains(&"ping"));
        assert!(METHODS.contains(&"daemon.capabilities"));
        assert!(METHODS.contains(&"session.subscribe"));
        assert!(METHODS.contains(&"session.set_thinking_budget"));
        assert!(METHODS.contains(&"session.cache_stats"));
        assert!(METHODS.contains(&"subagent.collect"));
    }

    #[test]
    fn methods_has_no_duplicates() {
        let unique: std::collections::HashSet<_> = METHODS.iter().collect();
        assert_eq!(unique.len(), METHODS.len(), "duplicate entry in METHODS");
    }

    /// `ALL` is hand-written; the compiler does not check it. `EnumIter` walks
    /// what the compiler *does* know, so a variant added without an `ALL` entry
    /// fails here rather than quietly dropping out of `daemon.capabilities`.
    #[test]
    fn every_rpc_method_variant_is_listed() {
        use strum::IntoEnumIterator;
        let listed: Vec<RpcMethod> = RpcMethod::ALL.to_vec();
        let known: Vec<RpcMethod> = RpcMethod::iter().collect();
        assert_eq!(listed, known, "RpcMethod::ALL is missing a variant");
    }

    /// `METHODS` is what `daemon.capabilities` advertises and
    /// [`RpcMethod::parse`] is what the dispatcher resolves; both come from the
    /// one `rpc_methods!` table, and this pins that they still agree.
    #[test]
    fn every_advertised_method_resolves() {
        assert_eq!(METHODS.len(), RpcMethod::ALL.len());
        for name in METHODS {
            let parsed = RpcMethod::parse(name)
                .unwrap_or_else(|| panic!("`{name}` is advertised but does not resolve"));
            assert_eq!(parsed.as_str(), *name);
        }
    }

    #[test]
    fn an_unknown_method_does_not_resolve() {
        assert_eq!(RpcMethod::parse("session.no_such_method"), None);
        assert_eq!(RpcMethod::parse(""), None);
    }

    /// `config.set` merges into the same store `cru.config.get` reads (the
    /// crucible-lua app-config store), and `config.get` reads it back — the
    /// :set/:lua shared-store bridge.
    ///
    /// NOTE: that store is process-global. Under nextest each test gets its
    /// own process, but under plain `cargo test` (the justfile fallback)
    /// tests in this binary share it — so config tests here must use
    /// test-unique keys and never reset or read the whole store expecting
    /// exclusivity.
    #[tokio::test]
    async fn dispatch_config_set_then_get_round_trips() {
        let dispatcher = RpcDispatcher::new(test_context());

        let set_req = make_request(
            "config.set",
            serde_json::json!({ "values": { "myplugin.debug": true, "answer": 42 } }),
        );
        let resp = dispatcher.dispatch(ClientId::new(), set_req).await;
        assert!(resp.error.is_none(), "config.set failed: {:?}", resp.error);

        let get_req = make_request("config.get", serde_json::json!({ "key": "myplugin.debug" }));
        let resp = dispatcher.dispatch(ClientId::new(), get_req).await;
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap()["value"], serde_json::json!(true));

        // No key → the whole (merged) config object.
        let all_req = make_request("config.get", serde_json::json!({}));
        let resp = dispatcher.dispatch(ClientId::new(), all_req).await;
        let config = resp.result.unwrap();
        assert_eq!(config["config"]["answer"], serde_json::json!(42));
    }

    #[tokio::test]
    async fn dispatch_config_get_missing_key_returns_null() {
        let dispatcher = RpcDispatcher::new(test_context());
        let req = make_request(
            "config.get",
            serde_json::json!({ "key": "no.such.key.xyz" }),
        );
        let resp = dispatcher.dispatch(ClientId::new(), req).await;
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap()["value"], serde_json::Value::Null);
    }

    #[tokio::test]
    async fn dispatch_ping_no_socket() {
        let dispatcher = RpcDispatcher::new(test_context());
        let req = make_request("ping", serde_json::json!({}));

        let resp = dispatcher.dispatch(ClientId::new(), req).await;

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), "pong");
    }

    #[tokio::test]
    async fn dispatch_capabilities_returns_methods_list() {
        let dispatcher = RpcDispatcher::new(test_context());
        let req = make_request("daemon.capabilities", serde_json::json!({}));

        let resp = dispatcher.dispatch(ClientId::new(), req).await;

        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let methods = result["methods"].as_array().unwrap();
        assert!(methods.iter().any(|m| m == "ping"));
        assert!(methods.iter().any(|m| m == "session.set_thinking_budget"));
    }

    #[tokio::test]
    async fn dispatch_unknown_method_returns_error() {
        let dispatcher = RpcDispatcher::new(test_context());
        let req = make_request("nonexistent.method", serde_json::json!({}));

        let resp = dispatcher.dispatch(ClientId::new(), req).await;

        assert!(resp.error.is_some());
        let err = resp.error.unwrap();
        assert_eq!(err.code, METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn dispatch_subscribe_tracks_subscription() {
        let ctx = test_context();
        let dispatcher = RpcDispatcher::new(ctx);
        let client_id = ClientId::new();
        let req = make_request(
            "session.subscribe",
            serde_json::json!({
                "session_ids": ["session-123"]
            }),
        );

        let resp = dispatcher.dispatch(client_id, req).await;

        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let subscribed = result["subscribed"].as_array().unwrap();
        assert_eq!(subscribed.len(), 1);
        assert_eq!(subscribed[0], "session-123");
    }

    /// Regression: `session.end` and `lua.shutdown_session` both
    /// fire `on_session_end` hooks. The CLI chat REPL invokes both — once
    /// when the user runs `:end` and again when the REPL exits — so an
    /// `on_session_end` handler was being fired twice per session lifecycle.
    /// Non-idempotent hooks (LLM calls, file writes) would have run twice.
    ///
    /// Fix: the daemon tracks per-session `end_hooks_fired` in
    /// `LuaSessionState`. The second caller short-circuits.
    #[tokio::test]
    async fn end_then_shutdown_fires_on_session_end_hook_exactly_once() {
        use crate::server::LuaSessionState;
        use crucible_core::session::SessionType;
        use crucible_lua::{LuaExecutor, Session as LuaSession};
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let kiln_root = tempdir.path().to_path_buf();
        let ctx = test_context();

        // Create a real daemon-side session so handle_session_end can find it.
        let session = ctx
            .sessions
            .create_session(
                SessionType::Chat,
                vec![crate::test_support::kiln_name("kiln")],
                Some(kiln_root.clone()),
                None,
            )
            .await
            .expect("create session");
        let session_id = session.id.clone();

        // Build a Lua session with a hook that increments a Lua global counter.
        let mut executor = LuaExecutor::new().expect("lua executor");
        executor
            .lua()
            .load(
                r#"
                _G.test_end_hook_count = 0
                crucible.on_session_end(function(_session)
                    _G.test_end_hook_count = _G.test_end_hook_count + 1
                end)
                "#,
            )
            .exec()
            .expect("install end hook");
        executor.sync_session_end_hooks().expect("sync end hooks");

        // Bind a LuaSession into the executor's session manager so the
        // hook dispatcher has a target.
        let lua_session = LuaSession::new("chat".to_string());
        executor.session_manager().set_current(lua_session);

        let lua = executor.lua().clone();
        let state = LuaSessionState {
            executor,
            end_hooks_fired: false,
        };
        ctx.lua_sessions.insert(
            session_id.to_string(),
            Arc::new(tokio::sync::Mutex::new(state)),
        );

        let dispatcher = RpcDispatcher::new(ctx);

        // First: session.end (User reason)
        let resp1 = dispatcher
            .dispatch(
                ClientId::new(),
                make_request(
                    "session.end",
                    serde_json::json!({ "session_id": session_id }),
                ),
            )
            .await;
        assert!(
            resp1.error.is_none(),
            "session.end failed: {:?}",
            resp1.error
        );

        // Second: lua.shutdown_session (Shutdown reason) — pre-fix this
        // re-fires the hook against the same Lua session.
        let resp2 = dispatcher
            .dispatch(
                ClientId::new(),
                make_request(
                    "lua.shutdown_session",
                    serde_json::json!({ "session_id": session_id }),
                ),
            )
            .await;
        assert!(
            resp2.error.is_none(),
            "lua.shutdown_session failed: {:?}",
            resp2.error
        );

        // Read back the Lua counter. `lua.shutdown_session` removes the
        // session from `lua_sessions`, so we use the cloned Lua handle.
        let count: i64 = lua
            .globals()
            .get("test_end_hook_count")
            .expect("read counter");
        assert_eq!(
            count, 1,
            "on_session_end fired {count} times; expected exactly 1 \
             (session.end and lua.shutdown_session must not both fire)"
        );
    }

    /// The `shutdown` RPC confirms first and stops the daemon second.
    ///
    /// `Server::run` breaks its accept loop the instant the signal lands and the
    /// process exits behind it, so a handler that signals inline is racing the
    /// reply the caller is still blocked reading — `cru daemon stop` and the
    /// lifecycle e2e test both see EOF instead of their own confirmation. The
    /// handler therefore only *arms* the shutdown; the connection fires it once
    /// the confirmation is on the wire.
    #[tokio::test]
    async fn dispatching_shutdown_confirms_before_it_signals() {
        let ctx = test_context();
        let mut signal = ctx.shutdown.subscribe();
        let dispatcher = RpcDispatcher::new(ctx.clone());

        let resp = dispatcher
            .dispatch(
                ClientId::new(),
                make_request("shutdown", serde_json::json!({})),
            )
            .await;

        assert_eq!(
            resp.result.as_ref().and_then(|v| v.as_str()),
            Some("shutting down"),
            "the caller is owed a confirmation: {resp:?}"
        );
        assert!(
            matches!(
                signal.try_recv(),
                Err(tokio::sync::broadcast::error::TryRecvError::Empty)
            ),
            "shutdown was signalled from inside the handler, before the \
             confirmation could be written"
        );

        // The armed request is not lost — the connection fires it once the
        // confirmation is on the wire.
        ctx.shutdown.fire_if_armed();
        assert!(
            signal.try_recv().is_ok(),
            "the armed shutdown never reached the accept loop"
        );
    }

    /// Build a context whose ProjectManager persists to `projects_path`.
    /// Mirrors `test_context` but lets the SCM tests isolate the registry.
    fn scm_test_context(projects_path: std::path::PathBuf) -> Arc<RpcContext> {
        use crate::agent_manager::{AgentManager, AgentManagerParams};
        use crate::background_manager::BackgroundJobManager;
        use crate::kiln_manager::KilnManager;
        use crate::mcp_server::McpServerManager;
        use crate::project_manager::ProjectManager;
        use crate::subscription::SubscriptionManager;
        use dashmap::DashMap;
        use tokio::sync::broadcast;

        let (event_tx, _) = broadcast::channel(16);
        let (shutdown_tx, _) = broadcast::channel(1);
        let kiln_manager = Arc::new(KilnManager::new());
        let session_manager = temp_session_manager();
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx.clone()));
        let agent_manager = Arc::new(AgentManager::new(AgentManagerParams {
            kiln_manager: kiln_manager.clone(),
            session_manager: session_manager.clone(),
            background_manager,
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            context_config: None,
            permission_config: None,
            plugin_loader: None,
        }));

        Arc::new(RpcContext::new(
            kiln_manager,
            session_manager,
            agent_manager,
            Arc::new(SubscriptionManager::new()),
            event_tx,
            shutdown_tx,
            Arc::new(ProjectManager::new(projects_path)),
            Arc::new(DashMap::new()),
            Arc::new(tokio::sync::Mutex::new(None)),
            None,
            Arc::new(McpServerManager::new()),
            None,
            std::path::PathBuf::from("/tmp"),
            None,
            Some(crucible_core::config::ScmConfig::default()),
            Arc::new(crate::kiln_registry::KilnRegistry::empty(
                crate::kiln_registry::KilnRegistryContext::for_daemon(std::path::PathBuf::from(
                    "/tmp",
                )),
            )),
        ))
    }

    /// `scm.clone` rejects non-remote / hostile URLs at the RPC layer before
    /// git ever runs. (The clone *execution* path is covered by the scm.rs
    /// integration test, which can use a local fixture path.)
    #[tokio::test]
    async fn dispatch_scm_clone_rejects_bad_urls() {
        let tmp = tempfile::TempDir::new().unwrap();
        let ctx = scm_test_context(tmp.path().join("projects.json"));
        let dispatcher = RpcDispatcher::new(ctx);

        for bad in [
            "/home/user/local-repo",
            "file:///etc/passwd",
            "-oProxyCommand=evil",
            "owner/repo/extra",
        ] {
            let resp = dispatcher
                .dispatch(
                    ClientId::new(),
                    make_request("scm.clone", serde_json::json!({ "url": bad })),
                )
                .await;
            assert!(resp.error.is_some(), "expected rejection for {bad:?}");
            assert_eq!(
                resp.error.unwrap().code,
                crate::protocol::INVALID_PARAMS,
                "wrong error code for {bad:?}"
            );
        }
    }

    /// `config.set` is reachable from the unauthenticated socket, and the
    /// app-config store it writes is where the kiln registry's source of truth
    /// would otherwise live. Letting a caller merge `kilns` there is a way to
    /// introduce or re-point an entry — the registry's floor never sees the
    /// path, because the caller never registered one.
    ///
    /// Asserted on the store, not on the response: a handler that answered
    /// `{"ok": true}` and merged anyway would pass a response-shaped check.
    #[tokio::test]
    async fn config_set_refuses_to_write_kiln_and_project_locations() {
        let tmp = tempfile::TempDir::new().unwrap();
        let ctx = scm_test_context(tmp.path().join("projects.json"));
        let dispatcher = RpcDispatcher::new(ctx);

        let resp = dispatcher
            .dispatch(
                ClientId::new(),
                make_request(
                    "config.set",
                    serde_json::json!({ "values": {
                        "kilns": { "evil": "/" },
                        "kiln_path": "/",
                        "session_kiln": "/etc",
                        "projects": { "evil": { "path": "/" } },
                        "chat": { "model": "test-model" },
                    }}),
                ),
            )
            .await;
        assert!(resp.error.is_none(), "the allowed key must still merge");

        let stored = crucible_lua::get_app_config().unwrap_or(serde_json::Value::Null);
        for refused in ["kilns", "kiln_path", "session_kiln", "projects"] {
            assert!(
                stored.get(refused).is_none(),
                "config.set wrote '{refused}' into the app config: {stored}"
            );
        }
        assert_eq!(
            stored.pointer("/chat/model").and_then(|v| v.as_str()),
            Some("test-model"),
            "precondition: an ordinary key must still be merged, or this test \
             proves nothing about the refused ones"
        );
    }
}
