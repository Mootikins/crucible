//! Unix socket server for JSON-RPC

use crate::kiln_manager::KilnManager;
use crate::protocol::{Request, Response, METHOD_NOT_FOUND, PARSE_ERROR, INVALID_PARAMS, INTERNAL_ERROR};
use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;
use tracing::{error, info, warn};

/// Daemon server that listens on a Unix socket
pub struct Server {
    listener: UnixListener,
    shutdown_tx: broadcast::Sender<()>,
    kiln_manager: Arc<KilnManager>,
}

impl Server {
    /// Bind to a Unix socket path
    pub async fn bind(path: &Path) -> Result<Self> {
        // Remove stale socket
        if path.exists() {
            std::fs::remove_file(path)?;
        }

        // Create parent directory
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(path)?;
        let (shutdown_tx, _) = broadcast::channel(1);

        info!("Daemon listening on {:?}", path);
        Ok(Self {
            listener,
            shutdown_tx,
            kiln_manager: Arc::new(KilnManager::new()),
        })
    }

    /// Get a shutdown sender for external shutdown triggers
    pub fn shutdown_handle(&self) -> broadcast::Sender<()> {
        self.shutdown_tx.clone()
    }

    /// Run the server until shutdown
    pub async fn run(self) -> Result<()> {
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                accept_result = self.listener.accept() => {
                    match accept_result {
                        Ok((stream, _)) => {
                            let shutdown_tx = self.shutdown_tx.clone();
                            let km = self.kiln_manager.clone();
                            tokio::spawn(async move {
                                if let Err(e) = handle_client(stream, shutdown_tx, km).await {
                                    error!("Client error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            error!("Accept error: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received");
                    break;
                }
            }
        }

        Ok(())
    }
}

async fn handle_client(
    stream: UnixStream,
    shutdown_tx: broadcast::Sender<()>,
    kiln_manager: Arc<KilnManager>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break; // EOF - client disconnected
        }

        let response = match serde_json::from_str::<Request>(&line) {
            Ok(req) => handle_request(req, &shutdown_tx, &kiln_manager).await,
            Err(e) => {
                warn!("Parse error: {}", e);
                Response::error(None, PARSE_ERROR, e.to_string())
            }
        };

        let mut output = serde_json::to_string(&response)?;
        output.push('\n');
        writer.write_all(output.as_bytes()).await?;
    }

    Ok(())
}

async fn handle_request(
    req: Request,
    shutdown_tx: &broadcast::Sender<()>,
    kiln_manager: &Arc<KilnManager>,
) -> Response {
    match req.method.as_str() {
        "ping" => Response::success(req.id, "pong"),
        "shutdown" => {
            info!("Shutdown requested via RPC");
            let _ = shutdown_tx.send(());
            Response::success(req.id, "shutting down")
        }
        "kiln.open" => handle_kiln_open(req, kiln_manager).await,
        "kiln.close" => handle_kiln_close(req, kiln_manager).await,
        "kiln.list" => handle_kiln_list(req, kiln_manager).await,
        _ => Response::error(req.id, METHOD_NOT_FOUND, format!("Unknown method: {}", req.method)),
    }
}

async fn handle_kiln_open(req: Request, km: &Arc<KilnManager>) -> Response {
    let path = match req.params.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return Response::error(req.id, INVALID_PARAMS, "Missing 'path' parameter"),
    };

    match km.open(Path::new(path)).await {
        Ok(()) => Response::success(req.id, serde_json::json!({"status": "ok"})),
        Err(e) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
    }
}

async fn handle_kiln_close(req: Request, km: &Arc<KilnManager>) -> Response {
    let path = match req.params.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return Response::error(req.id, INVALID_PARAMS, "Missing 'path' parameter"),
    };

    match km.close(Path::new(path)).await {
        Ok(()) => Response::success(req.id, serde_json::json!({"status": "ok"})),
        Err(e) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
    }
}

async fn handle_kiln_list(req: Request, km: &Arc<KilnManager>) -> Response {
    let kilns = km.list().await;
    let list: Vec<_> = kilns.iter()
        .map(|(path, last_access)| serde_json::json!({
            "path": path.to_string_lossy(),
            "last_access_secs_ago": last_access.elapsed().as_secs()
        }))
        .collect();
    Response::success(req.id, list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    #[tokio::test]
    async fn test_server_ping() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let shutdown_handle = server.shutdown_handle();

        // Spawn server
        let server_task = tokio::spawn(async move {
            server.run().await
        });

        // Give server time to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Connect and send ping
        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n").await.unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("\"result\":\"pong\""));
        assert!(response.contains("\"id\":1"));

        // Shutdown
        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }
}
