use super::super::*;
use crate::{optional_param, require_param};

use crucible_core::session::{SessionState, SessionSummary, SessionType};

pub(crate) async fn handle_session_list(
    req: Request,
    sm: &Arc<SessionManager>,
    km: &Arc<KilnManager>,
    data_home: &std::path::Path,
) -> Response {
    // Parse optional filters
    let kiln = optional_param!(req, "kiln", as_str).map(PathBuf::from);
    let workspace = optional_param!(req, "workspace", as_str).map(PathBuf::from);
    let session_type =
        optional_param!(req, "type", as_str).and_then(|s| s.parse::<SessionType>().ok());
    let state = optional_param!(req, "state", as_str).and_then(|s| match s {
        "active" => Some(SessionState::Active),
        "paused" => Some(SessionState::Paused),
        "compacting" => Some(SessionState::Compacting),
        "ended" => Some(SessionState::Ended),
        _ => None,
    });
    let include_archived = optional_param!(req, "include_archived", as_bool).unwrap_or(false);
    // Delegated child sessions are hidden by default: full sessions in
    // behavior, but not first-class in visibility.
    let include_children = optional_param!(req, "include_children", as_bool).unwrap_or(false);

    let mut sessions = if kiln.is_none() {
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

        // Also load from the daemon data home if not already included.
        //
        // Deliberately without `km.open(&home)`: `server/mod.rs` documents that
        // home is scanned but "never OPEN[ed] as a kiln — that is the leak this
        // split fixes" (`61f28c144`), and an open kiln is a watched kiln, so
        // opening it here indexed every session body under the data root into
        // SQLite and LanceDB. Listing never needed it — the lookup below reads
        // the in-memory map and `storage.list`, not `KilnManager`.
        let home = data_home.to_path_buf();
        if !kilns.iter().any(|(k, _, _)| k == &home) {
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

    if !include_children {
        sessions.retain(|s| s.parent_session_id.is_none());
    }

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
                "last_activity": s.last_activity.map(|t| t.to_rfc3339()),
                "title": s.title,
                "agent_model": s.agent_model,
                "event_count": s.event_count,
                "archived": s.archived,
                "parent_session_id": s.parent_session_id,
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
                "parent_session_id": session.parent_session_id,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kiln_manager::KilnManager;
    use crate::session_manager::SessionManager;
    use tempfile::TempDir;

    /// `server/mod.rs` documents the invariant this handler broke: the title
    /// sweep scans `~/.crucible` "but never OPENS home as a kiln — that is the
    /// leak this split fixes" (commit `61f28c144`). Listing sessions opened it
    /// anyway, and an open kiln is a *watched* kiln, so every session body
    /// under the data root — including a chat integration's transcript of a
    /// stranger's messages — was parsed into SQLite and embedded into LanceDB
    /// permanently.
    ///
    /// The open was never load-bearing: `list_sessions_filtered_async` reads
    /// the in-memory map and `storage.list(kiln_path)`, and never consults
    /// `KilnManager` at all.
    #[tokio::test]
    async fn listing_sessions_never_opens_the_data_root_as_a_kiln() {
        let km = Arc::new(KilnManager::new());
        let sm = Arc::new(SessionManager::new());
        let tmp = TempDir::new().unwrap();
        let data_home = tmp.path().to_path_buf();

        let req: Request = serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "session.list",
            "params": {},
        }))
        .unwrap();

        let _ = handle_session_list(req, &sm, &km, &data_home).await;

        let opened: Vec<_> = km.list().await.into_iter().map(|(p, _, _)| p).collect();
        assert!(
            opened.is_empty(),
            "session.list opened kilns as a side effect: {opened:?}"
        );
    }
}
