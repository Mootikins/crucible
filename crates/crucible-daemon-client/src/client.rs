//! Daemon client implementation

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tracing::warn;

/// Session event received from daemon
///
/// Events are pushed to subscribed clients asynchronously when session
/// state changes occur.
#[derive(Debug, Clone)]
pub struct SessionEvent {
    /// The session this event belongs to
    pub session_id: String,
    /// The type of event (e.g., "message", "state_change", "error")
    pub event_type: String,
    /// Event-specific data payload
    pub data: serde_json::Value,
}

pub struct DaemonClient {
    reader: Mutex<BufReader<tokio::net::unix::OwnedReadHalf>>,
    writer: Mutex<tokio::net::unix::OwnedWriteHalf>,
    next_id: AtomicU64,
    /// Optional channel for sending events to callers
    event_tx: Option<mpsc::UnboundedSender<SessionEvent>>,
}

impl DaemonClient {
    /// Connect to the daemon at the default socket path
    pub async fn connect() -> Result<Self> {
        let path = crucible_daemon::socket_path();
        Self::connect_to(&path).await
    }

    /// Connect to daemon or start it if not running
    ///
    /// Uses socket-based detection:
    /// - If socket exists and connectable -> daemon running
    /// - If socket exists but not connectable -> stale socket, safe to replace
    /// - If socket doesn't exist -> daemon not running
    pub async fn connect_or_start() -> Result<Self> {
        // Try to connect first
        if let Ok(client) = Self::connect().await {
            return Ok(client);
        }

        // Connection failed - daemon not running or stale socket
        // Start the daemon
        Self::start_daemon().await?;

        // Wait for daemon to be ready with exponential backoff
        let mut delay = std::time::Duration::from_millis(50);
        for attempt in 0..10 {
            tokio::time::sleep(delay).await;
            if let Ok(client) = Self::connect().await {
                return Ok(client);
            }
            delay *= 2;
            if attempt > 5 {
                tracing::warn!("Daemon not ready after {} attempts", attempt + 1);
            }
        }

        anyhow::bail!("Failed to start daemon or connect after multiple attempts")
    }

    /// Start the daemon process in the background
    async fn start_daemon() -> Result<()> {
        use std::process::Command;

        // Find the cru-server binary
        let exe = std::env::current_exe()?;
        let daemon_exe = if exe.ends_with("cru") {
            // In development or installed alongside
            exe.parent()
                .ok_or_else(|| anyhow::anyhow!("No parent directory"))?
                .join("cru-server")
        } else {
            // Try PATH
            PathBuf::from("cru-server")
        };

        tracing::info!("Starting daemon: {:?}", daemon_exe);

        Command::new(daemon_exe)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn daemon: {}", e))?;

        Ok(())
    }

    /// Connect to daemon at a specific socket path
    pub async fn connect_to(path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(path).await?;
        let (read, write) = stream.into_split();

        Ok(Self {
            reader: Mutex::new(BufReader::new(read)),
            writer: Mutex::new(write),
            next_id: AtomicU64::new(1),
            event_tx: None,
        })
    }

    /// Connect to the daemon with event handling
    ///
    /// Returns a client and a receiver for async session events.
    /// Events are pushed to the receiver when the daemon sends them
    /// (e.g., after subscribing to a session).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (client, mut event_rx) = DaemonClient::connect_with_events().await?;
    ///
    /// // Subscribe to session events
    /// client.session_subscribe(&[session_id]).await?;
    ///
    /// // Receive events asynchronously
    /// while let Some(event) = event_rx.recv().await {
    ///     println!("Event: {} - {}", event.session_id, event.event_type);
    /// }
    /// ```
    pub async fn connect_with_events() -> Result<(Self, mpsc::UnboundedReceiver<SessionEvent>)> {
        let path = crucible_daemon::socket_path();
        Self::connect_to_with_events(&path).await
    }

    /// Connect to daemon at a specific socket path with event handling
    pub async fn connect_to_with_events(
        path: &Path,
    ) -> Result<(Self, mpsc::UnboundedReceiver<SessionEvent>)> {
        let stream = UnixStream::connect(path).await?;
        let (read, write) = stream.into_split();

        let (tx, rx) = mpsc::unbounded_channel();

        let client = Self {
            reader: Mutex::new(BufReader::new(read)),
            writer: Mutex::new(write),
            next_id: AtomicU64::new(1),
            event_tx: Some(tx),
        };

        Ok((client, rx))
    }

    /// Read the next message from the daemon, dispatching events to the channel
    ///
    /// This method loops until it receives an RPC response. Any async events
    /// encountered are dispatched to the event channel (if configured) and
    /// reading continues.
    async fn read_message(&self) -> Result<serde_json::Value> {
        loop {
            let mut line = String::new();
            {
                let mut reader = self.reader.lock().await;
                let bytes_read = reader.read_line(&mut line).await?;
                if bytes_read == 0 {
                    anyhow::bail!("Connection closed by daemon");
                }
            }

            let msg: serde_json::Value = serde_json::from_str(&line)?;

            // Check if this is an async event (has "type": "event")
            if msg.get("type").and_then(|t| t.as_str()) == Some("event") {
                // Dispatch to event channel if configured
                if let Some(ref tx) = self.event_tx {
                    let event = SessionEvent {
                        session_id: msg
                            .get("session_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        event_type: msg
                            .get("event")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        data: msg.get("data").cloned().unwrap_or(serde_json::Value::Null),
                    };
                    // Ignore send errors (receiver may have been dropped)
                    let _ = tx.send(event);
                }
                // Continue reading for RPC response
                continue;
            }

            // This is an RPC response
            return Ok(msg);
        }
    }

    /// Send a JSON-RPC request and get the response
    ///
    /// If the client was created with event handling (`connect_with_events`),
    /// any async events received while waiting for the response will be
    /// dispatched to the event channel.
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

        {
            let mut writer = self.writer.lock().await;
            writer.write_all(req_str.as_bytes()).await?;
        }

        // Read response, dispatching any events encountered along the way
        let response = self.read_message().await?;

        if let Some(error) = response.get("error") {
            anyhow::bail!("RPC error: {}", error);
        }

        Ok(response
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    /// Ping the daemon
    pub async fn ping(&self) -> Result<String> {
        let result = self.call("ping", serde_json::json!({})).await?;
        Ok(result.as_str().unwrap_or("").to_string())
    }

    /// Request daemon shutdown
    pub async fn shutdown(&self) -> Result<()> {
        self.call("shutdown", serde_json::json!({})).await?;
        Ok(())
    }

    /// Open a kiln
    pub async fn kiln_open(&self, path: &Path) -> Result<()> {
        self.call(
            "kiln.open",
            serde_json::json!({
                "path": path.to_string_lossy()
            }),
        )
        .await?;
        Ok(())
    }

    /// Close a kiln
    pub async fn kiln_close(&self, path: &Path) -> Result<()> {
        self.call(
            "kiln.close",
            serde_json::json!({
                "path": path.to_string_lossy()
            }),
        )
        .await?;
        Ok(())
    }

    /// List open kilns
    pub async fn kiln_list(&self) -> Result<Vec<serde_json::Value>> {
        let result = self.call("kiln.list", serde_json::json!({})).await?;
        Ok(result.as_array().cloned().unwrap_or_default())
    }

    /// Search for similar vectors (backend-agnostic VSS)
    ///
    /// Returns a list of (document_id, score) pairs.
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

    /// List notes in a kiln (backend-agnostic)
    ///
    /// Returns a list of (name, path, title, tags, updated_at) tuples.
    pub async fn list_notes(
        &self,
        kiln_path: &Path,
        path_filter: Option<&str>,
    ) -> Result<Vec<(String, String, Option<String>, Vec<String>, Option<String>)>> {
        let mut params = serde_json::json!({
            "kiln": kiln_path.to_string_lossy()
        });
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

                // Log warning for malformed items (helps debug API changes or data issues)
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

    /// Get a note by name (backend-agnostic)
    ///
    /// Returns the note data as JSON if found, None if not found.
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

    /// Upsert a note record via daemon RPC
    ///
    /// Sends the note record to the daemon for storage.
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

    /// Get a note by path via daemon RPC
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

    /// Delete a note by path via daemon RPC
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

    /// List all notes via daemon RPC
    pub async fn note_list(
        &self,
        kiln_path: &Path,
    ) -> Result<Vec<crucible_core::storage::NoteRecord>> {
        let result = self
            .call(
                "note.list",
                serde_json::json!({
                    "kiln": kiln_path.to_string_lossy()
                }),
            )
            .await?;

        let notes: Vec<crucible_core::storage::NoteRecord> = serde_json::from_value(result)?;
        Ok(notes)
    }

    // =========================================================================
    // Pipeline RPC Methods
    // =========================================================================

    /// Process a single file through the daemon's pipeline
    ///
    /// Returns true if the file was processed, false if skipped (unchanged).
    pub async fn process_file(&self, kiln_path: &Path, file_path: &Path) -> Result<bool> {
        let result = self
            .call(
                "process_file",
                serde_json::json!({
                    "kiln": kiln_path.to_string_lossy(),
                    "path": file_path.to_string_lossy()
                }),
            )
            .await?;

        let status = result
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        Ok(status == "processed")
    }

    /// Process multiple files through the daemon's pipeline
    ///
    /// Returns (processed_count, skipped_count, errors)
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
    // Session RPC Methods
    // =========================================================================

    /// Create a new session
    pub async fn session_create(
        &self,
        session_type: &str,
        kiln: &Path,
        workspace: Option<&Path>,
        connect_kilns: Vec<&Path>,
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

        self.call("session.create", params).await
    }

    /// List sessions with optional filters
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

    /// Get a session by ID
    pub async fn session_get(&self, session_id: &str) -> Result<serde_json::Value> {
        self.call(
            "session.get",
            serde_json::json!({
                "session_id": session_id
            }),
        )
        .await
    }

    /// Pause a session
    pub async fn session_pause(&self, session_id: &str) -> Result<serde_json::Value> {
        self.call(
            "session.pause",
            serde_json::json!({
                "session_id": session_id
            }),
        )
        .await
    }

    /// Resume a paused session
    pub async fn session_resume(&self, session_id: &str) -> Result<serde_json::Value> {
        self.call(
            "session.resume",
            serde_json::json!({
                "session_id": session_id
            }),
        )
        .await
    }

    /// End a session
    pub async fn session_end(&self, session_id: &str) -> Result<serde_json::Value> {
        self.call(
            "session.end",
            serde_json::json!({
                "session_id": session_id
            }),
        )
        .await
    }

    /// Resume a session from storage with optional pagination for history
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

    /// Request compaction for a session
    ///
    /// Sets the session state to Compacting. The actual summarization
    /// is performed by the agent.
    pub async fn session_compact(&self, session_id: &str) -> Result<serde_json::Value> {
        self.call(
            "session.compact",
            serde_json::json!({
                "session_id": session_id
            }),
        )
        .await
    }

    /// Subscribe to session events
    pub async fn session_subscribe(&self, session_ids: &[&str]) -> Result<serde_json::Value> {
        self.call(
            "session.subscribe",
            serde_json::json!({
                "session_ids": session_ids
            }),
        )
        .await
    }

    /// Unsubscribe from session events
    pub async fn session_unsubscribe(&self, session_ids: &[&str]) -> Result<serde_json::Value> {
        self.call(
            "session.unsubscribe",
            serde_json::json!({
                "session_ids": session_ids
            }),
        )
        .await
    }

    /// Configure the agent for a session
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

    /// Send a message to a session (streaming via events)
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

    /// Cancel an active request for a session
    pub async fn session_cancel(&self, session_id: &str) -> Result<bool> {
        let result = self
            .call(
                "session.cancel",
                serde_json::json!({
                    "session_id": session_id
                }),
            )
            .await?;

        Ok(result["cancelled"].as_bool().unwrap_or(false))
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

        let server = Server::bind(&sock_path).await.unwrap();
        let _shutdown_handle = server.shutdown_handle();

        let handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

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

    // =========================================================================
    // Session RPC Tests (require running daemon)
    // =========================================================================

    // Note: These tests require a running daemon with session support,
    // so they are marked as ignored. Run manually with: cargo test --ignored

    #[tokio::test]
    #[ignore = "requires running daemon with session support"]
    async fn test_session_create_and_get() {
        let client = DaemonClient::connect().await.unwrap();
        let tmp = TempDir::new().unwrap();

        let result = client
            .session_create("chat", tmp.path(), None, vec![])
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

        // List all sessions (may be empty)
        let result = client.session_list(None, None, None, None).await.unwrap();
        assert!(result.is_array());
    }

    #[tokio::test]
    #[ignore = "requires running daemon with session support"]
    async fn test_session_lifecycle() {
        let client = DaemonClient::connect().await.unwrap();
        let tmp = TempDir::new().unwrap();

        // Create a session
        let result = client
            .session_create("chat", tmp.path(), None, vec![])
            .await
            .unwrap();
        let session_id = result["session_id"].as_str().unwrap();

        // Pause the session
        let pause_result = client.session_pause(session_id).await;
        assert!(pause_result.is_ok());

        // Resume the session
        let resume_result = client.session_resume(session_id).await;
        assert!(resume_result.is_ok());

        // End the session
        let end_result = client.session_end(session_id).await;
        assert!(end_result.is_ok());
    }

    #[tokio::test]
    #[ignore = "requires running daemon with session support"]
    async fn test_session_subscribe_unsubscribe() {
        let client = DaemonClient::connect().await.unwrap();
        let tmp = TempDir::new().unwrap();

        // Create a session
        let result = client
            .session_create("chat", tmp.path(), None, vec![])
            .await
            .unwrap();
        let session_id = result["session_id"].as_str().unwrap();

        // Subscribe to session events
        let sub_result = client.session_subscribe(&[session_id]).await;
        assert!(sub_result.is_ok());

        // Unsubscribe from session events
        let unsub_result = client.session_unsubscribe(&[session_id]).await;
        assert!(unsub_result.is_ok());

        // Clean up
        let _ = client.session_end(session_id).await;
    }

    #[tokio::test]
    #[ignore = "requires running daemon with session support"]
    async fn test_event_stream() {
        let (client, mut event_rx) = DaemonClient::connect_with_events().await.unwrap();
        let tmp = TempDir::new().unwrap();

        // Create a session
        let result = client
            .session_create("chat", tmp.path(), None, vec![])
            .await
            .unwrap();
        let session_id = result["session_id"].as_str().unwrap();

        // Subscribe to the session
        client.session_subscribe(&[session_id]).await.unwrap();

        // In a real scenario, events would be sent by the daemon when
        // the session is active. For testing, we just verify the channel exists
        // and can be polled without blocking indefinitely.

        // Try to receive with timeout (should timeout since no events yet)
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(100), event_rx.recv()).await;

        // Timeout expected since no events have been sent
        assert!(result.is_err(), "Expected timeout, got event");

        // Clean up
        let _ = client.session_end(session_id).await;
    }

    #[tokio::test]
    async fn test_connect_to_with_events() {
        let (_tmp, sock_path, _handle) = setup_test_server().await;

        // Connect with event handling
        let (client, _event_rx) = DaemonClient::connect_to_with_events(&sock_path)
            .await
            .unwrap();

        // Verify the client works normally
        let result = client.ping().await.unwrap();
        assert_eq!(result, "pong");
    }
}
