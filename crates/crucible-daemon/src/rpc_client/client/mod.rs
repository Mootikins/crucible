//! Daemon client implementation
//!
//! Provides a client for communicating with the Crucible daemon over Unix sockets.
//! Supports both request/response RPC calls and asynchronous event streaming.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::os::unix::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, error, trace, warn};

/// Reject socket paths the kernel can't use before we try to bind/connect to them.
///
/// Unix socket paths must fit in `sun_path` (108 bytes on Linux, 104 on macOS).
/// Hitting this in the retry loop of [`DaemonClient::connect_or_start`] wastes
/// ~50s on exponential backoff since a freshly-spawned daemon can't bind either.
fn validate_socket_path(path: &Path) -> Result<()> {
    SocketAddr::from_pathname(path).with_context(|| {
        format!(
            "invalid daemon socket path {:?} — set CRUCIBLE_SOCKET to a shorter path",
            path
        )
    })?;
    Ok(())
}

// Submodules for logical organization of RPC methods.
// Each submodule adds methods to `DaemonClient` via `impl` blocks and
// defines the associated request/response types near the methods that use
// them. The shared infrastructure (connection, JSON-RPC dispatch, error
// retries) lives here in `mod.rs`.
pub mod agent;
pub mod lua;
pub mod review;
pub mod session;
pub mod storage;
pub mod subscription;
pub mod types;
pub mod workflow;

// Re-export public types so the original `rpc_client::client::<Type>` paths
// still resolve after the split. Only types the parent `rpc_client` module
// re-exports externally need to land here; the rest remain reachable at
// `client::<submodule>::<Type>` if needed internally.
pub use lua::{
    LuaDiscoverPluginsRequest, LuaDiscoverPluginsResponse, LuaGenerateStubsRequest,
    LuaGenerateStubsResponse, LuaInitSessionRequest, LuaInitSessionResponse,
    LuaPluginHealthRequest, LuaPluginHealthResponse, LuaRegisterCommandsRequest,
    LuaRegisterCommandsResponse, LuaRunPluginTestsRequest, LuaRunPluginTestsResponse,
    LuaShutdownSessionRequest, LuaShutdownSessionResponse, PluginTestFailure,
    PluginTestLoadFailure,
};
// `SessionCreateRequest` is exported (it was `#[cfg(test)]`-only, for the
// wire-format tests below) because the daemon's own `handle_session_create`
// now deserializes it: the client struct IS the server's contract rather than
// a shape the server re-derives by hand.
pub use session::{SessionAgentSpec, SessionCreateParams, SessionCreateRequest};
pub use types::{DaemonCapabilities, NameRequest, SessionEvent, VersionCheck};

use session::SessionIdRequest;
use types::{extract_string_array, EmptyParams};

type PendingRequests = Arc<Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>>;

/// Client for communicating with the Crucible daemon
///
/// The client supports two modes:
/// - Simple mode: Created with `connect()` or `connect_to()`, suitable for RPC-only usage
/// - Event mode: Created with `connect_with_events()`, supports both RPC and async events
///
/// In event mode, a background task continuously reads from the socket, routing:
/// - RPC responses to their waiting callers
/// - Async events to the event channel
pub struct DaemonClient {
    writer: Arc<Mutex<OwnedWriteHalf>>,
    next_id: AtomicU64,
    pending_requests: PendingRequests,
    reader_task: Option<JoinHandle<()>>,
    // For simple mode (no background reader)
    simple_reader: Option<Mutex<BufReader<tokio::net::unix::OwnedReadHalf>>>,
}

impl Drop for DaemonClient {
    fn drop(&mut self) {
        if let Some(task) = self.reader_task.take() {
            task.abort();
        }
    }
}

impl DaemonClient {
    /// Connect to the daemon at the default socket path (simple mode)
    pub async fn connect() -> Result<Self> {
        let path = crucible_core::protocol::socket_path();
        Self::connect_to(&path).await
    }

    /// Connect to daemon or start it if not running (simple mode).
    ///
    /// Checks daemon version after connecting. If version mismatches (stale daemon),
    /// shuts down the old daemon and starts a fresh one.
    pub async fn connect_or_start() -> Result<Self> {
        validate_socket_path(&crucible_core::protocol::socket_path())?;
        if let Ok(client) = Self::connect().await {
            if client.verify_or_restart().await {
                return Ok(client);
            }
        }
        Self::start_and_retry(Self::connect).await
    }

    /// Connect to daemon or start it if not running (event mode).
    ///
    /// Returns event-mode client with receiver for streaming session events.
    /// Checks daemon version after connecting. If version mismatches (stale daemon),
    /// shuts down the old daemon and starts a fresh one.
    pub async fn connect_or_start_with_events(
    ) -> Result<(Self, mpsc::UnboundedReceiver<SessionEvent>)> {
        validate_socket_path(&crucible_core::protocol::socket_path())?;
        if let Ok((client, rx)) = Self::connect_with_events().await {
            if client.verify_or_restart().await {
                return Ok((client, rx));
            }
        }
        Self::start_and_retry(Self::connect_with_events).await
    }

    /// Check daemon version. Returns true if usable, false if restarted/needs restart.
    async fn verify_or_restart(&self) -> bool {
        match self.check_version().await {
            Ok(VersionCheck::Match) => true,
            Ok(VersionCheck::Mismatch {
                client: c,
                daemon: d,
            }) => {
                warn!(client_sha = %c, daemon_sha = %d, "Daemon version mismatch, restarting");
                let _ = self.shutdown().await;
                tokio::time::sleep(Duration::from_millis(100)).await;
                false
            }
            Err(e) => {
                debug!("Version check failed, assuming ok: {}", e);
                true
            }
        }
    }

    /// Start daemon and retry connecting with capped exponential backoff.
    async fn start_and_retry<T, F, Fut>(connect: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        Self::start_daemon().await?;

        let mut attempts = 0usize;
        for delay in Self::connect_backoff() {
            tokio::time::sleep(delay).await;
            attempts += 1;
            if let Ok(result) = connect().await {
                return Ok(result);
            }
            if attempts > 5 {
                warn!("Daemon not ready after {attempts} attempts");
            }
        }

        let log_path = crate::rpc_client::lifecycle::daemon_log_path();
        let tail = crate::rpc_client::lifecycle::read_log_tail(&log_path, 15);
        anyhow::bail!(
            "{}",
            Self::compose_connect_failure(attempts, &log_path, tail)
        )
    }

    /// Connect retry schedule: doubling from 50ms, capped at 1s, 8 attempts —
    /// ~4.6s total. A daemon that hasn't bound its socket by then is not
    /// coming up; the previous uncapped 10-doubling schedule left the user
    /// staring at nothing for 51 seconds.
    fn connect_backoff() -> impl Iterator<Item = Duration> {
        (0u32..8).map(|i| Duration::from_millis((50u64 << i).min(1000)))
    }

    /// The message shown when the spawned daemon never became reachable.
    /// Carries the *cause* (the daemon's own recent log output), not just the
    /// restart incantation — the log is where a startup crash actually lands.
    fn compose_connect_failure(
        attempts: usize,
        log_path: &Path,
        log_tail: Option<String>,
    ) -> String {
        let mut msg = format!(
            "Failed to connect to daemon after {attempts} attempts. \
             Try: cru daemon stop && cru daemon start (and `cru daemon logs` \
             or `cru doctor` to diagnose)."
        );
        match log_tail {
            Some(tail) => {
                msg.push_str(&format!(
                    "\nRecent daemon output ({}):\n{tail}",
                    log_path.display()
                ));
            }
            None => {
                msg.push_str(&format!(
                    "\nNo daemon output captured at {}.",
                    log_path.display()
                ));
            }
        }
        msg
    }

    /// Build the args for `cru <...> daemon serve`, forwarding `--config` (which
    /// is a global flag, so it must precede the subcommand) when a config path
    /// was resolved by the caller.
    fn daemon_serve_args(config: Option<&str>) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(cfg) = config.filter(|c| !c.is_empty()) {
            args.push("--config".to_string());
            args.push(cfg.to_string());
        }
        args.push("daemon".to_string());
        args.push("serve".to_string());
        args
    }

    async fn start_daemon() -> Result<()> {
        use std::process::Command;

        let exe = std::env::current_exe()?;

        // Guard: only spawn if current binary is the real `cru` CLI.
        // Test binaries (e.g. `storage_factory_integration-<hash>`) interpret
        // `daemon serve` as test filter patterns, which causes recursive fork
        // bombs — each spawned test binary runs tests that call connect_or_start()
        // again, spawning yet another copy of themselves.
        let exe_name = exe.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if exe_name != "cru" {
            anyhow::bail!(
                "Cannot auto-start daemon: current binary {:?} is not `cru`. \
                 Start the daemon manually with `cru daemon serve`.",
                exe
            );
        }

        // Forward the caller's --config (stashed by the CLI in CRUCIBLE_CONFIG)
        // so the cold-started daemon runs on the same config as the command that
        // spawned it.
        let args = Self::daemon_serve_args(std::env::var("CRUCIBLE_CONFIG").ok().as_deref());

        tracing::info!("Starting daemon: {:?} {:?}", exe, args);

        // Capture the daemon's output: a detached daemon that dies on startup
        // is otherwise a silent 4.6s timeout with no cause anywhere.
        let (out, err) = crate::rpc_client::lifecycle::daemon_log_stdio();
        Command::new(&exe)
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(out)
            .stderr(err)
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn daemon: {}", e))?;
        Ok(())
    }

    /// Connect to daemon at a specific socket path (simple mode)
    ///
    /// Simple mode does not support async events - use `connect_to_with_events`
    /// if you need to receive streaming events.
    pub async fn connect_to(path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(path).await?;
        let (read, write) = stream.into_split();

        Ok(Self {
            writer: Arc::new(Mutex::new(write)),
            next_id: AtomicU64::new(1),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            reader_task: None,
            simple_reader: Some(Mutex::new(BufReader::new(read))),
        })
    }

    /// Connect to the daemon with event handling (event mode)
    ///
    /// Returns a client and a receiver for async session events. A background
    /// task continuously reads from the socket, dispatching events to the
    /// receiver and routing RPC responses to their callers.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (client, mut event_rx) = DaemonClient::connect_with_events().await?;
    /// let client = Arc::new(client);
    ///
    /// // Subscribe to session events
    /// client.session_subscribe(&[session_id]).await?;
    ///
    /// // Events arrive via the channel
    /// while let Some(event) = event_rx.recv().await {
    ///     println!("Event: {} - {}", event.session_id, event.event_type);
    /// }
    /// ```
    pub async fn connect_with_events() -> Result<(Self, mpsc::UnboundedReceiver<SessionEvent>)> {
        let path = crucible_core::protocol::socket_path();
        Self::connect_to_with_events(&path).await
    }

    /// Connect to daemon at a specific socket path with event handling (event mode)
    pub async fn connect_to_with_events(
        path: &Path,
    ) -> Result<(Self, mpsc::UnboundedReceiver<SessionEvent>)> {
        let stream = UnixStream::connect(path).await?;
        let (read, write) = stream.into_split();

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let pending_requests: PendingRequests = Arc::new(Mutex::new(HashMap::new()));

        let reader_task = Self::spawn_reader_task(read, event_tx, pending_requests.clone());

        let client = Self {
            writer: Arc::new(Mutex::new(write)),
            next_id: AtomicU64::new(1),
            pending_requests,
            reader_task: Some(reader_task),
            simple_reader: None,
        };

        Ok((client, event_rx))
    }

    fn spawn_reader_task(
        read: tokio::net::unix::OwnedReadHalf,
        event_tx: mpsc::UnboundedSender<SessionEvent>,
        pending_requests: PendingRequests,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut reader = BufReader::new(read);
            let mut line = String::new();

            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => {
                        debug!("Daemon connection closed (EOF)");
                        break;
                    }
                    Ok(_) => {
                        trace!("Received line from daemon: {}", line.trim());

                        let msg: serde_json::Value = match serde_json::from_str(&line) {
                            Ok(m) => m,
                            Err(e) => {
                                warn!("Failed to parse daemon message: {}", e);
                                continue;
                            }
                        };

                        if Self::is_event(&msg) {
                            debug!("Detected event message from daemon");
                            Self::dispatch_event(&msg, &event_tx);
                        } else if let Some(id) = msg.get("id").and_then(|v| v.as_u64()) {
                            debug!(request_id = id, "Detected RPC response");
                            Self::dispatch_response(id, msg, &pending_requests).await;
                        } else {
                            trace!("Ignoring message without id or event type: {:?}", msg);
                        }
                    }
                    Err(e) => {
                        error!("Error reading from daemon: {}", e);
                        break;
                    }
                }
            }

            debug!("Reader task exiting");
        })
    }

    fn is_event(msg: &serde_json::Value) -> bool {
        matches!(
            msg.get("type").and_then(|t| t.as_str()),
            Some("event" | "replay_event")
        )
    }

    fn dispatch_event(msg: &serde_json::Value, event_tx: &mpsc::UnboundedSender<SessionEvent>) {
        let session_id = msg.get("session_id").and_then(|v| v.as_str()).unwrap_or("");
        let event_type = msg.get("event").and_then(|v| v.as_str()).unwrap_or("");

        debug!(
            session_id = %session_id,
            event_type = %event_type,
            "Dispatching daemon event to channel"
        );

        if session_id.is_empty() {
            warn!("Daemon event missing session_id: {:?}", msg);
        }

        let event = SessionEvent {
            session_id: session_id.to_string(),
            event_type: event_type.to_string(),
            data: msg.get("data").cloned().unwrap_or(serde_json::Value::Null),
        };

        if event_tx.send(event).is_err() {
            debug!("Event receiver dropped, stopping event dispatch");
        }
    }

    async fn dispatch_response(id: u64, msg: serde_json::Value, pending: &PendingRequests) {
        let mut pending = pending.lock().await;
        if let Some(tx) = pending.remove(&id) {
            if tx.send(msg).is_err() {
                debug!("Response receiver dropped for request {}", id);
            }
        } else {
            warn!("Received response for unknown request id: {}", id);
        }
    }

    /// Send a JSON-RPC request with automatic retry on transient failures.
    ///
    /// Retries up to 2 times with exponential backoff (200ms, 400ms) on timeout errors.
    /// RPC-level errors (application errors from the daemon) are NOT retried.
    pub async fn call_with_retry(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        const MAX_RETRIES: u32 = 2;
        const INITIAL_DELAY_MS: u64 = 200;

        let mut last_err = None;
        for attempt in 0..=MAX_RETRIES {
            match self.call(method, params.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if !Self::is_transient_error(&e) || attempt >= MAX_RETRIES {
                        return Err(e);
                    }
                    let delay_ms = INITIAL_DELAY_MS * 2u64.pow(attempt);
                    warn!(
                        method = %method,
                        attempt = attempt + 1,
                        delay_ms = delay_ms,
                        "RPC call timed out, retrying"
                    );
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Retry exhausted with no error")))
    }

    /// Transient error patterns that indicate a retry may succeed.
    const TRANSIENT_ERROR_PATTERNS: &[&str] = &[
        "timed out",
        "Request timeout",
        "deadline has elapsed",
        "connection reset",
        "broken pipe",
    ];

    fn is_transient_error(err: &anyhow::Error) -> bool {
        let msg = err.to_string();
        Self::TRANSIENT_ERROR_PATTERNS
            .iter()
            .any(|pattern| msg.contains(pattern))
    }

    /// Send a typed JSON-RPC request and deserialize the response.
    ///
    /// Wraps `call()` with automatic serialization/deserialization.
    pub async fn typed_call<Req, Resp>(&self, method: &str, params: Req) -> Result<Resp>
    where
        Req: serde::Serialize,
        Resp: serde::de::DeserializeOwned,
    {
        let result = self.call(method, serde_json::to_value(params)?).await?;
        Ok(serde_json::from_value(result)?)
    }

    /// Send a typed JSON-RPC request with an explicit per-request timeout.
    ///
    /// Wraps `call_with_timeout()` with automatic serialization/deserialization
    /// for long-running methods (e.g. `scm.clone`). Not retried — a clone that
    /// times out should surface, not silently restart.
    pub async fn typed_call_with_timeout<Req, Resp>(
        &self,
        method: &str,
        params: Req,
        timeout: Duration,
    ) -> Result<Resp>
    where
        Req: serde::Serialize,
        Resp: serde::de::DeserializeOwned,
    {
        let result = self
            .call_with_timeout(method, serde_json::to_value(params)?, timeout)
            .await?;
        Ok(serde_json::from_value(result)?)
    }

    /// Send a typed JSON-RPC request with retry and deserialize the response.
    ///
    /// Wraps `call_with_retry()` with automatic serialization/deserialization.
    pub async fn typed_call_with_retry<Req, Resp>(&self, method: &str, params: Req) -> Result<Resp>
    where
        Req: serde::Serialize,
        Resp: serde::de::DeserializeOwned,
    {
        let result = self
            .call_with_retry(method, serde_json::to_value(params)?)
            .await?;
        Ok(serde_json::from_value(result)?)
    }

    /// Send a typed JSON-RPC request and discard the response.
    ///
    /// Wraps `typed_call()` for methods that return unit (Ok(())).
    /// Discards the response value to avoid unused variable warnings.
    pub(super) async fn typed_unit_call<Req>(&self, method: &str, params: Req) -> Result<()>
    where
        Req: serde::Serialize,
    {
        let _: serde_json::Value = self.typed_call(method, params).await?;
        Ok(())
    }

    /// Send a typed JSON-RPC request with retry and discard the response.
    ///
    /// Wraps `typed_call_with_retry()` for methods that return unit (Ok(())).
    /// Discards the response value to avoid unused variable warnings.
    pub(super) async fn typed_unit_call_with_retry<Req>(
        &self,
        method: &str,
        params: Req,
    ) -> Result<()>
    where
        Req: serde::Serialize,
    {
        let _: serde_json::Value = self.typed_call_with_retry(method, params).await?;
        Ok(())
    }

    /// Shorthand for RPC methods that only take a session_id parameter.
    pub(super) async fn session_id_call(
        &self,
        method: &str,
        session_id: &str,
    ) -> Result<serde_json::Value> {
        self.typed_call(
            method,
            SessionIdRequest {
                session_id: session_id.to_string(),
            },
        )
        .await
    }

    /// Fetch a nullable field from a session-scoped RPC method.
    pub(super) async fn get_session_option<T>(
        &self,
        method: &str,
        session_id: &str,
        field: &str,
        extract: impl FnOnce(&serde_json::Value) -> Option<T>,
    ) -> Result<Option<T>> {
        let result: serde_json::Value = self
            .typed_call_with_retry(
                method,
                SessionIdRequest {
                    session_id: session_id.to_string(),
                },
            )
            .await?;
        Ok(result
            .get(field)
            .and_then(|v| if v.is_null() { None } else { extract(v) }))
    }

    /// Default per-request timeout for [`Self::call`] (event mode only).
    const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

    /// Send a JSON-RPC request and get the response, using the default 30s
    /// per-request timeout in event mode.
    pub async fn call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        self.call_with_timeout(method, params, Self::DEFAULT_TIMEOUT)
            .await
    }

    /// Send a JSON-RPC request with an explicit per-request timeout.
    ///
    /// Slow operations (e.g. `scm.clone` cloning a large repo) need a generous
    /// timeout well past the 30s default. The timeout only applies in event
    /// mode; simple mode blocks on a direct socket read.
    pub async fn call_with_timeout(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let mut req_str = serde_json::to_string(&request)?;
        req_str.push('\n');

        // Register the pending slot before sending, in BOTH modes. Event mode's
        // background reader routes into it; simple mode has no background reader,
        // so whichever caller happens to hold the read half routes *other*
        // callers' responses into theirs. One correlation table, two ways of
        // being fed — rather than event mode correlating by id and simple mode
        // trusting arrival order.
        let (response_tx, response_rx) = oneshot::channel();
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(id, response_tx);
        }

        // Send request
        {
            let mut writer = self.writer.lock().await;
            writer.write_all(req_str.as_bytes()).await?;
        }

        // Get response
        let response = if self.reader_task.is_some() {
            // Event mode: wait for background reader to route response
            match tokio::time::timeout(timeout, response_rx).await {
                Ok(Ok(response)) => response,
                Ok(Err(_)) => anyhow::bail!("Response channel closed unexpectedly"),
                Err(_) => {
                    // Clean up pending request on timeout
                    let mut pending = self.pending_requests.lock().await;
                    pending.remove(&id);
                    anyhow::bail!("Request timeout after {} seconds", timeout.as_secs())
                }
            }
        } else {
            // Simple mode: read directly
            let result = self.read_response_simple(id, response_rx).await;
            if result.is_err() {
                self.pending_requests.lock().await.remove(&id);
            }
            result?
        };

        if let Some(error) = response.get("error") {
            anyhow::bail!("RPC error: {}", error);
        }

        Ok(response
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    /// Read this request's own response off the socket.
    ///
    /// Simple mode has no background reader, so the caller that gets the read
    /// half becomes the reader for everyone. Two things follow, and the old
    /// version did neither:
    ///
    /// 1. **It must match the id.** It used to return the first line carrying an
    ///    `id`, whichever request that belonged to. Harmless only while the
    ///    daemon answered strictly FIFO — and the daemon is what changed.
    /// 2. **A line that is not ours must be delivered, not discarded.** Dropping
    ///    it would hang the caller it belonged to, which is a worse failure than
    ///    the mis-delivery being fixed. It goes into `pending_requests`, the same
    ///    table event mode routes through.
    ///
    /// The `select!` is what keeps that deadlock-free: a caller waits on *either*
    /// the read half or its own slot, so a caller whose response has already been
    /// routed by someone else never has to acquire the reader to collect it.
    async fn read_response_simple(
        &self,
        id: u64,
        mut response_rx: oneshot::Receiver<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let reader = self
            .simple_reader
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No reader available in event mode"))?;

        // Biased so an already-routed response wins over becoming the reader.
        let mut guard = tokio::select! {
            biased;
            routed = &mut response_rx => {
                return routed.context("response channel closed before the reply arrived");
            }
            guard = reader.lock() => guard,
        };

        loop {
            let mut line = String::new();
            if guard.read_line(&mut line).await? == 0 {
                anyhow::bail!("Connection closed by daemon");
            }

            let msg: serde_json::Value = serde_json::from_str(&line)?;

            let Some(msg_id) = msg.get("id").and_then(|v| v.as_u64()) else {
                // No id: either a server-pushed notification (skip it, a reply is
                // not necessarily the next line on the wire) or a parse-error
                // response for a request whose id the daemon never read. This
                // client always writes well-formed JSON, so the second cannot
                // arise from us — but answering it to whoever is reading is the
                // only thing that can be done with a reply that names no request.
                if msg.get("error").is_some() {
                    warn!("daemon returned an error response with no id: {msg}");
                    self.pending_requests.lock().await.remove(&id);
                    return Ok(msg);
                }
                continue;
            };

            if msg_id == id {
                self.pending_requests.lock().await.remove(&id);
                return Ok(msg);
            }

            // Another caller's reply. Hand it over; if nobody is waiting it is a
            // reply to a request that already timed out or was abandoned.
            match self.pending_requests.lock().await.remove(&msg_id) {
                Some(tx) => {
                    let _ = tx.send(msg);
                }
                None => warn!("Received response for unknown request id: {}", msg_id),
            }
        }
    }

    // =========================================================================
    // Basic RPC Methods
    // =========================================================================

    pub async fn ping(&self) -> Result<String> {
        let result: serde_json::Value = self.typed_call("ping", EmptyParams {}).await?;
        Ok(result.as_str().unwrap_or("").to_string())
    }

    pub async fn shutdown(&self) -> Result<()> {
        let _: serde_json::Value = self.typed_call("shutdown", EmptyParams {}).await?;
        Ok(())
    }

    pub async fn capabilities(&self) -> Result<DaemonCapabilities> {
        self.typed_call("daemon.capabilities", EmptyParams {}).await
    }

    pub async fn check_version(&self) -> Result<VersionCheck> {
        let caps = self.capabilities().await?;
        let client_sha = option_env!("CRUCIBLE_BUILD_SHA").unwrap_or("dev");
        let daemon_sha = caps.build_sha.as_deref().unwrap_or("unknown");

        if client_sha == daemon_sha {
            Ok(VersionCheck::Match)
        } else {
            Ok(VersionCheck::Mismatch {
                client: client_sha.to_string(),
                daemon: daemon_sha.to_string(),
            })
        }
    }

    // =========================================================================
    // Plugin Management RPC Methods
    // =========================================================================

    pub async fn plugin_reload(&self, name: &str) -> Result<serde_json::Value> {
        self.typed_call(
            "plugin.reload",
            NameRequest {
                name: name.to_string(),
            },
        )
        .await
    }

    pub async fn plugin_list(&self) -> Result<Vec<String>> {
        let result: serde_json::Value = self.typed_call("plugin.list", EmptyParams {}).await?;
        Ok(extract_string_array(&result, "plugins"))
    }

    /// Like [`plugin_list`] but returns the richer `plugin_info` array
    /// (name, version, source, state, dir, capability counts).
    pub async fn plugin_list_info(&self) -> Result<Vec<serde_json::Value>> {
        let result: serde_json::Value = self.typed_call("plugin.list", EmptyParams {}).await?;
        Ok(result
            .get("plugin_info")
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default())
    }

    /// What plugins published about themselves, as `key -> plugin -> value`.
    ///
    /// Values are opaque: the client passes them through so a contribution kind
    /// added later needs no change here.
    pub async fn plugin_publications(&self) -> Result<serde_json::Value> {
        let result: serde_json::Value = self
            .typed_call("plugin.publications", EmptyParams {})
            .await?;
        Ok(result
            .get("publications")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})))
    }

    /// Settings trees plugins declared, as `plugin -> tree`.
    ///
    /// `ui` is the frontend asking ("tui" or "web"); it drives the per-frontend
    /// hide flags and reaches Lua callbacks as `info.uiType`. Every
    /// function-valued field is evaluated per call, so this is a snapshot of
    /// what is true of this box now — not something to cache across a change.
    pub async fn plugin_options(&self, ui: &str) -> Result<serde_json::Value> {
        let result: serde_json::Value = self
            .typed_call("plugin.options", serde_json::json!({ "ui": ui }))
            .await?;
        Ok(result
            .get("options")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})))
    }

    /// Read one option, by its path through the settings tree.
    pub async fn plugin_option_get(
        &self,
        plugin: &str,
        path: &[String],
        ui: &str,
    ) -> Result<serde_json::Value> {
        let result: serde_json::Value = self
            .typed_call(
                "plugin.option_get",
                serde_json::json!({ "plugin": plugin, "path": path, "ui": ui }),
            )
            .await?;
        Ok(result
            .get("value")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    /// Write one option. The plugin's own setter decides what that means.
    pub async fn plugin_option_set(
        &self,
        plugin: &str,
        path: &[String],
        value: serde_json::Value,
        ui: &str,
    ) -> Result<()> {
        let _: serde_json::Value = self
            .typed_call(
                "plugin.option_set",
                serde_json::json!({ "plugin": plugin, "path": path, "value": value, "ui": ui }),
            )
            .await?;
        Ok(())
    }

    /// Press a `type = "execute"` node.
    pub async fn plugin_option_execute(
        &self,
        plugin: &str,
        path: &[String],
        ui: &str,
    ) -> Result<()> {
        let _: serde_json::Value = self
            .typed_call(
                "plugin.option_execute",
                serde_json::json!({ "plugin": plugin, "path": path, "ui": ui }),
            )
            .await?;
        Ok(())
    }

    /// Commands declared by loaded plugins: `plugin`, `name`, `description`,
    /// `hint`, `parameters`. Served from the daemon so TUI and web show the
    /// same slash-command set.
    pub async fn plugin_commands(&self) -> Result<Vec<serde_json::Value>> {
        let result: serde_json::Value = self.typed_call("plugin.commands", EmptyParams {}).await?;
        Ok(result
            .get("commands")
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default())
    }

    /// Invoke a plugin command. `args` is passed to the command's Lua `fn` as
    /// its single table argument.
    pub async fn plugin_run_command(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        #[derive(serde::Serialize)]
        struct RunCommandParams {
            name: String,
            args: serde_json::Value,
        }
        self.typed_call(
            "plugin.run_command",
            RunCommandParams {
                name: name.to_string(),
                args,
            },
        )
        .await
    }

    /// Install a plugin by URL. Synchronous (waits for the clone to
    /// finish) — can take 10+ seconds for first-clone over a slow network.
    pub async fn plugin_install(
        &self,
        url: &str,
        branch: Option<&str>,
        pin: Option<&str>,
    ) -> Result<serde_json::Value> {
        #[derive(serde::Serialize)]
        struct InstallParams {
            url: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            branch: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            pin: Option<String>,
        }
        self.typed_call(
            "plugin.install",
            InstallParams {
                url: url.to_string(),
                branch: branch.map(str::to_string),
                pin: pin.map(str::to_string),
            },
        )
        .await
    }

    /// Remove a plugin by name. With `purge = true`, also deletes the
    /// cloned plugin directory.
    pub async fn plugin_remove(&self, name: &str, purge: bool) -> Result<serde_json::Value> {
        #[derive(serde::Serialize)]
        struct RemoveParams {
            name: String,
            purge: bool,
        }
        self.typed_call(
            "plugin.remove",
            RemoveParams {
                name: name.to_string(),
                purge,
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests;
