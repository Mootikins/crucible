//! Daemon client implementation

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

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

    /// Execute a query against a kiln
    pub async fn query(&self, kiln_path: &Path, sql: &str) -> Result<serde_json::Value> {
        self.call(
            "query",
            serde_json::json!({
                "kiln": kiln_path.to_string_lossy(),
                "sql": sql
            }),
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
