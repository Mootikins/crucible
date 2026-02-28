//! Daemon client implementation
//!
//! Provides a client for communicating with the Crucible daemon over Unix sockets.
//! Supports both request/response RPC calls and asynchronous event streaming.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;
use tracing::{debug, error, trace, warn};

/// Session event received from daemon
#[derive(Debug, Clone)]
pub struct SessionEvent {
    pub session_id: String,
    pub event_type: String,
    pub data: serde_json::Value,
}

/// Daemon capabilities returned by `daemon.capabilities` RPC
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DaemonCapabilities {
    pub version: String,
    #[serde(default)]
    pub build_sha: Option<String>,
    pub protocol_version: String,
    pub capabilities: CapabilityFlags,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CapabilityFlags {
    pub kilns: bool,
    pub sessions: bool,
    pub agents: bool,
    pub events: bool,
    pub thinking_budget: bool,
    pub model_switching: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaInitSessionRequest {
    pub session_id: String,
    pub kiln_path: String,
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaInitSessionResponse {
    pub session_id: String,
    #[serde(default)]
    pub commands: Vec<serde_json::Value>,
    #[serde(default)]
    pub views: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRegisterHooksRequest {
    pub session_id: String,
    pub hooks: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRegisterHooksResponse {
    pub status: String,
    pub registered: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaExecuteHookRequest {
    pub session_id: String,
    pub hook_name: String,
    #[serde(default)]
    pub context: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaExecuteHookResponse {
    pub executed: usize,
    #[serde(default)]
    pub results: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaShutdownSessionRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaShutdownSessionResponse {
    pub shutdown: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaDiscoverPluginsRequest {
    pub kiln_path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaDiscoverPluginsResponse {
    #[serde(default)]
    pub plugins: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaPluginHealthRequest {
    pub plugin_path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaPluginHealthResponse {
    pub name: String,
    pub healthy: bool,
    #[serde(default)]
    pub checks: Vec<serde_json::Value>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaGenerateStubsRequest {
    pub output_dir: String,
    #[serde(default)]
    pub verify: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaGenerateStubsResponse {
    pub status: String,
    pub path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRunPluginTestsRequest {
    pub test_path: String,
    #[serde(default)]
    pub filter: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRunPluginTestsResponse {
    pub passed: usize,
    pub failed: usize,
    pub load_failures: usize,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRegisterCommandsRequest {
    pub session_id: String,
    pub commands: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LuaRegisterCommandsResponse {
    pub registered: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionCheck {
    Match,
    Mismatch { client: String, daemon: String },
}

impl VersionCheck {
    pub fn is_match(&self) -> bool {
        matches!(self, Self::Match)
    }
}

type PendingRequests = Arc<Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>>;

fn parse_models_response(result: &serde_json::Value) -> Vec<String> {
    result["models"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

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

    /// Connect to daemon or start it if not running (simple mode)
    ///
    /// Checks daemon version after connecting. If version mismatches (stale daemon),
    /// shuts down the old daemon and starts a fresh one.
    pub async fn connect_or_start() -> Result<Self> {
        if let Ok(client) = Self::connect().await {
            match client.check_version().await {
                Ok(VersionCheck::Match) => return Ok(client),
                Ok(VersionCheck::Mismatch {
                    client: c,
                    daemon: d,
                }) => {
                    warn!(client_sha = %c, daemon_sha = %d, "Daemon version mismatch, restarting");
                    let _ = client.shutdown().await;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(e) => {
                    debug!("Version check failed, assuming ok: {}", e);
                    return Ok(client);
                }
            }
        }

        Self::start_daemon().await?;

        let mut delay = Duration::from_millis(50);
        for attempt in 0..10 {
            tokio::time::sleep(delay).await;
            if let Ok(client) = Self::connect().await {
                return Ok(client);
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

    /// Connect to daemon or start it if not running (event mode).
    ///
    /// Returns event-mode client with receiver for streaming session events.
    /// Checks daemon version after connecting. If version mismatches (stale daemon),
    /// shuts down the old daemon and starts a fresh one.
    pub async fn connect_or_start_with_events(
    ) -> Result<(Self, mpsc::UnboundedReceiver<SessionEvent>)> {
        if let Ok((client, rx)) = Self::connect_with_events().await {
            match client.check_version().await {
                Ok(VersionCheck::Match) => return Ok((client, rx)),
                Ok(VersionCheck::Mismatch {
                    client: c,
                    daemon: d,
                }) => {
                    warn!(client_sha = %c, daemon_sha = %d, "Daemon version mismatch, restarting");
                    let _ = client.shutdown().await;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(e) => {
                    debug!("Version check failed, assuming ok: {}", e);
                    return Ok((client, rx));
                }
            }
        }

        Self::start_daemon().await?;

        let mut delay = Duration::from_millis(50);
        for attempt in 0..10 {
            tokio::time::sleep(delay).await;
            if let Ok(result) = Self::connect_with_events().await {
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
        let result = self.call("ping", serde_json::json!({})).await?;
        Ok(result.as_str().unwrap_or("").to_string())
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.call("shutdown", serde_json::json!({})).await?;
        Ok(())
    }

    pub async fn capabilities(&self) -> Result<DaemonCapabilities> {
        let result = self
            .call("daemon.capabilities", serde_json::json!({}))
            .await?;
        let caps: DaemonCapabilities = serde_json::from_value(result)?;
        Ok(caps)
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
    // Kiln RPC Methods
    // =========================================================================

    pub async fn kiln_open(&self, path: &Path) -> Result<()> {
        self.kiln_open_with_options(path, false, false).await?;
        Ok(())
    }

    pub async fn kiln_open_with_options(
        &self,
        path: &Path,
        process: bool,
        force: bool,
    ) -> Result<serde_json::Value> {
        let result = self
            .call(
                "kiln.open",
                serde_json::json!({
                    "path": path.to_string_lossy(),
                    "process": process,
                    "force": force
                }),
            )
            .await?;
        Ok(result)
    }

    pub async fn kiln_set_classification(&self, path: &Path, classification: &str) -> Result<()> {
        self.call(
            "kiln.set_classification",
            serde_json::json!({
                "path": path.to_string_lossy(),
                "classification": classification
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn kiln_list(&self) -> Result<Vec<serde_json::Value>> {
        let result = self.call("kiln.list", serde_json::json!({})).await?;
        Ok(result.as_array().cloned().unwrap_or_default())
    }

    // =========================================================================
    // Search RPC Methods
    // =========================================================================

    pub async fn search_vectors(
        &self,
        kiln_path: &Path,
        vector: &[f32],
        limit: usize,
    ) -> Result<Vec<(String, f64)>> {
        let result = self
            .call(
                "search_vectors",
                serde_json::json!({
                    "kiln": kiln_path.to_string_lossy(),
                    "vector": vector,
                    "limit": limit
                }),
            )
            .await?;

        let results: Vec<(String, f64)> = result
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|item| {
                let doc_id = item.get("document_id")?.as_str()?.to_string();
                let score = item.get("score")?.as_f64()?;
                Some((doc_id, score))
            })
            .collect();

        Ok(results)
    }

    pub async fn list_notes(
        &self,
        kiln_path: &Path,
        path_filter: Option<&str>,
    ) -> Result<Vec<(String, String, Option<String>, Vec<String>, Option<String>)>> {
        let mut params = serde_json::json!({ "kiln": kiln_path.to_string_lossy() });
        if let Some(filter) = path_filter {
            params["path_filter"] = serde_json::json!(filter);
        }

        let result = self.call("list_notes", params).await?;

        let notes: Vec<_> = result
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                let name = item.get("name").and_then(|v| v.as_str());
                let path = item.get("path").and_then(|v| v.as_str());

                if name.is_none() || path.is_none() {
                    warn!(
                        idx,
                        has_name = name.is_some(),
                        has_path = path.is_some(),
                        "Skipping malformed note record in list_notes response"
                    );
                    return None;
                }

                let name = name.unwrap().to_string();
                let path = path.unwrap().to_string();
                let title = item.get("title").and_then(|v| v.as_str()).map(String::from);
                let tags: Vec<String> = item
                    .get("tags")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|t| t.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let updated_at = item
                    .get("updated_at")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                Some((name, path, title, tags, updated_at))
            })
            .collect();

        Ok(notes)
    }

    pub async fn get_note_by_name(
        &self,
        kiln_path: &Path,
        name: &str,
    ) -> Result<Option<serde_json::Value>> {
        let result = self
            .call(
                "get_note_by_name",
                serde_json::json!({
                    "kiln": kiln_path.to_string_lossy(),
                    "name": name
                }),
            )
            .await?;

        if result.is_null() {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    // =========================================================================
    // NoteStore RPC Methods
    // =========================================================================

    pub async fn note_upsert(
        &self,
        kiln_path: &Path,
        note: &crucible_core::storage::NoteRecord,
    ) -> Result<()> {
        self.call(
            "note.upsert",
            serde_json::json!({
                "kiln": kiln_path.to_string_lossy(),
                "note": note
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn note_get(
        &self,
        kiln_path: &Path,
        path: &str,
    ) -> Result<Option<crucible_core::storage::NoteRecord>> {
        let result = self
            .call(
                "note.get",
                serde_json::json!({
                    "kiln": kiln_path.to_string_lossy(),
                    "path": path
                }),
            )
            .await?;

        if result.is_null() {
            Ok(None)
        } else {
            let note: crucible_core::storage::NoteRecord = serde_json::from_value(result)?;
            Ok(Some(note))
        }
    }

    pub async fn note_delete(&self, kiln_path: &Path, path: &str) -> Result<()> {
        self.call(
            "note.delete",
            serde_json::json!({
                "kiln": kiln_path.to_string_lossy(),
                "path": path
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn note_list(
        &self,
        kiln_path: &Path,
    ) -> Result<Vec<crucible_core::storage::NoteRecord>> {
        let result = self
            .call(
                "note.list",
                serde_json::json!({ "kiln": kiln_path.to_string_lossy() }),
            )
            .await?;

        let notes: Vec<crucible_core::storage::NoteRecord> = serde_json::from_value(result)?;
        Ok(notes)
    }

    // =========================================================================
    // Pipeline RPC Methods
    // =========================================================================

    pub async fn process_batch(
        &self,
        kiln_path: &Path,
        file_paths: &[PathBuf],
    ) -> Result<(usize, usize, Vec<(String, String)>)> {
        let paths: Vec<String> = file_paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        let result = self
            .call(
                "process_batch",
                serde_json::json!({
                    "kiln": kiln_path.to_string_lossy(),
                    "paths": paths
                }),
            )
            .await?;

        let processed = result
            .get("processed")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let skipped = result.get("skipped").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        let errors: Vec<(String, String)> = result
            .get("errors")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| {
                        let path = e.get("path")?.as_str()?.to_string();
                        let error = e.get("error")?.as_str()?.to_string();
                        Some((path, error))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok((processed, skipped, errors))
    }

    // =========================================================================
    // Storage Maintenance RPC Methods (stubs)
    // =========================================================================

    pub async fn storage_verify(&self, kiln_path: &Path) -> Result<serde_json::Value> {
        self.call(
            "storage.verify",
            serde_json::json!({ "kiln": kiln_path.to_string_lossy() }),
        )
        .await
    }

    pub async fn storage_cleanup(&self, kiln_path: &Path) -> Result<serde_json::Value> {
        self.call(
            "storage.cleanup",
            serde_json::json!({ "kiln": kiln_path.to_string_lossy() }),
        )
        .await
    }

    pub async fn storage_backup(&self, kiln_path: &Path, dest: &Path) -> Result<serde_json::Value> {
        self.call(
            "storage.backup",
            serde_json::json!({
                "kiln": kiln_path.to_string_lossy(),
                "dest": dest.to_string_lossy()
            }),
        )
        .await
    }

    pub async fn storage_restore(
        &self,
        kiln_path: &Path,
        source: &Path,
    ) -> Result<serde_json::Value> {
        self.call(
            "storage.restore",
            serde_json::json!({
                "kiln": kiln_path.to_string_lossy(),
                "source": source.to_string_lossy()
            }),
        )
        .await
    }

    pub async fn session_search(
        &self,
        query: &str,
        kiln_path: Option<&Path>,
        limit: Option<usize>,
    ) -> Result<serde_json::Value> {
        let mut params = serde_json::json!({ "query": query });
        if let Some(kiln) = kiln_path {
            params["kiln"] = serde_json::Value::String(kiln.to_string_lossy().to_string());
        }
        if let Some(lim) = limit {
            params["limit"] = serde_json::Value::Number(serde_json::Number::from(lim));
        }
        self.call("session.search", params).await
    }

    pub async fn lua_init_session(
        &self,
        params: LuaInitSessionRequest,
    ) -> Result<LuaInitSessionResponse> {
        self.typed_call("lua.init_session", params).await
    }

    pub async fn lua_register_hooks(
        &self,
        params: LuaRegisterHooksRequest,
    ) -> Result<LuaRegisterHooksResponse> {
        self.typed_call("lua.register_hooks", params).await
    }

    pub async fn lua_execute_hook(
        &self,
        params: LuaExecuteHookRequest,
    ) -> Result<LuaExecuteHookResponse> {
        self.typed_call("lua.execute_hook", params).await
    }

    pub async fn lua_shutdown_session(
        &self,
        params: LuaShutdownSessionRequest,
    ) -> Result<LuaShutdownSessionResponse> {
        self.typed_call("lua.shutdown_session", params).await
    }

    // =========================================================================
    // Lua Plugin Management RPC Methods
    // =========================================================================

    /// Discover plugins from a kiln path.
    pub async fn lua_discover_plugins(
        &self,
        params: LuaDiscoverPluginsRequest,
    ) -> Result<LuaDiscoverPluginsResponse> {
        self.typed_call("lua.discover_plugins", params).await
    }

    /// Run health checks for a plugin.
    pub async fn lua_plugin_health(
        &self,
        params: LuaPluginHealthRequest,
    ) -> Result<LuaPluginHealthResponse> {
        self.typed_call("lua.plugin_health", params).await
    }

    /// Generate or verify Lua type stubs.
    pub async fn lua_generate_stubs(
        &self,
        params: LuaGenerateStubsRequest,
    ) -> Result<LuaGenerateStubsResponse> {
        self.typed_call("lua.generate_stubs", params).await
    }

    /// Run plugin test files.
    pub async fn lua_run_plugin_tests(
        &self,
        params: LuaRunPluginTestsRequest,
    ) -> Result<LuaRunPluginTestsResponse> {
        self.typed_call("lua.run_plugin_tests", params).await
    }

    /// Register Lua commands in a session.
    pub async fn lua_register_commands(
        &self,
        params: LuaRegisterCommandsRequest,
    ) -> Result<LuaRegisterCommandsResponse> {
        self.typed_call("lua.register_commands", params).await
    }

    // =========================================================================
    // MCP Server RPC Methods
    // =========================================================================

    /// Start the daemon-managed MCP server.
    ///
    /// Spawns an MCP server exposing Crucible's tools for the given kiln.
    /// Supports SSE (default) and stdio transports.
    pub async fn mcp_start(
        &self,
        kiln_path: &str,
        transport: Option<&str>,
        port: Option<u16>,
        no_just: bool,
        just_dir: Option<&str>,
    ) -> Result<serde_json::Value> {
        let mut params = serde_json::json!({
            "kiln_path": kiln_path,
            "no_just": no_just,
        });
        if let Some(t) = transport {
            params["transport"] = serde_json::json!(t);
        }
        if let Some(p) = port {
            params["port"] = serde_json::json!(p);
        }
        if let Some(d) = just_dir {
            params["just_dir"] = serde_json::json!(d);
        }
        self.call("mcp.start", params).await
    }

    /// Stop the daemon-managed MCP server.
    pub async fn mcp_stop(&self) -> Result<serde_json::Value> {
        self.call("mcp.stop", serde_json::json!({})).await
    }

    /// Get the status of the daemon-managed MCP server.
    pub async fn mcp_status(&self) -> Result<serde_json::Value> {
        self.call("mcp.status", serde_json::json!({})).await
    }

    // =========================================================================
    // Session RPC Methods
    // =========================================================================

    pub async fn session_create(
        &self,
        session_type: &str,
        kiln: &Path,
        workspace: Option<&Path>,
        connect_kilns: Vec<&Path>,
        recording_mode: Option<&str>,
        recording_path: Option<&Path>,
    ) -> Result<serde_json::Value> {
        let mut params = serde_json::json!({
            "type": session_type,
            "kiln": kiln.to_string_lossy(),
        });

        if let Some(ws) = workspace {
            params["workspace"] = serde_json::json!(ws.to_string_lossy());
        }

        if !connect_kilns.is_empty() {
            params["connect_kilns"] = serde_json::json!(connect_kilns
                .iter()
                .map(|p| p.to_string_lossy())
                .collect::<Vec<_>>());
        }

        if let Some(mode) = recording_mode {
            params["recording_mode"] = serde_json::json!(mode);
        }

        if let Some(path) = recording_path {
            params["recording_path"] = serde_json::json!(path.to_string_lossy());
        }

        self.call("session.create", params).await
    }

    pub async fn session_list(
        &self,
        kiln: Option<&Path>,
        workspace: Option<&Path>,
        session_type: Option<&str>,
        state: Option<&str>,
    ) -> Result<serde_json::Value> {
        let mut params = serde_json::json!({});

        if let Some(k) = kiln {
            params["kiln"] = serde_json::json!(k.to_string_lossy());
        }
        if let Some(ws) = workspace {
            params["workspace"] = serde_json::json!(ws.to_string_lossy());
        }
        if let Some(t) = session_type {
            params["type"] = serde_json::json!(t);
        }
        if let Some(s) = state {
            params["state"] = serde_json::json!(s);
        }

        self.call("session.list", params).await
    }

    pub async fn session_get(&self, session_id: &str) -> Result<serde_json::Value> {
        self.call(
            "session.get",
            serde_json::json!({ "session_id": session_id }),
        )
        .await
    }

    pub async fn session_pause(&self, session_id: &str) -> Result<serde_json::Value> {
        self.call(
            "session.pause",
            serde_json::json!({ "session_id": session_id }),
        )
        .await
    }

    pub async fn session_resume(&self, session_id: &str) -> Result<serde_json::Value> {
        self.call(
            "session.resume",
            serde_json::json!({ "session_id": session_id }),
        )
        .await
    }

    pub async fn session_end(&self, session_id: &str) -> Result<serde_json::Value> {
        self.call(
            "session.end",
            serde_json::json!({ "session_id": session_id }),
        )
        .await
    }

    pub async fn session_replay(
        &self,
        recording_path: &Path,
        speed: f64,
    ) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "recording_path": recording_path.to_string_lossy(),
            "speed": speed,
        });
        self.call("session.replay", params).await
    }

    pub async fn session_resume_from_storage(
        &self,
        session_id: &str,
        kiln: &Path,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<serde_json::Value> {
        let mut params = serde_json::json!({
            "session_id": session_id,
            "kiln": kiln.to_string_lossy()
        });

        if let Some(l) = limit {
            params["limit"] = serde_json::json!(l);
        }
        if let Some(o) = offset {
            params["offset"] = serde_json::json!(o);
        }

        self.call("session.resume_from_storage", params).await
    }

    pub async fn session_subscribe(&self, session_ids: &[&str]) -> Result<serde_json::Value> {
        self.call(
            "session.subscribe",
            serde_json::json!({ "session_ids": session_ids }),
        )
        .await
    }

    pub async fn session_unsubscribe(&self, session_ids: &[&str]) -> Result<serde_json::Value> {
        self.call(
            "session.unsubscribe",
            serde_json::json!({ "session_ids": session_ids }),
        )
        .await
    }

    pub async fn subscribe_process_events(&self, batch_id: &str) -> Result<serde_json::Value> {
        let result = self.session_subscribe(&["process"]).await?;
        Ok(serde_json::json!({
            "batch_id": batch_id,
            "subscription": result
        }))
    }

    pub async fn session_configure_agent(
        &self,
        session_id: &str,
        agent: &crucible_core::session::SessionAgent,
    ) -> Result<()> {
        self.call(
            "session.configure_agent",
            serde_json::json!({
                "session_id": session_id,
                "agent": agent
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn session_send_message(&self, session_id: &str, content: &str) -> Result<String> {
        let result = self
            .call(
                "session.send_message",
                serde_json::json!({
                    "session_id": session_id,
                    "content": content
                }),
            )
            .await?;

        Ok(result["message_id"].as_str().unwrap_or("").to_string())
    }

    pub async fn session_interaction_respond(
        &self,
        session_id: &str,
        request_id: &str,
        response: crucible_core::interaction::InteractionResponse,
    ) -> Result<()> {
        self.call(
            "session.interaction_respond",
            serde_json::json!({
                "session_id": session_id,
                "request_id": request_id,
                "response": response
            }),
        )
        .await?;

        Ok(())
    }

    pub async fn session_cancel(&self, session_id: &str) -> Result<bool> {
        let result = self
            .call(
                "session.cancel",
                serde_json::json!({ "session_id": session_id }),
            )
            .await?;

        Ok(result["cancelled"].as_bool().unwrap_or(false))
    }

    pub async fn session_switch_model(&self, session_id: &str, model_id: &str) -> Result<()> {
        self.call_with_retry(
            "session.switch_model",
            serde_json::json!({
                "session_id": session_id,
                "model_id": model_id
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn session_set_title(&self, session_id: &str, title: &str) -> Result<()> {
        self.call_with_retry(
            "session.set_title",
            serde_json::json!({
                "session_id": session_id,
                "title": title
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn session_list_models(&self, session_id: &str) -> Result<Vec<String>> {
        let result = self
            .call_with_retry(
                "session.list_models",
                serde_json::json!({ "session_id": session_id }),
            )
            .await?;

        Ok(parse_models_response(&result))
    }

    /// List all available models without requiring an active session.
    ///
    /// If `kiln_path` is provided, the daemon resolves the kiln's data classification
    /// and filters providers whose trust level doesn't satisfy it.
    pub async fn list_all_models(&self, kiln_path: Option<&Path>) -> Result<Vec<String>> {
        let params = if let Some(p) = kiln_path {
            serde_json::json!({ "kiln_path": p.to_string_lossy() })
        } else {
            serde_json::json!({})
        };

        let result = self.call_with_retry("models.list", params).await?;

        Ok(parse_models_response(&result))
    }

    /// Set the thinking budget for a session's agent.
    ///
    /// The thinking budget controls reasoning token allocation for thinking models
    /// (e.g., Qwen, DeepSeek R1):
    /// - `None` - Use model's default behavior
    /// - `Some(-1)` - Unlimited thinking tokens
    /// - `Some(0)` - Disable thinking/reasoning
    /// - `Some(n)` where n > 0 - Maximum thinking tokens
    ///
    /// Changes take effect on the next message. Invalidates cached agent handles.
    pub async fn session_set_thinking_budget(
        &self,
        session_id: &str,
        budget: Option<i64>,
    ) -> Result<()> {
        let mut params = serde_json::json!({ "session_id": session_id });
        if let Some(b) = budget {
            params["thinking_budget"] = serde_json::json!(b);
        }

        self.call_with_retry("session.set_thinking_budget", params)
            .await?;
        Ok(())
    }

    /// Get the current thinking budget for a session's agent.
    ///
    /// Returns the configured thinking budget, or `None` if not set (using defaults).
    pub async fn session_get_thinking_budget(&self, session_id: &str) -> Result<Option<i64>> {
        let result = self
            .call_with_retry(
                "session.get_thinking_budget",
                serde_json::json!({ "session_id": session_id }),
            )
            .await?;

        let budget =
            result
                .get("thinking_budget")
                .and_then(|v| if v.is_null() { None } else { v.as_i64() });

        Ok(budget)
    }

    /// Set whether Precognition (auto-RAG) is enabled for a session.
    pub async fn session_set_precognition(&self, session_id: &str, enabled: bool) -> Result<()> {
        self.call_with_retry(
            "session.set_precognition",
            serde_json::json!({
                "session_id": session_id,
                "precognition_enabled": enabled,
            }),
        )
        .await?;
        Ok(())
    }

    /// Get whether Precognition is enabled for a session.
    pub async fn session_get_precognition(&self, session_id: &str) -> Result<bool> {
        let result = self
            .call_with_retry(
                "session.get_precognition",
                serde_json::json!({ "session_id": session_id }),
            )
            .await?;

        let enabled = result
            .get("precognition_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        Ok(enabled)
    }

    pub async fn session_set_temperature(&self, session_id: &str, temperature: f64) -> Result<()> {
        self.call_with_retry(
            "session.set_temperature",
            serde_json::json!({
                "session_id": session_id,
                "temperature": temperature,
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn session_get_temperature(&self, session_id: &str) -> Result<Option<f64>> {
        let result = self
            .call_with_retry(
                "session.get_temperature",
                serde_json::json!({ "session_id": session_id }),
            )
            .await?;

        let temperature =
            result
                .get("temperature")
                .and_then(|v| if v.is_null() { None } else { v.as_f64() });

        Ok(temperature)
    }

    pub async fn plugin_reload(&self, name: &str) -> Result<serde_json::Value> {
        self.call("plugin.reload", serde_json::json!({ "name": name }))
            .await
    }

    pub async fn plugin_list(&self) -> Result<Vec<String>> {
        let result = self.call("plugin.list", serde_json::json!({})).await?;
        Ok(result["plugins"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn session_set_max_tokens(
        &self,
        session_id: &str,
        max_tokens: Option<u32>,
    ) -> Result<()> {
        let mut params = serde_json::json!({ "session_id": session_id });
        if let Some(mt) = max_tokens {
            params["max_tokens"] = serde_json::json!(mt);
        }

        self.call_with_retry("session.set_max_tokens", params)
            .await?;
        Ok(())
    }

    pub async fn session_get_max_tokens(&self, session_id: &str) -> Result<Option<u32>> {
        let result = self
            .call_with_retry(
                "session.get_max_tokens",
                serde_json::json!({ "session_id": session_id }),
            )
            .await?;

        let max_tokens = result
            .get("max_tokens")
            .and_then(|v| if v.is_null() { None } else { v.as_u64() })
            .map(|v| v as u32);

        Ok(max_tokens)
    }

    pub async fn project_register(&self, path: &Path) -> Result<crucible_core::Project> {
        let result = self
            .call_with_retry(
                "project.register",
                serde_json::json!({ "path": path.to_string_lossy() }),
            )
            .await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn project_unregister(&self, path: &Path) -> Result<()> {
        self.call_with_retry(
            "project.unregister",
            serde_json::json!({ "path": path.to_string_lossy() }),
        )
        .await?;
        Ok(())
    }

    pub async fn project_list(&self) -> Result<Vec<crucible_core::Project>> {
        let result = self
            .call_with_retry("project.list", serde_json::json!({}))
            .await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn project_get(&self, path: &Path) -> Result<Option<crucible_core::Project>> {
        let result = self
            .call_with_retry(
                "project.get",
                serde_json::json!({ "path": path.to_string_lossy() }),
            )
            .await?;

        if result.is_null() {
            Ok(None)
        } else {
            Ok(Some(serde_json::from_value(result)?))
        }
    }

    // =========================================================================
    // Session Observe RPC Methods
    // =========================================================================

    /// Load events from a persisted session's JSONL log.
    pub async fn session_load_events(&self, session_dir: &Path) -> Result<serde_json::Value> {
        self.call(
            "session.load_events",
            serde_json::json!({ "session_dir": session_dir.to_string_lossy() }),
        )
        .await
    }

    /// List persisted sessions from a kiln's session directory.
    pub async fn session_list_persisted(
        &self,
        kiln: &Path,
        session_type: Option<&str>,
        limit: Option<usize>,
    ) -> Result<serde_json::Value> {
        let mut params = serde_json::json!({ "kiln": kiln.to_string_lossy() });
        if let Some(t) = session_type {
            params["session_type"] = serde_json::json!(t);
        }
        if let Some(l) = limit {
            params["limit"] = serde_json::json!(l);
        }
        self.call("session.list_persisted", params).await
    }

    /// Render a persisted session's events to markdown.
    pub async fn session_render_markdown(
        &self,
        session_dir: &Path,
        include_timestamps: Option<bool>,
        include_tokens: Option<bool>,
        include_tools: Option<bool>,
        max_content_length: Option<usize>,
    ) -> Result<String> {
        let mut params = serde_json::json!({
            "session_dir": session_dir.to_string_lossy()
        });
        if let Some(v) = include_timestamps {
            params["include_timestamps"] = serde_json::json!(v);
        }
        if let Some(v) = include_tokens {
            params["include_tokens"] = serde_json::json!(v);
        }
        if let Some(v) = include_tools {
            params["include_tools"] = serde_json::json!(v);
        }
        if let Some(v) = max_content_length {
            params["max_content_length"] = serde_json::json!(v);
        }
        let result = self.call("session.render_markdown", params).await?;
        Ok(result["markdown"].as_str().unwrap_or("").to_string())
    }

    /// Export a session to a markdown file.
    pub async fn session_export_to_file(
        &self,
        session_dir: &Path,
        output_path: Option<&Path>,
        include_timestamps: Option<bool>,
    ) -> Result<String> {
        let mut params = serde_json::json!({
            "session_dir": session_dir.to_string_lossy()
        });
        if let Some(p) = output_path {
            params["output_path"] = serde_json::json!(p.to_string_lossy());
        }
        if let Some(v) = include_timestamps {
            params["include_timestamps"] = serde_json::json!(v);
        }
        let result = self.call("session.export_to_file", params).await?;
        Ok(result["output_path"].as_str().unwrap_or("").to_string())
    }

    /// Clean up old persisted sessions.
    pub async fn session_cleanup(
        &self,
        kiln: &Path,
        older_than_days: u64,
        dry_run: bool,
    ) -> Result<serde_json::Value> {
        self.call(
            "session.cleanup",
            serde_json::json!({
                "kiln": kiln.to_string_lossy(),
                "older_than_days": older_than_days,
                "dry_run": dry_run,
            }),
        )
        .await
    }

    /// Reindex persisted sessions into the kiln's NoteStore.
    pub async fn session_reindex(&self, kiln: &Path, force: bool) -> Result<serde_json::Value> {
        self.call(
            "session.reindex",
            serde_json::json!({
                "kiln": kiln.to_string_lossy(),
                "force": force,
            }),
        )
        .await
    }

    // =========================================================================
    // Skills Discovery RPC Methods
    // =========================================================================

    /// List discovered skills with optional scope filter.
    pub async fn skills_list(
        &self,
        kiln_path: &Path,
        scope_filter: Option<&str>,
    ) -> Result<serde_json::Value> {
        let mut params = serde_json::json!({ "kiln_path": kiln_path.to_string_lossy() });
        if let Some(scope) = scope_filter {
            params["scope_filter"] = serde_json::json!(scope);
        }
        self.call("skills.list", params).await
    }

    /// Get a single skill by name with full body.
    pub async fn skills_get(&self, name: &str, kiln_path: &Path) -> Result<serde_json::Value> {
        self.call(
            "skills.get",
            serde_json::json!({
                "name": name,
                "kiln_path": kiln_path.to_string_lossy(),
            }),
        )
        .await
    }

    /// Search skills by text query (case-insensitive match on name + description).
    pub async fn skills_search(
        &self,
        query: &str,
        kiln_path: &Path,
        limit: Option<usize>,
    ) -> Result<serde_json::Value> {
        let mut params = serde_json::json!({
            "query": query,
            "kiln_path": kiln_path.to_string_lossy(),
        });
        if let Some(l) = limit {
            params["limit"] = serde_json::json!(l);
        }
        self.call("skills.search", params).await
    }

    /// List all available agent profiles (builtins + configured).
    pub async fn agents_list_profiles(&self) -> Result<serde_json::Value> {
        self.call("agents.list_profiles", serde_json::json!({}))
            .await
    }

    /// Resolve a named agent profile.
    pub async fn agents_resolve_profile(&self, name: &str) -> Result<serde_json::Value> {
        self.call(
            "agents.resolve_profile",
            serde_json::json!({ "name": name }),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_daemon::Server;
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
            .session_create("chat", tmp.path(), None, vec![], None, None)
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
        let result = client.session_list(None, None, None, None).await.unwrap();
        assert!(result.is_array() || result.is_object());
    }

    #[tokio::test]
    #[ignore = "requires running daemon with session support"]
    async fn test_session_lifecycle() {
        let client = DaemonClient::connect().await.unwrap();
        let tmp = TempDir::new().unwrap();

        let result = client
            .session_create("chat", tmp.path(), None, vec![], None, None)
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
            .session_create("chat", tmp.path(), None, vec![], None, None)
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
            .session_create("chat", tmp.path(), None, vec![], None, None)
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
            .session_create("chat", tmp.path(), None, vec![], None, None)
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
}
