use super::forwarding::ReplayPolicy;
use crate::{Result, WebError};
use crucible_core::config::CliAppConfig;
use crucible_daemon::rpc::RpcMethod;
use crucible_daemon::{agent_manager::providers::ProviderInfo, DaemonClient, SessionEvent};
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

const EVENT_CHANNEL_CAPACITY: usize = 256;

#[derive(Clone)]
pub struct AppState {
    pub daemon: Arc<ReconnectingDaemon>,
    pub events: Arc<EventBroker>,
    pub config: Arc<CliAppConfig>,
    pub http_client: reqwest::Client,
    /// Where the web UI's serialized pane layout is persisted (JSON blob,
    /// opaque to the server). Tests point this at a tempdir.
    pub layout_path: Arc<std::path::PathBuf>,
    /// Whether non-loopback terminal/shell access is active (opt-in env var
    /// AND an API key configured) — surfaced to the frontend via /api/config
    /// so the terminal panel knows whether to connect from a LAN client.
    pub remote_shell: bool,
    /// Stale-while-revalidate cache for slow daemon catalog calls
    /// (agent profiles, providers) — see `services::catalog`.
    pub swr: Arc<crate::services::catalog::SwrCache>,
    /// Serializes /api/recents read-modify-writes (concurrent records would
    /// clobber each other's entries).
    pub recents_lock: Arc<tokio::sync::Mutex<()>>,
    /// Serializes the read-compare-write of one note against itself, so the
    /// kiln write routes cannot lose a change to their own concurrency.
    pub write_locks: Arc<PathLocks>,
}

/// One lock per file, so two writes to one note are ordered and two writes to
/// two notes are not.
///
/// `PUT` and `PATCH /api/kiln/file` each read the file, compare the caller's
/// base against what they read, and then write. Without a lock the compare is
/// a promise about a moment that has passed: a second writer can land between
/// the read and the write, and the first write removes its change with no
/// refusal — the very race `refuse_if_base_is_stale` exists to report. The
/// merge in `PUT` widens the window, because it reads the disk a second time.
///
/// The lock is per web server. Two servers over one kiln still race, and a
/// process outside this one always could; the recorded answer to that is one
/// daemon-owned write door, not a second copy of this map.
///
/// The map keeps one entry per note written in this process's lifetime. Notes
/// are bounded by the kilns on disk, so it does not grow without end, and an
/// entry is one path plus one mutex.
#[derive(Default)]
pub struct PathLocks {
    locks: dashmap::DashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>,
}

impl PathLocks {
    /// Take this file's lock, waiting for whoever holds it. Hold the guard
    /// across the read, the compare AND the write.
    pub async fn lock(&self, path: &Path) -> tokio::sync::OwnedMutexGuard<()> {
        let mutex = self
            .locks
            // The entry API holds one shard, not the map, and the clone
            // happens under it, so no await sits between the lookup and the
            // handle a waiter needs.
            .entry(lock_key(path))
            .or_default()
            .clone();
        mutex.lock_owned().await
    }
}

/// The key one note's writes share: its canonical directory plus its file
/// name.
///
/// The file itself cannot be canonicalized, because a write may be creating
/// it; its parent can, and both write routes validate that parent before they
/// reach here. So `/kiln/./Note.md` and `/symlink-to-kiln/Note.md` take one
/// lock, and a path with no parent or no name falls back to itself — it names
/// nothing writable, and a lock on a key nothing else uses harms nobody.
fn lock_key(path: &Path) -> PathBuf {
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .unwrap_or_else(|_| parent.to_path_buf())
            .join(name),
        _ => path.to_path_buf(),
    }
}

/// Default persistence location for the web UI layout:
/// `~/.config/crucible/web-layout.json` (alongside `api_key`).
pub fn default_layout_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("crucible")
        .join("web-layout.json")
}

/// Layout file for `--standalone` instances: same directory, separate file,
/// so a debug/test web server never overwrites the installed instance's
/// restored workspace.
pub fn standalone_layout_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("crucible")
        .join("web-layout.standalone.json")
}

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
    /// Session ids that must be re-subscribed on every reconnect. Chat sessions
    /// re-subscribe themselves via the browser `EventSource`; process-wide
    /// channels with no browser-driven re-subscribe (e.g. the file-watch
    /// `"system"` channel) live here so a daemon restart doesn't silently kill
    /// them. See `subscribe_sticky`.
    sticky_subscriptions: std::sync::Mutex<std::collections::HashSet<String>>,
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
            sticky_subscriptions: std::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// Subscribe the daemon client to `session_id` and record it so the
    /// subscription is re-issued on every reconnect. Idempotent.
    ///
    /// Callers (the `/api/fs/events` handler) call this once per SSE connection
    /// with the shared key `"system"`. The `"system"` entry is intentionally
    /// NEVER removed: it is one cheap, process-wide subscription shared by all
    /// browser SSE connections, and the daemon's only cost is keeping one
    /// broadcast fan-out key alive. Refcounted teardown would buy nothing, so
    /// this is an accepted, bounded single-entry "leak" for Phase 1.
    pub async fn subscribe_sticky(&self, session_id: &str) -> anyhow::Result<()> {
        self.sticky_subscriptions
            .lock()
            .unwrap()
            .insert(session_id.to_string());
        self.session_subscribe(&[session_id]).await.map(|_| ())
    }

    /// Every forwarder declares replay safety. A lost response to a write is
    /// ambiguous, so only replay-safe calls reconnect and submit again.
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
            Err(err) if policy == ReplayPolicy::Safe && Self::is_connection_error(&err) => {
                tracing::warn!(
                    method = method.as_str(),
                    error = %err,
                    "Daemon connection failed, reconnecting and retrying once"
                );
                self.reconnect_if_stale(observed_generation).await?;

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
        *daemon = new_daemon.without_timeout_retries();

        // Rewire SSE: abort the old router (its event_rx died with the old
        // connection) and point a fresh one at the same broker, so fan-out
        // survives a daemon restart instead of staying dead until a manual
        // restart. (Live per-session subscriptions still need re-issuing by the
        // client after a reconnect.)
        let new_router = spawn_event_router(event_rx, self.broker.clone());
        if let Ok(mut guard) = self.router.lock() {
            if let Some(old) = guard.replace(new_router) {
                old.abort();
            }
        }

        // Re-issue sticky subscriptions (e.g. the file-watch "system" channel)
        // on the fresh connection, directly through the held write guard so we
        // don't re-enter `forward_rpc` (which would deadlock on the
        // read lock). Best-effort: a failure here is retried on the next
        // reconnect. Browser-driven per-session subscriptions re-issue
        // themselves via `EventSource`, so they are NOT in this set.
        let sticky: Vec<String> = self
            .sticky_subscriptions
            .lock()
            .unwrap()
            .iter()
            .cloned()
            .collect();
        if !sticky.is_empty() {
            let borrowed: Vec<&str> = sticky.iter().map(String::as_str).collect();
            if let Err(e) = daemon.session_subscribe(&borrowed).await {
                tracing::warn!(error = %e, "Failed to re-issue sticky subscriptions after reconnect");
            }
        }

        self.generation.fetch_add(1, Ordering::AcqRel);
        tracing::warn!("Daemon reconnected; SSE fan-out rewired to the new event stream");
        Ok(())
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
        Safe KilnList =>
        kiln_list()
        -> Vec<serde_json::Value> = kiln_list();
    }

    forward_rpc! {
        Safe ListNotes =>
        list_notes(kiln_path: &Path, path_filter: Option<&str> => path_filter.map(str::to_owned))
        -> Vec<crucible_daemon::rpc_client::NoteListRow> = list_notes(&kiln_path, path_filter.as_deref(), None);
    }

    forward_rpc! {
        Safe GetNoteByName =>
        get_note_by_name(kiln_path: &Path, name: &str)
        -> Option<serde_json::Value> = get_note_by_name(&kiln_path, &name, None);
    }

    forward_rpc! {
        Safe GetBacklinks =>
        get_backlinks(kiln_path: &Path, name: &str)
        -> Option<serde_json::Value> = get_backlinks(&kiln_path, &name, None);
    }

    forward_rpc! {
        Safe KilnGraph =>
        kiln_graph(kiln_path: &Path)
        -> serde_json::Value = kiln_graph(&kiln_path, None);
    }

    forward_rpc! {
        Safe SuggestLinks =>
        suggest_links(kiln_path: &Path, text: &str)
        -> Vec<serde_json::Value> = suggest_links(&kiln_path, &text, None);
    }

    forward_rpc! {
        Safe SearchVectors =>
        search_vectors(kiln_path: &Path, vector: &[f32], limit: usize)
        -> Vec<crucible_daemon::VectorHit> = search_vectors(&kiln_path, &vector, limit, None);
    }

    forward_rpc! {
        /// Embed a query string into a vector via the kiln's configured embedding
        /// provider (the first half of semantic search; feed the result to
        /// [`Self::search_vectors`]).
        Safe EmbedQuery =>
        embed_query(kiln_path: &Path, text: &str)
        -> Vec<f32> = embed_query(&kiln_path, &text);
    }

    forward_rpc! {
        /// Ripgrep-style content search. `root` containment (registered project or
        /// open kiln) is enforced daemon-side. `regex` switches `query` from
        /// literal substring to regex matching.
        Safe SearchGrep =>
        search_grep(
            root: &str, query: &str, regex: bool,
            glob: Option<&str> => glob.map(str::to_owned),
            limit: usize, case_insensitive: bool,
        )
        -> crucible_daemon::GrepSearchResponse = search_grep(&root, &query, regex, glob.as_deref(), limit, case_insensitive);
    }

    forward_rpc! {
        Safe McpStatus =>
        mcp_status()
        -> serde_json::Value = mcp_status();
    }

    forward_rpc! {
        Safe SkillsList =>
        skills_list(kiln: &Path, scope_filter: Option<&str> => scope_filter.map(str::to_owned))
        -> serde_json::Value = skills_list(&kiln, scope_filter.as_deref());
    }

    forward_rpc! {
        Safe SkillsGet =>
        skills_get(name: &str, kiln: &Path)
        -> serde_json::Value = skills_get(&name, &kiln);
    }

    forward_rpc! {
        Safe SkillsSearch =>
        skills_search(query: &str, kiln: &Path, limit: Option<usize>)
        -> serde_json::Value = skills_search(&query, &kiln, limit);
    }

    forward_rpc! {
        /// Create a session and have the daemon resolve + configure its agent in
        /// one call (ACP profile or config-derived internal defaults). The daemon
        /// owns default resolution, so the web never builds its own copy.
        Once SessionCreate =>
        session_create_with_agent(
            params: crucible_daemon::rpc_client::SessionCreateParams,
            agent: crucible_daemon::rpc_client::SessionAgentSpec,
        )
        -> serde_json::Value = session_create_with_agent(params, agent);
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
        -> serde_json::Value = session_list(
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
        -> serde_json::Value = session_search(&query, &kilns, limit);
    }

    forward_rpc! {
        Safe SessionGet =>
        session_get(session_id: &str)
        -> serde_json::Value = session_get(&session_id);
    }

    forward_rpc! {
        Once SessionResumeFromStorage =>
        session_resume_from_storage(session_id: &str, limit: Option<usize>, offset: Option<usize>)
        -> serde_json::Value = session_resume_from_storage(&session_id, limit, offset);
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

    pub async fn session_subscribe(
        &self,
        session_ids: &[&str],
    ) -> anyhow::Result<serde_json::Value> {
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
        session_send_message(session_id: &str, content: &str)
        -> String = session_send_message(&session_id, &content, true);
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
        Once SessionSwitchModel =>
        session_switch_model(session_id: &str, model_id: &str)
        -> () = session_switch_model(&session_id, &model_id);
    }

    forward_rpc! {
        Once SessionSetMode =>
        session_set_mode(session_id: &str, mode_id: &str)
        -> () = session_set_mode(&session_id, &mode_id);
    }

    forward_rpc! {
        /// Beside `session_set_mode` rather than in `daemon_session_config`: `mode`
        /// is not a `config/` knob — switching it changes tool policy, not a scalar
        /// setting — and it has its own route pair. A settings panel that can set a
        /// value it cannot read is how a stale control gets shown.
        Safe SessionGetMode =>
        session_get_mode(session_id: &str)
        -> Option<String> = session_get_mode(&session_id);
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
        /// Plugin status slots for a session, forwarded as the daemon shaped them
        /// (`{"status": [{key, plugin, text, level}, …]}`).
        Safe SessionStatus =>
        session_status(session_id: &str)
        -> serde_json::Value = session_status(&session_id);
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

    forward_rpc! {
        /// List all chat models across providers without an active session.
        Safe ModelsList =>
        list_all_models(kiln_path: Option<&std::path::Path> => kiln_path.map(Path::to_path_buf))
        -> Vec<String> = list_all_models(kiln_path.as_deref());
    }

    forward_rpc! {
        /// List ACP agent profiles (builtins + config) with probed availability.
        Safe AgentsListProfiles =>
        agents_list_profiles()
        -> serde_json::Value = agents_list_profiles();
    }

    forward_rpc! {
        Once SessionSetPrecognition =>
        session_set_precognition(session_id: &str, enabled: bool)
        -> () = session_set_precognition(&session_id, enabled);
    }

    forward_rpc! {
        Safe SessionGetPrecognition =>
        session_get_precognition(session_id: &str)
        -> bool = session_get_precognition(&session_id);
    }

    forward_rpc! {
        Once ProjectRegister =>
        project_register(path: &Path)
        -> crucible_core::Project = project_register(&path);
    }

    forward_rpc! {
        Once ProjectUnregister =>
        project_unregister(path: &Path)
        -> () = project_unregister(&path);
    }

    forward_rpc! {
        Safe ProjectList =>
        project_list()
        -> Vec<crucible_core::Project> = project_list();
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

    forward_rpc! {
        Safe FsListDir =>
        fs_list_dir(root: &str, rel_path: &str, show_ignored: bool, show_hidden: bool)
        -> serde_json::Value = fs_list_dir(&root, &rel_path, show_ignored, show_hidden);
    }

    forward_rpc! {
        Once FsMove =>
        fs_move(root: &str, kind: &str, from_rel: &str, to_rel: &str)
        -> serde_json::Value = fs_move(&root, &kind, &from_rel, &to_rel);
    }

    forward_rpc! {
        Once FsMkdir =>
        fs_mkdir(root: &str, kind: &str, rel_path: &str)
        -> () = fs_mkdir(&root, &kind, &rel_path);
    }

    forward_rpc! {
        Once FsTrash =>
        fs_trash(root: &str, kind: &str, rel_path: &str)
        -> serde_json::Value = fs_trash(&root, &kind, &rel_path);
    }

    forward_rpc! {
        Safe ProjectGet =>
        project_get(path: &Path)
        -> Option<crucible_core::Project> = project_get(&path);
    }

    forward_rpc! {
        Once WebhookReceive =>
        webhook_receive(name: String, headers: std::collections::HashMap<String, String>, body: String)
        -> serde_json::Value = call("webhook.receive", serde_json::json!({ "name": name, "headers": headers, "body": body, }));
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

    pub async fn remove_session(&self, session_id: &str) {
        self.sessions.write().await.remove(session_id);
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
        layout_path: Arc::new(default_layout_path()),
        // start_server overwrites this once the API key is resolved.
        remote_shell: false,
        swr: Arc::new(crate::services::catalog::SwrCache::default()),
        recents_lock: Arc::new(tokio::sync::Mutex::new(())),
        write_locks: Arc::new(PathLocks::default()),
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
    async fn remove_session_deletes_channel() {
        let broker = EventBroker::new();
        let _rx = broker.subscribe("session-1").await;

        {
            let sessions = broker.sessions.read().await;
            assert_eq!(sessions.len(), 1);
        }

        broker.remove_session("session-1").await;

        let sessions = broker.sessions.read().await;
        assert_eq!(sessions.len(), 0, "Session should be removed");
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
