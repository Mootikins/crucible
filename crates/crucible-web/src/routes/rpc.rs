//! `POST /api/rpc/{method}` — one authenticated route for every daemon RPC
//! method the browser may call.
//!
//! [[Simplification Plan#Step 19]] item 1. Before this route, each new daemon
//! method the browser needed cost about nine places to edit: the core request
//! and reply types, the `rpc_methods!` row, the dispatch arm, the handler,
//! the `DaemonClient` method, a web route and its types, a forwarding
//! function in `services/daemon.rs`, and a TS function with its own types.
//! This route removes the last three of those for any *new* method a browser
//! needs: the body and reply type, the `rpc_methods!` row and its dispatch
//! arm already exist for every method, so adding one here is one line in
//! [`browser_may_call`].
//!
//! The route does three things, in order, and nothing else:
//! 1. Look up `method` in [`RpcMethod`]. A name it does not know answers 404.
//! 2. Check [`browser_may_call`]. A method that answers `false` is 403 for
//!    every caller — this table is the security boundary, not the route.
//! 3. Check [`plugin_may_call`], the stricter per-caller list. A plugin block
//!    reaches only its own plugin's methods.
//!
//! Then the body is forwarded to the daemon unread (`ReconnectingDaemon::rpc_forward`)
//! and the reply comes back unchanged, except for `plugin.publications`,
//! which is narrowed to the caller the same way `routes/plugin.rs`'s own
//! route already narrows it (see [`plugin::narrow_to_caller`]).
//!
//! This route does not replace the existing REST routes (that migration, and
//! deleting the routes that only forward one RPC, is a later change); it is
//! additive, so the frontend keeps working while it moves over one call at a
//! time.

use crate::error::WebResultExt;
use crate::routes::plugin::{self, PublicationsByKey};
use crate::routes::plugin_caller::{PluginCaller, APP_CALLER, PLUGIN_CALLER_HEADER};
use crate::services::daemon::AppState;
use crate::WebError;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use crucible_core::protocol::RpcMethod;
use crucible_core::types::plugin_reply::PluginPublicationsReply;
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn rpc_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(call_rpc_method))
}

/// Forward one JSON-RPC method call to the daemon.
///
/// The request and reply bodies are exactly the daemon's own wire shapes —
/// `crucible_core::protocol::rpc::method::RpcMethod::params_type`/
/// `reply_type` name them per method, and the generated
/// `crates/crucible-web/web/src/lib/rpc-methods.d.ts` is the typed contract a
/// caller reads, not this document. Documenting all 169 methods as separate
/// OpenAPI paths would repeat that map in a second, hand-maintained form; this
/// path is documented once, generically, on purpose (step 19 item 6).
#[utoipa::path(
    post,
    path = "/api/rpc/{method}",
    params(
        ("method" = String, Path, description = "A wire method name from `RpcMethod`, e.g. `session.get`"),
        ("x-crucible-plugin" = Option<String>, Header, description = "Who is asking: `app`, or the plugin being drawn for. Omitted, the caller is treated as the app."),
    ),
    request_body(
        content = serde_json::Value,
        description = "The method's own params type, exactly as `rpc_methods!` names it. See `rpc-methods.d.ts` for the typed map."
    ),
    responses(
        (status = 200, description = "The method's own reply type, forwarded unchanged"),
        (status = 403, description = "This method is not on the browser's allow list, or a plugin block reached for another plugin's method"),
        (status = 404, description = "No `RpcMethod` has this wire name"),
        (status = 409, description = "The daemon reports the resource busy; retry"),
        (status = 422, description = "The daemon refused the params (JSON-RPC INVALID_PARAMS)"),
        (status = 502, description = "The daemon could not answer, or answered with a different error"),
    )
)]
async fn call_rpc_method(
    State(state): State<AppState>,
    Path(method): Path<String>,
    headers: HeaderMap,
    Json(params): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, WebError> {
    let method = RpcMethod::parse(&method)
        .ok_or_else(|| WebError::NotFound(format!("No such RPC method: {method}")))?;

    if !browser_may_call(method) {
        return Err(WebError::Forbidden(format!(
            "`{method}` is not on the browser's allow list"
        )));
    }

    let caller = rpc_caller(&headers);
    plugin_may_call(&state, &caller, method, &params).await?;

    let reply = state
        .daemon
        .rpc_forward(method, params)
        .await
        .daemon_err()?;

    Ok(Json(narrow_reply_for_caller(method, &caller, reply)?))
}

/// Who is asking, read the way `routes/plugin_caller.rs`'s own extractor
/// reads it — same header, same three-valued identity — except that an
/// absent header is the app rather than a refusal.
///
/// `PluginCaller`'s own [`axum::extract::FromRequestParts`] refuses a missing
/// header, which is right for the plugin-only routes it guards: every caller
/// there is either the app or a plugin block, and omitting the header would
/// be the bypass. This route serves every RPC method, and almost none of them
/// are plugin business — the app calling `session.get` sends no
/// `x-crucible-plugin` header at all, and treating that omission as a
/// refusal would 403 the whole route for its main caller. Only a caller that
/// *declares* itself a plugin (and only for the two methods
/// [`plugin_may_call`] lists) gets the stricter treatment.
///
/// See `routes/plugin_caller.rs`'s own module comment: this identity is
/// asserted by the caller, not proved. A hostile same-origin script can
/// still call itself `app`. Nothing here or there closes that; it is closed
/// only once plugin blocks run in a sandboxed origin behind a bridge that
/// stamps their identity (`docs/Meta/Analysis/Plugin API Plan.md`).
fn rpc_caller(headers: &HeaderMap) -> PluginCaller {
    let declared = headers
        .get(PLUGIN_CALLER_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match declared {
        Some(APP_CALLER) | None => PluginCaller::App,
        Some(name) => PluginCaller::Plugin(name.to_string()),
    }
}

/// The second, stricter allow list: what a caller that named itself a plugin
/// may reach through this route.
///
/// The app reaches everything [`browser_may_call`] allows. A plugin block
/// reaches only its own plugin's commands and its own publications — the same
/// two operations `routes/plugin.rs`'s dedicated routes already gate this
/// way, so this is the generic route's copy of an existing policy, not a new
/// one.
async fn plugin_may_call(
    state: &AppState,
    caller: &PluginCaller,
    method: RpcMethod,
    params: &serde_json::Value,
) -> Result<(), WebError> {
    let PluginCaller::Plugin(plugin) = caller else {
        return Ok(());
    };

    match method {
        RpcMethod::PluginRunCommand => {
            let command = params
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            plugin::refuse_another_plugins_command(state, caller, command).await
        }
        // Publications are narrowed after the daemon answers, in
        // `narrow_reply_for_caller`, the same way `routes/plugin.rs`'s own
        // `list_publications` narrows rather than refuses (a `?key=` reply is
        // a courtesy; narrowing is the boundary the courtesy stands in for).
        RpcMethod::PluginPublications => Ok(()),
        _ => Err(WebError::Forbidden(format!(
            "plugin `{plugin}` may not call `{method}` through this route: only its own \
             commands and publications reach a plugin block here"
        ))),
    }
}

/// Narrow a `plugin.publications` reply to the caller's own rows. Every other
/// method's reply passes through untouched.
fn narrow_reply_for_caller(
    method: RpcMethod,
    caller: &PluginCaller,
    reply: serde_json::Value,
) -> Result<serde_json::Value, WebError> {
    if method != RpcMethod::PluginPublications || matches!(caller, PluginCaller::App) {
        return Ok(reply);
    }

    let parsed: PluginPublicationsReply = serde_json::from_value(reply).map_err(|e| {
        WebError::Daemon(format!(
            "plugin.publications answered a shape this route cannot read: {e}"
        ))
    })?;
    let narrowed: PublicationsByKey = plugin::narrow_to_caller(parsed.publications, caller);
    serde_json::to_value(PluginPublicationsReply {
        publications: narrowed,
    })
    .map_err(|e| WebError::Internal(format!("could not re-encode a narrowed reply: {e}")))
}

/// Whether the browser's generic RPC route may call this method at all.
///
/// One arm per [`RpcMethod`] variant and no wildcard, so a row added to
/// `rpc_methods!` does not compile here until someone decides whether the
/// browser may reach it. `true` methods are the ones a current web route
/// already forwards (see `services/daemon.rs`, `services/daemon_plugins.rs`,
/// `services/daemon_proposals.rs`); every other method is `false`, named with
/// the reason it stays off, per the groups the Simplification Plan's step 19
/// names: local-admin methods, methods with no current browser caller, and
/// methods whose existing REST route applies a check this raw passthrough
/// would skip.
fn browser_may_call(method: RpcMethod) -> bool {
    use RpcMethod::{
        AgentsListCards, AgentsListProfiles, AgentsResolveProfile, BaseCreateEntry, BaseList,
        BaseQuery, BaseReorderGroups, BaseSetProperty, BaseViews, ClientStateGet, ClientStateSet,
        ConfigControls, ConfigEffective, ConfigGet, ConfigOrigin, ConfigPop, ConfigReset,
        ConfigSave, ConfigSet, ConfigUnset, DaemonCapabilities, DiffComment, DiffComments,
        DiffDeleteComment, DiffFile, DiffGet, DiffResolveComment, EmbedQuery, EmbeddingsModels,
        FsListDir, FsMkdir, FsMove, FsRead, FsTrash, FsWrite, GetBacklinks, GetNoteByName,
        KilnClose, KilnForget, KilnGraph, KilnList, KilnOpen, KilnRegister, KilnRegistryList,
        ListNotes, LlmRegisterProvider, LuaDiscoverPlugins, LuaEval, LuaGenerateStubs,
        LuaInitSession, LuaPluginHealth, LuaRegisterCommands, LuaRunPluginTests,
        LuaShutdownSession, McpStart, McpStatus, McpStop, ModelsList, NoteDelete, NoteGet,
        NoteList, NoteMove, NoteRename, NoteUpsert, NotificationDismiss, NotificationList, Ping,
        PluginCommands, PluginInstall, PluginList, PluginOptionExecute, PluginOptionGet,
        PluginOptionSet, PluginOptions, PluginPublications, PluginReload, PluginRemove,
        PluginRunCommand, ProcessBatch, ProcessFile, ProjectGet, ProjectList, ProjectOpenKilns,
        ProjectRegister, ProjectRegistryList, ProjectUnregister, ProposalAccept, ProposalDismiss,
        ProposalGet, ProposalList, ProposalReject, ProposalResolve, ProvidersList, ScmClone,
        SearchGrep, SearchText, SearchVectors, SessionAddNotification, SessionArchive,
        SessionCacheStats, SessionCanUndo, SessionCancel, SessionCleanup, SessionClear,
        SessionCommands, SessionCompact, SessionConfigureAgent, SessionConnectKiln, SessionCreate,
        SessionDelete, SessionDisconnectKiln, SessionDismissNotification, SessionEnd,
        SessionEventsAfter, SessionExportToFile, SessionFork, SessionGenerateTitle, SessionGet,
        SessionGetPluginApproval, SessionHistory, SessionInjectContext, SessionInteractionRespond,
        SessionKnobGet, SessionKnobSet, SessionList, SessionListAgentOptions, SessionListKnobs,
        SessionListModels, SessionListModes, SessionListNotifications, SessionListPersisted,
        SessionListPluginApprovals, SessionPause, SessionPendingInteractions, SessionReindex,
        SessionRenderMarkdown, SessionReplay, SessionResume, SessionResumeFromStorage,
        SessionSearch, SessionSendMessage, SessionSetAgentOption, SessionSetPluginApproval,
        SessionSetTitle, SessionSetWorkspace, SessionStatus, SessionSubscribe,
        SessionTestInteraction, SessionUnarchive, SessionUndo, SessionUndoDepth,
        SessionUnsubscribe, Shutdown, SkillsGet, SkillsList, SkillsSearch, StorageBackup,
        StorageCleanup, StorageRestore, StorageVerify, SubagentCollect, SuggestLinks, SurfaceGet,
        SurfaceList, UiConfig, UiSetTheme, WebhookReceive, WorkflowApproveGate, WorkflowCancel,
        WorkflowStart, WorkflowStatus,
    };

    match method {
        // ---- Health/liveness: a harmless read. ----
        Ping => true,

        // Not read by any web route today; each panel already answers a
        // narrower question (`GET /api/plugins`, `/api/skills`, ...) than
        // "list every method the daemon has".
        DaemonCapabilities => false,

        // Local admin: stops the daemon process itself.
        Shutdown => false,

        // The web server's own layout/recents store — written by
        // `services/daemon.rs` under the web's own client-state id, never by
        // a key the browser names directly.
        ClientStateGet | ClientStateSet => false,

        // kiln.open/close/register/forget are local admin: the CLI and the
        // web server's own startup open kilns. The browser reads through
        // `kiln.list`, `kiln.graph` and the note/fs methods instead.
        KilnOpen | KilnClose | KilnRegister | KilnForget => false,
        KilnList => true,
        // Not read by any web route today (a hand-built stand-in shape, per
        // `rpc_methods!`'s own row comment — not something to hand a browser
        // raw either way).
        KilnRegistryList => false,

        // Local admin: adds a provider to the daemon's own config.
        LlmRegisterProvider => false,

        SearchVectors => true,
        // Not used by the web today (`routes/search.rs` reads
        // `search_vectors` and `search_grep`, not `search_text`).
        SearchText => false,
        SearchGrep => true,
        EmbedQuery => true,
        ListNotes => true,
        GetNoteByName => true,

        // Six operations answered by one raw-`&Request` handler
        // (`rpc_methods!`'s own row comment); `base.list` itself is not read
        // by any web route today, unlike the other five.
        BaseList => false,
        BaseViews | BaseQuery | BaseCreateEntry | BaseSetProperty | BaseReorderGroups => true,

        GetBacklinks => true,
        KilnGraph => true,

        // note.upsert/get/delete/list are not used by the web today —
        // `routes/kiln.rs` reads and writes through `fs.read`/`fs.write`.
        NoteUpsert | NoteGet | NoteDelete | NoteList => false,

        // Not used by the web today.
        ProcessFile | ProcessBatch => false,

        SessionCreate | SessionList | SessionGet | SessionPause | SessionResume => true,
        // Not used by the web today — `session.resume` already falls back to
        // storage for a session held but not `Paused` (step 19's own "resume
        // fallback" decision covers the case this method used to patch over).
        SessionResumeFromStorage => false,
        SessionHistory | SessionEnd | SessionArchive | SessionUnarchive | SessionDelete => true,
        // Not used by the web today.
        SessionCompact => false,
        SessionSubscribe | SessionUnsubscribe => true,
        SessionConfigureAgent | SessionSendMessage | SessionCancel | SessionClear => true,
        SessionConnectKiln | SessionDisconnectKiln | SessionSetWorkspace => true,
        SessionListModels | SessionListModes | SessionCommands => true,
        SessionListKnobs | SessionKnobSet | SessionKnobGet => true,
        SessionListAgentOptions | SessionSetAgentOption => true,
        // Not used by the web today.
        SessionCacheStats | SessionAddNotification => false,
        SessionListNotifications | SessionDismissNotification => true,
        // The daemon's own broadcast notification list, distinct from a
        // session's own (`session.list_notifications`/`.dismiss_notification`,
        // which the web does use); not read by the web today.
        NotificationList | NotificationDismiss => false,
        SessionInteractionRespond | SessionPendingInteractions => true,
        SessionSetPluginApproval | SessionGetPluginApproval | SessionListPluginApprovals => true,
        // Not used by the web today.
        SessionInjectContext | SessionTestInteraction | SessionFork => false,
        SessionSetTitle | SessionGenerateTitle => true,
        SessionSearch | SessionEventsAfter => true,
        // Deliberately `serde_json::Value` (a page of mixed session-summary
        // shapes, per `rpc_methods!`'s own comment); not used by the web
        // today either way.
        SessionListPersisted => false,
        SessionRenderMarkdown => true,
        // Not used by the web today.
        SessionExportToFile | SessionReplay | SessionCleanup => false,
        // Retired: always answers `METHOD_NOT_FOUND`, so there is nothing a
        // browser could do with it.
        SessionReindex => false,
        SessionUndo => true,
        // Not used by the web today.
        SessionCanUndo | SessionUndoDepth => false,

        PluginReload | PluginList | PluginCommands | PluginPublications => true,
        SurfaceList => true,
        // Not used by the web today.
        SurfaceGet => false,
        PluginOptions | PluginOptionGet | PluginOptionSet | PluginOptionExecute => true,
        SessionStatus => true,
        PluginRunCommand => true,
        // Local admin: installs/removes operator code the daemon's one
        // shared plugin VM runs.
        PluginInstall | PluginRemove => false,

        // Local admin: the Lua lifecycle/test methods manage the daemon's
        // shared plugin VM itself, and `lua.eval` runs arbitrary Lua with the
        // daemon's own privileges.
        LuaInitSession | LuaShutdownSession | LuaDiscoverPlugins | LuaPluginHealth
        | LuaGenerateStubs | LuaRunPluginTests | LuaRegisterCommands | LuaEval => false,

        // config.* writes and the open-shaped reads: local admin. The app's
        // config lives in `init.lua`; a browser session does not edit it
        // directly, and `/api/config`'s own route (kept) applies the
        // credential redaction `daemon_config.rs` documents, which this raw
        // passthrough would skip.
        ConfigGet | ConfigSet | ConfigSave | ConfigReset | ConfigPop | ConfigUnset
        | ConfigOrigin | ConfigEffective | ConfigControls => false,

        // Not used by the web today.
        UiConfig | UiSetTheme => false,

        // Local admin: `project.register`'s untrusted-root rule and
        // `project.unregister`'s pairing with it live in the web's own
        // dedicated route (step 19's own table), not in this raw passthrough.
        ProjectRegister | ProjectUnregister => false,
        ProjectList | ProjectGet => true,
        // Not used by the web today.
        ProjectOpenKilns | ProjectRegistryList => false,

        ScmClone => true,
        FsListDir => true,
        DiffGet | DiffFile | DiffComment | DiffResolveComment | DiffDeleteComment
        | DiffComments => true,
        ProposalList | ProposalGet | ProposalAccept | ProposalReject | ProposalDismiss
        | ProposalResolve => true,
        FsRead | FsWrite | FsMove | FsMkdir | FsTrash => true,
        // Not used by the web today.
        NoteRename | NoteMove => false,

        // Local admin: backup/restore/verify/cleanup of a kiln's own
        // database files.
        StorageVerify | StorageCleanup | StorageBackup | StorageRestore => false,

        McpStatus => true,
        // Local admin: spawns/kills an MCP server process from the daemon's
        // own config.
        McpStart | McpStop => false,

        SkillsList | SkillsGet | SkillsSearch => true,
        AgentsListProfiles => true,
        // Not used by the web today.
        AgentsListCards | AgentsResolveProfile => false,
        ModelsList | ProvidersList => true,
        // Not used by the web today.
        EmbeddingsModels | SubagentCollect => false,

        // Local admin: an inbound webhook is verified and routed by the
        // web's own dedicated `webhook.rs` route, never by a browser tab.
        WebhookReceive => false,

        SuggestLinks => true,

        // Not used by the web today.
        WorkflowStart | WorkflowApproveGate | WorkflowStatus | WorkflowCancel => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{request_json, request_json_as, request_json_with_errors};
    use axum::http::StatusCode;
    use crucible_core::protocol::rpc::RpcMethod as Method;
    use serde_json::json;
    use std::collections::HashMap;

    /// A method with no [`RpcMethod`] entry is 404, whatever the caller sends.
    #[tokio::test]
    async fn an_unknown_method_answers_404() {
        let (status, _) = request_json("POST", "/api/rpc/no.such.method", Some(json!({}))).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// Every local-admin method [`browser_may_call`] keeps off the list
    /// answers 403 through the real route, whoever asks — the allow list is
    /// the boundary, not a per-caller decision.
    #[tokio::test]
    async fn a_local_admin_method_answers_403() {
        let local_admin = [
            "shutdown",
            "lua.eval",
            "lua.init_session",
            "plugin.install",
            "plugin.remove",
            "config.get",
            "config.save",
            "storage.verify",
            "kiln.register",
            "kiln.forget",
            "project.register",
            "project.unregister",
            "llm.register_provider",
            "webhook.receive",
            "client_state.get",
            "client_state.set",
            "mcp.start",
        ];
        for method in local_admin {
            let (status, body) =
                request_json("POST", &format!("/api/rpc/{method}"), Some(json!({}))).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{method}: {body}");
        }
    }

    /// `POST /api/rpc/session.get` answers the same body `GET
    /// /api/session/{id}` does: the row's reply (`SessionDetail`) is not
    /// wrapped either way.
    ///
    /// Compared through `SessionDetail` rather than the raw bytes: the REST
    /// route decodes the daemon's reply into that type and re-encodes it,
    /// which spells out an absent optional field as an explicit `null` and a
    /// defaulted one (`plugin_approvals`) as `{}`; this route forwards the
    /// daemon's own bytes unchanged, which is item 1's own contract, so the
    /// mock's leaner JSON simply omits the keys the type would default. Both
    /// decode to the same value, which is the parity that matters.
    #[tokio::test]
    async fn session_get_matches_the_existing_route() {
        let (_, rest) = request_json("GET", "/api/session/test-session-001", None).await;
        let (status, rpc) = request_json(
            "POST",
            "/api/rpc/session.get",
            Some(json!({ "session_id": "test-session-001" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let rest: crucible_core::session::SessionDetail = serde_json::from_value(rest).unwrap();
        let rpc: crucible_core::session::SessionDetail = serde_json::from_value(rpc).unwrap();
        assert_eq!(
            serde_json::to_value(&rest).unwrap(),
            serde_json::to_value(&rpc).unwrap()
        );
    }

    /// `POST /api/rpc/search_grep` answers the same body `POST
    /// /api/search/grep` does: both name the row's reply (`GrepSearchResponse`)
    /// directly.
    #[tokio::test]
    async fn search_grep_matches_the_existing_route() {
        let body = json!({ "root": "/tmp/test-kiln", "query": "needle" });
        let (_, rest) = request_json("POST", "/api/search/grep", Some(body.clone())).await;
        let (status, rpc) = request_json("POST", "/api/rpc/search_grep", Some(body)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(rpc, rest);
    }

    /// `GET /api/kilns` wraps the row's own reply under a `kilns` key (a
    /// thin, documented rename); `POST /api/rpc/kiln.list` answers the row's
    /// bare `Vec<KilnRow>` instead, since this route forwards the reply
    /// unchanged rather than reshaping it the way a dedicated REST route may.
    /// The data is the same; the envelope is the REST route's own addition.
    #[tokio::test]
    async fn kiln_list_matches_the_existing_routes_own_data() {
        let (_, rest) = request_json("GET", "/api/kilns", None).await;
        let (status, rpc) = request_json("POST", "/api/rpc/kiln.list", Some(json!({}))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(rpc, rest["kilns"]);
    }

    /// `GET /api/plugins` renames the row's `plugin_info` field to `plugins`
    /// (documented on `PluginListResponse`); `POST /api/rpc/plugin.list`
    /// answers the row unchanged, so the REST route's `plugins` is the raw
    /// row's `plugin_info`.
    #[tokio::test]
    async fn plugin_list_matches_the_existing_routes_own_data() {
        let (_, rest) = request_json("GET", "/api/plugins", None).await;
        let (status, rpc) = request_json("POST", "/api/rpc/plugin.list", Some(json!({}))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(rpc["plugin_info"], rest["plugins"]);
    }

    /// A plugin block may run its own command.
    #[tokio::test]
    async fn a_plugin_may_run_its_own_command() {
        let (status, _) = request_json_as(
            "POST",
            "/api/rpc/plugin.run_command",
            Some(json!({ "name": "mock_command", "args": {} })),
            vec![(PLUGIN_CALLER_HEADER, "mock-plugin".to_string())],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    /// A plugin block may not run another plugin's command through this
    /// route, the same refusal `POST /api/plugins/command` already gives.
    #[tokio::test]
    async fn a_plugin_may_not_run_another_plugins_command() {
        let (status, body) = request_json_as(
            "POST",
            "/api/rpc/plugin.run_command",
            Some(json!({ "name": "mock_command", "args": {} })),
            vec![(PLUGIN_CALLER_HEADER, "other-plugin".to_string())],
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    }

    /// A plugin block may not reach a method outside its own two
    /// (`plugin.run_command`, `plugin.publications`) through this route, even
    /// one [`browser_may_call`] allows the app to use.
    #[tokio::test]
    async fn a_plugin_may_not_call_a_method_that_is_not_its_own() {
        let (status, body) = request_json_as(
            "POST",
            "/api/rpc/session.get",
            Some(json!({ "session_id": "test-session-001" })),
            vec![(PLUGIN_CALLER_HEADER, "mock-plugin".to_string())],
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    }

    /// `plugin.publications` is narrowed to the caller's own rows, the same
    /// way `GET /api/plugins/publications` narrows it — a key with nothing
    /// left under it is dropped rather than answered empty.
    #[tokio::test]
    async fn plugin_publications_are_narrowed_to_the_caller() {
        let (status, body) = request_json_as(
            "POST",
            "/api/rpc/plugin.publications",
            Some(json!({})),
            vec![(PLUGIN_CALLER_HEADER, "mock-plugin".to_string())],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let publications = body["publications"].as_object().unwrap();
        assert_eq!(publications.len(), 1, "{body}");
        assert!(publications.contains_key("everything"), "{body}");
        assert!(
            !publications.contains_key("and-more"),
            "and-more holds only other-plugin's row: {body}"
        );
    }

    /// The app is narrowed to nothing: it sees every plugin's publications,
    /// same as `GET /api/plugins/publications` with no caller narrowing.
    #[tokio::test]
    async fn the_app_sees_every_plugins_publications() {
        let (status, body) =
            request_json("POST", "/api/rpc/plugin.publications", Some(json!({}))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let publications = body["publications"].as_object().unwrap();
        assert_eq!(publications.len(), 2, "{body}");
    }

    /// A daemon `INVALID_PARAMS` refusal is a 422, not a 502 — the same
    /// mapping `error.rs`'s `daemon_err()` gives every other route, proved
    /// here through the real HTTP route rather than a hand-built string.
    #[tokio::test]
    async fn a_daemon_invalid_params_error_is_422() {
        let mut errors = HashMap::new();
        errors.insert(
            Method::SessionGet,
            (-32602i64, "no such session".to_string()),
        );
        let (status, body) = request_json_with_errors(
            "POST",
            "/api/rpc/session.get",
            Some(json!({ "session_id": "does-not-exist" })),
            errors,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(body["error"]["message"], "no such session");
    }

    /// Every other daemon-side JSON-RPC error is a 502: the daemon answered,
    /// but with a fault this route did not expect to be able to fix.
    #[tokio::test]
    async fn any_other_daemon_error_is_502() {
        let mut errors = HashMap::new();
        errors.insert(
            Method::SessionGet,
            (-32000i64, "session.get exploded".to_string()),
        );
        let (status, body) = request_json_with_errors(
            "POST",
            "/api/rpc/session.get",
            Some(json!({ "session_id": "test-session-001" })),
            errors,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
        assert_eq!(body["error"]["message"], "session.get exploded");
    }
}
