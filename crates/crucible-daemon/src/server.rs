//! Unix socket server for JSON-RPC

use crate::agent_manager::AgentManager;
use crate::kiln_manager::KilnManager;
use crate::protocol::{
    Request, Response, SessionEventMessage, INTERNAL_ERROR, INVALID_PARAMS, METHOD_NOT_FOUND,
    PARSE_ERROR,
};
use crate::rpc_helpers::{
    optional_str_param, optional_u64_param, require_array_param, require_i64_param,
    require_str_param,
};
use crate::session_manager::SessionManager;
use crate::session_storage::{FileSessionStorage, SessionStorage};
use anyhow::Result;

use crate::protocol::RequestId;
use crate::subscription::{ClientId, SubscriptionManager};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// Log internal error details and return a generic error message.
fn internal_error(req_id: Option<RequestId>, err: impl std::fmt::Display) -> Response {
    error!("Internal error: {}", err);
    Response::error(req_id, INTERNAL_ERROR, "Internal server error")
}

/// Log client error details and return a sanitized error message.
fn invalid_state_error(
    req_id: Option<RequestId>,
    operation: &str,
    err: impl std::fmt::Display,
) -> Response {
    debug!("Invalid state for {}: {}", operation, err);
    Response::error(
        req_id,
        INVALID_PARAMS,
        format!("Operation '{}' not allowed in current state", operation),
    )
}

fn session_not_found(req_id: Option<RequestId>, session_id: &str) -> Response {
    Response::error(
        req_id,
        INVALID_PARAMS,
        format!("Session not found: {}", session_id),
    )
}

fn agent_not_configured(req_id: Option<RequestId>, session_id: &str) -> Response {
    Response::error(
        req_id,
        INVALID_PARAMS,
        format!("No agent configured for session: {}", session_id),
    )
}

fn concurrent_request(req_id: Option<RequestId>, session_id: &str) -> Response {
    Response::error(
        req_id,
        INVALID_PARAMS,
        format!("Request already in progress for session: {}", session_id),
    )
}

fn invalid_param_range(req_id: Option<RequestId>, param: &str, constraint: &str) -> Response {
    Response::error(
        req_id,
        INVALID_PARAMS,
        format!("Invalid '{}': {}", param, constraint),
    )
}

/// Daemon server that listens on a Unix socket
pub struct Server {
    listener: UnixListener,
    shutdown_tx: broadcast::Sender<()>,
    kiln_manager: Arc<KilnManager>,
    session_manager: Arc<SessionManager>,
    agent_manager: Arc<AgentManager>,
    subscription_manager: Arc<SubscriptionManager>,
    event_tx: broadcast::Sender<SessionEventMessage>,
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
        let (event_tx, _) = broadcast::channel(1024);

        let session_manager = Arc::new(SessionManager::new());
        let agent_manager = Arc::new(AgentManager::new(session_manager.clone()));

        info!("Daemon listening on {:?}", path);
        Ok(Self {
            listener,
            shutdown_tx,
            kiln_manager: Arc::new(KilnManager::new()),
            session_manager,
            agent_manager,
            subscription_manager: Arc::new(SubscriptionManager::new()),
            event_tx,
        })
    }

    /// Get a shutdown sender for external shutdown triggers
    #[allow(dead_code)]
    pub fn shutdown_handle(&self) -> broadcast::Sender<()> {
        self.shutdown_tx.clone()
    }

    /// Get a clone of the event broadcast sender.
    ///
    /// Used to send session events to all subscribed clients.
    #[allow(dead_code)]
    pub fn event_sender(&self) -> broadcast::Sender<SessionEventMessage> {
        self.event_tx.clone()
    }

    /// Run the server until shutdown
    pub async fn run(self) -> Result<()> {
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        // Spawn event persistence task with cancellation support
        let storage = FileSessionStorage::new();
        let sm_clone = self.session_manager.clone();
        let mut persist_rx = self.event_tx.subscribe();
        let persist_cancel = CancellationToken::new();
        let persist_cancel_clone = persist_cancel.clone();

        let persist_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = persist_cancel_clone.cancelled() => {
                        debug!("Persist task received shutdown signal, draining remaining events");
                        // Drain any remaining events with a short timeout
                        while let Ok(event) = persist_rx.try_recv() {
                            if let Some(session) = sm_clone.get_session(&event.session_id) {
                                if let Ok(json) = serde_json::to_string(&event) {
                                    let _ = storage.append_event(&session, &json).await;
                                }
                            }
                        }
                        break;
                    }
                    result = persist_rx.recv() => {
                        match result {
                            Ok(event) => {
                                // Try to get the session and persist the event
                                if let Some(session) = sm_clone.get_session(&event.session_id) {
                                    let json = match serde_json::to_string(&event) {
                                        Ok(j) => j,
                                        Err(e) => {
                                            tracing::warn!("Failed to serialize event: {}", e);
                                            continue;
                                        }
                                    };

                                    if let Err(e) = storage.append_event(&session, &json).await {
                                        tracing::warn!("Failed to persist event: {}", e);
                                    }
                                }
                            }
                            Err(_) => break, // Channel closed
                        }
                    }
                }
            }
        });

        loop {
            tokio::select! {
                accept_result = self.listener.accept() => {
                    match accept_result {
                        Ok((stream, _)) => {
                            let shutdown_tx = self.shutdown_tx.clone();
                            let km = self.kiln_manager.clone();
                            let sm = self.session_manager.clone();
                            let am = self.agent_manager.clone();
                            let sub_m = self.subscription_manager.clone();
                            let event_tx = self.event_tx.clone();
                            let event_rx = self.event_tx.subscribe();
                            tokio::spawn(async move {
                                if let Err(e) = handle_client(stream, shutdown_tx, km, sm, am, sub_m, event_tx, event_rx).await {
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

        // Graceful shutdown: signal cancellation, wait with timeout, then abort if needed
        persist_cancel.cancel();
        match tokio::time::timeout(std::time::Duration::from_secs(5), persist_task).await {
            Ok(Ok(())) => debug!("Persist task completed gracefully"),
            Ok(Err(e)) => warn!("Persist task panicked: {}", e),
            Err(_) => warn!("Persist task did not complete within timeout, aborting"),
        }

        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_client(
    stream: UnixStream,
    shutdown_tx: broadcast::Sender<()>,
    kiln_manager: Arc<KilnManager>,
    session_manager: Arc<SessionManager>,
    agent_manager: Arc<AgentManager>,
    subscription_manager: Arc<SubscriptionManager>,
    event_tx: broadcast::Sender<SessionEventMessage>,
    mut event_rx: broadcast::Receiver<SessionEventMessage>,
) -> Result<()> {
    let client_id = ClientId::new();
    let (reader, writer) = stream.into_split();
    let writer: Arc<Mutex<OwnedWriteHalf>> = Arc::new(Mutex::new(writer));
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    // Spawn event forwarding task with cancellation support
    let writer_clone = writer.clone();
    let sub_manager = subscription_manager.clone();
    let event_cancel = CancellationToken::new();
    let event_cancel_clone = event_cancel.clone();
    let event_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = event_cancel_clone.cancelled() => break,
                result = event_rx.recv() => {
                    match result {
                        Ok(event) => {
                            if sub_manager.is_subscribed(client_id, &event.session_id) {
                                if let Ok(json) = event.to_json_line() {
                                    let mut w = writer_clone.lock().await;
                                    if w.write_all(json.as_bytes()).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    });

    // Main request loop
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break; // EOF - client disconnected
        }

        let response = match serde_json::from_str::<Request>(&line) {
            Ok(req) => {
                handle_request(
                    req,
                    client_id,
                    &shutdown_tx,
                    &kiln_manager,
                    &session_manager,
                    &agent_manager,
                    &subscription_manager,
                    &event_tx,
                )
                .await
            }
            Err(e) => {
                warn!("Parse error: {}", e);
                Response::error(None, PARSE_ERROR, e.to_string())
            }
        };

        let mut output = serde_json::to_string(&response)?;
        output.push('\n');

        let mut w = writer.lock().await;
        w.write_all(output.as_bytes()).await?;
    }

    // Graceful shutdown of event forwarding
    event_cancel.cancel();
    let _ = tokio::time::timeout(std::time::Duration::from_millis(100), event_task).await;
    subscription_manager.remove_client(client_id);

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_request(
    req: Request,
    client_id: ClientId,
    shutdown_tx: &broadcast::Sender<()>,
    kiln_manager: &Arc<KilnManager>,
    session_manager: &Arc<SessionManager>,
    agent_manager: &Arc<AgentManager>,
    subscription_manager: &Arc<SubscriptionManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    tracing::debug!("RPC request: method={:?}, id={:?}", req.method, req.id);
    match req.method.as_str() {
        "ping" => Response::success(req.id, "pong"),
        "daemon.capabilities" => handle_daemon_capabilities(req),
        "shutdown" => {
            info!("Shutdown requested via RPC");
            let _ = shutdown_tx.send(());
            Response::success(req.id, "shutting down")
        }
        "kiln.open" => handle_kiln_open(req, kiln_manager).await,
        "kiln.close" => handle_kiln_close(req, kiln_manager).await,
        "kiln.list" => handle_kiln_list(req, kiln_manager).await,
        "search_vectors" => handle_search_vectors(req, kiln_manager).await,
        "list_notes" => handle_list_notes(req, kiln_manager).await,
        "get_note_by_name" => handle_get_note_by_name(req, kiln_manager).await,
        // NoteStore RPC methods
        "note.upsert" => handle_note_upsert(req, kiln_manager).await,
        "note.get" => handle_note_get(req, kiln_manager).await,
        "note.delete" => handle_note_delete(req, kiln_manager).await,
        "note.list" => handle_note_list(req, kiln_manager).await,
        // Pipeline RPC methods
        "process_file" => handle_process_file(req, kiln_manager).await,
        "process_batch" => handle_process_batch(req, kiln_manager).await,
        // Session RPC methods
        "session.create" => handle_session_create(req, session_manager).await,
        "session.list" => handle_session_list(req, session_manager).await,
        "session.get" => handle_session_get(req, session_manager).await,
        "session.pause" => handle_session_pause(req, session_manager).await,
        "session.resume" => handle_session_resume(req, session_manager).await,
        "session.resume_from_storage" => {
            handle_session_resume_from_storage(req, session_manager).await
        }
        "session.end" => handle_session_end(req, session_manager).await,
        "session.compact" => handle_session_compact(req, session_manager).await,
        // Subscription RPC methods
        "session.subscribe" => handle_session_subscribe(req, client_id, subscription_manager).await,
        "session.unsubscribe" => {
            handle_session_unsubscribe(req, client_id, subscription_manager).await
        }
        // Agent RPC methods
        "session.configure_agent" => handle_session_configure_agent(req, agent_manager).await,
        "session.send_message" => handle_session_send_message(req, agent_manager, event_tx).await,
        "session.cancel" => handle_session_cancel(req, agent_manager).await,
        "session.switch_model" => handle_session_switch_model(req, agent_manager, event_tx).await,
        "session.list_models" => handle_session_list_models(req, agent_manager).await,
        "session.set_thinking_budget" => {
            handle_session_set_thinking_budget(req, agent_manager, event_tx).await
        }
        "session.get_thinking_budget" => {
            handle_session_get_thinking_budget(req, agent_manager).await
        }
        _ => {
            tracing::warn!("Unknown RPC method: {:?}", req.method);
            Response::error(
                req.id,
                METHOD_NOT_FOUND,
                format!("Unknown method: {}", req.method),
            )
        }
    }
}

fn handle_daemon_capabilities(req: Request) -> Response {
    Response::success(
        req.id,
        serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "protocol_version": "1.0",
            "capabilities": {
                "kilns": true,
                "sessions": true,
                "agents": true,
                "events": true,
                "thinking_budget": true,
                "model_switching": true,
            },
            "methods": [
                "ping",
                "shutdown",
                "daemon.capabilities",
                "kiln.open",
                "kiln.close",
                "kiln.list",
                "search_vectors",
                "list_notes",
                "get_note_by_name",
                "note.upsert",
                "note.get",
                "note.delete",
                "note.list",
                "process_file",
                "process_batch",
                "session.create",
                "session.list",
                "session.get",
                "session.pause",
                "session.resume",
                "session.resume_from_storage",
                "session.end",
                "session.compact",
                "session.subscribe",
                "session.unsubscribe",
                "session.configure_agent",
                "session.send_message",
                "session.cancel",
                "session.switch_model",
                "session.list_models",
                "session.set_thinking_budget",
                "session.get_thinking_budget",
            ]
        }),
    )
}

async fn handle_kiln_open(req: Request, km: &Arc<KilnManager>) -> Response {
    let path = require_str_param!(req, "path");

    match km.open(Path::new(path)).await {
        Ok(()) => Response::success(req.id, serde_json::json!({"status": "ok"})),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_kiln_close(req: Request, km: &Arc<KilnManager>) -> Response {
    let path = require_str_param!(req, "path");

    match km.close(Path::new(path)).await {
        Ok(()) => Response::success(req.id, serde_json::json!({"status": "ok"})),
        Err(e) => internal_error(req.id, e),
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

async fn handle_search_vectors(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_str_param!(req, "kiln");
    let vector_arr = require_array_param!(req, "vector");
    let vector: Vec<f32> = vector_arr
        .iter()
        .filter_map(|v: &serde_json::Value| v.as_f64().map(|f| f as f32))
        .collect();
    let limit = optional_u64_param!(req, "limit").unwrap_or(20) as usize;

    // Get or open connection to the kiln
    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    // Execute vector search using the backend-agnostic method
    match handle.search_vectors(vector, limit).await {
        Ok(results) => {
            let json_results: Vec<_> = results
                .into_iter()
                .map(|(doc_id, score)| {
                    serde_json::json!({
                        "document_id": doc_id,
                        "score": score
                    })
                })
                .collect();
            Response::success(req.id, json_results)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_list_notes(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_str_param!(req, "kiln");
    let path_filter = optional_str_param!(req, "path_filter");

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    match handle.list_notes(path_filter).await {
        Ok(notes) => {
            let json_notes: Vec<_> = notes
                .into_iter()
                .map(|n| {
                    serde_json::json!({
                        "name": n.name,
                        "path": n.path,
                        "title": n.title,
                        "tags": n.tags,
                        "updated_at": n.updated_at.map(|t| t.to_rfc3339())
                    })
                })
                .collect();
            Response::success(req.id, json_notes)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_get_note_by_name(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_str_param!(req, "kiln");
    let name = require_str_param!(req, "name");

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    match handle.get_note_by_name(name).await {
        Ok(Some(note)) => Response::success(
            req.id,
            serde_json::json!({
                "path": note.path,
                "title": note.title,
                "tags": note.tags,
                "links_to": note.links_to,
                "content_hash": note.content_hash.to_string()
            }),
        ),
        Ok(None) => Response::success(req.id, serde_json::Value::Null),
        Err(e) => internal_error(req.id, e),
    }
}

// =============================================================================
// NoteStore RPC Handlers
// =============================================================================

async fn handle_note_upsert(req: Request, km: &Arc<KilnManager>) -> Response {
    use crucible_core::storage::{NoteRecord, NoteStore};

    let kiln_path = require_str_param!(req, "kiln");

    let note_json = match req.params.get("note") {
        Some(n) => n,
        None => return Response::error(req.id, INVALID_PARAMS, "Missing 'note' parameter"),
    };

    let note: NoteRecord = match serde_json::from_value(note_json.clone()) {
        Ok(n) => n,
        Err(e) => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!("Invalid note record: {}", e),
            )
        }
    };

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();
    match note_store.upsert(note).await {
        Ok(events) => Response::success(
            req.id,
            serde_json::json!({
                "status": "ok",
                "events_count": events.len()
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_note_get(req: Request, km: &Arc<KilnManager>) -> Response {
    use crucible_core::storage::NoteStore;

    let kiln_path = require_str_param!(req, "kiln");
    let path = require_str_param!(req, "path");

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();
    match note_store.get(path).await {
        Ok(Some(note)) => match serde_json::to_value(&note) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => internal_error(req.id, e),
        },
        Ok(None) => Response::success(req.id, serde_json::Value::Null),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_note_delete(req: Request, km: &Arc<KilnManager>) -> Response {
    use crucible_core::storage::NoteStore;

    let kiln_path = require_str_param!(req, "kiln");
    let path = require_str_param!(req, "path");

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();
    match note_store.delete(path).await {
        Ok(_event) => Response::success(req.id, serde_json::json!({"status": "ok"})),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_note_list(req: Request, km: &Arc<KilnManager>) -> Response {
    use crucible_core::storage::NoteStore;

    let kiln_path = require_str_param!(req, "kiln");

    let handle = match km.get_or_open(Path::new(kiln_path)).await {
        Ok(c) => c,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();
    match note_store.list().await {
        Ok(notes) => match serde_json::to_value(&notes) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => internal_error(req.id, e),
        },
        Err(e) => internal_error(req.id, e),
    }
}

// =============================================================================
// Pipeline RPC Handlers
// =============================================================================

async fn handle_process_file(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_str_param!(req, "kiln");
    let file_path = require_str_param!(req, "path");

    match km
        .process_file(Path::new(kiln_path), Path::new(file_path))
        .await
    {
        Ok(processed) => Response::success(
            req.id,
            serde_json::json!({
                "status": if processed { "processed" } else { "skipped" },
                "path": file_path
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_process_batch(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_path = require_str_param!(req, "kiln");
    let paths_arr = require_array_param!(req, "paths");
    let paths: Vec<std::path::PathBuf> = paths_arr
        .iter()
        .filter_map(|v: &serde_json::Value| v.as_str().map(std::path::PathBuf::from))
        .collect();

    match km.process_batch(Path::new(kiln_path), &paths).await {
        Ok((processed, skipped, errors)) => Response::success(
            req.id,
            serde_json::json!({
                "processed": processed,
                "skipped": skipped,
                "errors": errors.iter().map(|(p, _)| {
                    serde_json::json!({
                        "path": p.to_string_lossy(),
                        "error": "processing failed"
                    })
                }).collect::<Vec<_>>()
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Session RPC handlers
// ─────────────────────────────────────────────────────────────────────────────

use crucible_core::session::{SessionState, SessionType};

async fn handle_session_create(req: Request, sm: &Arc<SessionManager>) -> Response {
    // Parse session type (optional, defaults to "chat")
    let session_type_str = optional_str_param!(req, "type").unwrap_or("chat");
    let session_type = match session_type_str {
        "chat" => SessionType::Chat,
        "agent" => SessionType::Agent,
        "workflow" => SessionType::Workflow,
        _ => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!("Invalid session type: {}", session_type_str),
            );
        }
    };

    // Parse kiln path (required)
    let kiln = PathBuf::from(require_str_param!(req, "kiln"));

    // Parse optional workspace
    let workspace = optional_str_param!(req, "workspace").map(PathBuf::from);

    // Parse optional connected kilns
    let connected_kilns: Vec<PathBuf> = req
        .params
        .get("connect_kilns")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(PathBuf::from))
                .collect()
        })
        .unwrap_or_default();

    match sm
        .create_session(session_type, kiln, workspace, connected_kilns)
        .await
    {
        Ok(session) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session.id,
                "type": session.session_type.as_prefix(),
                "kiln": session.kiln,
                "workspace": session.workspace,
                "state": format!("{}", session.state),
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_list(req: Request, sm: &Arc<SessionManager>) -> Response {
    // Parse optional filters
    let kiln = optional_str_param!(req, "kiln").map(PathBuf::from);
    let workspace = optional_str_param!(req, "workspace").map(PathBuf::from);
    let session_type = optional_str_param!(req, "type").and_then(|s| match s {
        "chat" => Some(SessionType::Chat),
        "agent" => Some(SessionType::Agent),
        "workflow" => Some(SessionType::Workflow),
        _ => None,
    });
    let state = optional_str_param!(req, "state").and_then(|s| match s {
        "active" => Some(SessionState::Active),
        "paused" => Some(SessionState::Paused),
        "compacting" => Some(SessionState::Compacting),
        "ended" => Some(SessionState::Ended),
        _ => None,
    });

    let sessions =
        sm.list_sessions_filtered(kiln.as_ref(), workspace.as_ref(), session_type, state);

    let sessions_json: Vec<_> = sessions
        .iter()
        .map(|s| {
            serde_json::json!({
                "session_id": s.id,
                "type": s.session_type.as_prefix(),
                "kiln": s.kiln,
                "workspace": s.workspace,
                "state": format!("{}", s.state),
                "started_at": s.started_at.to_rfc3339(),
                "title": s.title,
            })
        })
        .collect();

    Response::success(
        req.id,
        serde_json::json!({
            "sessions": sessions_json,
            "total": sessions_json.len(),
        }),
    )
}

async fn handle_session_get(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_str_param!(req, "session_id");

    match sm.get_session(session_id) {
        Some(session) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session.id,
                "type": session.session_type.as_prefix(),
                "kiln": session.kiln,
                "workspace": session.workspace,
                "connected_kilns": session.connected_kilns,
                "state": format!("{}", session.state),
                "started_at": session.started_at.to_rfc3339(),
                "title": session.title,
                "continued_from": session.continued_from,
                "agent": session.agent,
            }),
        ),
        None => Response::error(
            req.id,
            INVALID_PARAMS,
            format!("Session not found: {}", session_id),
        ),
    }
}

async fn handle_session_pause(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_str_param!(req, "session_id");

    match sm.pause_session(session_id).await {
        Ok(previous_state) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "previous_state": format!("{}", previous_state),
                "state": "paused",
            }),
        ),
        Err(e) => invalid_state_error(req.id, "pause", e),
    }
}

async fn handle_session_resume(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_str_param!(req, "session_id");

    match sm.resume_session(session_id).await {
        Ok(previous_state) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "previous_state": format!("{}", previous_state),
                "state": "active",
            }),
        ),
        Err(e) => invalid_state_error(req.id, "resume", e),
    }
}

async fn handle_session_resume_from_storage(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_str_param!(req, "session_id");
    let kiln = PathBuf::from(require_str_param!(req, "kiln"));

    // Optional pagination params
    let limit = optional_u64_param!(req, "limit").map(|n| n as usize);
    let offset = optional_u64_param!(req, "offset").map(|n| n as usize);

    // Resume session from storage
    let session = match sm.resume_session_from_storage(session_id, &kiln).await {
        Ok(s) => s,
        Err(e) => return invalid_state_error(req.id, "resume_from_storage", e),
    };

    // Load event history with pagination
    let history = match sm
        .load_session_events(session_id, &kiln, limit, offset)
        .await
    {
        Ok(events) => events,
        Err(e) => {
            // Session resumed but history load failed - return session without history
            // Log internally but don't expose error details to client
            warn!("Failed to load session history: {}", e);
            return Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session.id,
                    "type": session.session_type.as_prefix(),
                    "state": format!("{}", session.state),
                    "kiln": session.kiln,
                    "history": [],
                    "total_events": 0,
                }),
            );
        }
    };

    // Get total event count for pagination
    let total = sm
        .count_session_events(session_id, &kiln)
        .await
        .unwrap_or(0);

    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session.id,
            "type": session.session_type.as_prefix(),
            "state": format!("{}", session.state),
            "kiln": session.kiln,
            "history": history,
            "total_events": total,
        }),
    )
}

async fn handle_session_end(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_str_param!(req, "session_id");

    match sm.end_session(session_id).await {
        Ok(session) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session.id,
                "state": "ended",
                "kiln": session.kiln,
            }),
        ),
        Err(e) => invalid_state_error(req.id, "end", e),
    }
}

async fn handle_session_compact(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_str_param!(req, "session_id");

    match sm.request_compaction(session_id).await {
        Ok(session) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session.id,
                "state": format!("{}", session.state),
                "compaction_requested": true,
            }),
        ),
        Err(e) => invalid_state_error(req.id, "compact", e),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Subscription RPC handlers
// ─────────────────────────────────────────────────────────────────────────────

async fn handle_session_subscribe(
    req: Request,
    client_id: ClientId,
    sm: &Arc<SubscriptionManager>,
) -> Response {
    let session_ids_arr = require_array_param!(req, "session_ids");
    let session_ids: Vec<String> = session_ids_arr
        .iter()
        .filter_map(|v: &serde_json::Value| v.as_str().map(String::from))
        .collect();

    for session_id in &session_ids {
        if session_id == "*" {
            sm.subscribe_all(client_id);
        } else {
            sm.subscribe(client_id, session_id);
        }
    }

    Response::success(
        req.id,
        serde_json::json!({
            "subscribed": session_ids,
            "client_id": client_id.as_u64(),
        }),
    )
}

async fn handle_session_unsubscribe(
    req: Request,
    client_id: ClientId,
    sm: &Arc<SubscriptionManager>,
) -> Response {
    let session_ids_arr = require_array_param!(req, "session_ids");
    let session_ids: Vec<String> = session_ids_arr
        .iter()
        .filter_map(|v: &serde_json::Value| v.as_str().map(String::from))
        .collect();

    for session_id in &session_ids {
        sm.unsubscribe(client_id, session_id);
    }

    Response::success(
        req.id,
        serde_json::json!({
            "unsubscribed": session_ids,
        }),
    )
}

async fn handle_session_configure_agent(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_str_param!(req, "session_id");

    let agent_json = match req.params.get("agent") {
        Some(v) => v.clone(),
        None => {
            return Response::error(req.id, INVALID_PARAMS, "Missing 'agent' parameter");
        }
    };

    let agent: crucible_core::session::SessionAgent = match serde_json::from_value(agent_json) {
        Ok(a) => a,
        Err(e) => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!("Invalid agent config: {}", e),
            );
        }
    };

    match am.configure_agent(session_id, agent).await {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "configured": true,
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_send_message(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_str_param!(req, "session_id");
    let content = require_str_param!(req, "content");

    match am
        .send_message(session_id, content.to_string(), event_tx)
        .await
    {
        Ok(message_id) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "message_id": message_id,
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_cancel(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_str_param!(req, "session_id");

    let cancelled = am.cancel(session_id).await;
    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "cancelled": cancelled,
        }),
    )
}

async fn handle_session_switch_model(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_str_param!(req, "session_id");
    let model_id = require_str_param!(req, "model_id");

    match am.switch_model(session_id, model_id, Some(event_tx)).await {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "model_id": model_id,
                "switched": true,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            Response::error(req.id, INVALID_PARAMS, format!("Session not found: {}", id))
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => Response::error(
            req.id,
            INVALID_PARAMS,
            format!("No agent configured for session: {}", id),
        ),
        Err(crate::agent_manager::AgentError::ConcurrentRequest(id)) => Response::error(
            req.id,
            INVALID_PARAMS,
            format!(
                "Cannot switch model while request is in progress for session: {}",
                id
            ),
        ),
        Err(crate::agent_manager::AgentError::InvalidModelId(msg)) => {
            Response::error(req.id, INVALID_PARAMS, msg)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_list_models(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_str_param!(req, "session_id");

    match am.list_models(session_id).await {
        Ok(models) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "models": models,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_set_thinking_budget(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_str_param!(req, "session_id");
    let budget = require_i64_param!(req, "budget");

    match am.set_thinking_budget(session_id, budget, Some(event_tx)).await {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "thinking_budget": budget,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::ConcurrentRequest(id)) => {
            concurrent_request(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

async fn handle_session_get_thinking_budget(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_str_param!(req, "session_id");

    match am.get_thinking_budget(session_id) {
        Ok(budget) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "thinking_budget": budget,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
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
        let server_task = tokio::spawn(async move { server.run().await });

        // Give server time to start
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Connect and send ping
        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("\"result\":\"pong\""));
        assert!(response.contains("\"id\":1"));

        // Shutdown
        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_kiln_open_missing_path_param() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        // Missing "path" parameter
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"kiln.open\",\"params\":{}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("error"));
        assert!(response.contains("-32602")); // INVALID_PARAMS

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_kiln_close_missing_path_param() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        // Missing "path" parameter
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"kiln.close\",\"params\":{}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("error"));
        assert!(response.contains("-32602")); // INVALID_PARAMS

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_kiln_list_returns_array() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"kiln.list\",\"params\":{}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("\"result\":[]")); // Empty array initially

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_method_not_found() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(
                b"{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"unknown.method\",\"params\":{}}\n",
            )
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("error"));
        assert!(response.contains("-32601")); // METHOD_NOT_FOUND

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_parse_error() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        // Invalid JSON
        client.write_all(b"{invalid json}\n").await.unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("error"));
        assert!(response.contains("-32700")); // PARSE_ERROR

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_shutdown_method() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"shutdown\",\"params\":{}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("\"result\":\"shutting down\""));

        // Server should shut down gracefully
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), server_task).await;

        assert!(result.is_ok(), "Server should shutdown within timeout");
    }

    #[tokio::test]
    async fn test_kiln_open_nonexistent_path_fails() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        // Valid request format, but path doesn't exist
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"kiln.open\",\"params\":{\"path\":\"/nonexistent/path/to/kiln\"}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("error"));
        assert!(response.contains("-32603")); // INTERNAL_ERROR (can't open DB)

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_client_disconnect_closes_connection() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(async move { server.run().await });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Connect and immediately disconnect
        {
            let _client = UnixStream::connect(&sock_path).await.unwrap();
            // Client drops here, closing connection
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Server should still be running and accept new connections
        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("\"result\":\"pong\""));

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_server_has_event_broadcast() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let event_tx = server.event_sender();

        // Subscribe a receiver so send() succeeds
        let mut rx = event_tx.subscribe();

        // Should be able to send events
        let event = SessionEventMessage::text_delta("test-session", "hello");
        assert!(event_tx.send(event).is_ok());

        // Verify the event was received
        let received = rx.recv().await.unwrap();
        assert_eq!(received.session_id, "test-session");
        assert_eq!(received.event, "text_delta");
    }

    #[tokio::test]
    async fn test_session_subscribe_rpc() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"chat-test\"]}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("\"subscribed\""));
        assert!(response.contains("chat-test"));
        assert!(response.contains("\"client_id\""));

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_subscribe_multiple_sessions() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"session-1\",\"session-2\",\"session-3\"]}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("\"subscribed\""));
        assert!(response.contains("session-1"));
        assert!(response.contains("session-2"));
        assert!(response.contains("session-3"));

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_subscribe_wildcard() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"*\"]}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("\"subscribed\""));
        assert!(response.contains("\"*\""));

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_subscribe_missing_session_ids() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(
                b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{}}\n",
            )
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("error"));
        assert!(response.contains("-32602")); // INVALID_PARAMS
        assert!(response.contains("'session_ids'"));

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_subscribe_invalid_session_ids_type() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        // session_ids is a string, not an array
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":\"not-an-array\"}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("error"));
        assert!(response.contains("-32602")); // INVALID_PARAMS
                                              // Macro produces "Missing or invalid 'session_ids' parameter" for wrong type
        assert!(response.contains("'session_ids'"));

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_unsubscribe_rpc() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();

        // First subscribe
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"chat-test\"]}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let _ = client.read(&mut buf).await.unwrap();

        // Then unsubscribe
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"session.unsubscribe\",\"params\":{\"session_ids\":[\"chat-test\"]}}\n")
            .await
            .unwrap();

        buf.fill(0);
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("\"unsubscribed\""));
        assert!(response.contains("chat-test"));

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_session_unsubscribe_missing_session_ids() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.unsubscribe\",\"params\":{}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);

        assert!(response.contains("error"));
        assert!(response.contains("-32602")); // INVALID_PARAMS
        assert!(response.contains("'session_ids'"));

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_event_broadcast_to_subscriber() {
        use std::time::Duration;

        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let event_tx = server.event_sender();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();

        // Subscribe to a session
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"chat-test\"]}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 4096];
        let _ = client.read(&mut buf).await.unwrap(); // consume subscription response

        // Send event through broadcast channel
        let event = SessionEventMessage::text_delta("chat-test", "hello world");
        event_tx.send(event).unwrap();

        // Client should receive the event
        tokio::time::sleep(Duration::from_millis(100)).await;

        buf.fill(0);
        let n = tokio::time::timeout(Duration::from_millis(500), client.read(&mut buf))
            .await
            .expect("timeout waiting for event")
            .unwrap();

        let received = String::from_utf8_lossy(&buf[..n]);
        assert!(
            received.contains("\"type\":\"event\""),
            "Response: {}",
            received
        );
        assert!(
            received.contains("\"session_id\":\"chat-test\""),
            "Response: {}",
            received
        );
        assert!(received.contains("hello world"), "Response: {}", received);

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_event_not_sent_to_non_subscriber() {
        use std::time::Duration;

        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");

        let server = Server::bind(&sock_path).await.unwrap();
        let event_tx = server.event_sender();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();

        // Subscribe to session "other-session"
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.subscribe\",\"params\":{\"session_ids\":[\"other-session\"]}}\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 4096];
        let _ = client.read(&mut buf).await.unwrap(); // consume subscription response

        // Send event for "chat-test" (different session)
        let event = SessionEventMessage::text_delta("chat-test", "should not receive");
        event_tx.send(event).unwrap();

        // Client should NOT receive the event (timeout expected)
        tokio::time::sleep(Duration::from_millis(50)).await;

        buf.fill(0);
        let result = tokio::time::timeout(Duration::from_millis(100), client.read(&mut buf)).await;
        assert!(
            result.is_err(),
            "Should timeout - client shouldn't receive unsubscribed events"
        );

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_events_auto_persisted() {
        use std::time::Duration;

        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");
        let kiln_path = tmp.path().join("kiln");
        std::fs::create_dir_all(&kiln_path).unwrap();

        let server = Server::bind(&sock_path).await.unwrap();
        let event_tx = server.event_sender();
        let shutdown_handle = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock_path).await.unwrap();

        // Create a session
        let create_req = format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"session.create","params":{{"type":"chat","kiln":"{}"}}}}"#,
            kiln_path.display()
        );
        client.write_all(create_req.as_bytes()).await.unwrap();
        client.write_all(b"\n").await.unwrap();

        let mut buf = vec![0u8; 4096];
        let n = client.read(&mut buf).await.unwrap();
        let response: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();
        let session_id = response["result"]["session_id"]
            .as_str()
            .unwrap()
            .to_string();

        // Send event through broadcast channel
        let event = SessionEventMessage::text_delta(&session_id, "hello world");
        event_tx.send(event).unwrap();

        // Wait for persistence
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Check that event was persisted
        let session_dir = kiln_path
            .join(".crucible")
            .join("sessions")
            .join(&session_id);
        let jsonl_path = session_dir.join("session.jsonl");

        let content = tokio::fs::read_to_string(&jsonl_path).await.unwrap();
        assert!(content.contains("hello world"));
        assert!(content.contains("text_delta"));

        let _ = shutdown_handle.send(());
        let _ = server_task.await;
    }
}
