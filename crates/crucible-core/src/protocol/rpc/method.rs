//! The JSON-RPC methods of the daemon, as one closed set.
//!
//! The daemon dispatches on [`RpcMethod`], and each client names a method
//! through it, so a misspelled method does not compile. The set lives in
//! core, beside the wire types, because the server and every client share it.

use crate::types::SessionKnob;

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

        impl std::fmt::Display for RpcMethod {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
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
    KilnRegister = "kiln.register",
    KilnRegistryList = "kiln.registry_list",
    KilnForget = "kiln.forget",
    LlmRegisterProvider = "llm.register_provider",
    SearchVectors = "search_vectors",
    SearchText = "search_text",
    SearchGrep = "search_grep",
    EmbedQuery = "embed.query",
    ListNotes = "list_notes",
    GetNoteByName = "get_note_by_name",
    BaseList = "base.list",
    BaseViews = "base.views",
    BaseQuery = "base.query",
    BaseCreateEntry = "base.create_entry",
    BaseSetProperty = "base.set_property",
    BaseReorderGroups = "base.reorder_groups",
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
    SessionHistory = "session.history",
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
    SessionClear = "session.clear",
    SessionSwitchModel = "session.switch_model",
    SessionConnectKiln = "session.connect_kiln",
    SessionDisconnectKiln = "session.disconnect_kiln",
    SessionSetWorkspace = "session.set_workspace",
    SessionSetMode = "session.set_mode",
    SessionGetMode = "session.get_mode",
    SessionListModels = "session.list_models",
    SessionListModes = "session.list_modes",
    SessionCommands = "session.commands",
    SessionListKnobs = "session.list_knobs",
    SessionListAgentOptions = "session.list_agent_options",
    SessionSetAgentOption = "session.set_agent_option",
    SessionCacheStats = "session.cache_stats",
    SessionAddNotification = "session.add_notification",
    SessionListNotifications = "session.list_notifications",
    SessionDismissNotification = "session.dismiss_notification",
    NotificationList = "notification.list",
    NotificationDismiss = "notification.dismiss",
    SessionInteractionRespond = "session.interaction_respond",
    SessionPendingInteractions = "session.pending_interactions",
    SessionSetContextStrategy = "session.set_context_strategy",
    SessionGetContextStrategy = "session.get_context_strategy",
    SessionSetPrecognition = "session.set_precognition",
    SessionGetPrecognition = "session.get_precognition",
    SessionSetPluginApproval = "session.set_plugin_approval",
    SessionSetPluginTurnLimit = "session.set_plugin_turn_limit",
    SessionGetPluginTurnLimit = "session.get_plugin_turn_limit",
    SessionGetPluginApproval = "session.get_plugin_approval",
    SessionListPluginApprovals = "session.list_plugin_approvals",
    SessionInjectContext = "session.inject_context",
    SessionTestInteraction = "session.test_interaction",
    SessionFork = "session.fork",
    SessionSetTitle = "session.set_title",
    SessionGenerateTitle = "session.generate_title",
    SessionSearch = "session.search",
    SessionLoadEvents = "session.load_events",
    SessionEventsAfter = "session.events_after",
    SessionListPersisted = "session.list_persisted",
    SessionRenderMarkdown = "session.render_markdown",
    SessionExportToFile = "session.export_to_file",
    SessionReplay = "session.replay",
    SessionCleanup = "session.cleanup",
    SessionReindex = "session.reindex",
    SessionUndo = "session.undo",
    SessionCanUndo = "session.can_undo",
    SessionUndoDepth = "session.undo_depth",
    PluginReload = "plugin.reload",
    PluginList = "plugin.list",
    PluginCommands = "plugin.commands",
    PluginPublications = "plugin.publications",
    SurfaceList = "surface.list",
    SurfaceGet = "surface.get",
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
    ConfigSave = "config.save",
    ConfigReset = "config.reset",
    ConfigPop = "config.pop",
    ConfigUnset = "config.unset",
    ConfigOrigin = "config.origin",
    ConfigEffective = "config.effective",
    ConfigControls = "config.controls",
    UiConfig = "ui.config",
    UiSetTheme = "ui.set_theme",
    ProjectRegister = "project.register",
    ProjectUnregister = "project.unregister",
    ProjectList = "project.list",
    ProjectGet = "project.get",
    ProjectOpenKilns = "project.open_kilns",
    ProjectRegistryList = "project.registry_list",
    ScmClone = "scm.clone",
    FsListDir = "fs.list_dir",
    DiffGet = "diff.get",
    DiffFile = "diff.file",
    DiffComment = "diff.comment",
    DiffResolveComment = "diff.resolve_comment",
    DiffDeleteComment = "diff.delete_comment",
    DiffComments = "diff.comments",
    ProposalList = "proposal.list",
    ProposalGet = "proposal.get",
    ProposalAccept = "proposal.accept",
    ProposalReject = "proposal.reject",
    ProposalDismiss = "proposal.dismiss",
    ProposalResolve = "proposal.resolve",
    FsRead = "fs.read",
    FsWrite = "fs.write",
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
    AgentsListCards = "agents.list_cards",
    AgentsResolveProfile = "agents.resolve_profile",
    ModelsList = "models.list",
    ProvidersList = "providers.list",
    EmbeddingsModels = "embeddings.models",
    SubagentCollect = "subagent.collect",
    WebhookReceive = "webhook.receive",
    SuggestLinks = "suggest_links",
    WorkflowStart = "workflow.start",
    WorkflowApproveGate = "workflow.approve_gate",
    WorkflowStatus = "workflow.status",
    WorkflowCancel = "workflow.cancel",
}

// ── Which method writes each session knob ─────────────────────────────────
//
// Which JSON-RPC method writes each session knob.
//
// A knob's write method is NOT its id with a `session.set_` prefix.
// [`SessionKnob::Model`] is `session.switch_model`, which carries no `set_`
// prefix at all. A gate that looked for the prefix therefore never saw the
// model knob, and neither front end was ever checked for it.
//
// So the mapping is declared here, once, as a total function of the knob
// identity. The `match` carries no wildcard arm, and the two module-level
// denies below close the other way out: an arm added as `_ =>
// RpcMethod::SessionSetMode` would give a new knob a wrong method with
// nobody deciding. Both lints are necessary. Clippy reports a wildcard that
// covers one remaining variant as `match_wildcard_for_single_variants` and
// only a wildcard that covers two or more as `wildcard_enum_match_arm`, and
// knobs arrive one at a time.
//
// The return type is [`RpcMethod`], not a string. A string can name a method
// the daemon does not answer; an [`RpcMethod`] cannot, because every variant
// comes from the one `rpc_methods!` table that also builds `METHODS`.
// [`RpcMethod`] has no `Default`, for the reason `AcpKnob` has none: a
// default would make "unclassified" mean something, and the only safe
// meaning is "someone decides".

// A wildcard arm here would give a new knob a wrong method with nobody
// deciding.
#[deny(clippy::wildcard_enum_match_arm)]
#[deny(clippy::match_wildcard_for_single_variants)]
/// The method a client calls to write `knob`.
///
/// Total over [`SessionKnob`]: a knob added later does not compile until
/// someone names its method.
#[must_use]
pub fn rpc_set_method(knob: SessionKnob) -> RpcMethod {
    match knob {
        // No `set_` prefix. The method predates the knob vocabulary and the
        // name is on the wire, so the gate learns the exception instead.
        SessionKnob::Model => RpcMethod::SessionSwitchModel,
        SessionKnob::Mode => RpcMethod::SessionSetMode,
        SessionKnob::ContextStrategy => RpcMethod::SessionSetContextStrategy,
        SessionKnob::Precognition => RpcMethod::SessionSetPrecognition,
        SessionKnob::PluginTurnLimit => RpcMethod::SessionSetPluginTurnLimit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn methods_list_includes_core_methods() {
        assert!(METHODS.contains(&"ping"));
        assert!(METHODS.contains(&"daemon.capabilities"));
        assert!(METHODS.contains(&"session.subscribe"));
        assert!(METHODS.contains(&"session.set_context_strategy"));
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

    /// Every knob names a method the daemon advertises.
    ///
    /// Walks `SessionKnob::ALL`. `crucible-core` derives `EnumIter` only under
    /// `cfg(test)`, so the iterator is not visible from this crate; core's own
    /// `the_all_array_lists_every_variant` proves the array holds every
    /// variant, and this test then proves each one reaches a real method. The
    /// join against `METHODS` is what `daemon.capabilities` reports, so a knob
    /// whose method is not advertised cannot be reached by any client.
    #[test]
    fn every_knob_names_an_advertised_method() {
        for knob in SessionKnob::ALL.iter().copied() {
            let method = rpc_set_method(knob).as_str();
            assert!(
                METHODS.contains(&method),
                "knob `{}` maps to `{method}`, which METHODS does not advertise",
                knob.id()
            );
        }
    }

    /// Two knobs that share a write method would make one of them
    /// unreachable, and the gates downstream would still pass.
    #[test]
    fn no_two_knobs_share_a_method() {
        let methods: Vec<RpcMethod> = SessionKnob::ALL
            .iter()
            .copied()
            .map(rpc_set_method)
            .collect();
        let unique: BTreeSet<RpcMethod> = methods.iter().copied().collect();
        assert_eq!(
            methods.len(),
            unique.len(),
            "two knobs share a write method: {methods:?}"
        );
    }

    /// The model knob is the reason this module exists. Its method carries no
    /// `set_` prefix, so a prefix scan misses it.
    #[test]
    fn the_model_knob_keeps_its_prefixless_method() {
        assert_eq!(
            rpc_set_method(SessionKnob::Model),
            RpcMethod::SessionSwitchModel
        );
        assert!(!RpcMethod::SessionSwitchModel.as_str().contains(".set_"));
    }
}
