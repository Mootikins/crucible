use super::forwarding::ReplayPolicy;
use crate::{Result, WebError};
use crucible_core::config::CliAppConfig;
use crucible_core::protocol::requests::{GetBacklinksReply, SessionCreateRequest, VectorHit};
use crucible_core::protocol::RpcMethod;
use crucible_daemon::{DaemonClient, SessionEvent};
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
            Box::pin(client.rpc_ping(()))
        })
        .await
    }

    /// The one generic forwarder behind `POST /api/rpc/{method}`
    /// (`routes/rpc.rs`). Sends `params` as the request body of `method`,
    /// unread and untouched, and hands back the daemon's reply the same way.
    ///
    /// Every named forwarder above declares its own replay policy by hand,
    /// because each one already knows whether its call is a read or a
    /// write. This one carries an arbitrary [`RpcMethod`] chosen at the HTTP
    /// layer, so it asks [`RpcMethod::is_replay_safe`] instead — the same
    /// exhaustive table `routes/rpc.rs`'s own `browser_may_call` is modelled
    /// on, so a method added to `rpc_methods!` does not compile until
    /// someone decides both questions. Before this method existed here,
    /// every route on this passthrough was `Once`, so a read that lost its
    /// connection failed outright rather than retrying — the first read
    /// through the route after a daemon restart, in particular, where the
    /// old per-route `Safe` reads used to just retry.
    pub async fn rpc_forward(
        &self,
        method: RpcMethod,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let policy = if method.is_replay_safe() {
            ReplayPolicy::Safe
        } else {
            ReplayPolicy::Once
        };
        self.forward_rpc(policy, method, move |daemon| {
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

    // `DaemonClient::mcp_status` itself stays for now: deleting a
    // hand-written `DaemonClient` method that only forwards one row is
    // step 19 item 9, a separate pass.

    // `DaemonClient::skills_list`/`skills_get`/`skills_search` stay, because
    // they reshape an ergonomic `&Path` argument into the wire request's
    // `String` field, which the CLI still calls directly (item 9).

    forward_rpc! {
        /// Create a session and have the daemon resolve + configure its agent in
        /// one call (ACP profile or config-derived internal defaults). The daemon
        /// owns default resolution, so the web never builds its own copy.
        Once SessionCreate =>
        session_create(request: SessionCreateRequest)
        -> crucible_core::session::SessionSummary = session_create(request);
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
        Once SessionArchive =>
        session_archive(session_id: &str)
        -> serde_json::Value = session_archive(&session_id);
    }

    forward_rpc! {
        Once SessionDelete =>
        session_delete(session_id: &str)
        -> serde_json::Value = session_delete(&session_id);
    }

    forward_rpc! {
        Once SessionClear =>
        session_clear(session_id: &str)
        -> () = session_clear(&session_id);
    }

    /// `DaemonClient::session_undo` was a thin forwarder with no transform of
    /// its own; gone per step 19 item 9. This hand-written forwarder stays
    /// (the `forward_rpc!` macro only spells a single `ident(args)` call, and
    /// this one also unwraps the reply's `undone` field), calling the
    /// generated `rpc_session_undo` and building its `Scoped<UndoCount>` body.
    pub async fn session_undo(
        &self,
        session_id: &str,
        count: usize,
    ) -> anyhow::Result<Vec<crucible_core::types::UndoSummary>> {
        let session_id = session_id.to_owned();
        self.forward_rpc(ReplayPolicy::Once, RpcMethod::SessionUndo, move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move {
                let reply = daemon
                    .rpc_session_undo(crucible_core::protocol::requests::Scoped::new(
                        session_id,
                        crucible_core::protocol::requests::UndoCount { count: Some(count) },
                    ))
                    .await?;
                Ok(reply.undone)
            })
        })
        .await
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
        /// Write one session knob. One RPC method serves every
        /// [`crucible_core::types::KnobValue`] variant — model, mode, context
        /// strategy, precognition, plugin turn limit — so a knob added later
        /// needs no sibling row here.
        Once SessionKnobSet =>
        session_knob_set(session_id: &str, value: crucible_core::types::KnobValue)
        -> () = session_knob_set(&session_id, value);
    }

    forward_rpc! {
        Safe SessionListModels =>
        session_list_models(session_id: &str)
        -> Vec<String> = session_list_models(&session_id);
    }

    forward_rpc! {
        Safe SessionListModes =>
        session_list_modes(session_id: &str)
        -> crucible_core::types::mode::SessionModes = session_list_modes(&session_id);
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
    // outside a configured `[web] registration_roots` entry; this method
    // keeps only that internal caller.
    forward_rpc! {
        Once ProjectUnregister =>
        project_unregister(path: &Path)
        -> () = project_unregister(&path);
    }

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
