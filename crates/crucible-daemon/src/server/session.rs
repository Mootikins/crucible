use super::*;

use crucible_core::session::{SessionState, SessionType};

pub(crate) async fn handle_session_create(
    req: Request,
    sm: &Arc<SessionManager>,
    pm: &Arc<ProjectManager>,
    llm_config: &Option<LlmConfig>,
    km: &Arc<KilnManager>,
) -> Response {
    let session_type_str = optional_param!(req, "type", as_str).unwrap_or("chat");
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

    let kiln = optional_param!(req, "kiln", as_str)
        .map(PathBuf::from)
        .unwrap_or_else(crucible_config::crucible_home);

    let workspace = optional_param!(req, "workspace", as_str).map(PathBuf::from);

    let provider_trust_level = resolve_provider_trust_level_for_create(&req, llm_config);
    let classification = resolve_kiln_classification_for_create(&kiln, workspace.as_ref());
    if let Some(classification) = classification {
        if let Err(message) = validate_trust_level(provider_trust_level, classification) {
            return Response::error(req.id, INVALID_PARAMS, message);
        }
    }

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

    let recording_mode = optional_param!(req, "recording_mode", as_str)
        .and_then(|s| s.parse::<RecordingMode>().ok());
    let custom_recording_path = optional_param!(req, "recording_path", as_str).map(PathBuf::from);

    let project_path = workspace.as_ref().unwrap_or(&kiln);
    if let Err(e) = pm.register_if_missing(project_path) {
        tracing::warn!(path = %project_path.display(), error = %e, "Failed to auto-register project");
    }

    match sm
        .create_session(
            session_type,
            kiln,
            workspace,
            connected_kilns,
            recording_mode,
        )
        .await
    {
        Ok(session) => {
            // Open the kiln in KilnManager so it's discoverable by session.list()
            if let Err(e) = km.open(&session.kiln).await {
                tracing::warn!(kiln = %session.kiln.display(), error = %e, "Failed to open kiln in manager");
            }

            if session.recording_mode == Some(RecordingMode::Granular) {
                let recording_path = match custom_recording_path {
                    Some(ref p) => p.clone(),
                    None => {
                        let session_dir = FileSessionStorage::session_dir_for(&session);
                        session_dir.join("recording.jsonl")
                    }
                };
                let (writer, tx) = RecordingWriter::new(
                    recording_path,
                    session.id.clone(),
                    RecordingMode::Granular,
                    None,
                );
                sm.set_recording_sender(&session.id, tx);
                let _handle = writer.start();
            }

            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session.id,
                    "type": session.session_type.as_prefix(),
                    "kiln": session.kiln,
                    "workspace": session.workspace,
                    "state": format!("{}", session.state),
                }),
            )
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(super) fn validate_trust_level(
    provider_trust_level: TrustLevel,
    classification: DataClassification,
) -> Result<(), String> {
    if provider_trust_level.satisfies(classification) {
        return Ok(());
    }

    Err(format!(
        "Provider trust level '{}' is insufficient for kiln data classification '{}'. Requires '{}' trust or higher.",
        provider_trust_level,
        classification,
        classification.required_trust_level()
    ))
}

pub(super) fn resolve_provider_trust_level_for_create(
    req: &Request,
    llm_config: &Option<LlmConfig>,
) -> TrustLevel {
    if optional_param!(req, "agent_type", as_str) == Some("acp") {
        return TrustLevel::Cloud;
    }

    if let Some(provider_key) = optional_param!(req, "provider_key", as_str) {
        if let Some(config) = llm_config
            .as_ref()
            .and_then(|cfg| cfg.get_provider(provider_key))
        {
            return config.effective_trust_level();
        }
    }

    if let Some(provider_name) = optional_param!(req, "provider", as_str) {
        if let Ok(backend) = provider_name.parse::<crucible_config::BackendType>() {
            return backend.default_trust_level();
        }
    }

    llm_config
        .as_ref()
        .and_then(LlmConfig::default_provider)
        .map(|(_, provider)| provider.effective_trust_level())
        .unwrap_or(TrustLevel::Cloud)
}

pub(super) fn resolve_kiln_classification_for_create(
    kiln: &Path,
    workspace: Option<&PathBuf>,
) -> Option<DataClassification> {
    let workspace_path = workspace.cloned().unwrap_or_else(|| kiln.to_path_buf());
    crate::trust_resolution::resolve_kiln_classification(&workspace_path, kiln)
}

pub(crate) async fn handle_session_list(
    req: Request,
    sm: &Arc<SessionManager>,
    km: &Arc<KilnManager>,
) -> Response {
    // Parse optional filters
    let kiln = optional_param!(req, "kiln", as_str).map(PathBuf::from);
    let workspace = optional_param!(req, "workspace", as_str).map(PathBuf::from);
    let session_type = optional_param!(req, "type", as_str).and_then(|s| match s {
        "chat" => Some(SessionType::Chat),
        "agent" => Some(SessionType::Agent),
        "workflow" => Some(SessionType::Workflow),
        _ => None,
    });
    let state = optional_param!(req, "state", as_str).and_then(|s| match s {
        "active" => Some(SessionState::Active),
        "paused" => Some(SessionState::Paused),
        "compacting" => Some(SessionState::Compacting),
        "ended" => Some(SessionState::Ended),
        _ => None,
    });
    let include_archived = optional_param!(req, "include_archived", as_bool).unwrap_or(false);

    let sessions = if kiln.is_none() {
        // When no kiln is specified, load sessions from all open kilns + crucible home
        let mut all_sessions = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        // First, get sessions from all open kilns
        let kilns = km.list().await;
        for (kiln_path, _, _) in &kilns {
            let filtered = sm
                .list_sessions_filtered_async(
                    Some(kiln_path),
                    workspace.as_ref(),
                    session_type,
                    state,
                    include_archived,
                )
                .await;
            for session in filtered {
                if !seen_ids.contains(&session.id) {
                    seen_ids.insert(session.id.clone());
                    all_sessions.push(session);
                }
            }
        }

        // Also load from crucible home if not already included
        let home = crucible_config::crucible_home();
        if !kilns.iter().any(|(k, _, _)| k == &home) {
            // Try to open crucible home if not already open
            let _ = km.open(&home).await;
            let home_sessions = sm
                .list_sessions_filtered_async(
                    Some(&home),
                    workspace.as_ref(),
                    session_type,
                    state,
                    include_archived,
                )
                .await;
            for session in home_sessions {
                if !seen_ids.contains(&session.id) {
                    seen_ids.insert(session.id.clone());
                    all_sessions.push(session);
                }
            }
        }

        all_sessions
    } else {
        sm.list_sessions_filtered_async(
            kiln.as_ref(),
            workspace.as_ref(),
            session_type,
            state,
            include_archived,
        )
        .await
    };

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

pub(crate) async fn handle_session_search(req: Request, sm: &Arc<SessionManager>) -> Response {
    let query = require_param!(req, "query", as_str);
    let kiln = optional_param!(req, "kiln", as_str).map(PathBuf::from);
    let limit = optional_param!(req, "limit", as_u64).unwrap_or(20) as usize;

    // Determine sessions directory
    let sessions_path = if let Some(kiln_path) = kiln {
        kiln_path.join(".crucible").join("sessions")
    } else {
        return Response::success(
            req.id,
            serde_json::json!({
                "matches": [],
                "total": 0,
                "note": "Specify 'kiln' parameter to search sessions"
            }),
        );
    };

    if !sessions_path.exists() {
        return Response::success(
            req.id,
            serde_json::json!({
                "matches": [],
                "total": 0
            }),
        );
    }

    let query_lower = query.to_lowercase();
    let mut matches = Vec::new();

    let read_dir = match tokio::fs::read_dir(&sessions_path).await {
        Ok(rd) => rd,
        Err(e) => {
            return internal_error(
                req.id,
                anyhow::anyhow!("Failed to read sessions dir: {}", e),
            )
        }
    };

    let mut rd = read_dir;
    while let Ok(Some(entry)) = rd.next_entry().await {
        if matches.len() >= limit {
            break;
        }
        let session_dir = entry.path();
        if !session_dir.is_dir() {
            continue;
        }
        let session_id = session_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let jsonl_path = session_dir.join("session.jsonl");
        if !jsonl_path.exists() {
            continue;
        }
        let content = match tokio::fs::read_to_string(&jsonl_path).await {
            Ok(c) => c,
            Err(_) => continue,
        };
        for (line_num, line) in content.lines().enumerate() {
            if line.to_lowercase().contains(&query_lower) {
                let truncated = if line.len() > 100 {
                    // Use floor_char_boundary to avoid panicking on multi-byte UTF-8
                    let end = line.floor_char_boundary(100);
                    format!("{}...", &line[..end])
                } else {
                    line.to_string()
                };
                matches.push(serde_json::json!({
                    "session_id": session_id,
                    "line": line_num + 1,
                    "context": truncated
                }));
                break;
            }
        }
    }

    // Also include active sessions matching by title
    let active_sessions = sm
        .list_sessions_filtered_async(None, None, None, None, true)
        .await;
    for session in &active_sessions {
        if matches.len() >= limit {
            break;
        }
        if let Some(title) = &session.title {
            if title.to_lowercase().contains(&query_lower)
                && !matches
                    .iter()
                    .any(|m| m["session_id"] == session.id.as_str())
            {
                matches.push(serde_json::json!({
                    "session_id": session.id,
                    "line": 0,
                    "context": format!("[active] {}", title)
                }));
            }
        }
    }

    let total = matches.len();
    Response::success(
        req.id,
        serde_json::json!({
            "matches": matches,
            "total": total
        }),
    )
}

pub(crate) async fn handle_session_get(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match sm.get_session(session_id) {
        Some(session) => {
            let mut response = serde_json::json!({
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
            });

            if let Some(mode) = session.recording_mode {
                response["recording_mode"] = serde_json::json!(format!("{}", mode));
            }

            Response::success(req.id, response)
        }
        None => Response::error(
            req.id,
            INVALID_PARAMS,
            format!("Session not found: {}", session_id),
        ),
    }
}

pub(crate) async fn handle_session_pause(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

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

pub(crate) async fn handle_session_resume(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

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

pub(crate) async fn handle_session_resume_from_storage(
    req: Request,
    sm: &Arc<SessionManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let kiln = PathBuf::from(require_param!(req, "kiln", as_str));

    // Optional pagination params
    let limit = optional_param!(req, "limit", as_u64).map(|n| n as usize);
    let offset = optional_param!(req, "offset", as_u64).map(|n| n as usize);

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

pub(crate) async fn handle_session_end(
    req: Request,
    sm: &Arc<SessionManager>,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match sm.end_session(session_id).await {
        Ok(session) => {
            am.cleanup_session(session_id);
            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session.id,
                    "state": "ended",
                    "kiln": session.kiln,
                }),
            )
        }
        Err(e) => invalid_state_error(req.id, "end", e),
    }
}

pub(crate) async fn handle_session_delete(
    req: Request,
    sm: &Arc<SessionManager>,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let kiln = PathBuf::from(require_param!(req, "kiln", as_str));

    match sm.delete_session(session_id, &kiln).await {
        Ok(()) => {
            am.cleanup_session(session_id);
            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session_id,
                    "deleted": true,
                }),
            )
        }
        Err(e) => invalid_state_error(req.id, "delete", e),
    }
}

pub(crate) async fn handle_session_archive(
    req: Request,
    sm: &Arc<SessionManager>,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let kiln = PathBuf::from(require_param!(req, "kiln", as_str));

    match sm.archive_session(session_id, &kiln).await {
        Ok(session) => {
            am.cleanup_session(session_id);
            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session.id,
                    "archived": session.archived,
                }),
            )
        }
        Err(e) => invalid_state_error(req.id, "archive", e),
    }
}

pub(crate) async fn handle_session_unarchive(
    req: Request,
    sm: &Arc<SessionManager>,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let kiln = PathBuf::from(require_param!(req, "kiln", as_str));

    match sm.unarchive_session(session_id, &kiln).await {
        Ok(session) => {
            am.cleanup_session(session_id);
            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session.id,
                    "archived": session.archived,
                }),
            )
        }
        Err(e) => invalid_state_error(req.id, "unarchive", e),
    }
}

pub(crate) async fn handle_session_replay(
    req: Request,
    sm: &Arc<SessionManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let recording_path = require_param!(req, "recording_path", as_str);
    let speed = req
        .params
        .get("speed")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);

    let recording_path = PathBuf::from(recording_path);
    let replay_session_id = format!("replay-{}", uuid::Uuid::new_v4());

    match ReplaySession::new(
        recording_path,
        speed,
        event_tx.clone(),
        replay_session_id.clone(),
    ) {
        Ok(replay) => {
            sm.register_transient(replay.session().clone());
            let _handle = replay.start();

            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": replay_session_id,
                    "status": "replaying",
                    "speed": speed,
                }),
            )
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_compact(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

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
pub(crate) async fn handle_session_configure_agent(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

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

pub(crate) async fn handle_session_send_message(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let content = require_param!(req, "content", as_str);
    let is_interactive = req
        .params
        .get("is_interactive")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let permission_override = req
        .params
        .get("permission_mode")
        .and_then(|v| v.as_str())
        .and_then(|s| {
            s.parse::<crucible_config::components::permissions::PermissionMode>()
                .ok()
        });

    match am
        .send_message(
            session_id,
            content.to_string(),
            event_tx,
            is_interactive,
            permission_override,
        )
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

pub(crate) async fn handle_session_cancel(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    let cancelled = am.cancel(session_id).await;
    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "cancelled": cancelled,
        }),
    )
}

pub(crate) async fn handle_session_interaction_respond(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let request_id = require_param!(req, "request_id", as_str);
    let response_obj = require_param!(req, "response", as_object);

    let response: crucible_core::interaction::InteractionResponse =
        match serde_json::from_value(serde_json::Value::Object(response_obj.clone())) {
            Ok(r) => r,
            Err(e) => {
                return Response::error(
                    req.id,
                    INVALID_PARAMS,
                    format!("Invalid interaction response: {}", e),
                )
            }
        };

    if let crucible_core::interaction::InteractionResponse::Permission(perm_response) = &response {
        if let Err(e) = am.respond_to_permission(session_id, request_id, perm_response.clone()) {
            tracing::warn!(
                session_id = %session_id,
                request_id = %request_id,
                error = %e,
                "Failed to send permission response to channel (may have timed out)"
            );
        }
    }

    if !emit_event(
        event_tx,
        SessionEventMessage::new(
            session_id,
            "interaction_completed",
            serde_json::json!({
                "request_id": request_id,
                "response": response,
            }),
        ),
    ) {
        tracing::debug!("Failed to emit interaction_completed event (no subscribers)");
    }

    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "request_id": request_id,
        }),
    )
}

pub(crate) async fn handle_session_test_interaction(
    req: Request,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    let get_str = |key: &str| -> Option<&str> { req.params.get(key)?.as_str() };

    let interaction_type = get_str("type").unwrap_or("ask");
    let request_id = format!("test-{}", uuid::Uuid::new_v4());

    let request = match interaction_type {
        "ask" => {
            let question =
                get_str("question").unwrap_or("Test question: Which option do you prefer?");

            // InteractionRequest uses #[serde(tag = "kind")] internally-tagged format
            serde_json::json!({
                "kind": "ask",
                "question": question,
                "choices": ["Option A", "Option B", "Option C"],
                "allow_other": true,
                "multi_select": false
            })
        }
        "permission" => {
            let action = get_str("action").unwrap_or("rm -rf /tmp/test");

            // PermRequest uses externally-tagged format for its inner Bash/Read/Write/Tool
            serde_json::json!({
                "kind": "permission",
                "Bash": {
                    "command": action
                }
            })
        }
        _ => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!(
                    "Unknown interaction type: {}. Use 'ask' or 'permission'",
                    interaction_type
                ),
            )
        }
    };

    if !emit_event(
        event_tx,
        SessionEventMessage::new(
            session_id.to_string(),
            "interaction_requested",
            serde_json::json!({
                "request_id": request_id,
                "request": request,
            }),
        ),
    ) {
        tracing::debug!("Failed to emit interaction_requested event (no subscribers)");
    }

    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "request_id": request_id,
            "type": interaction_type,
        }),
    )
}

pub(crate) async fn handle_session_switch_model(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let model_id = require_param!(req, "model_id", as_str);

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

pub(crate) async fn handle_session_list_models(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    let classification = match am.get_session_with_agent(session_id) {
        Ok((session, _)) => {
            crate::trust_resolution::resolve_kiln_classification(&session.workspace, &session.kiln)
        }
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            return session_not_found(req.id, &id);
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(_)) => {
            return Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session_id,
                    "models": Vec::<String>::new(),
                }),
            );
        }
        Err(e) => return internal_error(req.id, e),
    };

    match am.list_models(session_id, classification).await {
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
        Err(crate::agent_manager::AgentError::NoAgentConfigured(_)) => {
            // Return empty models list if no agent is configured
            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session_id,
                    "models": Vec::<String>::new(),
                }),
            )
        }
        Err(e) => internal_error(req.id, e),
    }
}

/// List all available models without requiring an active session.
///
/// Accepts an optional `kiln_path` parameter. When provided, the handler
pub(crate) async fn handle_models_list(req: Request, am: &Arc<AgentManager>) -> Response {
    let kiln_path = req
        .params
        .get("kiln_path")
        .and_then(|v| v.as_str())
        .map(PathBuf::from);

    let classification = kiln_path
        .as_ref()
        .and_then(|kiln| crate::trust_resolution::find_workspace_and_resolve_classification(kiln));

    match am.list_models("", classification).await {
        Ok(models) => Response::success(req.id, serde_json::json!({ "models": models })),
        Err(crate::agent_manager::AgentError::SessionNotFound(_)) => {
            // No session fallback path hit — return empty list
            Response::success(req.id, serde_json::json!({ "models": [] }))
        }
        Err(e) => internal_error(req.id, e),
    }
}

/// List all available providers without requiring an active session.
pub(crate) async fn handle_providers_list(req: Request, am: &Arc<AgentManager>) -> Response {
    let kiln_path = req
        .params
        .get("kiln_path")
        .and_then(|v| v.as_str())
        .map(PathBuf::from);

    let classification = kiln_path
        .as_ref()
        .and_then(|kiln| crate::trust_resolution::find_workspace_and_resolve_classification(kiln));

    let providers = am.list_providers(classification).await;
    Response::success(req.id, serde_json::json!({ "providers": providers }))
}

pub(crate) async fn handle_session_set_thinking_budget(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let budget = optional_param!(req, "thinking_budget", as_i64);

    // When budget is None, clear the thinking budget override
    let effective_budget = budget.unwrap_or(0);

    match am
        .set_thinking_budget(session_id, effective_budget, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "thinking_budget": budget,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_thinking_budget(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

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

pub(crate) async fn handle_session_set_system_prompt(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let prompt = require_param!(req, "system_prompt", as_str);

    match am
        .set_system_prompt(session_id, prompt, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "system_prompt": prompt,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_system_prompt(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_system_prompt(session_id) {
        Ok(prompt) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "system_prompt": prompt,
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

pub(crate) async fn handle_session_set_precognition(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let enabled = optional_param!(req, "enabled", as_bool).unwrap_or(true);

    match am
        .set_precognition(session_id, enabled, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "precognition_enabled": enabled,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_precognition(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_precognition(session_id) {
        Ok(enabled) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "precognition_enabled": enabled,
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

pub(crate) async fn handle_session_add_notification(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let notification_obj = require_param!(req, "notification", as_object);

    let notification = match serde_json::from_value::<crucible_core::types::Notification>(
        serde_json::Value::Object(notification_obj.clone()),
    ) {
        Ok(n) => n,
        Err(e) => return Response::error(req.id, -32602, format!("Invalid notification: {}", e)),
    };

    match am
        .add_notification(session_id, notification, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "success": true,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_list_notifications(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.list_notifications(session_id).await {
        Ok(notifications) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "notifications": notifications,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_dismiss_notification(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let notification_id = require_param!(req, "notification_id", as_str);

    match am
        .dismiss_notification(session_id, notification_id, Some(event_tx))
        .await
    {
        Ok(success) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "notification_id": notification_id,
                "success": success,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_set_temperature(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let temperature = require_param!(req, "temperature", as_f64);

    match am
        .set_temperature(session_id, temperature, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "temperature": temperature,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_temperature(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_temperature(session_id) {
        Ok(temperature) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "temperature": temperature,
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

pub(crate) async fn handle_session_set_max_tokens(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    // max_tokens can be null to clear the limit, so we use optional
    let max_tokens = optional_param!(req, "max_tokens", as_u64).map(|v| v as u32);

    match am
        .set_max_tokens(session_id, max_tokens, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "max_tokens": max_tokens,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_max_tokens(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_max_tokens(session_id) {
        Ok(max_tokens) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "max_tokens": max_tokens,
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
