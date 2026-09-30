use super::forwarding::ReplayPolicy;
use crate::{Result, WebError};
use crucible_core::config::CliAppConfig;
use crucible_core::protocol::requests::{GetBacklinksReply, SessionCreateRequest, VectorHit};
use crucible_core::protocol::RpcMethod;
use crucible_daemon::{agent_manager::providers::ProviderInfo, DaemonClient, SessionEvent};
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

const EVENT_CHANNEL_CAPACITY: usize = 256;

#[path = "daemon_event_stream.rs"]
mod event_stream;
pub use event_stream::EventStream;

#[derive(Clone)]
pub struct AppState {
    pub daemon: Arc<ReconnectingDaemon>,
    pub events: Arc<EventBroker>,
    pub config: Arc<CliAppConfig>,
    pub http_client: reqwest::Client,
    /// The `client` this process is, in the daemon's generic
    /// `client_state.get`/`client_state.set` store — `"web"` normally,
    /// `"web-standalone"` for a `--standalone` (debug/test) instance, so it
    /// never shares state with the production one.
    pub client_state_id: Arc<str>,
    /// Whether non-loopback terminal/shell access is active (opt-in env var
    /// AND an API key configured) — surfaced to the frontend via /api/config
    /// so the terminal panel knows whether to connect from a LAN client.
    pub remote_shell: bool,
    /// Serializes /api/recents read-modify-writes (concurrent records would
    /// clobber each other's entries).
    pub recents_lock: Arc<tokio::sync::Mutex<()>>,
}

/// The `client` this process is in the daemon's client-state store, for a
/// normal (non-`--standalone`) instance.
pub const WEB_CLIENT_STATE_ID: &str = "web";

/// As [`WEB_CLIENT_STATE_ID`], for a `--standalone` (debug/test) instance —
/// a distinct namespace, so it never shares a pane layout or a recents list
/// with the production instance it is standing in for.
pub const WEB_STANDALONE_CLIENT_STATE_ID: &str = "web-standalone";

pub struct ReconnectingDaemon {
    daemon: Arc<RwLock<DaemonClient>>,
    generation: AtomicU64,
    #[cfg(test)]
    reconnect_socket: Option<PathBuf>,
    /// The SSE fan-out target. Held so a reconnect can rewire a fresh event
    /// stream into it instead of leaving SSE permanently dead.
    broker: Arc<EventBroker>,
    /// Handle to the current event-router task, aborted and replaced on reconnect.
    router: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// The daemon subscription of each session, changed one flight at a
    /// time for each session. Reconnect reads receiver ownership from the
    /// broker while holding the daemon write lock.
    interest: event_stream::Interest,
}

impl ReconnectingDaemon {
    pub fn new(
        daemon: DaemonClient,
        event_rx: mpsc::UnboundedReceiver<SessionEvent>,
        broker: Arc<EventBroker>,
    ) -> Self {
        let router = spawn_event_router(event_rx, broker.clone());
        Self {
            daemon: Arc::new(RwLock::new(daemon.without_timeout_retries())),
            generation: AtomicU64::new(0),
            #[cfg(test)]
            reconnect_socket: None,
            broker,
            router: std::sync::Mutex::new(Some(router)),
            interest: event_stream::Interest::new(),
        }
    }

    forward_rpc! { Safe BaseQuery => base_query(params: crucible_daemon::bases::QueryParams) -> crucible_daemon::bases::QueryResult = call(RpcMethod::BaseQuery, params); }
    forward_rpc! { Safe BaseViews => base_views(params: crucible_daemon::bases::ViewsParams) -> Vec<crucible_daemon::bases::ViewSummary> = call(RpcMethod::BaseViews, params); }
    forward_rpc! { Once BaseCreateEntry => base_create_entry(params: crucible_daemon::bases::CreateEntryParams) -> crucible_daemon::bases::WriteOutcome = call(RpcMethod::BaseCreateEntry, params); }
    forward_rpc! { Once BaseSetProperty => base_set_property(params: crucible_daemon::bases::SetPropertyParams) -> crucible_daemon::bases::WriteOutcome = call(RpcMethod::BaseSetProperty, params); }
    forward_rpc! { Once BaseReorderGroups => base_reorder_groups(params: crucible_daemon::bases::ReorderGroupsParams) -> crucible_daemon::bases::WriteOutcome = call(RpcMethod::BaseReorderGroups, params); }

    /// The daemon's cheapest RPC, for the readiness probe.
    ///
    /// `Safe` replay: `ping` changes nothing, so reconnecting and retrying once
    /// after a dropped connection is what a probe wants to do — and it means the
    /// probe repairs the link it found broken instead of only reporting it.
    pub async fn ping(&self) -> anyhow::Result<String> {
        self.forward_rpc(ReplayPolicy::Safe, RpcMethod::Ping, |client| {
            Box::pin(client.ping())
        })
        .await
    }

    /// The one generic forwarder behind `POST /api/rpc/{method}`
    /// (`routes/rpc.rs`). Sends `params` as the request body of `method`,
    /// unread and untouched, and hands back the daemon's reply the same way.
    ///
    /// `Once`, not `Safe`: every named forwarder above declares its own
    /// replay policy because it knows whether its call is a read or a write.
    /// This one carries an arbitrary [`RpcMethod`] chosen at the HTTP layer,
    /// so it cannot tell the two apart — replaying an ambiguous write after a
    /// dropped connection would risk applying it twice, and refusing to
    /// guess is the safer default for a call this general.
    pub async fn rpc_forward(
        &self,
        method: RpcMethod,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        self.forward_rpc(ReplayPolicy::Once, method, move |daemon| {
            let params = params.clone();
            Box::pin(async move { daemon.call(method, params).await })
        })
        .await
    }

    /// Every forwarder declares replay safety. A lost response to a write is
    /// ambiguous, so only a replay-safe call is RESUBMITTED after a dropped
    /// connection.
    ///
    /// The connection itself is healed either way. Reconnecting performs no
    /// daemon-side action of its own and so carries none of the
    /// double-execution risk a resubmitted write does — the risk lives in
    /// calling `call` a second time, not in swapping the socket underneath
    /// it. Healing on every policy is what lets `POST /api/rpc/{method}`
    /// (`ReplayPolicy::Once`, since it carries an arbitrary method chosen at
    /// the HTTP layer) recover after the daemon restarts: its own first call
    /// still surfaces the error, unretried, but the reconnect it triggers
    /// lets the NEXT call — the next poll of a caller like
    /// `kiln-restart.live.spec.ts` — succeed, rather than failing forever
    /// because nothing else was polling a `Safe` method to notice the daemon
    /// came back.
    pub(super) async fn forward_rpc<T>(
        &self,
        policy: ReplayPolicy,
        method: RpcMethod,
        call: impl for<'a> Fn(&'a DaemonClient) -> BoxFuture<'a, anyhow::Result<T>>,
    ) -> anyhow::Result<T> {
        let observed_generation = self.generation.load(Ordering::Acquire);
        let first_attempt = {
            let daemon = self.daemon.read().await;
            call(&daemon).await
        };

        match first_attempt {
            Ok(value) => Ok(value),
            Err(err) if Self::is_connection_error(&err) => {
                tracing::warn!(
                    method = method.as_str(),
                    error = %err,
                    policy = ?policy,
                    "Daemon connection failed, reconnecting"
                );
                self.reconnect_if_stale(observed_generation).await?;

                if policy != ReplayPolicy::Safe {
                    return Err(err);
                }

                let daemon = self.daemon.read().await;
                call(&daemon).await
            }
            Err(err) => Err(err),
        }
    }

    async fn reconnect_if_stale(&self, observed_generation: u64) -> anyhow::Result<()> {
        if self.generation.load(Ordering::Acquire) != observed_generation {
            return Ok(());
        }

        let mut daemon = self.daemon.write().await;
        if self.generation.load(Ordering::Acquire) != observed_generation {
            return Ok(());
        }

        // Reconnect in EVENT mode (matching the initial connect). A simple-mode
        // client returns the next line with ANY id without matching the request,
        // so under the web server's concurrent RPC load two calls could swap
        // responses (silent wrong data). Event mode keeps responses id-matched.
        #[cfg(not(test))]
        let connection = DaemonClient::connect_or_start_with_events().await;
        #[cfg(test)]
        let connection = match &self.reconnect_socket {
            Some(path) => DaemonClient::connect_to_with_events(path).await,
            None => DaemonClient::connect_or_start_with_events().await,
        };
        let (new_daemon, event_rx) = connection?;
        let new_daemon = new_daemon.without_timeout_retries();
        let active: Vec<String> = self
            .broker
            .sessions
            .read()
            .await
            .iter()
            .filter(|(_, tx)| tx.receiver_count() > 0)
            .map(|(id, _)| id.clone())
            .collect();
        if !active.is_empty() {
            let borrowed: Vec<&str> = active.iter().map(String::as_str).collect();
            // Refuse this reconnect if restoration fails. Leaving generation
            // unchanged lets the next safe call retry; a half-restored link
            // must not look healthy while its browser streams stay silent.
            new_daemon.session_subscribe(&borrowed).await?;
        }
        *daemon = new_daemon;
        self.rewire_events(event_rx).await;
        self.generation.fetch_add(1, Ordering::AcqRel);
        tracing::warn!("Daemon reconnected; SSE fan-out rewired to the new event stream");
        Ok(())
    }

    /// Move the SSE fan-out onto the event stream of a new connection.
    ///
    /// The router of the dead connection stops first, and this waits for it:
    /// an event that it still held must not reach a browser after the gap.
    /// HTTP streams survive a daemon reconnect. Their lost span is unknown
    /// (zero), so the gap goes out before the new live tail. A chat stream
    /// reads the gap as a new anchor, because a restarted daemon numbers its
    /// events from the persisted log again.
    async fn rewire_events(&self, event_rx: mpsc::UnboundedReceiver<SessionEvent>) {
        let old = self
            .router
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(old) = old {
            old.abort();
            let _ = old.await;
        }
        self.broker
            .dispatch(event_stream::stream_gap(
                crucible_daemon::subscription::WILDCARD_SESSION,
                0,
            ))
            .await;
        let router = spawn_event_router(event_rx, self.broker.clone());
        *self
            .router
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(router);
    }

    fn is_connection_error(err: &anyhow::Error) -> bool {
        let msg = err.to_string();
        let lower = msg.to_ascii_lowercase();
        let has_connection_text = [
            "broken pipe",
            "connection reset",
            "connection refused",
            "os error 32",
        ]
        .iter()
        .any(|needle| lower.contains(needle));

        if has_connection_text {
            return true;
        }

        for cause in err.chain() {
            if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
                if matches!(
                    io_err.kind(),
                    std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionRefused
                ) {
                    return true;
                }
            }
        }

        false
    }

    // kiln.list/list_notes/get_note_by_name/kiln.graph: the browser calls
    // them through `POST /api/rpc/{method}` now (Simplification Plan step 19
    // item 3, the kiln/note migration), so these forwarders are gone; no
    // other caller in this crate named them.

    forward_rpc! {
        Safe GetBacklinks =>
        get_backlinks(kiln_path: &Path, name: &str)
        -> Option<GetBacklinksReply> = get_backlinks(&kiln_path, &name, None);
    }

    forward_rpc! {
        Safe SuggestLinks =>
        suggest_links(kiln_path: &Path, text: &str)
        -> Vec<crucible_daemon::tools::autolink::LinkSuggestion> = suggest_links(&kiln_path, &text, None);
    }

    forward_rpc! {
        Safe SearchVectors =>
        search_vectors(kiln_path: &Path, vector: &[f32], limit: usize)
        -> Vec<VectorHit> = search_vectors(&kiln_path, &vector, limit, None);
    }

    forward_rpc! {
        /// Embed a query string into a vector via the kiln's configured embedding
        /// provider (the first half of semantic search; feed the result to
        /// [`Self::search_vectors`]).
        Safe EmbedQuery =>
        embed_query(kiln_path: &Path, text: &str)
        -> Vec<f32> = embed_query(&kiln_path, &text);
    }

    // search_grep: the browser calls `search_grep` through
    // `POST /api/rpc/{method}` now (Simplification Plan step 19 item 3), so
    // this forwarder is gone; `routes/search.rs`'s own route was its only
    // caller.

    // mcp.status: the browser calls it through `POST /api/rpc/{method}` now
    // (Simplification Plan step 19), so this forwarder is gone.
    // `DaemonClient::mcp_status` itself stays for now: deleting a
    // hand-written `DaemonClient` method that only forwards one row is
    // step 19 item 9, a separate pass.

    // skills.list/get/search: the browser calls them through
    // `POST /api/rpc/{method}` now (Simplification Plan step 19 item 3), so
    // this forwarder is gone; `DaemonClient::skills_list`/`skills_get`/
    // `skills_search` stay, because they reshape an ergonomic `&Path`
    // argument into the wire request's `String` field, which the CLI still
    // calls directly (item 9).

    forward_rpc! {
        /// Create a session and have the daemon resolve + configure its agent in
        /// one call (ACP profile or config-derived internal defaults). The daemon
        /// owns default resolution, so the web never builds its own copy.
        Once SessionCreate =>
        session_create(request: SessionCreateRequest)
        -> crucible_core::session::SessionSummary = session_create(request);
    }

    forward_rpc! {
        Safe SessionList =>
        session_list(
            kiln: Option<&crucible_core::config::KilnName> => kiln.cloned(),
            workspace: Option<&Path> => workspace.map(Path::to_path_buf),
            session_type: Option<&str> => session_type.map(str::to_owned),
            state: Option<&str> => state.map(str::to_owned),
            include_archived: Option<bool>,
        )
        -> crucible_core::protocol::requests::SessionListReply = session_list(
            kiln.as_ref(), workspace.as_deref(), session_type.as_deref(),
            state.as_deref(), include_archived,
        );
    }

    forward_rpc! {
        /// `kilns` is the caller's whole kiln set: `session.search` scopes by
        /// kiln-set overlap, so sending a subset hides the sessions that share the
        /// members left out.
        Safe SessionSearch =>
        session_search(query: &str, kilns: &[crucible_core::config::KilnName], limit: Option<usize>)
        -> crucible_core::session::SessionSearchResponse = session_search(&query, &kilns, limit);
    }

    forward_rpc! {
        Safe SessionGet =>
        session_get(session_id: &str)
        -> crucible_core::session::SessionDetail = session_get(&session_id);
    }

    forward_rpc! {
        /// One page of a session's stored events, read without making the
        /// session live.
        Safe SessionHistory =>
        session_history(session_id: &str, page: crucible_core::protocol::requests::Page)
        -> serde_json::Value = session_history(&session_id, page);
    }

    forward_rpc! {
        /// The persisted wire envelopes past a seq cursor — the tail the chat
        /// stream replays to a reconnecting client before its live tail.
        Safe SessionEventsAfter =>
        session_events_after(session_id: &str, after: u64)
        -> Vec<SessionEvent> = session_events_after(&session_id, after);
    }

    forward_rpc! {
        Once SessionPause =>
        session_pause(session_id: &str)
        -> serde_json::Value = session_pause(&session_id);
    }

    forward_rpc! {
        Once SessionResume =>
        session_resume(session_id: &str)
        -> serde_json::Value = session_resume(&session_id);
    }

    forward_rpc! {
        Once SessionEnd =>
        session_end(session_id: &str)
        -> serde_json::Value = session_end(&session_id);
    }

    forward_rpc! {
        Once SessionDelete =>
        session_delete(session_id: &str)
        -> serde_json::Value = session_delete(&session_id);
    }

    forward_rpc! {
        Once SessionArchive =>
        session_archive(session_id: &str)
        -> serde_json::Value = session_archive(&session_id);
    }

    forward_rpc! {
        Once SessionUnarchive =>
        session_unarchive(session_id: &str)
        -> serde_json::Value = session_unarchive(&session_id);
    }

    forward_rpc! {
        Once SessionCancel =>
        session_cancel(session_id: &str)
        -> bool = session_cancel(&session_id);
    }

    forward_rpc! {
        Once SessionClear =>
        session_clear(session_id: &str)
        -> () = session_clear(&session_id);
    }

    forward_rpc! {
        Once SessionUndo =>
        session_undo(session_id: &str, count: usize)
        -> Vec<crucible_core::types::UndoSummary> = session_undo(&session_id, count);
    }

    forward_rpc! {
        Safe SessionCommands =>
        session_commands(session_id: &str)
        -> Vec<crucible_core::types::SessionCommand> = session_commands(&session_id);
    }

    async fn session_subscribe(&self, session_ids: &[&str]) -> anyhow::Result<serde_json::Value> {
        let ids: Vec<String> = session_ids.iter().map(|id| (*id).to_string()).collect();
        self.forward_rpc(
            ReplayPolicy::Safe,
            RpcMethod::SessionSubscribe,
            move |daemon| {
                let ids = ids.clone();
                Box::pin(async move {
                    let borrowed: Vec<&str> = ids.iter().map(String::as_str).collect();
                    daemon.session_subscribe(&borrowed).await
                })
            },
        )
        .await
    }

    forward_rpc! {
        Once SessionConfigureAgent =>
        session_configure_agent(session_id: &str, agent: &crucible_core::session::SessionAgent)
        -> () = session_configure_agent(&session_id, &agent);
    }

    forward_rpc! {
        Once SessionSendMessage =>
        session_send_message(session_id: &str, content: &str, comments: &[crucible_core::diff::CommentRef])
        -> crucible_core::types::SendOutcome = session_send_message_with_comments(&session_id, &content, &comments, true);
    }

    forward_rpc! {
        Once SessionInteractionRespond =>
        session_interaction_respond(session_id: &str, request_id: &str, response: crucible_core::interaction::InteractionResponse)
        -> () = session_interaction_respond(&session_id, &request_id, response);
    }

    forward_rpc! {
        /// Aggregate pending interactions across all sessions (Inbox poll).
        Safe SessionPendingInteractions =>
        session_pending_interactions()
        -> serde_json::Value = session_pending_interactions();
    }

    forward_rpc! {
        /// Attach a kiln to a session's connected set. Returns the updated scope.
        Once SessionConnectKiln =>
        session_connect_kiln(session_id: &str, kiln: &crucible_core::config::KilnName)
        -> serde_json::Value = session_connect_kiln(&session_id, &kiln);
    }

    forward_rpc! {
        /// Detach a kiln from the session's set. Any member may be detached — the
        /// set is flat, including the kiln the session was created with.
        Once SessionDisconnectKiln =>
        session_disconnect_kiln(session_id: &str, kiln: &crucible_core::config::KilnName)
        -> serde_json::Value = session_disconnect_kiln(&session_id, &kiln);
    }

    forward_rpc! {
        /// Set (Some) or detach (None) the session's workspace.
        Once SessionSetWorkspace =>
        session_set_workspace(session_id: &str, workspace: Option<&Path> => workspace.map(Path::to_path_buf))
        -> serde_json::Value = session_set_workspace(&session_id, workspace.as_deref());
    }

    forward_rpc! {
        /// Write one session knob. One RPC method serves every
        /// [`crucible_core::types::KnobValue`] variant — model, mode, context
        /// strategy, precognition, plugin turn limit — so a knob added later
        /// needs no sibling row here.
        Once SessionKnobSet =>
        session_knob_set(session_id: &str, value: crucible_core::types::KnobValue)
        -> () = session_knob_set(&session_id, value);
    }

    forward_rpc! {
        /// Read one session knob, in the same [`crucible_core::types::KnobValue`]
        /// shape [`Self::session_knob_set`] writes.
        Safe SessionKnobGet =>
        session_knob_get(session_id: &str, knob: crucible_core::types::SessionKnob)
        -> crucible_core::types::KnobValue = session_knob_get(&session_id, knob);
    }

    forward_rpc! {
        Once SessionSetTitle =>
        session_set_title(session_id: &str, title: &str)
        -> () = session_set_title(&session_id, &title);
    }

    forward_rpc! {
        Once SessionGenerateTitle =>
        session_generate_title(session_id: &str)
        -> serde_json::Value = session_generate_title(&session_id);
    }

    forward_rpc! {
        Safe SessionListModels =>
        session_list_models(session_id: &str)
        -> Vec<String> = session_list_models(&session_id);
    }

    forward_rpc! {
        /// The status list of a session, forwarded as the daemon shaped it
        /// (`{"status": [StatusDisplayItem, …]}`).
        Safe SessionStatus =>
        session_status(session_id: &str)
        -> serde_json::Value = session_status(&session_id);
    }

    forward_rpc! {
        /// The notifications of a session, from the daemon's one store.
        Safe SessionListNotifications =>
        session_list_notifications(session_id: &str)
        -> Vec<crucible_core::types::Notification> = session_list_notifications(&session_id);
    }

    forward_rpc! {
        /// Close one notification for one session: the daemon drops a
        /// notification of the session and hides a shared one for this
        /// session only. `Once`, because a replayed removal answers `false`.
        Once SessionDismissNotification =>
        session_dismiss_notification(session_id: &str, notification_id: &str)
        -> bool = session_dismiss_notification(&session_id, &notification_id);
    }

    forward_rpc! {
        Safe SessionListAgentOptions =>
        session_list_agent_options(session_id: &str)
        -> serde_json::Value = session_list_agent_options(&session_id);
    }

    forward_rpc! {
        Once SessionSetAgentOption =>
        session_set_agent_option(session_id: &str, option_id: &str, value: &str)
        -> () = session_set_agent_option(&session_id, &option_id, &value);
    }

    forward_rpc! {
        Safe SessionListKnobs =>
        session_list_knobs(session_id: &str)
        -> crucible_core::types::SessionKnobSupport = session_list_knobs(&session_id);
    }

    forward_rpc! {
        Safe SessionListModes =>
        session_list_modes(session_id: &str)
        -> crucible_core::types::mode::SessionModes = session_list_modes(&session_id);
    }

    forward_rpc! {
        Safe ProvidersList =>
        list_providers(kiln_path: Option<&std::path::Path> => kiln_path.map(Path::to_path_buf))
        -> Vec<ProviderInfo> = list_providers(kiln_path.as_deref());
    }

    // models.list and agents.list_profiles: the browser calls them through
    // `POST /api/rpc/{method}` now (Simplification Plan step 19), so these
    // forwarders are gone. `DaemonClient::list_all_models`/
    // `agents_list_profiles` stay (item 9 is a separate pass).

    forward_rpc! {
        Once SessionSetPluginApproval =>
        session_set_plugin_approval(session_id: &str, plugin: &str, approval: crucible_core::session::PluginApproval)
        -> () = session_set_plugin_approval(&session_id, &plugin, approval);
    }

    forward_rpc! {
        Safe SessionGetPluginApproval =>
        session_get_plugin_approval(session_id: &str, plugin: &str)
        -> crucible_core::session::PluginApproval = session_get_plugin_approval(&session_id, &plugin);
    }

    forward_rpc! {
        Safe SessionListPluginApprovals =>
        session_list_plugin_approvals(session_id: &str)
        -> std::collections::BTreeMap<String, crucible_core::session::PluginApproval> = session_list_plugin_approvals(&session_id);
    }

    // Used only by this module's own startup auto-registration of the
    // operator-configured kiln path, which is a local, trusted decision, not
    // a browser request.
    forward_rpc! {
        Once ProjectRegister =>
        project_register(path: &Path)
        -> crucible_core::Project = project_register(&path);
    }

    // The HTTP route uses this one: the browser is never the local user at
    // the machine, so every registration it asks for is untrusted —
    // `project_manager::untrusted_root_refusal` refuses a credential store or
    // the user's config/state tree on top of the floor every caller gets.
    forward_rpc! {
        Once ProjectRegister =>
        project_register_untrusted(path: &Path)
        -> crucible_core::Project = project_register_untrusted(&path);
    }

    // The HTTP route's own rollback calls this after a registration lands
    // outside a configured `[web] registration_roots` entry; the browser
    // reaches `project.unregister` itself through `POST /api/rpc/{method}`
    // now (Simplification Plan step 19 item 3), so this method keeps only
    // that internal caller.
    forward_rpc! {
        Once ProjectUnregister =>
        project_unregister(path: &Path)
        -> () = project_unregister(&path);
    }

    // project.list: the browser calls it through `POST /api/rpc/{method}`
    // now (Simplification Plan step 19 item 3), so this forwarder is gone;
    // `routes/project.rs`'s own route was its only caller.

    pub async fn scm_clone(
        &self,
        url: &str,
        dest: Option<&Path>,
        name: Option<&str>,
    ) -> anyhow::Result<crucible_daemon::ScmCloneResponse> {
        // Single attempt: a connection error after the
        // clone started would re-run `git clone` (the DestExists check turns
        // that into a confusing error over a partial dir). One attempt only.
        let daemon = self.daemon.read().await;
        daemon.scm_clone(url, dest, name).await
    }

    // fs.list_dir: the browser calls it through `POST /api/rpc/{method}` now
    // (Simplification Plan step 19 item 3, the fs migration), so this
    // forwarder is gone; `routes/fs.rs` (deleted) was its only caller.

    // diff.get/file/comment/resolve_comment/delete_comment/comments: the
    // browser calls them through `POST /api/rpc/{method}` now
    // (Simplification Plan step 19), so these forwarders are gone.
    // `DaemonClient::diff_get`/`diff_file_request`/`diff_comment`/
    // `diff_resolve_comment`/`diff_delete_comment`/`diff_comments` stay
    // (item 9 is a separate pass); `daemon_retry_tests.rs` now proves the
    // `Once` replay policy of a decision through `rpc_forward` directly,
    // since no named forwarder is left to call.

    forward_rpc! {
        Safe FsRead =>
        fs_read(request: &crucible_core::file_write::FileReadRequest)
        -> serde_json::Value = fs_read(&request);
    }

    forward_rpc! {
        Once FsWrite =>
        fs_write(request: &crucible_core::file_write::FileWriteRequest)
        -> serde_json::Value = fs_write(&request);
    }

    // fs.move/fs.mkdir/fs.trash: the browser calls them through
    // `POST /api/rpc/{method}` now (Simplification Plan step 19 item 3), so
    // these forwarders are gone; `routes/fs.rs` (deleted) was their only
    // caller.

    // project.get: the browser calls it through `POST /api/rpc/{method}` now
    // (Simplification Plan step 19 item 3), so this forwarder is gone;
    // `routes/project.rs`'s own route was its only caller.

    forward_rpc! {
        Once WebhookReceive =>
        webhook_receive(name: String, headers: std::collections::HashMap<String, String>, body: String)
        -> crucible_core::protocol::requests::WebhookReceiveReply = webhook_receive(name, headers, body);
    }

    forward_rpc! {
        Safe SessionRenderMarkdown =>
        session_render_markdown(
            session_id: &str, include_timestamps: Option<bool>,
            include_tokens: Option<bool>, include_tools: Option<bool>,
            max_content_length: Option<usize>,
        )
        -> String = session_render_markdown(&session_id, include_timestamps, include_tokens, include_tools, max_content_length);
    }

    forward_rpc! {
        Safe ClientStateGet =>
        client_state_get(client: &str, key: &str)
        -> Option<serde_json::Value> = client_state_get(&client, &key);
    }

    forward_rpc! {
        Once ClientStateSet =>
        client_state_set(client: &str, key: &str, value: serde_json::Value)
        -> () = client_state_set(&client, &key, value);
    }
}

pub struct EventBroker {
    sessions: RwLock<HashMap<String, broadcast::Sender<SessionEvent>>>,
}

impl Default for EventBroker {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBroker {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Raw reception for transport fixtures. Production readers hold an EventStream.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn subscribe(&self, session_id: &str) -> broadcast::Receiver<SessionEvent> {
        let mut sessions = self.sessions.write().await;
        let tx = sessions
            .entry(session_id.to_string())
            .or_insert_with(|| broadcast::channel(EVENT_CHANNEL_CAPACITY).0);
        tx.subscribe()
    }

    /// Route one daemon event to the SSE streams that should see it.
    ///
    /// The wildcard is symmetric on the daemon's side — an event addressed to
    /// `"*"` reaches every connected client (`daemon/src/server/core.rs`) —
    /// because such an event belongs to no single session by construction. Two
    /// exist: `stream_gap`, which reports that this connection's event stream
    /// lost N events and cannot say which sessions they came from, and
    /// `ui_style_changed`'s config-level pushes.
    ///
    /// Without the fan-out below they arrived here and were dropped: no
    /// per-session sender is keyed `"*"`, so the exact-match lookup found
    /// nothing and returned success. A dropped `stream_gap` is the worst case —
    /// it is the marker that stops a transcript with a hole in it from being
    /// silently wrong.
    async fn dispatch(&self, event: SessionEvent) {
        let sessions = self.sessions.read().await;
        if event.session_id == crucible_daemon::subscription::WILDCARD_SESSION {
            for tx in sessions.values() {
                let _ = tx.send(event.clone());
            }
            return;
        }
        if let Some(tx) = sessions.get(&event.session_id) {
            let _ = tx.send(event);
        }
    }

    /// Publish one event into the fan-out, as the daemon's router task would.
    ///
    /// Test-only: production events enter through `dispatch` from the
    /// daemon's event stream, and no production caller may inject.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn publish_for_tests(&self, event: SessionEvent) {
        self.dispatch(event).await;
    }
}

pub async fn init_daemon(config: CliAppConfig) -> Result<AppState> {
    let (daemon, event_rx) = crucible_daemon::DaemonClient::connect_or_start_with_events()
        .await
        .map_err(|e| WebError::Daemon(format!("Failed to connect to daemon: {e}")))?;

    let broker = Arc::new(EventBroker::new());
    // The daemon owns the event router now, so it can rewire SSE on reconnect.
    let daemon = Arc::new(ReconnectingDaemon::new(daemon, event_rx, broker.clone()));

    // Auto-register the configured kiln so the frontend has a project on startup
    let kiln_path = config.kiln_path_str().unwrap_or_default();
    if !kiln_path.is_empty() {
        if let Err(e) = daemon
            .project_register(std::path::Path::new(&kiln_path))
            .await
        {
            tracing::warn!("Failed to auto-register kiln {kiln_path}: {e}");
        }
    }

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| WebError::Config(format!("Failed to create HTTP client: {e}")))?;

    Ok(AppState {
        daemon,
        events: broker,
        config: Arc::new(config),
        http_client,
        client_state_id: Arc::from(WEB_CLIENT_STATE_ID),
        // start_server overwrites this once the API key is resolved.
        remote_shell: false,
        recents_lock: Arc::new(tokio::sync::Mutex::new(())),
    })
}

fn spawn_event_router(
    mut event_rx: mpsc::UnboundedReceiver<SessionEvent>,
    broker: Arc<EventBroker>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            broker.dispatch(event).await;
        }
        tracing::warn!("Daemon event stream ended");
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a test SessionEvent
    fn test_event(session_id: &str, event_type: &str) -> SessionEvent {
        SessionEvent::new(
            session_id.to_string(),
            event_type.to_string(),
            serde_json::json!({}),
        )
    }

    #[tokio::test]
    async fn new_creates_empty_broker() {
        let broker = EventBroker::new();
        let sessions = broker.sessions.read().await;
        assert_eq!(sessions.len(), 0, "New broker should have no sessions");
    }

    #[tokio::test]
    async fn subscribe_creates_channel_for_new_session() {
        let broker = EventBroker::new();
        let _rx = broker.subscribe("session-1").await;

        let sessions = broker.sessions.read().await;
        assert_eq!(sessions.len(), 1, "Should have one session after subscribe");
        assert!(
            sessions.contains_key("session-1"),
            "Session key should exist"
        );
    }

    #[tokio::test]
    async fn subscribe_twice_same_session_returns_two_receivers() {
        let broker = EventBroker::new();
        let rx1 = broker.subscribe("session-1").await;
        let rx2 = broker.subscribe("session-1").await;

        // Both receivers should be valid (not panicked)
        drop(rx1);
        drop(rx2);

        let sessions = broker.sessions.read().await;
        assert_eq!(sessions.len(), 1, "Should still have only one session");
    }

    #[tokio::test]
    async fn dispatch_sends_event_to_subscribers() {
        let broker = Arc::new(EventBroker::new());
        let mut rx = broker.subscribe("session-1").await;

        let event = test_event("session-1", "test_event");
        broker.dispatch(event.clone()).await;

        // Receive the event
        let received = rx.recv().await;
        assert!(received.is_ok(), "Should receive event");
        let received_event = received.unwrap();
        assert_eq!(received_event.session_id, "session-1");
        assert_eq!(received_event.event, "test_event");
    }

    /// A wildcard-addressed event reaches every open stream, because it belongs
    /// to no session and the daemon has no way to attribute it to one.
    ///
    /// `stream_gap` is the case that matters: the exact-match lookup this
    /// replaces found no `"*"` sender and dropped the marker, so a browser
    /// rendering a transcript with a hole in it was never told.
    #[tokio::test]
    async fn a_wildcard_addressed_event_reaches_every_session_stream() {
        let broker = Arc::new(EventBroker::new());
        let mut rx1 = broker.subscribe("session-1").await;
        let mut rx2 = broker.subscribe("session-2").await;

        broker
            .dispatch(SessionEvent::new(
                crucible_daemon::subscription::WILDCARD_SESSION.to_string(),
                "stream_gap".to_string(),
                serde_json::json!({ "dropped": 5 }),
            ))
            .await;

        for rx in [&mut rx1, &mut rx2] {
            let got = rx.recv().await.expect("every stream must see the marker");
            assert_eq!(got.event, "stream_gap");
            assert_eq!(got.data["dropped"], 5);
        }
    }

    #[tokio::test]
    async fn dispatch_ignores_unsubscribed_sessions() {
        let broker = Arc::new(EventBroker::new());

        let event = test_event("unknown-session", "test_event");
        // Should not panic
        broker.dispatch(event).await;
    }

    #[tokio::test]
    async fn multiple_subscribers_both_receive_event() {
        let broker = Arc::new(EventBroker::new());
        let mut rx1 = broker.subscribe("session-1").await;
        let mut rx2 = broker.subscribe("session-1").await;

        let event = test_event("session-1", "broadcast_test");
        broker.dispatch(event.clone()).await;

        // Both receivers should get the event
        let received1 = rx1.recv().await;
        let received2 = rx2.recv().await;

        assert!(received1.is_ok(), "Subscriber 1 should receive event");
        assert!(received2.is_ok(), "Subscriber 2 should receive event");

        assert_eq!(received1.unwrap().event, "broadcast_test");
        assert_eq!(received2.unwrap().event, "broadcast_test");
    }

    #[tokio::test]
    async fn multiple_sessions_receive_only_their_events() {
        let broker = Arc::new(EventBroker::new());
        let mut rx1 = broker.subscribe("session-1").await;
        let mut rx2 = broker.subscribe("session-2").await;

        let event1 = test_event("session-1", "event_for_1");
        let event2 = test_event("session-2", "event_for_2");

        broker.dispatch(event1).await;
        broker.dispatch(event2).await;

        let received1 = rx1.recv().await.unwrap();
        let received2 = rx2.recv().await.unwrap();

        assert_eq!(received1.event, "event_for_1");
        assert_eq!(received2.event, "event_for_2");
    }
}

#[cfg(test)]
#[path = "daemon_retry_tests.rs"]
mod retry_tests;
