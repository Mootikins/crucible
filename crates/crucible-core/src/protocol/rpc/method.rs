//! The JSON-RPC methods of the daemon, as one closed set.
//!
//! The daemon dispatches on [`RpcMethod`], and each client names a method
//! through it, so a misspelled method does not compile. The set lives in
//! core, beside the wire types, because the server and every client share it.
//!
//! Each row also names the method's params type and its reply type. A caller
//! that uses [`crate::protocol::rpc::method::RpcMethod`] together with
//! `DaemonClient::call` (`crucible-daemon`) still chooses its own `Req`/`Resp`
//! type parameters — Rust has no way to bind one concrete type pair to one
//! enum *value* without a marker type per variant, and a marker type per
//! method is the one-struct-per-method growth this table exists to avoid.
//! What the row buys instead: every method must name a real, resolvable pair
//! (`ASSERT_ROW_TYPES_RESOLVE` below fails to compile on a typo or a private
//! type), and the pair is machine-readable — [`RpcMethod::params_type`] and
//! [`RpcMethod::reply_type`] hand the exact source text to the TS generator
//! and to a test — so the table cannot silently drift from the methods it
//! describes the way a hand-written doc comment could.
//!
//! A reply still built with `json!` (not yet a named core type) is marked
//! `serde_json::Value`, not invented a name to fill the cell. Most of these
//! sit behind a raw `&Request` handler in `crates/crucible-daemon/src/rpc/dispatch.rs`
//! that never called `typed_params`, or behind a reply type that lives in the
//! `crucible-daemon` crate — which `crucible-core` cannot name, since core is
//! the crate daemon depends on, not the other way around. Moving those reply
//! types to core is unfinished step 6/10 work, not part of this table.

/// Declare the closed set of JSON-RPC method names once, with the params and
/// reply type of each.
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
    ($( $(#[$attr:meta])* $variant:ident = $name:literal : $req:ty => $resp:ty ),* $(,)?) => {
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

            /// The row's params type, exactly as written in `rpc_methods!`
            /// (e.g. `"Scoped<NoteRef>"`). For the TS method map and for
            /// `every_row_names_its_types`; not a substitute for reading the
            /// dispatch handler.
            #[must_use]
            pub const fn params_type(self) -> &'static str {
                match self { $( Self::$variant => stringify!($req), )* }
            }

            /// The row's reply type, exactly as written in `rpc_methods!`.
            #[must_use]
            pub const fn reply_type(self) -> &'static str {
                match self { $( Self::$variant => stringify!($resp), )* }
            }
        }

        impl std::fmt::Display for RpcMethod {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        /// The names `daemon.capabilities` advertises.
        pub const METHODS: &[&str] = &[ $( $name, )* ];

        // Every row's params/reply type must resolve to a real, visible type,
        // or this fails to compile. `stringify!` above captures the row's
        // source text unconditionally — it does not need the type to exist —
        // so without this, a typo'd or private type in a row would compile
        // silently and only show up as a bad TS map entry. `Option<T>` needs
        // no `T: Default`/construction, only that `T` names something; the
        // closure is never called.
        #[allow(dead_code, clippy::type_complexity)]
        const ASSERT_ROW_TYPES_RESOLVE: fn() = || {
            $( let _: (Option<$req>, Option<$resp>) = (None, None); )*
        };

        /// Hand every row of this table to `$callback!`, once, as
        /// `Variant, "wire.name", ReqTy, RespTy;` for each row in turn.
        ///
        /// `crucible-core` cannot name `DaemonClient` (the daemon depends on
        /// core, not the reverse), so it cannot generate a typed client
        /// method itself. This is the seam: a crate that CAN name the client
        /// supplies `$callback`, a `macro_rules!` macro of its own, and this
        /// macro feeds it the row data so the generated method's `Req`/`Resp`
        /// can never drift from the row that names them — one macro expands
        /// both. See `crucible_daemon::rpc_client::client::generated` for the
        /// callback that turns this into `DaemonClient::rpc().<method>()`.
        #[macro_export]
        macro_rules! for_each_rpc_method {
            ($callback:path) => {
                $callback! {
                    $( $variant, $name, $req, $resp; )*
                }
            };
        }
    };
}

rpc_methods! {
    Ping = "ping": () => String,
    DaemonCapabilities = "daemon.capabilities": () => crucible_core::protocol::requests::DaemonCapabilities,
    Shutdown = "shutdown": () => String,
    KilnOpen = "kiln.open": crucible_core::protocol::requests::KilnOpenRequest => crucible_core::protocol::requests::KilnOpenReply,
    KilnClose = "kiln.close": crucible_core::protocol::requests::PathRequest => crucible_core::protocol::requests::StatusReply,
    KilnList = "kiln.list": () => Vec<crucible_core::protocol::requests::KilnRow>,
    KilnRegister = "kiln.register": crucible_core::protocol::requests::KilnRegisterRequest => crucible_core::protocol::requests::KilnRegisterReply,
    // Each row starts from a real record (`crucible_core::Project`, here a
    // kiln entry) or a hand-built object, then gains two keys the daemon
    // injects afterward (`origin`, plus `also_registered`/`shadows`). No
    // single struct names both the "from a record" and the "built by hand"
    // starting shapes without the Option-per-field merge this pass forbids.
    KilnRegistryList = "kiln.registry_list": () => serde_json::Value,
    KilnForget = "kiln.forget": crucible_core::protocol::requests::NameRequest => crucible_core::protocol::requests::KilnForgetReply,
    LlmRegisterProvider = "llm.register_provider": crucible_core::protocol::requests::LlmRegisterProviderRequest => crucible_core::protocol::requests::LlmRegisterProviderReply,
    SearchVectors = "search_vectors": crucible_core::protocol::requests::SearchVectorsRequest => Vec<crucible_core::protocol::requests::VectorHit>,
    SearchText = "search_text": crucible_core::protocol::requests::SearchTextRequest => Vec<crucible_core::protocol::requests::FtsResult>,
    SearchGrep = "search_grep": crucible_core::protocol::requests::GrepSearchRequest => crucible_core::protocol::requests::GrepSearchResponse,
    EmbedQuery = "embed.query": crucible_core::protocol::requests::EmbedQueryRequest => crucible_core::protocol::requests::EmbedQueryReply,
    ListNotes = "list_notes": crucible_core::protocol::requests::ListNotesRequest => Vec<crucible_core::protocol::requests::NoteListRow>,
    GetNoteByName = "get_note_by_name": crucible_core::protocol::requests::NoteRef => Option<crucible_core::protocol::requests::NoteByNameReply>,
    // Bases: one handler answers six operations from a raw `&Request`
    // (`crucible_core::bases::handle_inner` merges `req.params` with a resolved
    // `kiln` key before dispatch), so no single Req/Resp pair names a base
    // operation without a dispatch-level split this pass does not make.
    BaseList = "base.list": serde_json::Value => serde_json::Value,
    BaseViews = "base.views": serde_json::Value => serde_json::Value,
    BaseQuery = "base.query": serde_json::Value => serde_json::Value,
    BaseCreateEntry = "base.create_entry": serde_json::Value => serde_json::Value,
    BaseSetProperty = "base.set_property": serde_json::Value => serde_json::Value,
    BaseReorderGroups = "base.reorder_groups": serde_json::Value => serde_json::Value,
    GetBacklinks = "get_backlinks": crucible_core::protocol::requests::NoteRef => Option<crucible_core::protocol::requests::GetBacklinksReply>,
    KilnGraph = "kiln.graph": crucible_core::protocol::requests::KilnRef => crucible_core::protocol::requests::KilnGraphReply,
    NoteUpsert = "note.upsert": crucible_core::protocol::requests::NoteUpsertRequest => crucible_core::protocol::requests::NoteUpsertReply,
    NoteGet = "note.get": crucible_core::protocol::requests::NotePathRequest => Option<crucible_core::storage::note_store::NoteRecord>,
    NoteDelete = "note.delete": crucible_core::protocol::requests::NotePathRequest => crucible_core::protocol::requests::StatusReply,
    NoteList = "note.list": crucible_core::protocol::requests::KilnRef => Vec<crucible_core::storage::note_store::NoteRecord>,
    ProcessFile = "process_file": crucible_core::protocol::requests::ProcessFileRequest => crucible_core::protocol::requests::ProcessFileReply,
    ProcessBatch = "process_batch": crucible_core::protocol::requests::ProcessBatchRequest => crucible_core::protocol::requests::ProcessBatchReply,
    SessionCreate = "session.create": crucible_core::protocol::requests::SessionCreateRequest => crucible_core::session::SessionSummary,
    SessionList = "session.list": crucible_core::protocol::requests::SessionListRequest => crucible_core::protocol::requests::SessionListReply,
    SessionGet = "session.get": crucible_core::protocol::requests::Scoped<()> => crucible_core::session::SessionDetail,
    SessionPause = "session.pause": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionTransitionReply,
    SessionResume = "session.resume": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionTransitionReply,
    SessionResumeFromStorage = "session.resume_from_storage": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::Page> => crucible_core::protocol::requests::SessionHistoryReply,
    SessionHistory = "session.history": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::Page> => crucible_core::protocol::requests::SessionHistoryReply,
    SessionEnd = "session.end": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionEndReply,
    SessionArchive = "session.archive": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionArchiveReply,
    SessionUnarchive = "session.unarchive": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionArchiveReply,
    SessionDelete = "session.delete": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionDeleteReply,
    SessionCompact = "session.compact": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionCompactReply,
    SessionSubscribe = "session.subscribe": crucible_core::protocol::requests::SessionSubscribeRequest => crucible_core::protocol::requests::SessionSubscribeReply,
    SessionUnsubscribe = "session.unsubscribe": crucible_core::protocol::requests::SessionSubscribeRequest => crucible_core::protocol::requests::SessionUnsubscribeReply,
    SessionConfigureAgent = "session.configure_agent": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::AgentConfig> => crucible_core::protocol::requests::SessionConfigureAgentReply,
    SessionSendMessage = "session.send_message": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::MessageInput> => crucible_core::types::SendOutcome,
    SessionCancel = "session.cancel": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionCancelResponse,
    SessionClear = "session.clear": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionClearReply,
    SessionConnectKiln = "session.connect_kiln": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::NamedKiln> => crucible_core::protocol::requests::SessionScopeReply,
    SessionDisconnectKiln = "session.disconnect_kiln": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::NamedKiln> => crucible_core::protocol::requests::SessionScopeReply,
    SessionSetWorkspace = "session.set_workspace": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::WorkspaceChoice> => crucible_core::protocol::requests::SessionScopeReply,
    SessionListModels = "session.list_models": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionListModelsReply,
    SessionListModes = "session.list_modes": crucible_core::protocol::requests::Scoped<()> => crucible_core::types::mode::SessionModes,
    SessionCommands = "session.commands": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionCommandsReply,
    SessionListKnobs = "session.list_knobs": crucible_core::protocol::requests::Scoped<()> => crucible_core::types::SessionKnobSupport,
    SessionKnobSet = "session.knob.set": crucible_core::protocol::requests::Scoped<crucible_core::types::KnobValue> => crucible_core::protocol::requests::SessionKnobSetReply,
    SessionKnobGet = "session.knob.get": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::KnobRef> => crucible_core::types::KnobValue,
    SessionListAgentOptions = "session.list_agent_options": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionListAgentOptionsReply,
    SessionSetAgentOption = "session.set_agent_option": crucible_core::protocol::requests::SessionSetAgentOptionRequest => crucible_core::types::plugin_reply::PluginAck,
    SessionCacheStats = "session.cache_stats": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionCacheStatsReply,
    SessionAddNotification = "session.add_notification": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::NewNotification> => crucible_core::protocol::requests::SessionAddNotificationReply,
    SessionListNotifications = "session.list_notifications": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionListNotificationsReply,
    SessionDismissNotification = "session.dismiss_notification": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::NotificationKey> => crucible_core::protocol::requests::SessionDismissNotificationReply,
    NotificationList = "notification.list": crucible_core::protocol::requests::NotificationListRequest => crucible_core::protocol::requests::NotificationListResponse,
    NotificationDismiss = "notification.dismiss": crucible_core::protocol::requests::NotificationDismissRequest => crucible_core::protocol::requests::NotificationDismissResponse,
    SessionInteractionRespond = "session.interaction_respond": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::InteractionAnswer> => crucible_core::protocol::requests::SessionInteractionRespondReply,
    SessionPendingInteractions = "session.pending_interactions": () => crucible_core::protocol::requests::SessionPendingInteractionsReply,
    SessionSetPluginApproval = "session.set_plugin_approval": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::PluginApprovalChange> => crucible_core::protocol::requests::PluginApprovalReply,
    SessionGetPluginApproval = "session.get_plugin_approval": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::PluginRef> => crucible_core::protocol::requests::PluginApprovalReply,
    SessionListPluginApprovals = "session.list_plugin_approvals": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionListPluginApprovalsReply,
    SessionInjectContext = "session.inject_context": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::ContextInjection> => crucible_core::protocol::requests::SessionInjectContextReply,
    SessionTestInteraction = "session.test_interaction": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::TestInteraction> => crucible_core::protocol::requests::SessionTestInteractionReply,
    SessionFork = "session.fork": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::ForkPoint> => crucible_core::protocol::requests::SessionForkReply,
    SessionSetTitle = "session.set_title": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::Title> => crucible_core::protocol::requests::SessionTitleReply,
    SessionGenerateTitle = "session.generate_title": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionTitleReply,
    SessionSearch = "session.search": crucible_core::protocol::requests::SessionSearchRequest => crucible_core::session::SessionSearchResponse,
    SessionEventsAfter = "session.events_after": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::EventCursor> => Vec<crucible_core::protocol::SessionEventMessage>,
    // The wire reply is `serde_json::Value` on purpose today (`session.list_persisted`
    // answers a page of mixed session-summary shapes); see the client for the read.
    SessionListPersisted = "session.list_persisted": crucible_core::protocol::requests::SessionListPersistedRequest => serde_json::Value,
    SessionRenderMarkdown = "session.render_markdown": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::MarkdownOptions> => crucible_core::protocol::requests::SessionRenderMarkdownResponse,
    SessionExportToFile = "session.export_to_file": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::ExportOptions> => crucible_core::protocol::requests::SessionExportToFileResponse,
    SessionReplay = "session.replay": crucible_core::protocol::requests::SessionReplayRequest => crucible_core::protocol::requests::SessionReplayStartedReply,
    SessionCleanup = "session.cleanup": crucible_core::protocol::requests::SessionCleanupRequest => crucible_core::protocol::requests::SessionCleanupReply,
    // Retired: always answers `METHOD_NOT_FOUND`. No params are read, and no
    // reply value is ever built, so there is no shape to name beyond "nothing".
    SessionReindex = "session.reindex": () => (),
    SessionUndo = "session.undo": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::UndoCount> => crucible_core::protocol::requests::SessionUndoReply,
    SessionCanUndo = "session.can_undo": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionCanUndoReply,
    SessionUndoDepth = "session.undo_depth": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionUndoDepthReply,
    PluginReload = "plugin.reload": crucible_core::protocol::requests::NameRequest => crucible_core::types::plugin_reply::PluginReloadReply,
    PluginList = "plugin.list": () => crucible_core::types::plugin_reply::PluginListReply,
    PluginCommands = "plugin.commands": () => crucible_core::types::plugin_reply::PluginCommandsReply,
    PluginPublications = "plugin.publications": crucible_core::protocol::requests::PluginPublicationsRequest => crucible_core::types::plugin_reply::PluginPublicationsReply,
    SurfaceList = "surface.list": crucible_core::protocol::requests::SurfaceRequest => crucible_core::protocol::requests::SurfaceListReply,
    SurfaceGet = "surface.get": crucible_core::protocol::requests::SurfaceRequest => crucible_core::protocol::requests::SurfaceGetReply,
    PluginOptions = "plugin.options": crucible_core::protocol::requests::PluginOptionsRequest => crucible_core::types::plugin_reply::PluginOptionsReply,
    PluginOptionGet = "plugin.option_get": crucible_core::protocol::requests::PluginOptionCallRequest => crucible_core::types::plugin_reply::PluginOptionValue,
    PluginOptionSet = "plugin.option_set": crucible_core::protocol::requests::PluginOptionCallRequest => crucible_core::types::plugin_reply::PluginAck,
    PluginOptionExecute = "plugin.option_execute": crucible_core::protocol::requests::PluginOptionCallRequest => crucible_core::types::plugin_reply::PluginAck,
    SessionStatus = "session.status": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionStatusReply,
    PluginRunCommand = "plugin.run_command": crucible_core::protocol::requests::PluginRunCommandRequest => crucible_core::types::plugin_reply::PluginRunCommandReply,
    PluginInstall = "plugin.install": crucible_core::protocol::requests::PluginInstallRequest => crucible_core::types::plugin_reply::PluginInstallReply,
    PluginRemove = "plugin.remove": crucible_core::protocol::requests::PluginRemoveRequest => crucible_core::types::plugin_reply::PluginRemoveReply,
    LuaInitSession = "lua.init_session": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::LuaSessionInit> => crucible_core::protocol::requests::LuaInitSessionResponse,
    LuaShutdownSession = "lua.shutdown_session": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::LuaShutdownSessionResponse,
    LuaDiscoverPlugins = "lua.discover_plugins": crucible_core::protocol::requests::LuaDiscoverPluginsRequest => crucible_core::protocol::requests::LuaDiscoverPluginsResponse,
    LuaPluginHealth = "lua.plugin_health": crucible_core::protocol::requests::LuaPluginHealthRequest => crucible_core::protocol::requests::LuaPluginHealthResponse,
    LuaGenerateStubs = "lua.generate_stubs": crucible_core::protocol::requests::LuaGenerateStubsRequest => crucible_core::protocol::requests::LuaGenerateStubsResponse,
    LuaRunPluginTests = "lua.run_plugin_tests": crucible_core::protocol::requests::LuaRunPluginTestsRequest => crucible_core::protocol::requests::LuaRunPluginTestsResponse,
    LuaRegisterCommands = "lua.register_commands": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::LuaCommands> => crucible_core::protocol::requests::LuaRegisterCommandsReply,
    // The wire reply is `{"result": <whatever the evaluated Lua returned>}`;
    // the result is arbitrary Lua data by the nature of `eval`, not a shape
    // this table can name.
    LuaEval = "lua.eval": crucible_core::protocol::requests::LuaEvalRequest => serde_json::Value,
    // The wire reply is `{"value": <any config value>}` or `{"config": <the
    // whole tree>}`: the leaf or the store is any config value by nature.
    ConfigGet = "config.get": crucible_core::protocol::requests::ConfigLookupRequest => serde_json::Value,
    ConfigSet = "config.set": crucible_core::protocol::requests::ConfigValuesRequest => crucible_core::protocol::requests::ConfigSetReply,
    ConfigSave = "config.save": crucible_core::protocol::requests::ConfigValuesRequest => crucible_core::protocol::requests::ConfigSaveReply,
    // The wire reply embeds the leaf's own value, which is any config value
    // (`config_origin_row` in `crucible-daemon`); `config.reset`/`.pop` add
    // `outcome`/`dropped` to that same open row.
    ConfigReset = "config.reset": crucible_core::protocol::requests::ConfigKeyRequest => serde_json::Value,
    ConfigPop = "config.pop": crucible_core::protocol::requests::ConfigKeyRequest => serde_json::Value,
    ConfigUnset = "config.unset": crucible_core::protocol::requests::ConfigKeyRequest => serde_json::Value,
    // The wire reply is a leaf row (or every leaf row) each carrying the
    // leaf's own value, which is any config value.
    ConfigOrigin = "config.origin": crucible_core::protocol::requests::ConfigLookupRequest => serde_json::Value,
    ConfigEffective = "config.effective": () => serde_json::Value,
    // The wire reply is the Lua-declared app-config control tree
    // (`crucible_lua::options::app_config`), shaped by whatever options the
    // running init.lua declared — the same kind of openness `lua.eval` has.
    ConfigControls = "config.controls": () => serde_json::Value,
    // `req.params` is read by hand (`session_id`), not `typed_params`; the
    // reply is the Lua-declared theme/highlight/geometry/layout snapshot
    // (`crucible_core::rpc::ui::style_payload`), open for the same reason
    // `config.controls` is.
    UiConfig = "ui.config": crucible_core::protocol::requests::UiConfigRequest => serde_json::Value,
    UiSetTheme = "ui.set_theme": crucible_core::protocol::requests::UiSetThemeRequest => crucible_core::protocol::requests::UiSetThemeReply,
    ProjectRegister = "project.register": crucible_core::protocol::requests::ProjectRegisterRequest => crucible_core::project::Project,
    ProjectUnregister = "project.unregister": crucible_core::protocol::requests::PathRequest => crucible_core::protocol::requests::StatusReply,
    ProjectList = "project.list": () => Vec<crucible_core::project::Project>,
    ProjectGet = "project.get": crucible_core::protocol::requests::PathRequest => Option<crucible_core::project::Project>,
    ProjectOpenKilns = "project.open_kilns": crucible_core::protocol::requests::PathRequest => crucible_core::protocol::requests::ProjectOpenKilnsReply,
    // Same shape problem as `kiln.registry_list`: each row is a
    // `crucible_core::Project` OR a hand-built stand-in for an entry with no
    // record, plus two injected keys (`origin`, `kiln_names`). Typing it
    // would merge the two starting shapes into one Option-heavy struct.
    ProjectRegistryList = "project.registry_list": () => serde_json::Value,
    ScmClone = "scm.clone": crucible_core::protocol::requests::ScmCloneRequest => crucible_core::protocol::requests::ScmCloneResponse,
    FsListDir = "fs.list_dir": crucible_core::protocol::requests::FsListDirRequest => crucible_core::protocol::requests::FsListing,
    DiffGet = "diff.get": crucible_core::protocol::requests::DiffsetRef => crucible_core::diff::Diffset,
    DiffFile = "diff.file": crucible_core::protocol::requests::DiffFileRequest => crucible_core::diff::DiffFileText,
    DiffComment = "diff.comment": crucible_core::protocol::requests::DiffCommentRequest => crucible_core::protocol::requests::DiffCommentReply,
    DiffResolveComment = "diff.resolve_comment": crucible_core::protocol::requests::DiffCommentKey => crucible_core::protocol::requests::DiffResolveCommentReply,
    DiffDeleteComment = "diff.delete_comment": crucible_core::protocol::requests::DiffCommentKey => crucible_core::protocol::requests::DiffDeleteCommentReply,
    DiffComments = "diff.comments": crucible_core::protocol::requests::DiffsetRef => crucible_core::protocol::requests::DiffCommentsReply,
    ProposalList = "proposal.list": crucible_core::protocol::requests::ProposalListRequest => Vec<crucible_core::proposal::Proposal>,
    ProposalGet = "proposal.get": crucible_core::protocol::requests::ProposalIdRequest => crucible_core::proposal::Proposal,
    ProposalAccept = "proposal.accept": crucible_core::protocol::requests::ProposalAcceptRequest => crucible_core::proposal::Proposal,
    ProposalReject = "proposal.reject": crucible_core::protocol::requests::ProposalRejectRequest => crucible_core::proposal::Proposal,
    ProposalDismiss = "proposal.dismiss": crucible_core::protocol::requests::ProposalIdRequest => crucible_core::proposal::Proposal,
    ProposalResolve = "proposal.resolve": crucible_core::protocol::requests::ProposalResolveRequest => crucible_core::proposal::Proposal,
    // `file_write::read_for_roots`/`write_for_roots` answer with one of
    // several mutually exclusive shapes (`ok`, a hash mismatch with the
    // disk content, a merge result, a batch of per-path failures) chosen at
    // runtime by which branch of a multi-way retry/merge/restore ran. A
    // faithful type is a tagged union of at least five variants; this pass
    // types a reply, not a dispatch-level rewrite of the write path.
    FsRead = "fs.read": crucible_core::file_write::FileReadRequest => serde_json::Value,
    FsWrite = "fs.write": crucible_core::file_write::FileWriteRequest => serde_json::Value,
    FsMove = "fs.move": crucible_core::protocol::requests::FsMoveRequest => crucible_core::protocol::requests::FsMoveReply,
    FsMkdir = "fs.mkdir": crucible_core::protocol::requests::FsPathRequest => crucible_core::protocol::requests::FsMkdirReply,
    FsTrash = "fs.trash": crucible_core::protocol::requests::FsPathRequest => crucible_core::protocol::requests::FsTrashReply,
    NoteRename = "note.rename": crucible_core::protocol::requests::NoteRenameRequest => crucible_core::protocol::requests::NoteRenameReply,
    NoteMove = "note.move": crucible_core::protocol::requests::NoteRenameRequest => crucible_core::protocol::requests::NoteRenameReply,
    StorageVerify = "storage.verify": crucible_core::protocol::requests::KilnPathRequest => crucible_core::protocol::requests::NotImplementedReply,
    StorageCleanup = "storage.cleanup": crucible_core::protocol::requests::KilnPathRequest => crucible_core::protocol::requests::NotImplementedReply,
    StorageBackup = "storage.backup": crucible_core::protocol::requests::StorageBackupRequest => crucible_core::protocol::requests::NotImplementedReply,
    StorageRestore = "storage.restore": crucible_core::protocol::requests::StorageRestoreRequest => crucible_core::protocol::requests::NotImplementedReply,
    McpStart = "mcp.start": crucible_core::protocol::requests::McpStartRequest => crucible_core::protocol::requests::McpStartReply,
    McpStop = "mcp.stop": () => crucible_core::protocol::requests::McpStopReply,
    McpStatus = "mcp.status": () => crucible_core::protocol::requests::McpStatus,
    SkillsList = "skills.list": crucible_core::protocol::requests::SkillsListRequest => crucible_core::types::skill::SkillsReply,
    SkillsGet = "skills.get": crucible_core::protocol::requests::SkillsGetRequest => crucible_core::types::skill::SkillDetail,
    SkillsSearch = "skills.search": crucible_core::protocol::requests::SkillsSearchRequest => crucible_core::types::skill::SkillsReply,
    AgentsListProfiles = "agents.list_profiles": () => crucible_core::protocol::requests::AgentProfilesReply,
    AgentsListCards = "agents.list_cards": crucible_core::protocol::requests::AgentsListCardsRequest => crucible_core::protocol::requests::AgentCardsListReply,
    AgentsResolveProfile = "agents.resolve_profile": crucible_core::protocol::requests::NameRequest => Option<crucible_core::protocol::requests::AgentProfileResolved>,
    ModelsList = "models.list": crucible_core::protocol::requests::ListAllModelsRequest => crucible_core::protocol::requests::ModelsListReply,
    ProvidersList = "providers.list": crucible_core::protocol::requests::ListProvidersRequest => crucible_core::protocol::requests::ProvidersListReply,
    EmbeddingsModels = "embeddings.models": crucible_core::protocol::requests::EmbeddingModelsRequest => crucible_core::protocol::requests::EmbeddingCatalog,
    // The wire reply is `{"results": [...]}`; each job answers with whatever
    // shape its own tool call produced (`AgentManager::collect_jobs` answers
    // `Vec<serde_json::Value>`), which is not one shape this table can name.
    SubagentCollect = "subagent.collect": crucible_core::protocol::requests::SubagentCollectRequest => serde_json::Value,
    WebhookReceive = "webhook.receive": crucible_core::protocol::requests::WebhookReceiveRequest => crucible_core::protocol::requests::WebhookReceiveReply,
    SuggestLinks = "suggest_links": crucible_core::protocol::requests::SuggestLinksRequest => crucible_core::protocol::requests::SuggestLinksReply,
    WorkflowStart = "workflow.start": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::WorkflowSource> => crucible_core::protocol::requests::WorkflowRunReply,
    WorkflowApproveGate = "workflow.approve_gate": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::GateRef> => crucible_core::protocol::requests::WorkflowRunReply,
    WorkflowStatus = "workflow.status": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::WorkflowStatusReply,
    WorkflowCancel = "workflow.cancel": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::WorkflowCancelReply,
}

// Every knob now shares one write method, `session.knob.set`, and one read
// method, `session.knob.get`: see [`crate::types::KnobValue`]. There is no
// per-knob method to name any more, so there is nothing here for a mapping
// function to return.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methods_list_includes_core_methods() {
        assert!(METHODS.contains(&"ping"));
        assert!(METHODS.contains(&"daemon.capabilities"));
        assert!(METHODS.contains(&"session.subscribe"));
        assert!(METHODS.contains(&"session.knob.set"));
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

    /// Every knob shares one write method and one read method, and both are
    /// advertised. There is no per-knob method left for a knob to miss.
    #[test]
    fn the_knob_methods_are_advertised() {
        assert!(METHODS.contains(&RpcMethod::SessionKnobSet.as_str()));
        assert!(METHODS.contains(&RpcMethod::SessionKnobGet.as_str()));
    }

    /// `rpc_methods!`'s grammar already makes a row with no types fail to
    /// compile (`ASSERT_ROW_TYPES_RESOLVE` proves each one names a real
    /// type). This test is the runtime-visible half: every method's row text
    /// is non-empty, so a generator reading [`RpcMethod::params_type`] /
    /// [`RpcMethod::reply_type`] never emits a blank cell.
    #[test]
    fn every_row_names_its_types() {
        for method in RpcMethod::ALL {
            assert!(
                !method.params_type().is_empty(),
                "{method} has no params type"
            );
            assert!(
                !method.reply_type().is_empty(),
                "{method} has no reply type"
            );
        }
    }

    /// `schema_types.rs` is a committed, generated file: one
    /// `#[derive(utoipa::OpenApi)]` struct naming every row's params/reply
    /// type, which `crucible-web`'s `api_spec` merges into its own document
    /// (step 19 item 7 of the Simplification Plan) so a row's type gets a
    /// schema whether or not a live route names it too. A row added without
    /// regenerating that file would silently miss this gate — the derive
    /// itself only proves the *committed* list resolves, not that the list
    /// is current — so this test re-renders the same text
    /// (`type_text::render_schema_types_file`, the renderer
    /// `examples/gen_rpc_schema_types.rs` also calls) and compares it
    /// against the committed file, the way `openapi_contract.rs`'s
    /// `the_committed_openapi_json_is_current` holds `openapi.json` current
    /// against its own router.
    #[test]
    fn the_committed_schema_types_file_matches_the_rows() {
        let committed = include_str!("schema_types.rs");
        let fresh = crate::protocol::rpc::type_text::render_schema_types_file();
        assert_eq!(
            committed, fresh,
            "crates/crucible-core/src/protocol/rpc/schema_types.rs is stale; regenerate with \
             `cargo run -p crucible-core --features openapi --example gen_rpc_schema_types \
             > crates/crucible-core/src/protocol/rpc/schema_types.rs`"
        );
    }
}
