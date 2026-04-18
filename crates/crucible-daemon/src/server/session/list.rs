use super::super::*;
use crate::{optional_param, require_param};

use crucible_core::session::{SessionState, SessionSummary, SessionType};

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

        // Helper: fetch sessions for a kiln and dedup into the accumulator
        let mut collect_from = |sessions: Vec<SessionSummary>| {
            for session in sessions {
                if seen_ids.insert(session.id.clone()) {
                    all_sessions.push(session);
                }
            }
        };

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
            collect_from(filtered);
        }

        // Also load from crucible home if not already included
        let home = crucible_config::crucible_home();
        if !kilns.iter().any(|(k, _, _)| k == &home) {
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
            collect_from(home_sessions);
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
