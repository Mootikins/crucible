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
    // Turns a row's `read`/`write` word into its replay-safety bool. Only
    // these two idents match, so a row that types anything else — a typo, a
    // third word — fails to compile here, the same closed-set proof every
    // other column of the row gets.
    (@safety read) => { true };
    (@safety write) => { false };

    ($( $(#[$attr:meta])* $variant:ident = $safety:ident $name:literal : $req:ty => $resp:ty ),* $(,)?) => {
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

            /// Whether a caller that lost the answer to this call may repeat
            /// it. Read straight off the row's own `read`/`write` word, so a
            /// row cannot be added without the decision, and the decision
            /// cannot drift from the row the way a separate hand-written
            /// match could.
            ///
            /// `true` means a pure read: nothing it does depends on how many
            /// times it runs, so a caller whose connection dropped before the
            /// reply arrived — `crates/crucible-web/src/services/daemon.rs`'s
            /// `ReconnectingDaemon`, behind `POST /api/rpc/{method}` — may
            /// reconnect and ask again rather than surfacing the drop as a
            /// failure. `false` covers every write and every read this table
            /// cannot yet prove side-effect-free: a doubled write is a bug a
            /// doubled read can never be, so an uncertain method stays
            /// `write` rather than guessing safe. `webhook.receive` is the
            /// clearest example — an incoming webhook is itself the record of
            /// an external event, and replaying it changes what the daemon
            /// believes happened.
            ///
            /// [`crucible_daemon::rpc_client::client::generated`]'s
            /// `rpc_<variant>` methods use this same word to choose
            /// `call_with_retry` (`read`) over `call` (`write`) — one row,
            /// one decision, read by both the browser's retry and the native
            /// client's.
            #[must_use]
            pub const fn is_replay_safe(self) -> bool {
                match self { $( Self::$variant => rpc_methods!(@safety $safety), )* }
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
        /// `Variant, "wire.name", read|write, ReqTy, RespTy;` for each row in
        /// turn.
        ///
        /// `crucible-core` cannot name `DaemonClient` (the daemon depends on
        /// core, not the reverse), so it cannot generate a typed client
        /// method itself. This is the seam: a crate that CAN name the client
        /// supplies `$callback`, a `macro_rules!` macro of its own, and this
        /// macro feeds it the row data so the generated method's `Req`/`Resp`
        /// can never drift from the row that names them — one macro expands
        /// both. The `read`/`write` word travels with the rest of the row, so
        /// the generated method retries by the same decision
        /// [`RpcMethod::is_replay_safe`] answers. See
        /// `crucible_daemon::rpc_client::client::generated` for the callback
        /// that turns this into `DaemonClient::rpc_<variant>(...)`.
        #[macro_export]
        macro_rules! for_each_rpc_method {
            ($callback:path) => {
                $callback! {
                    $( $variant, $name, $safety, $req, $resp; )*
                }
            };
        }
    };
}

rpc_methods! {
    Ping = read "ping": () => String,
    DaemonCapabilities = read "daemon.capabilities": () => crucible_core::protocol::requests::DaemonCapabilities,
    Shutdown = write "shutdown": () => String,
    ClientStateGet = read "client_state.get": crucible_core::protocol::requests::ClientStateKey => crucible_core::protocol::requests::ClientStateGetReply,
    ClientStateSet = write "client_state.set": crucible_core::protocol::requests::ClientStateSetRequest => crucible_core::protocol::requests::StatusReply,
    KilnOpen = write "kiln.open": crucible_core::protocol::requests::KilnOpenRequest => crucible_core::protocol::requests::KilnOpenReply,
    KilnClose = write "kiln.close": crucible_core::protocol::requests::PathRequest => crucible_core::protocol::requests::StatusReply,
    KilnList = read "kiln.list": () => Vec<crucible_core::protocol::requests::KilnRow>,
    KilnRegister = write "kiln.register": crucible_core::protocol::requests::KilnRegisterRequest => crucible_core::protocol::requests::KilnRegisterReply,
    // Each row starts from a real record (`crucible_core::Project`, here a
    // kiln entry) or a hand-built object, then gains two keys the daemon
    // injects afterward (`origin`, plus `also_registered`/`shadows`). No
    // single struct names both the "from a record" and the "built by hand"
    // starting shapes without the Option-per-field merge this pass forbids.
    KilnRegistryList = read "kiln.registry_list": () => serde_json::Value,
    KilnForget = write "kiln.forget": crucible_core::protocol::requests::NameRequest => crucible_core::protocol::requests::KilnForgetReply,
    LlmRegisterProvider = write "llm.register_provider": crucible_core::protocol::requests::LlmRegisterProviderRequest => crucible_core::protocol::requests::LlmRegisterProviderReply,
    SearchVectors = read "search_vectors": crucible_core::protocol::requests::SearchVectorsRequest => Vec<crucible_core::protocol::requests::VectorHit>,
    SearchText = read "search_text": crucible_core::protocol::requests::SearchTextRequest => Vec<crucible_core::protocol::requests::FtsResult>,
    SearchGrep = read "search_grep": crucible_core::protocol::requests::GrepSearchRequest => crucible_core::protocol::requests::GrepSearchResponse,
    EmbedQuery = read "embed.query": crucible_core::protocol::requests::EmbedQueryRequest => crucible_core::protocol::requests::EmbedQueryReply,
    ListNotes = read "list_notes": crucible_core::protocol::requests::ListNotesRequest => Vec<crucible_core::protocol::requests::NoteListRow>,
    GetNoteByName = read "get_note_by_name": crucible_core::protocol::requests::NoteRef => Option<crucible_core::protocol::requests::NoteByNameReply>,
    // Bases: one handler answers six operations from a raw `&Request`
    // (`crucible_core::bases::handle_inner` merges `req.params` with a resolved
    // `kiln` key before dispatch), so no single Req/Resp pair names a base
    // operation without a dispatch-level split this pass does not make.
    BaseList = read "base.list": serde_json::Value => serde_json::Value,
    BaseViews = read "base.views": serde_json::Value => serde_json::Value,
    BaseQuery = read "base.query": serde_json::Value => serde_json::Value,
    BaseCreateEntry = write "base.create_entry": serde_json::Value => serde_json::Value,
    BaseSetProperty = write "base.set_property": serde_json::Value => serde_json::Value,
    BaseReorderGroups = write "base.reorder_groups": serde_json::Value => serde_json::Value,
    GetBacklinks = read "get_backlinks": crucible_core::protocol::requests::NoteRef => Option<crucible_core::protocol::requests::GetBacklinksReply>,
    KilnGraph = read "kiln.graph": crucible_core::protocol::requests::KilnRef => crucible_core::protocol::requests::KilnGraphReply,
    NoteUpsert = write "note.upsert": crucible_core::protocol::requests::NoteUpsertRequest => crucible_core::protocol::requests::NoteUpsertReply,
    NoteGet = read "note.get": crucible_core::protocol::requests::NotePathRequest => Option<crucible_core::storage::note_store::NoteRecord>,
    NoteDelete = write "note.delete": crucible_core::protocol::requests::NotePathRequest => crucible_core::protocol::requests::StatusReply,
    NoteList = read "note.list": crucible_core::protocol::requests::KilnRef => Vec<crucible_core::storage::note_store::NoteRecord>,
    ProcessFile = write "process_file": crucible_core::protocol::requests::ProcessFileRequest => crucible_core::protocol::requests::ProcessFileReply,
    ProcessBatch = write "process_batch": crucible_core::protocol::requests::ProcessBatchRequest => crucible_core::protocol::requests::ProcessBatchReply,
    SessionCreate = write "session.create": crucible_core::protocol::requests::SessionCreateRequest => crucible_core::session::SessionSummary,
    SessionList = read "session.list": crucible_core::protocol::requests::SessionListRequest => crucible_core::protocol::requests::SessionListReply,
    SessionGet = read "session.get": crucible_core::protocol::requests::Scoped<()> => crucible_core::session::SessionDetail,
    SessionPause = write "session.pause": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionTransitionReply,
    SessionResume = write "session.resume": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionTransitionReply,
    SessionResumeFromStorage = write "session.resume_from_storage": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::Page> => crucible_core::protocol::requests::SessionHistoryReply,
    SessionHistory = read "session.history": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::Page> => crucible_core::protocol::requests::SessionHistoryReply,
    SessionEnd = write "session.end": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionEndReply,
    SessionArchive = write "session.archive": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionArchiveReply,
    SessionUnarchive = write "session.unarchive": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionArchiveReply,
    SessionDelete = write "session.delete": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionDeleteReply,
    SessionCompact = write "session.compact": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionCompactReply,
    SessionSubscribe = write "session.subscribe": crucible_core::protocol::requests::SessionSubscribeRequest => crucible_core::protocol::requests::SessionSubscribeReply,
    SessionUnsubscribe = write "session.unsubscribe": crucible_core::protocol::requests::SessionSubscribeRequest => crucible_core::protocol::requests::SessionUnsubscribeReply,
    SessionConfigureAgent = write "session.configure_agent": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::AgentConfig> => crucible_core::protocol::requests::SessionConfigureAgentReply,
    SessionSendMessage = write "session.send_message": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::MessageInput> => crucible_core::types::SendOutcome,
    SessionCancel = write "session.cancel": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionCancelResponse,
    SessionClear = write "session.clear": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionClearReply,
    SessionConnectKiln = write "session.connect_kiln": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::NamedKiln> => crucible_core::protocol::requests::SessionScopeReply,
    SessionDisconnectKiln = write "session.disconnect_kiln": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::NamedKiln> => crucible_core::protocol::requests::SessionScopeReply,
    SessionSetWorkspace = write "session.set_workspace": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::WorkspaceChoice> => crucible_core::protocol::requests::SessionScopeReply,
    SessionListModels = read "session.list_models": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionListModelsReply,
    SessionListModes = read "session.list_modes": crucible_core::protocol::requests::Scoped<()> => crucible_core::types::mode::SessionModes,
    SessionCommands = read "session.commands": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionCommandsReply,
    SessionListKnobs = read "session.list_knobs": crucible_core::protocol::requests::Scoped<()> => crucible_core::types::SessionKnobSupport,
    SessionKnobSet = write "session.knob.set": crucible_core::protocol::requests::Scoped<crucible_core::types::KnobValue> => crucible_core::protocol::requests::SessionKnobSetReply,
    SessionKnobGet = read "session.knob.get": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::KnobRef> => crucible_core::types::KnobValue,
    SessionListAgentOptions = read "session.list_agent_options": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionListAgentOptionsReply,
    SessionSetAgentOption = write "session.set_agent_option": crucible_core::protocol::requests::SessionSetAgentOptionRequest => crucible_core::types::plugin_reply::PluginAck,
    SessionCacheStats = read "session.cache_stats": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionCacheStatsReply,
    SessionAddNotification = write "session.add_notification": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::NewNotification> => crucible_core::protocol::requests::SessionAddNotificationReply,
    SessionListNotifications = read "session.list_notifications": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionListNotificationsReply,
    SessionDismissNotification = write "session.dismiss_notification": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::NotificationKey> => crucible_core::protocol::requests::SessionDismissNotificationReply,
    NotificationList = read "notification.list": crucible_core::protocol::requests::NotificationListRequest => crucible_core::protocol::requests::NotificationListResponse,
    NotificationDismiss = write "notification.dismiss": crucible_core::protocol::requests::NotificationDismissRequest => crucible_core::protocol::requests::NotificationDismissResponse,
    SessionInteractionRespond = write "session.interaction_respond": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::InteractionAnswer> => crucible_core::protocol::requests::SessionInteractionRespondReply,
    SessionPendingInteractions = read "session.pending_interactions": () => crucible_core::protocol::requests::SessionPendingInteractionsReply,
    SessionSetPluginApproval = write "session.set_plugin_approval": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::PluginApprovalChange> => crucible_core::protocol::requests::PluginApprovalReply,
    SessionGetPluginApproval = read "session.get_plugin_approval": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::PluginRef> => crucible_core::protocol::requests::PluginApprovalReply,
    SessionListPluginApprovals = read "session.list_plugin_approvals": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionListPluginApprovalsReply,
    SessionInjectContext = write "session.inject_context": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::ContextInjection> => crucible_core::protocol::requests::SessionInjectContextReply,
    SessionTestInteraction = write "session.test_interaction": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::TestInteraction> => crucible_core::protocol::requests::SessionTestInteractionReply,
    SessionFork = write "session.fork": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::ForkPoint> => crucible_core::protocol::requests::SessionForkReply,
    SessionSetTitle = write "session.set_title": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::Title> => crucible_core::protocol::requests::SessionTitleReply,
    SessionGenerateTitle = write "session.generate_title": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionTitleReply,
    SessionSearch = read "session.search": crucible_core::protocol::requests::SessionSearchRequest => crucible_core::session::SessionSearchResponse,
    SessionEventsAfter = read "session.events_after": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::EventCursor> => Vec<crucible_core::protocol::SessionEventMessage>,
    // The wire reply is `serde_json::Value` on purpose today (`session.list_persisted`
    // answers a page of mixed session-summary shapes); see the client for the read.
    SessionListPersisted = read "session.list_persisted": crucible_core::protocol::requests::SessionListPersistedRequest => serde_json::Value,
    SessionRenderMarkdown = read "session.render_markdown": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::MarkdownOptions> => crucible_core::protocol::requests::SessionRenderMarkdownResponse,
    SessionExportToFile = write "session.export_to_file": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::ExportOptions> => crucible_core::protocol::requests::SessionExportToFileResponse,
    SessionReplay = write "session.replay": crucible_core::protocol::requests::SessionReplayRequest => crucible_core::protocol::requests::SessionReplayStartedReply,
    SessionCleanup = write "session.cleanup": crucible_core::protocol::requests::SessionCleanupRequest => crucible_core::protocol::requests::SessionCleanupReply,
    // Retired: always answers `METHOD_NOT_FOUND`. No params are read, and no
    // reply value is ever built, so there is no shape to name beyond "nothing".
    SessionReindex = write "session.reindex": () => (),
    SessionUndo = write "session.undo": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::UndoCount> => crucible_core::protocol::requests::SessionUndoReply,
    SessionCanUndo = read "session.can_undo": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionCanUndoReply,
    SessionUndoDepth = read "session.undo_depth": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionUndoDepthReply,
    PluginReload = write "plugin.reload": crucible_core::protocol::requests::NameRequest => crucible_core::types::plugin_reply::PluginReloadReply,
    PluginList = read "plugin.list": () => crucible_core::types::plugin_reply::PluginListReply,
    PluginCommands = read "plugin.commands": () => crucible_core::types::plugin_reply::PluginCommandsReply,
    PluginPublications = read "plugin.publications": crucible_core::protocol::requests::PluginPublicationsRequest => crucible_core::types::plugin_reply::PluginPublicationsReply,
    SurfaceList = read "surface.list": crucible_core::protocol::requests::SurfaceRequest => crucible_core::protocol::requests::SurfaceListReply,
    SurfaceGet = read "surface.get": crucible_core::protocol::requests::SurfaceRequest => crucible_core::protocol::requests::SurfaceGetReply,
    PluginOptions = read "plugin.options": crucible_core::protocol::requests::PluginOptionsRequest => crucible_core::types::plugin_reply::PluginOptionsReply,
    PluginOptionGet = read "plugin.option_get": crucible_core::protocol::requests::PluginOptionCallRequest => crucible_core::types::plugin_reply::PluginOptionValue,
    PluginOptionSet = write "plugin.option_set": crucible_core::protocol::requests::PluginOptionCallRequest => crucible_core::types::plugin_reply::PluginAck,
    PluginOptionExecute = write "plugin.option_execute": crucible_core::protocol::requests::PluginOptionCallRequest => crucible_core::types::plugin_reply::PluginAck,
    SessionStatus = read "session.status": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::SessionStatusReply,
    PluginRunCommand = write "plugin.run_command": crucible_core::protocol::requests::PluginRunCommandRequest => crucible_core::types::plugin_reply::PluginRunCommandReply,
    PluginInstall = write "plugin.install": crucible_core::protocol::requests::PluginInstallRequest => crucible_core::types::plugin_reply::PluginInstallReply,
    PluginRemove = write "plugin.remove": crucible_core::protocol::requests::PluginRemoveRequest => crucible_core::types::plugin_reply::PluginRemoveReply,
    LuaInitSession = write "lua.init_session": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::LuaSessionInit> => crucible_core::protocol::requests::LuaInitSessionResponse,
    LuaShutdownSession = write "lua.shutdown_session": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::LuaShutdownSessionResponse,
    LuaDiscoverPlugins = write "lua.discover_plugins": crucible_core::protocol::requests::LuaDiscoverPluginsRequest => crucible_core::protocol::requests::LuaDiscoverPluginsResponse,
    LuaPluginHealth = read "lua.plugin_health": crucible_core::protocol::requests::LuaPluginHealthRequest => crucible_core::protocol::requests::LuaPluginHealthResponse,
    LuaGenerateStubs = write "lua.generate_stubs": crucible_core::protocol::requests::LuaGenerateStubsRequest => crucible_core::protocol::requests::LuaGenerateStubsResponse,
    LuaRunPluginTests = write "lua.run_plugin_tests": crucible_core::protocol::requests::LuaRunPluginTestsRequest => crucible_core::protocol::requests::LuaRunPluginTestsResponse,
    LuaRegisterCommands = write "lua.register_commands": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::LuaCommands> => crucible_core::protocol::requests::LuaRegisterCommandsReply,
    // The wire reply is `{"result": <whatever the evaluated Lua returned>}`;
    // the result is arbitrary Lua data by the nature of `eval`, not a shape
    // this table can name.
    LuaEval = write "lua.eval": crucible_core::protocol::requests::LuaEvalRequest => serde_json::Value,
    // The wire reply is `{"value": <any config value>}` or `{"config": <the
    // whole tree>}`: the leaf or the store is any config value by nature.
    ConfigGet = read "config.get": crucible_core::protocol::requests::ConfigLookupRequest => serde_json::Value,
    ConfigSet = write "config.set": crucible_core::protocol::requests::ConfigValuesRequest => crucible_core::protocol::requests::ConfigSetReply,
    ConfigSave = write "config.save": crucible_core::protocol::requests::ConfigValuesRequest => crucible_core::protocol::requests::ConfigSaveReply,
    // The wire reply embeds the leaf's own value, which is any config value
    // (`config_origin_row` in `crucible-daemon`); `config.reset`/`.pop` add
    // `outcome`/`dropped` to that same open row.
    ConfigReset = write "config.reset": crucible_core::protocol::requests::ConfigKeyRequest => serde_json::Value,
    ConfigPop = write "config.pop": crucible_core::protocol::requests::ConfigKeyRequest => serde_json::Value,
    ConfigUnset = write "config.unset": crucible_core::protocol::requests::ConfigKeyRequest => serde_json::Value,
    // The wire reply is a leaf row (or every leaf row) each carrying the
    // leaf's own value, which is any config value.
    ConfigOrigin = read "config.origin": crucible_core::protocol::requests::ConfigLookupRequest => serde_json::Value,
    ConfigEffective = read "config.effective": () => serde_json::Value,
    // The wire reply is the Lua-declared app-config control tree
    // (`crucible_lua::options::app_config`), shaped by whatever options the
    // running init.lua declared — the same kind of openness `lua.eval` has.
    ConfigControls = read "config.controls": () => serde_json::Value,
    // `req.params` is read by hand (`session_id`), not `typed_params`; the
    // reply is the Lua-declared theme/highlight/geometry/layout snapshot
    // (`crucible_core::rpc::ui::style_payload`), open for the same reason
    // `config.controls` is.
    UiConfig = read "ui.config": crucible_core::protocol::requests::UiConfigRequest => serde_json::Value,
    UiSetTheme = write "ui.set_theme": crucible_core::protocol::requests::UiSetThemeRequest => crucible_core::protocol::requests::UiSetThemeReply,
    ProjectRegister = write "project.register": crucible_core::protocol::requests::ProjectRegisterRequest => crucible_core::project::Project,
    ProjectUnregister = write "project.unregister": crucible_core::protocol::requests::PathRequest => crucible_core::protocol::requests::StatusReply,
    ProjectList = read "project.list": () => Vec<crucible_core::project::Project>,
    ProjectGet = read "project.get": crucible_core::protocol::requests::PathRequest => Option<crucible_core::project::Project>,
    ProjectOpenKilns = read "project.open_kilns": crucible_core::protocol::requests::PathRequest => crucible_core::protocol::requests::ProjectOpenKilnsReply,
    // Same shape problem as `kiln.registry_list`: each row is a
    // `crucible_core::Project` OR a hand-built stand-in for an entry with no
    // record, plus two injected keys (`origin`, `kiln_names`). Typing it
    // would merge the two starting shapes into one Option-heavy struct.
    ProjectRegistryList = read "project.registry_list": () => serde_json::Value,
    ScmClone = write "scm.clone": crucible_core::protocol::requests::ScmCloneRequest => crucible_core::protocol::requests::ScmCloneResponse,
    FsListDir = read "fs.list_dir": crucible_core::protocol::requests::FsListDirRequest => crucible_core::protocol::requests::FsListing,
    DiffGet = read "diff.get": crucible_core::protocol::requests::DiffsetRef => crucible_core::diff::Diffset,
    DiffFile = read "diff.file": crucible_core::protocol::requests::DiffFileRequest => crucible_core::diff::DiffFileText,
    DiffComment = write "diff.comment": crucible_core::protocol::requests::DiffCommentRequest => crucible_core::protocol::requests::DiffCommentReply,
    DiffResolveComment = write "diff.resolve_comment": crucible_core::protocol::requests::DiffCommentKey => crucible_core::protocol::requests::DiffResolveCommentReply,
    DiffDeleteComment = write "diff.delete_comment": crucible_core::protocol::requests::DiffCommentKey => crucible_core::protocol::requests::DiffDeleteCommentReply,
    DiffComments = read "diff.comments": crucible_core::protocol::requests::DiffsetRef => crucible_core::protocol::requests::DiffCommentsReply,
    ProposalList = read "proposal.list": crucible_core::protocol::requests::ProposalListRequest => Vec<crucible_core::proposal::Proposal>,
    ProposalGet = read "proposal.get": crucible_core::protocol::requests::ProposalIdRequest => crucible_core::proposal::Proposal,
    ProposalAccept = write "proposal.accept": crucible_core::protocol::requests::ProposalAcceptRequest => crucible_core::proposal::Proposal,
    ProposalReject = write "proposal.reject": crucible_core::protocol::requests::ProposalRejectRequest => crucible_core::proposal::Proposal,
    ProposalDismiss = write "proposal.dismiss": crucible_core::protocol::requests::ProposalIdRequest => crucible_core::proposal::Proposal,
    ProposalResolve = write "proposal.resolve": crucible_core::protocol::requests::ProposalResolveRequest => crucible_core::proposal::Proposal,
    // `file_write::read_for_roots`/`write_for_roots` answer with one of
    // several mutually exclusive shapes (`ok`, a hash mismatch with the
    // disk content, a merge result, a batch of per-path failures) chosen at
    // runtime by which branch of a multi-way retry/merge/restore ran. A
    // faithful type is a tagged union of at least five variants; this pass
    // types a reply, not a dispatch-level rewrite of the write path.
    FsRead = read "fs.read": crucible_core::file_write::FileReadRequest => serde_json::Value,
    FsWrite = write "fs.write": crucible_core::file_write::FileWriteRequest => serde_json::Value,
    FsMove = write "fs.move": crucible_core::protocol::requests::FsMoveRequest => crucible_core::protocol::requests::FsMoveReply,
    FsMkdir = write "fs.mkdir": crucible_core::protocol::requests::FsPathRequest => crucible_core::protocol::requests::FsMkdirReply,
    FsTrash = write "fs.trash": crucible_core::protocol::requests::FsPathRequest => crucible_core::protocol::requests::FsTrashReply,
    NoteRename = write "note.rename": crucible_core::protocol::requests::NoteRenameRequest => crucible_core::protocol::requests::NoteRenameReply,
    NoteMove = write "note.move": crucible_core::protocol::requests::NoteRenameRequest => crucible_core::protocol::requests::NoteRenameReply,
    StorageVerify = read "storage.verify": crucible_core::protocol::requests::KilnPathRequest => crucible_core::protocol::requests::NotImplementedReply,
    StorageCleanup = write "storage.cleanup": crucible_core::protocol::requests::KilnPathRequest => crucible_core::protocol::requests::NotImplementedReply,
    StorageBackup = write "storage.backup": crucible_core::protocol::requests::StorageBackupRequest => crucible_core::protocol::requests::NotImplementedReply,
    StorageRestore = write "storage.restore": crucible_core::protocol::requests::StorageRestoreRequest => crucible_core::protocol::requests::NotImplementedReply,
    McpStart = write "mcp.start": crucible_core::protocol::requests::McpStartRequest => crucible_core::protocol::requests::McpStartReply,
    McpStop = write "mcp.stop": () => crucible_core::protocol::requests::McpStopReply,
    McpStatus = read "mcp.status": () => crucible_core::protocol::requests::McpStatus,
    SkillsList = read "skills.list": crucible_core::protocol::requests::SkillsListRequest => crucible_core::types::skill::SkillsReply,
    SkillsGet = read "skills.get": crucible_core::protocol::requests::SkillsGetRequest => crucible_core::types::skill::SkillDetail,
    SkillsSearch = read "skills.search": crucible_core::protocol::requests::SkillsSearchRequest => crucible_core::types::skill::SkillsReply,
    AgentsListProfiles = read "agents.list_profiles": () => crucible_core::protocol::requests::AgentProfilesReply,
    AgentsListCards = read "agents.list_cards": crucible_core::protocol::requests::AgentsListCardsRequest => crucible_core::protocol::requests::AgentCardsListReply,
    AgentsResolveProfile = read "agents.resolve_profile": crucible_core::protocol::requests::NameRequest => Option<crucible_core::protocol::requests::AgentProfileResolved>,
    ModelsList = read "models.list": crucible_core::protocol::requests::ListAllModelsRequest => crucible_core::protocol::requests::ModelsListReply,
    ProvidersList = read "providers.list": crucible_core::protocol::requests::ListProvidersRequest => crucible_core::protocol::requests::ProvidersListReply,
    EmbeddingsModels = read "embeddings.models": crucible_core::protocol::requests::EmbeddingModelsRequest => crucible_core::protocol::requests::EmbeddingCatalog,
    // The wire reply is `{"results": [...]}`; each job answers with whatever
    // shape its own tool call produced (`AgentManager::collect_jobs` answers
    // `Vec<serde_json::Value>`), which is not one shape this table can name.
    SubagentCollect = write "subagent.collect": crucible_core::protocol::requests::SubagentCollectRequest => serde_json::Value,
    WebhookReceive = write "webhook.receive": crucible_core::protocol::requests::WebhookReceiveRequest => crucible_core::protocol::requests::WebhookReceiveReply,
    SuggestLinks = read "suggest_links": crucible_core::protocol::requests::SuggestLinksRequest => crucible_core::protocol::requests::SuggestLinksReply,
    WorkflowStart = write "workflow.start": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::WorkflowSource> => crucible_core::protocol::requests::WorkflowRunReply,
    WorkflowApproveGate = write "workflow.approve_gate": crucible_core::protocol::requests::Scoped<crucible_core::protocol::requests::GateRef> => crucible_core::protocol::requests::WorkflowRunReply,
    WorkflowStatus = read "workflow.status": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::WorkflowStatusReply,
    WorkflowCancel = write "workflow.cancel": crucible_core::protocol::requests::Scoped<()> => crucible_core::protocol::requests::WorkflowCancelReply,
}

// Every knob now shares one write method, `session.knob.set`, and one read
// method, `session.knob.get`: see [`crate::types::KnobValue`]. There is no
// per-knob method to name any more, so there is nothing here for a mapping
// function to return.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_is_replay_safe_and_a_write_is_not() {
        assert!(RpcMethod::SessionGet.is_replay_safe());
        assert!(RpcMethod::KilnList.is_replay_safe());
        assert!(!RpcMethod::SessionSendMessage.is_replay_safe());
        assert!(!RpcMethod::WebhookReceive.is_replay_safe());
    }

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
