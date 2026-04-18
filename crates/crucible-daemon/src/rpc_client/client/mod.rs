//! Daemon client implementation
//!
//! Provides a client for communicating with the Crucible daemon over Unix sockets.
//! Supports both request/response RPC calls and asynchronous event streaming.

use anyhow::Result;
use std::collections::HashMap;
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

// Submodules for logical organization of RPC methods.
// Each submodule adds methods to `DaemonClient` via `impl` blocks and
// defines the associated request/response types near the methods that use
// them. The shared infrastructure (connection, JSON-RPC dispatch, error
// retries) lives here in `mod.rs`.
pub mod agent;
pub mod lua;
pub mod session;
pub mod storage;
pub mod subscription;
pub mod types;

// Re-export public types so the original `rpc_client::client::<Type>` paths
// still resolve after the split. Only types the parent `rpc_client` module
// re-exports externally need to land here; the rest remain reachable at
// `client::<submodule>::<Type>` if needed internally.
pub use lua::{
    LuaDiscoverPluginsRequest, LuaDiscoverPluginsResponse, LuaExecuteHookRequest,
    LuaExecuteHookResponse, LuaGenerateStubsRequest, LuaGenerateStubsResponse,
    LuaInitSessionRequest, LuaInitSessionResponse, LuaPluginHealthRequest,
    LuaPluginHealthResponse, LuaRegisterHooksRequest, LuaRegisterHooksResponse,
    LuaRunPluginTestsRequest, LuaRunPluginTestsResponse, LuaShutdownSessionRequest,
    LuaShutdownSessionResponse,
};
pub use session::SessionCreateParams;
pub use types::{DaemonCapabilities, NameRequest, SessionEvent, VersionCheck};

use session::SessionIdRequest;
use types::{extract_string_array, EmptyParams};

// Pull in SessionCreateRequest for the wire-format tests below.
#[cfg(test)]
use session::SessionCreateRequest;

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

    /// Start daemon and retry connecting with exponential backoff.
    async fn start_and_retry<T, F, Fut>(connect: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        Self::start_daemon().await?;

        let mut delay = Duration::from_millis(50);
        for attempt in 0..10 {
            tokio::time::sleep(delay).await;
            if let Ok(result) = connect().await {
                return Ok(result);
            }
            delay *= 2;
            if attempt > 5 {
                warn!("Daemon not ready after {} attempts", attempt + 1);
            }
        }

        anyhow::bail!(
            "Failed to connect to daemon after 10 attempts. \
             Try: cru daemon stop && cru daemon start"
        )
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

        tracing::info!("Starting daemon: {:?} daemon serve", exe);

        Command::new(&exe)
            .args(["daemon", "serve"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
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

    #[allow(dead_code)]
    /// Build a request containing only a session_id.
    ///
    /// Helper for methods that take only a session_id parameter.
    fn session_id_request(&self, session_id: &str) -> serde_json::Value {
        serde_json::json!({"session_id": session_id.to_string()})
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

    /// Send a JSON-RPC request and get the response
    pub async fn call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let mut req_str = serde_json::to_string(&request)?;
        req_str.push('\n');

        // Register pending request before sending (for event mode)
        let response_rx = if self.reader_task.is_some() {
            let (tx, rx) = oneshot::channel();
            {
                let mut pending = self.pending_requests.lock().await;
                pending.insert(id, tx);
            }
            Some(rx)
        } else {
            None
        };

        // Send request
        {
            let mut writer = self.writer.lock().await;
            writer.write_all(req_str.as_bytes()).await?;
        }

        // Get response
        let response = if let Some(rx) = response_rx {
            // Event mode: wait for background reader to route response
            match tokio::time::timeout(Duration::from_secs(30), rx).await {
                Ok(Ok(response)) => response,
                Ok(Err(_)) => anyhow::bail!("Response channel closed unexpectedly"),
                Err(_) => {
                    // Clean up pending request on timeout
                    let mut pending = self.pending_requests.lock().await;
                    pending.remove(&id);
                    anyhow::bail!("Request timeout after 30 seconds")
                }
            }
        } else {
            // Simple mode: read directly
            self.read_response_simple().await?
        };

        if let Some(error) = response.get("error") {
            anyhow::bail!("RPC error: {}", error);
        }

        Ok(response
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    async fn read_response_simple(&self) -> Result<serde_json::Value> {
        let reader = self
            .simple_reader
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No reader available in event mode"))?;

        loop {
            let mut line = String::new();
            {
                let mut reader = reader.lock().await;
                let bytes_read = reader.read_line(&mut line).await?;
                if bytes_read == 0 {
                    anyhow::bail!("Connection closed by daemon");
                }
            }

            let msg: serde_json::Value = serde_json::from_str(&line)?;

            if msg.get("id").is_some() || msg.get("error").is_some() {
                return Ok(msg);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Server;
    use tempfile::TempDir;

    async fn setup_test_server() -> (TempDir, std::path::PathBuf, tokio::task::JoinHandle<()>) {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path, None).await.unwrap();
        let _shutdown_handle = server.shutdown_handle();

        let handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        (tmp, sock_path.clone(), handle)
    }

    #[tokio::test]
    async fn test_client_ping() {
        let (_tmp, sock_path, _handle) = setup_test_server().await;

        let client = DaemonClient::connect_to(&sock_path).await.unwrap();
        let result = client.ping().await.unwrap();
        assert_eq!(result, "pong");
    }

    // ---- SessionCreateRequest wire-format tests (Task 1.2a) ----
    // The daemon and CLI may be at different versions; the `agent_type` field
    // must be forward/backward compatible.

    #[test]
    fn session_create_request_without_agent_type_deserializes_as_none() {
        // Old-style payload (pre-Task 1.2a) — no `agent_type`.
        let json = serde_json::json!({
            "type": "chat",
            "kiln": "/tmp/kiln",
        });
        let req: SessionCreateRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.session_type, "chat");
        assert_eq!(req.kiln, "/tmp/kiln");
        assert_eq!(req.agent_type, None);
    }

    #[test]
    fn session_create_request_with_agent_type_acp_roundtrips() {
        let json = serde_json::json!({
            "type": "chat",
            "kiln": "/tmp/kiln",
            "agent_type": "acp",
        });
        let req: SessionCreateRequest = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(req.agent_type.as_deref(), Some("acp"));
        // Re-serialize and confirm the field survives the round-trip.
        let roundtrip = serde_json::to_value(&req).unwrap();
        assert_eq!(roundtrip["agent_type"], "acp");
    }

    #[test]
    fn session_create_request_with_agent_type_internal_roundtrips() {
        let json = serde_json::json!({
            "type": "chat",
            "kiln": "/tmp/kiln",
            "agent_type": "internal",
        });
        let req: SessionCreateRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.agent_type.as_deref(), Some("internal"));
        let roundtrip = serde_json::to_value(&req).unwrap();
        assert_eq!(roundtrip["agent_type"], "internal");
    }

    #[test]
    fn session_create_request_omits_agent_type_when_none() {
        // Ensure over-the-wire backward compatibility: a None `agent_type`
        // must not appear in the serialized payload, so old daemons don't
        // see an unexpected field.
        let req = SessionCreateRequest {
            session_type: "chat".to_string(),
            kiln: "/tmp/kiln".to_string(),
            workspace: None,
            connect_kilns: None,
            recording_mode: None,
            recording_path: None,
            agent_type: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert!(
            json.get("agent_type").is_none(),
            "agent_type should be omitted when None, got: {json}"
        );
    }

    #[tokio::test]
    async fn test_client_capabilities() {
        let (_tmp, sock_path, _handle) = setup_test_server().await;

        let client = DaemonClient::connect_to(&sock_path).await.unwrap();
        let caps = client.capabilities().await.unwrap();

        assert_eq!(caps.protocol_version, "1.0");
        assert!(caps.capabilities.kilns);
        assert!(caps.capabilities.sessions);
        assert!(caps.capabilities.agents);
        assert!(caps.capabilities.events);
        assert!(caps.capabilities.thinking_budget);
        assert!(caps.capabilities.model_switching);
        assert!(caps.methods.contains(&"ping".to_string()));
        assert!(caps
            .methods
            .contains(&"session.set_thinking_budget".to_string()));
    }

    #[tokio::test]
    async fn test_client_version_check_matches() {
        let (_tmp, sock_path, _handle) = setup_test_server().await;

        let client = DaemonClient::connect_to(&sock_path).await.unwrap();
        let check = client.check_version().await.unwrap();

        assert!(check.is_match());
    }

    #[tokio::test]
    async fn test_client_ping_event_mode() {
        let (_tmp, sock_path, _handle) = setup_test_server().await;

        let (client, _event_rx) = DaemonClient::connect_to_with_events(&sock_path)
            .await
            .unwrap();
        let result = client.ping().await.unwrap();
        assert_eq!(result, "pong");
    }

    #[tokio::test]
    async fn test_client_kiln_list_initially_empty() {
        let (_tmp, sock_path, _handle) = setup_test_server().await;

        let client = DaemonClient::connect_to(&sock_path).await.unwrap();
        let list = client.kiln_list().await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_client_connect_fails_without_server() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("nonexistent.sock");

        let result = DaemonClient::connect_to(&sock_path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_connect_to_with_events() {
        let (_tmp, sock_path, _handle) = setup_test_server().await;

        let (client, _event_rx) = DaemonClient::connect_to_with_events(&sock_path)
            .await
            .unwrap();

        let result = client.ping().await.unwrap();
        assert_eq!(result, "pong");
    }

    #[tokio::test]
    async fn test_multiple_sequential_calls_event_mode() {
        let (_tmp, sock_path, _handle) = setup_test_server().await;

        let (client, _event_rx) = DaemonClient::connect_to_with_events(&sock_path)
            .await
            .unwrap();

        for _ in 0..5 {
            let result = client.ping().await.unwrap();
            assert_eq!(result, "pong");
        }
    }

    #[tokio::test]
    async fn test_subscribe_process_events() {
        let (_tmp, sock_path, _handle) = setup_test_server().await;

        let (client, _event_rx) = DaemonClient::connect_to_with_events(&sock_path)
            .await
            .unwrap();
        let result = client.subscribe_process_events("batch-123").await.unwrap();

        assert_eq!(result["batch_id"], "batch-123");
        assert_eq!(result["subscription"]["subscribed"][0], "process");
    }

    #[tokio::test]
    #[ignore = "requires running daemon with session support"]
    async fn test_session_create_and_get() {
        let client = DaemonClient::connect().await.unwrap();
        let tmp = TempDir::new().unwrap();

        let result = client
            .session_create(SessionCreateParams {
                session_type: "chat".to_string(),
                kiln: tmp.path().to_path_buf(),
                workspace: None,
                connect_kilns: vec![],
                recording_mode: None,
                recording_path: None,
                agent_type: None,
            })
            .await
            .unwrap();
        let session_id = result["session_id"].as_str().unwrap();

        let session = client.session_get(session_id).await.unwrap();
        assert_eq!(session["session_id"], session_id);
        assert_eq!(session["type"], "chat");
    }

    #[tokio::test]
    #[ignore = "requires running daemon with session support"]
    async fn test_session_list() {
        let client = DaemonClient::connect().await.unwrap();
        let result = client
            .session_list(None, None, None, None, None)
            .await
            .unwrap();
        assert!(result.is_array() || result.is_object());
    }

    #[tokio::test]
    #[ignore = "requires running daemon with session support"]
    async fn test_session_lifecycle() {
        let client = DaemonClient::connect().await.unwrap();
        let tmp = TempDir::new().unwrap();

        let result = client
            .session_create(SessionCreateParams {
                session_type: "chat".to_string(),
                kiln: tmp.path().to_path_buf(),
                workspace: None,
                connect_kilns: vec![],
                recording_mode: None,
                recording_path: None,
                agent_type: None,
            })
            .await
            .unwrap();
        let session_id = result["session_id"].as_str().unwrap();

        let pause_result = client.session_pause(session_id).await;
        assert!(pause_result.is_ok());

        let resume_result = client.session_resume(session_id).await;
        assert!(resume_result.is_ok());

        let end_result = client.session_end(session_id).await;
        assert!(end_result.is_ok());
    }

    #[tokio::test]
    #[ignore = "requires running daemon with session support"]
    async fn test_session_subscribe_unsubscribe() {
        let client = DaemonClient::connect().await.unwrap();
        let tmp = TempDir::new().unwrap();

        let result = client
            .session_create(SessionCreateParams {
                session_type: "chat".to_string(),
                kiln: tmp.path().to_path_buf(),
                workspace: None,
                connect_kilns: vec![],
                recording_mode: None,
                recording_path: None,
                agent_type: None,
            })
            .await
            .unwrap();
        let session_id = result["session_id"].as_str().unwrap();

        let sub_result = client.session_subscribe(&[session_id]).await;
        assert!(sub_result.is_ok());

        let unsub_result = client.session_unsubscribe(&[session_id]).await;
        assert!(unsub_result.is_ok());

        let _ = client.session_end(session_id).await;
    }

    #[tokio::test]
    #[ignore = "requires running daemon with session support"]
    async fn test_event_stream() {
        let (client, mut event_rx) = DaemonClient::connect_with_events().await.unwrap();
        let tmp = TempDir::new().unwrap();

        let result = client
            .session_create(SessionCreateParams {
                session_type: "chat".to_string(),
                kiln: tmp.path().to_path_buf(),
                workspace: None,
                connect_kilns: vec![],
                recording_mode: None,
                recording_path: None,
                agent_type: None,
            })
            .await
            .unwrap();
        let session_id = result["session_id"].as_str().unwrap();

        client.session_subscribe(&[session_id]).await.unwrap();

        let result = tokio::time::timeout(Duration::from_millis(100), event_rx.recv()).await;
        assert!(result.is_err(), "Expected timeout, got event");

        let _ = client.session_end(session_id).await;
    }

    #[tokio::test]
    #[ignore = "requires running daemon with session and agent support"]
    async fn test_session_thinking_budget() {
        let client = DaemonClient::connect().await.unwrap();
        let tmp = TempDir::new().unwrap();

        let result = client
            .session_create(SessionCreateParams {
                session_type: "chat".to_string(),
                kiln: tmp.path().to_path_buf(),
                workspace: None,
                connect_kilns: vec![],
                recording_mode: None,
                recording_path: None,
                agent_type: None,
            })
            .await
            .unwrap();
        let session_id = result["session_id"].as_str().unwrap();

        let initial = client
            .session_get_thinking_budget(session_id)
            .await
            .unwrap();
        assert!(initial.is_none(), "Initial budget should be None");

        client
            .session_set_thinking_budget(session_id, Some(10000))
            .await
            .unwrap();
        let budget = client
            .session_get_thinking_budget(session_id)
            .await
            .unwrap();
        assert_eq!(budget, Some(10000));

        client
            .session_set_thinking_budget(session_id, Some(-1))
            .await
            .unwrap();
        let unlimited = client
            .session_get_thinking_budget(session_id)
            .await
            .unwrap();
        assert_eq!(unlimited, Some(-1));

        client
            .session_set_thinking_budget(session_id, Some(0))
            .await
            .unwrap();
        let cleared = client
            .session_get_thinking_budget(session_id)
            .await
            .unwrap();
        assert_eq!(cleared, Some(0), "Budget should be 0 (disabled)");

        let _ = client.session_end(session_id).await;
    }

    #[tokio::test]
    async fn test_call_with_retry_succeeds_on_valid_method() {
        let (_tmp, sock_path, _handle) = setup_test_server().await;

        let client = DaemonClient::connect_to(&sock_path).await.unwrap();
        let result = client
            .call_with_retry("ping", serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(result, "pong");
    }

    #[tokio::test]
    async fn test_call_with_retry_does_not_retry_rpc_errors() {
        let (_tmp, sock_path, _handle) = setup_test_server().await;

        let client = DaemonClient::connect_to(&sock_path).await.unwrap();
        let result = client
            .call_with_retry("nonexistent.method", serde_json::json!({}))
            .await;
        assert!(result.is_err(), "Unknown method should fail without retry");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            !err_msg.contains("timeout"),
            "Error should not be timeout-related: {}",
            err_msg
        );
    }

    #[test]
    fn version_check_enum_match() {
        let check = VersionCheck::Match;
        assert!(check.is_match());
    }

    #[test]
    fn version_check_enum_mismatch() {
        let check = VersionCheck::Mismatch {
            client: "abc1234".to_string(),
            daemon: "def5678".to_string(),
        };
        assert!(!check.is_match());
    }

    #[test]
    fn version_check_mismatch_dev_vs_real() {
        // This is the exact scenario that was broken before build.rs was added:
        // client had "dev" (no build.rs), daemon had "dev" → false Match
        let check_dev_match = VersionCheck::Match; // "dev" == "dev"
        assert!(check_dev_match.is_match());

        // After build.rs: client has real SHA, daemon has old "dev" → correct Mismatch
        let check_dev_mismatch = VersionCheck::Mismatch {
            client: "abc1234".to_string(),
            daemon: "dev".to_string(),
        };
        assert!(!check_dev_mismatch.is_match());
    }

    #[test]
    fn build_sha_is_set_by_build_rs() {
        // After Task 2 added build.rs, this should NOT be "dev" anymore.
        // This is the CRITICAL test — proves the SHA is actually embedded.
        let sha = option_env!("CRUCIBLE_BUILD_SHA");
        assert!(
            sha.is_some(),
            "CRUCIBLE_BUILD_SHA should be set by build.rs"
        );
        let sha = sha.unwrap();
        assert_ne!(sha, "dev", "Should be a real git SHA, not 'dev'");
        assert!(sha.len() >= 7, "SHA should be at least 7 chars: got {sha}");
        assert!(
            sha.chars().all(|c| c.is_ascii_hexdigit()),
            "SHA should be hex chars only: got {sha}"
        );
    }
}
