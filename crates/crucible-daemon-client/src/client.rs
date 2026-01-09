//! Daemon client implementation

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Mutex;
use tracing::warn;

pub struct DaemonClient {
    reader: Mutex<BufReader<tokio::net::unix::OwnedReadHalf>>,
    writer: Mutex<tokio::net::unix::OwnedWriteHalf>,
    next_id: AtomicU64,
}

impl DaemonClient {
    /// Connect to the daemon at the default socket path
    pub async fn connect() -> Result<Self> {
        let path = crucible_daemon::socket_path();
        Self::connect_to(&path).await
    }

    /// Connect to daemon or start it if not running
    pub async fn connect_or_start() -> Result<Self> {
        // Try to connect first
        match Self::connect().await {
            Ok(client) => return Ok(client),
            Err(_) => {
                // Check if daemon is actually running (stale socket)
                if crucible_daemon::is_daemon_running() {
                    // PID exists but can't connect - try again after a short delay
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    return Self::connect().await;
                }
            }
        }

        // Daemon not running - start it
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

        // Find the cru-daemon binary
        let exe = std::env::current_exe()?;
        let daemon_exe = if exe.ends_with("cru") {
            // In development or installed alongside
            exe.parent()
                .ok_or_else(|| anyhow::anyhow!("No parent directory"))?
                .join("cru-daemon")
        } else {
            // Try PATH
            PathBuf::from("cru-daemon")
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
        })
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

        {
            let mut writer = self.writer.lock().await;
            writer.write_all(req_str.as_bytes()).await?;
        }

        let mut line = String::new();
        {
            let mut reader = self.reader.lock().await;
            reader.read_line(&mut line).await?;
        }

        let response: serde_json::Value = serde_json::from_str(&line)?;

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

        let processed = result.get("processed").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
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
}
