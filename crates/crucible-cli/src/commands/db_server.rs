//! Internal database server command
//!
//! This module implements the hidden `db-server` subcommand that runs
//! the database as a socket server. It's not intended for direct user
//! invocation - it's spawned automatically when `storage.mode = "daemon"`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, watch};
use tracing::{debug, error, info, warn};

use crucible_daemon::kiln_manager::KilnManager;
use crucible_daemon::protocol::{
    Request, Response, INTERNAL_ERROR, INVALID_PARAMS, METHOD_NOT_FOUND, PARSE_ERROR,
};

use crate::config::CliConfig;

/// Connection tracking for idle detection
struct ConnectionTracker {
    /// Number of active connections
    count: AtomicUsize,
    /// Sender for activity updates
    activity_tx: watch::Sender<Instant>,
}

impl ConnectionTracker {
    fn new() -> (Self, watch::Receiver<Instant>) {
        let (activity_tx, activity_rx) = watch::channel(Instant::now());
        (
            Self {
                count: AtomicUsize::new(0),
                activity_tx,
            },
            activity_rx,
        )
    }

    fn connect(&self) {
        self.count.fetch_add(1, Ordering::SeqCst);
        let _ = self.activity_tx.send(Instant::now());
    }

    fn disconnect(&self) {
        self.count.fetch_sub(1, Ordering::SeqCst);
        let _ = self.activity_tx.send(Instant::now());
    }

    fn active_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    fn record_activity(&self) {
        let _ = self.activity_tx.send(Instant::now());
    }
}

/// Execute the database server
///
/// Runs a Unix socket server that accepts JSON-RPC connections for
/// database queries. Automatically shuts down after idle_timeout
/// seconds with no active connections.
pub async fn execute(_config: CliConfig, socket: Option<PathBuf>, idle_timeout: u64) -> Result<()> {
    let socket_path = socket.unwrap_or_else(default_socket_path);
    let idle_duration = Duration::from_secs(idle_timeout);

    info!(
        "db-server starting on {} (idle timeout: {}s)",
        socket_path.display(),
        idle_timeout
    );

    // Remove stale socket
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    // Create parent directory if needed
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Bind socket
    let listener = UnixListener::bind(&socket_path)?;
    info!("db-server listening on {:?}", socket_path);

    // Setup shutdown and tracking
    let (shutdown_tx, _) = broadcast::channel::<()>(1);
    let (tracker, activity_rx) = ConnectionTracker::new();
    let tracker = Arc::new(tracker);
    let kiln_manager = Arc::new(KilnManager::new());

    // Spawn idle watchdog
    let watchdog_shutdown = shutdown_tx.clone();
    let watchdog_tracker = tracker.clone();
    let watchdog_handle = tokio::spawn(async move {
        idle_watchdog(
            watchdog_tracker,
            activity_rx,
            idle_duration,
            watchdog_shutdown,
        )
        .await
    });

    // Main accept loop
    let mut shutdown_rx = shutdown_tx.subscribe();

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _)) => {
                        let shutdown_sub = shutdown_tx.subscribe();
                        let km = kiln_manager.clone();
                        let t = tracker.clone();
                        let stx = shutdown_tx.clone();

                        t.connect();
                        debug!("Client connected (active: {})", t.active_count());

                        tokio::spawn(async move {
                            if let Err(e) = handle_client(stream, shutdown_sub, km, stx).await {
                                if !e.to_string().contains("connection reset") {
                                    error!("Client error: {}", e);
                                }
                            }
                            t.disconnect();
                            debug!("Client disconnected (active: {})", t.active_count());
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

    // Cleanup
    watchdog_handle.abort();
    let _ = std::fs::remove_file(&socket_path);
    info!("db-server shutdown complete");

    Ok(())
}

/// Idle watchdog - shuts down server after idle_timeout with no connections
async fn idle_watchdog(
    tracker: Arc<ConnectionTracker>,
    activity_rx: watch::Receiver<Instant>,
    timeout: Duration,
    shutdown_tx: broadcast::Sender<()>,
) {
    // Check interval - more frequently near timeout
    let check_interval = Duration::from_secs(std::cmp::max(1, timeout.as_secs() / 10));

    loop {
        tokio::time::sleep(check_interval).await;

        let active = tracker.active_count();
        let last_activity = *activity_rx.borrow();
        let idle_for = last_activity.elapsed();

        debug!(
            "Idle watchdog: {} connections, idle for {:?}",
            active, idle_for
        );

        if active == 0 && idle_for >= timeout {
            info!(
                "Idle timeout reached ({:?} with 0 connections), initiating shutdown",
                idle_for
            );
            let _ = shutdown_tx.send(());
            break;
        }
    }
}

async fn handle_client(
    stream: UnixStream,
    mut shutdown_rx: broadcast::Receiver<()>,
    kiln_manager: Arc<KilnManager>,
    shutdown_tx: broadcast::Sender<()>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();

        tokio::select! {
            result = reader.read_line(&mut line) => {
                match result {
                    Ok(0) => break, // EOF - client disconnected
                    Ok(_) => {
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
                    Err(e) => return Err(e.into()),
                }
            }
            _ = shutdown_rx.recv() => {
                debug!("Client handler received shutdown");
                break;
            }
        }
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
        _ => Response::error(
            req.id,
            METHOD_NOT_FOUND,
            format!("Unknown method: {}", req.method),
        ),
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
    let list: Vec<_> = kilns
        .iter()
        .map(|(path, last_access)| {
            serde_json::json!({
                "path": path.to_string_lossy(),
                "last_access_secs_ago": last_access.elapsed().as_secs()
            })
        })
        .collect();
    Response::success(req.id, list)
}

/// Get the default socket path
///
/// Uses XDG_RUNTIME_DIR if available, otherwise falls back to /tmp
fn default_socket_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("crucible-db.sock")
    } else {
        PathBuf::from("/tmp/crucible-db.sock")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_socket_path_ends_with_sock() {
        let path = default_socket_path();
        assert!(path.ends_with("crucible-db.sock"));
    }

    #[test]
    fn test_default_socket_path_is_absolute() {
        let path = default_socket_path();
        assert!(path.is_absolute());
    }

    #[test]
    fn test_connection_tracker_counting() {
        let (tracker, _rx) = ConnectionTracker::new();
        assert_eq!(tracker.active_count(), 0);

        tracker.connect();
        assert_eq!(tracker.active_count(), 1);

        tracker.connect();
        assert_eq!(tracker.active_count(), 2);

        tracker.disconnect();
        assert_eq!(tracker.active_count(), 1);

        tracker.disconnect();
        assert_eq!(tracker.active_count(), 0);
    }

    #[tokio::test]
    async fn test_connection_tracker_activity() {
        let (tracker, mut rx) = ConnectionTracker::new();
        let initial = *rx.borrow();

        tokio::time::sleep(Duration::from_millis(10)).await;

        tracker.record_activity();

        // Wait for the change
        rx.changed().await.unwrap();
        let updated = *rx.borrow();

        assert!(updated > initial);
    }
}
