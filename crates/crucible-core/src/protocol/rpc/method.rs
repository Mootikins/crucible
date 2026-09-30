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
    };
}

rpc_methods! {
    Ping = "ping": () => String,
    DaemonCapabilities = "daemon.capabilities": () => crate::protocol::requests::DaemonCapabilities,
    Shutdown = "shutdown": () => serde_json::Value,
    KilnOpen = "kiln.open": crate::protocol::requests::KilnOpenRequest => serde_json::Value,
    KilnClose = "kiln.close": crate::protocol::requests::PathRequest => serde_json::Value,
    KilnList = "kiln.list": () => Vec<crate::protocol::requests::KilnRow>,
    KilnRegister = "kiln.register": crate::protocol::requests::KilnRegisterRequest => serde_json::Value,
    KilnRegistryList = "kiln.registry_list": () => serde_json::Value,
    KilnForget = "kiln.forget": crate::protocol::requests::NameRequest => serde_json::Value,
    LlmRegisterProvider = "llm.register_provider": crate::protocol::requests::LlmRegisterProviderRequest => serde_json::Value,
    SearchVectors = "search_vectors": crate::protocol::requests::SearchVectorsRequest => Vec<crate::protocol::requests::VectorHit>,
    // The wire reply is `crucible_daemon::storage::sqlite::fts::FtsResult`,
    // a daemon-local type core cannot name.
    SearchText = "search_text": crate::protocol::requests::SearchTextRequest => serde_json::Value,
    // The wire reply is `crucible_daemon::GrepSearchResponse`, daemon-local.
    SearchGrep = "search_grep": crate::protocol::requests::GrepSearchRequest => serde_json::Value,
    // The wire reply is `{"vector": [f32; N]}`; the client picks the one key
    // by hand, so the object shape as a whole has no named type.
    EmbedQuery = "embed.query": crate::protocol::requests::EmbedQueryRequest => serde_json::Value,
    ListNotes = "list_notes": crate::protocol::requests::ListNotesRequest => Vec<crate::protocol::requests::NoteListRow>,
    GetNoteByName = "get_note_by_name": crate::protocol::requests::NoteRef => Option<crate::protocol::requests::NoteByNameReply>,
    // Bases: params and reply live in `crucible_daemon::bases`, daemon-local.
    BaseList = "base.list": serde_json::Value => serde_json::Value,
    BaseViews = "base.views": serde_json::Value => serde_json::Value,
    BaseQuery = "base.query": serde_json::Value => serde_json::Value,
    BaseCreateEntry = "base.create_entry": serde_json::Value => serde_json::Value,
    BaseSetProperty = "base.set_property": serde_json::Value => serde_json::Value,
    BaseReorderGroups = "base.reorder_groups": serde_json::Value => serde_json::Value,
    GetBacklinks = "get_backlinks": crate::protocol::requests::NoteRef => Option<crate::protocol::requests::GetBacklinksReply>,
    KilnGraph = "kiln.graph": crate::protocol::requests::KilnRef => crate::protocol::requests::KilnGraphReply,
    NoteUpsert = "note.upsert": crate::protocol::requests::NoteUpsertRequest => serde_json::Value,
    NoteGet = "note.get": crate::protocol::requests::NotePathRequest => Option<crate::storage::note_store::NoteRecord>,
    NoteDelete = "note.delete": crate::protocol::requests::NotePathRequest => serde_json::Value,
    NoteList = "note.list": crate::protocol::requests::KilnRef => Vec<crate::storage::note_store::NoteRecord>,
    ProcessFile = "process_file": crate::protocol::requests::ProcessFileRequest => serde_json::Value,
    // The wire reply is `{"processed", "skipped", "errors"}`, picked apart by
    // hand into a tuple; the object as a whole has no named type.
    ProcessBatch = "process_batch": crate::protocol::requests::ProcessBatchRequest => serde_json::Value,
    SessionCreate = "session.create": crate::protocol::requests::SessionCreateRequest => crate::session::SessionSummary,
    SessionList = "session.list": crate::protocol::requests::SessionListRequest => crate::protocol::requests::SessionListReply,
    SessionGet = "session.get": crate::protocol::requests::Scoped<()> => crate::session::SessionDetail,
    SessionPause = "session.pause": crate::protocol::requests::Scoped<()> => serde_json::Value,
    SessionResume = "session.resume": crate::protocol::requests::Scoped<()> => serde_json::Value,
    SessionResumeFromStorage = "session.resume_from_storage": crate::protocol::requests::Scoped<crate::protocol::requests::Page> => serde_json::Value,
    SessionHistory = "session.history": crate::protocol::requests::Scoped<crate::protocol::requests::Page> => serde_json::Value,
    SessionEnd = "session.end": crate::protocol::requests::Scoped<()> => serde_json::Value,
    SessionArchive = "session.archive": crate::protocol::requests::Scoped<()> => serde_json::Value,
    SessionUnarchive = "session.unarchive": crate::protocol::requests::Scoped<()> => serde_json::Value,
    SessionDelete = "session.delete": crate::protocol::requests::Scoped<()> => serde_json::Value,
    SessionCompact = "session.compact": crate::protocol::requests::Scoped<()> => serde_json::Value,
    SessionSubscribe = "session.subscribe": crate::protocol::requests::SessionSubscribeRequest => serde_json::Value,
    SessionUnsubscribe = "session.unsubscribe": crate::protocol::requests::SessionSubscribeRequest => serde_json::Value,
    SessionConfigureAgent = "session.configure_agent": crate::protocol::requests::Scoped<crate::protocol::requests::AgentConfig> => serde_json::Value,
    SessionSendMessage = "session.send_message": crate::protocol::requests::Scoped<crate::protocol::requests::MessageInput> => crate::types::SendOutcome,
    SessionCancel = "session.cancel": crate::protocol::requests::Scoped<()> => crate::protocol::requests::SessionCancelResponse,
    SessionClear = "session.clear": crate::protocol::requests::Scoped<()> => serde_json::Value,
    SessionConnectKiln = "session.connect_kiln": crate::protocol::requests::Scoped<crate::protocol::requests::NamedKiln> => serde_json::Value,
    SessionDisconnectKiln = "session.disconnect_kiln": crate::protocol::requests::Scoped<crate::protocol::requests::NamedKiln> => serde_json::Value,
    SessionSetWorkspace = "session.set_workspace": crate::protocol::requests::Scoped<crate::protocol::requests::WorkspaceChoice> => serde_json::Value,
    // The wire reply is `{"models": [...]}` picked apart by hand.
    SessionListModels = "session.list_models": crate::protocol::requests::Scoped<()> => serde_json::Value,
    SessionListModes = "session.list_modes": crate::protocol::requests::Scoped<()> => crate::types::mode::SessionModes,
    SessionCommands = "session.commands": crate::protocol::requests::Scoped<()> => serde_json::Value,
    SessionListKnobs = "session.list_knobs": crate::protocol::requests::Scoped<()> => crate::types::SessionKnobSupport,
    SessionKnobSet = "session.knob.set": crate::protocol::requests::Scoped<crate::types::KnobValue> => serde_json::Value,
    SessionKnobGet = "session.knob.get": crate::protocol::requests::Scoped<crate::protocol::requests::KnobRef> => crate::types::KnobValue,
    SessionListAgentOptions = "session.list_agent_options": crate::protocol::requests::Scoped<()> => serde_json::Value,
    // Params is a function-local, lifetime-bearing struct in the client, not
    // a named core type; the reply is discarded there too.
    SessionSetAgentOption = "session.set_agent_option": serde_json::Value => serde_json::Value,
    // The wire reply is `{"session_id", "hits", "misses", ...}`, hand-built.
    SessionCacheStats = "session.cache_stats": crate::protocol::requests::Scoped<()> => serde_json::Value,
    SessionAddNotification = "session.add_notification": crate::protocol::requests::Scoped<crate::protocol::requests::NewNotification> => serde_json::Value,
    // The wire reply is `{"notifications": [...]}` picked apart by hand.
    SessionListNotifications = "session.list_notifications": crate::protocol::requests::Scoped<()> => serde_json::Value,
    // The wire reply is `{"success": bool}` picked apart by hand.
    SessionDismissNotification = "session.dismiss_notification": crate::protocol::requests::Scoped<crate::protocol::requests::NotificationKey> => serde_json::Value,
    NotificationList = "notification.list": crate::protocol::requests::NotificationListRequest => crate::protocol::requests::NotificationListResponse,
    NotificationDismiss = "notification.dismiss": crate::protocol::requests::NotificationDismissRequest => crate::protocol::requests::NotificationDismissResponse,
    SessionInteractionRespond = "session.interaction_respond": crate::protocol::requests::Scoped<crate::protocol::requests::InteractionAnswer> => serde_json::Value,
    SessionPendingInteractions = "session.pending_interactions": () => serde_json::Value,
    SessionSetPluginApproval = "session.set_plugin_approval": crate::protocol::requests::Scoped<crate::protocol::requests::PluginApprovalChange> => serde_json::Value,
    // The wire reply is `{"approval": PluginApproval}` picked apart by hand.
    SessionGetPluginApproval = "session.get_plugin_approval": crate::protocol::requests::Scoped<crate::protocol::requests::PluginRef> => serde_json::Value,
    // The wire reply is `{"approvals": {...}}` picked apart by hand.
    SessionListPluginApprovals = "session.list_plugin_approvals": crate::protocol::requests::Scoped<()> => serde_json::Value,
    SessionInjectContext = "session.inject_context": crate::protocol::requests::Scoped<crate::protocol::requests::ContextInjection> => serde_json::Value,
    SessionTestInteraction = "session.test_interaction": crate::protocol::requests::Scoped<crate::protocol::requests::TestInteraction> => serde_json::Value,
    SessionFork = "session.fork": crate::protocol::requests::Scoped<crate::protocol::requests::ForkPoint> => serde_json::Value,
    SessionSetTitle = "session.set_title": crate::protocol::requests::Scoped<crate::protocol::requests::Title> => serde_json::Value,
    SessionGenerateTitle = "session.generate_title": crate::protocol::requests::Scoped<()> => serde_json::Value,
    SessionSearch = "session.search": crate::protocol::requests::SessionSearchRequest => crate::session::SessionSearchResponse,
    SessionEventsAfter = "session.events_after": crate::protocol::requests::Scoped<crate::protocol::requests::EventCursor> => Vec<crate::protocol::SessionEventMessage>,
    // The wire reply is `serde_json::Value` on purpose today (`session.list_persisted`
    // answers a page of mixed session-summary shapes); see the client for the read.
    SessionListPersisted = "session.list_persisted": crate::protocol::requests::SessionListPersistedRequest => serde_json::Value,
    SessionRenderMarkdown = "session.render_markdown": crate::protocol::requests::Scoped<crate::protocol::requests::MarkdownOptions> => serde_json::Value,
    SessionExportToFile = "session.export_to_file": crate::protocol::requests::Scoped<crate::protocol::requests::ExportOptions> => serde_json::Value,
    SessionReplay = "session.replay": crate::protocol::requests::SessionReplayRequest => serde_json::Value,
    SessionCleanup = "session.cleanup": crate::protocol::requests::SessionCleanupRequest => serde_json::Value,
    // Retired: always answers `METHOD_NOT_FOUND`. No params are read.
    SessionReindex = "session.reindex": () => serde_json::Value,
    // The wire reply is `{"undone": [...]}` picked apart by hand.
    SessionUndo = "session.undo": crate::protocol::requests::Scoped<crate::protocol::requests::UndoCount> => serde_json::Value,
    // The wire reply is `{"session_id", "can_undo"}`, hand-built.
    SessionCanUndo = "session.can_undo": crate::protocol::requests::Scoped<()> => serde_json::Value,
    // The wire reply is `{"session_id", "undo_depth"}`, hand-built.
    SessionUndoDepth = "session.undo_depth": crate::protocol::requests::Scoped<()> => serde_json::Value,
    PluginReload = "plugin.reload": crate::protocol::requests::NameRequest => crate::types::plugin_reply::PluginReloadReply,
    PluginList = "plugin.list": () => crate::types::plugin_reply::PluginListReply,
    PluginCommands = "plugin.commands": () => crate::types::plugin_reply::PluginCommandsReply,
    PluginPublications = "plugin.publications": crate::protocol::requests::PluginPublicationsRequest => crate::types::plugin_reply::PluginPublicationsReply,
    SurfaceList = "surface.list": crate::protocol::requests::SurfaceRequest => crate::protocol::requests::SurfaceListReply,
    SurfaceGet = "surface.get": crate::protocol::requests::SurfaceRequest => crate::protocol::requests::SurfaceGetReply,
    PluginOptions = "plugin.options": crate::protocol::requests::PluginOptionsRequest => crate::types::plugin_reply::PluginOptionsReply,
    PluginOptionGet = "plugin.option_get": crate::protocol::requests::PluginOptionCallRequest => crate::types::plugin_reply::PluginOptionValue,
    PluginOptionSet = "plugin.option_set": crate::protocol::requests::PluginOptionCallRequest => crate::types::plugin_reply::PluginAck,
    PluginOptionExecute = "plugin.option_execute": crate::protocol::requests::PluginOptionCallRequest => crate::types::plugin_reply::PluginAck,
    // The wire reply is a hand-built display-item list (`SessionStatus` on
    // the daemon side is a projection, not yet a core reply type).
    SessionStatus = "session.status": crate::protocol::requests::Scoped<()> => serde_json::Value,
    PluginRunCommand = "plugin.run_command": crate::protocol::requests::PluginRunCommandRequest => crate::types::plugin_reply::PluginRunCommandReply,
    PluginInstall = "plugin.install": crate::protocol::requests::PluginInstallRequest => crate::types::plugin_reply::PluginInstallReply,
    PluginRemove = "plugin.remove": crate::protocol::requests::PluginRemoveRequest => crate::types::plugin_reply::PluginRemoveReply,
    LuaInitSession = "lua.init_session": crate::protocol::requests::Scoped<crate::protocol::requests::LuaSessionInit> => crate::protocol::requests::LuaInitSessionResponse,
    LuaShutdownSession = "lua.shutdown_session": crate::protocol::requests::Scoped<()> => crate::protocol::requests::LuaShutdownSessionResponse,
    LuaDiscoverPlugins = "lua.discover_plugins": crate::protocol::requests::LuaDiscoverPluginsRequest => crate::protocol::requests::LuaDiscoverPluginsResponse,
    LuaPluginHealth = "lua.plugin_health": crate::protocol::requests::LuaPluginHealthRequest => crate::protocol::requests::LuaPluginHealthResponse,
    LuaGenerateStubs = "lua.generate_stubs": crate::protocol::requests::LuaGenerateStubsRequest => crate::protocol::requests::LuaGenerateStubsResponse,
    LuaRunPluginTests = "lua.run_plugin_tests": crate::protocol::requests::LuaRunPluginTestsRequest => crate::protocol::requests::LuaRunPluginTestsResponse,
    LuaRegisterCommands = "lua.register_commands": crate::protocol::requests::Scoped<crate::protocol::requests::LuaCommands> => serde_json::Value,
    LuaEval = "lua.eval": crate::protocol::requests::LuaEvalRequest => serde_json::Value,
    ConfigGet = "config.get": crate::protocol::requests::ConfigLookupRequest => serde_json::Value,
    ConfigSet = "config.set": crate::protocol::requests::ConfigValuesRequest => serde_json::Value,
    ConfigSave = "config.save": crate::protocol::requests::ConfigValuesRequest => serde_json::Value,
    ConfigReset = "config.reset": crate::protocol::requests::ConfigKeyRequest => serde_json::Value,
    ConfigPop = "config.pop": crate::protocol::requests::ConfigKeyRequest => serde_json::Value,
    ConfigUnset = "config.unset": crate::protocol::requests::ConfigKeyRequest => serde_json::Value,
    ConfigOrigin = "config.origin": crate::protocol::requests::ConfigLookupRequest => serde_json::Value,
    ConfigEffective = "config.effective": () => serde_json::Value,
    ConfigControls = "config.controls": () => serde_json::Value,
    // `req.params` is read by hand (`session_id`/`name`), not `typed_params`.
    UiConfig = "ui.config": serde_json::Value => serde_json::Value,
    UiSetTheme = "ui.set_theme": serde_json::Value => serde_json::Value,
    ProjectRegister = "project.register": crate::protocol::requests::PathRequest => crate::project::Project,
    ProjectUnregister = "project.unregister": crate::protocol::requests::PathRequest => serde_json::Value,
    ProjectList = "project.list": () => Vec<crate::project::Project>,
    ProjectGet = "project.get": crate::protocol::requests::PathRequest => Option<crate::project::Project>,
    ProjectOpenKilns = "project.open_kilns": crate::protocol::requests::PathRequest => serde_json::Value,
    ProjectRegistryList = "project.registry_list": () => serde_json::Value,
    // The wire reply is `crucible_daemon::scm::ScmCloneResponse`, daemon-local.
    ScmClone = "scm.clone": crate::protocol::requests::ScmCloneRequest => serde_json::Value,
    // The wire reply is `crucible_daemon::server::fs::FsListing`, daemon-local.
    FsListDir = "fs.list_dir": crate::protocol::requests::FsListDirRequest => serde_json::Value,
    DiffGet = "diff.get": crate::protocol::requests::DiffsetRef => crate::diff::Diffset,
    DiffFile = "diff.file": crate::protocol::requests::DiffFileRequest => crate::diff::DiffFileText,
    DiffComment = "diff.comment": crate::protocol::requests::DiffCommentRequest => crate::protocol::requests::DiffCommentReply,
    DiffResolveComment = "diff.resolve_comment": crate::protocol::requests::DiffCommentKey => crate::protocol::requests::DiffResolveCommentReply,
    DiffDeleteComment = "diff.delete_comment": crate::protocol::requests::DiffCommentKey => crate::protocol::requests::DiffDeleteCommentReply,
    DiffComments = "diff.comments": crate::protocol::requests::DiffsetRef => crate::protocol::requests::DiffCommentsReply,
    ProposalList = "proposal.list": crate::protocol::requests::ProposalListRequest => Vec<crate::proposal::Proposal>,
    ProposalGet = "proposal.get": crate::protocol::requests::ProposalIdRequest => crate::proposal::Proposal,
    ProposalAccept = "proposal.accept": crate::protocol::requests::ProposalAcceptRequest => crate::proposal::Proposal,
    ProposalReject = "proposal.reject": crate::protocol::requests::ProposalRejectRequest => crate::proposal::Proposal,
    ProposalDismiss = "proposal.dismiss": crate::protocol::requests::ProposalIdRequest => crate::proposal::Proposal,
    ProposalResolve = "proposal.resolve": crate::protocol::requests::ProposalResolveRequest => crate::proposal::Proposal,
    FsRead = "fs.read": crate::file_write::FileReadRequest => serde_json::Value,
    FsWrite = "fs.write": crate::file_write::FileWriteRequest => serde_json::Value,
    // The wire reply is `crucible_daemon::server::fs::FsMoveReply`, daemon-local.
    FsMove = "fs.move": crate::protocol::requests::FsMoveRequest => serde_json::Value,
    FsMkdir = "fs.mkdir": crate::protocol::requests::FsPathRequest => serde_json::Value,
    // The wire reply is `crucible_daemon::server::fs::FsTrashReply`, daemon-local.
    FsTrash = "fs.trash": crate::protocol::requests::FsPathRequest => serde_json::Value,
    NoteRename = "note.rename": crate::protocol::requests::NoteRenameRequest => serde_json::Value,
    NoteMove = "note.move": crate::protocol::requests::NoteRenameRequest => serde_json::Value,
    StorageVerify = "storage.verify": crate::protocol::requests::KilnPathRequest => serde_json::Value,
    StorageCleanup = "storage.cleanup": crate::protocol::requests::KilnPathRequest => serde_json::Value,
    StorageBackup = "storage.backup": crate::protocol::requests::StorageBackupRequest => serde_json::Value,
    StorageRestore = "storage.restore": crate::protocol::requests::StorageRestoreRequest => serde_json::Value,
    McpStart = "mcp.start": crate::protocol::requests::McpStartRequest => serde_json::Value,
    McpStop = "mcp.stop": () => serde_json::Value,
    // The wire reply is `crucible_daemon::mcp_server::McpStatus`, daemon-local.
    McpStatus = "mcp.status": () => serde_json::Value,
    SkillsList = "skills.list": crate::protocol::requests::SkillsListRequest => crate::types::skill::SkillsReply,
    SkillsGet = "skills.get": crate::protocol::requests::SkillsGetRequest => crate::types::skill::SkillDetail,
    SkillsSearch = "skills.search": crate::protocol::requests::SkillsSearchRequest => crate::types::skill::SkillsReply,
    // The wire reply is `crucible_daemon::server::platform::AgentProfilesReply`, daemon-local.
    AgentsListProfiles = "agents.list_profiles": () => serde_json::Value,
    AgentsListCards = "agents.list_cards": crate::protocol::requests::AgentsListCardsRequest => serde_json::Value,
    AgentsResolveProfile = "agents.resolve_profile": crate::protocol::requests::NameRequest => serde_json::Value,
    // The wire reply is `{"models": [...]}` picked apart by hand.
    ModelsList = "models.list": crate::protocol::requests::ListAllModelsRequest => serde_json::Value,
    // The wire reply is `{"providers": [...]}` picked apart by hand.
    ProvidersList = "providers.list": crate::protocol::requests::ListProvidersRequest => serde_json::Value,
    EmbeddingsModels = "embeddings.models": crate::protocol::requests::EmbeddingModelsRequest => crate::protocol::requests::EmbeddingCatalog,
    SubagentCollect = "subagent.collect": crate::protocol::requests::SubagentCollectRequest => serde_json::Value,
    // The wire reply is `crucible_daemon::rpc::dispatch::WebhookReceiveReply`, daemon-local.
    WebhookReceive = "webhook.receive": crate::protocol::requests::WebhookReceiveRequest => serde_json::Value,
    // The wire reply is `crucible_daemon::tools::autolink::SuggestLinksReply`, daemon-local.
    SuggestLinks = "suggest_links": crate::protocol::requests::SuggestLinksRequest => serde_json::Value,
    WorkflowStart = "workflow.start": crate::protocol::requests::Scoped<crate::protocol::requests::WorkflowSource> => serde_json::Value,
    WorkflowApproveGate = "workflow.approve_gate": crate::protocol::requests::Scoped<crate::protocol::requests::GateRef> => serde_json::Value,
    WorkflowStatus = "workflow.status": crate::protocol::requests::Scoped<()> => serde_json::Value,
    WorkflowCancel = "workflow.cancel": crate::protocol::requests::Scoped<()> => serde_json::Value,
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
}
