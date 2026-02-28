use super::*;

pub(super) async fn handle_session_load_events(req: Request) -> Response {
    let session_dir = require_param!(req, "session_dir", as_str);

    match crucible_observe::load_events(session_dir).await {
        Ok(events) => match serde_json::to_value(&events) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => internal_error(req.id, e),
        },
        Err(e) => internal_error(req.id, e),
    }
}

/// List persisted sessions from a kiln's session directory.
///
/// Params:
///   - `kiln` (string, required): Path to the kiln
///   - `session_type` (string, optional): Filter by type ("chat", "agent", etc.)
///   - `limit` (u64, optional): Max sessions to return (default 50, newest first)
///     Returns: { sessions: [...], total: N }
pub(super) async fn handle_session_list_persisted(req: Request) -> Response {
    let kiln = require_param!(req, "kiln", as_str);
    let session_type_filter = optional_param!(req, "session_type", as_str);
    let limit = optional_param!(req, "limit", as_u64).unwrap_or(50) as usize;

    let sessions_path = FileSessionStorage::sessions_base(Path::new(kiln));

    if !sessions_path.exists() {
        return Response::success(req.id, serde_json::json!({ "sessions": [], "total": 0 }));
    }

    let mut ids = match crucible_observe::list_sessions(&sessions_path).await {
        Ok(ids) => ids,
        Err(e) => return internal_error(req.id, e),
    };

    // Filter by session type if specified
    if let Some(type_filter) = session_type_filter {
        if let Ok(filter_type) = type_filter.parse::<crucible_observe::SessionType>() {
            ids.retain(|id| id.session_type() == filter_type);
        }
    }

    // Newest first, then limit
    ids.reverse();
    ids.truncate(limit);

    let mut session_entries = Vec::new();
    for id in &ids {
        let session_dir = sessions_path.join(id.as_str());
        let events = crucible_observe::load_events(&session_dir)
            .await
            .unwrap_or_default();
        let msg_count = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    crucible_observe::LogEvent::User { .. }
                        | crucible_observe::LogEvent::Assistant { .. }
                )
            })
            .count();

        let title = events
            .iter()
            .find_map(|e| match e {
                crucible_observe::LogEvent::User { content, .. } => {
                    let preview: String = content.chars().take(50).collect();
                    if content.len() > 50 {
                        Some(format!("{}...", preview))
                    } else {
                        Some(preview)
                    }
                }
                _ => None,
            })
            .unwrap_or_else(|| "(empty)".to_string());

        session_entries.push(serde_json::json!({
            "id": id.as_str(),
            "session_type": format!("{}", id.session_type()),
            "message_count": msg_count,
            "title": title,
        }));
    }

    let total = session_entries.len();
    Response::success(
        req.id,
        serde_json::json!({
            "sessions": session_entries,
            "total": total,
        }),
    )
}

/// Render a persisted session's events to markdown.
///
/// Params:
///   - `session_dir` (string, required): Path to the session directory
///   - `include_timestamps` (bool, optional): Include timestamps (default false)
///   - `include_tokens` (bool, optional): Include token stats (default true)
///   - `include_tools` (bool, optional): Include tool details (default true)
///   - `max_content_length` (u64, optional): Truncation limit (default 0 = no limit)
///     Returns: { markdown: "..." }
pub(super) async fn handle_session_render_markdown(req: Request) -> Response {
    let session_dir = require_param!(req, "session_dir", as_str);
    let include_timestamps = optional_param!(req, "include_timestamps", as_bool).unwrap_or(false);
    let include_tokens = optional_param!(req, "include_tokens", as_bool).unwrap_or(true);
    let include_tools = optional_param!(req, "include_tools", as_bool).unwrap_or(true);
    let max_content_length =
        optional_param!(req, "max_content_length", as_u64).unwrap_or(0) as usize;

    let events = match crucible_observe::load_events(session_dir).await {
        Ok(e) => e,
        Err(e) => return internal_error(req.id, e),
    };

    let options = crucible_observe::RenderOptions {
        include_timestamps,
        include_tokens,
        include_tools,
        max_content_length,
    };

    let md = crucible_observe::render_to_markdown(&events, &options);

    Response::success(req.id, serde_json::json!({ "markdown": md }))
}

/// Export a session to a markdown file.
///
/// Params:
///   - `session_dir` (string, required): Path to the session directory
///   - `output_path` (string, optional): Output file path (default: session_dir/session.md)
///   - `include_timestamps` (bool, optional): Include timestamps (default false)
///     Returns: { status: "ok", output_path: "..." }
pub(super) async fn handle_session_export_to_file(req: Request) -> Response {
    let session_dir_str = require_param!(req, "session_dir", as_str);
    let output_path = optional_param!(req, "output_path", as_str);
    let timestamps = optional_param!(req, "include_timestamps", as_bool).unwrap_or(false);

    let session_dir = Path::new(session_dir_str);

    let events = match crucible_observe::load_events(session_dir).await {
        Ok(e) => e,
        Err(e) => return internal_error(req.id, e),
    };

    let options = crucible_observe::RenderOptions {
        include_timestamps: timestamps,
        ..Default::default()
    };

    let md = crucible_observe::render_to_markdown(&events, &options);

    let out_path = match output_path {
        Some(p) => PathBuf::from(p),
        None => session_dir.join("session.md"),
    };

    if let Err(e) = tokio::fs::write(&out_path, &md).await {
        return internal_error(req.id, e);
    }

    Response::success(
        req.id,
        serde_json::json!({
            "status": "ok",
            "output_path": out_path.to_string_lossy(),
        }),
    )
}

/// Clean up old persisted sessions.
///
/// Params:
///   - `kiln` (string, required): Path to the kiln
///   - `older_than_days` (u64, required): Delete sessions older than N days
///   - `dry_run` (bool, optional): If true, just report what would be deleted (default false)
///     Returns: { deleted: [...], total: N, dry_run: bool }
pub(super) async fn handle_session_cleanup(req: Request) -> Response {
    let kiln = require_param!(req, "kiln", as_str);
    let older_than_days = require_param!(req, "older_than_days", as_u64);
    let dry_run = optional_param!(req, "dry_run", as_bool).unwrap_or(false);

    let sessions_path = FileSessionStorage::sessions_base(Path::new(kiln));

    if !sessions_path.exists() {
        return Response::success(
            req.id,
            serde_json::json!({ "deleted": [], "total": 0, "dry_run": dry_run }),
        );
    }

    let ids = match crucible_observe::list_sessions(&sessions_path).await {
        Ok(ids) => ids,
        Err(e) => return internal_error(req.id, e),
    };

    let cutoff = chrono::Utc::now() - chrono::Duration::days(older_than_days as i64);

    let mut to_delete = Vec::new();

    for id in ids {
        let session_dir = sessions_path.join(id.as_str());
        let events = crucible_observe::load_events(&session_dir)
            .await
            .unwrap_or_default();

        let latest = events.iter().map(|e| e.timestamp()).max();
        if let Some(ts) = latest {
            if ts < cutoff {
                to_delete.push((id, session_dir));
            }
        }
    }

    let mut deleted_ids = Vec::new();
    if !dry_run {
        for (id, dir) in &to_delete {
            if let Err(e) = tokio::fs::remove_dir_all(dir).await {
                warn!(
                    session_id = %id,
                    error = %e,
                    "Failed to delete session directory"
                );
            } else {
                deleted_ids.push(id.as_str().to_string());
            }
        }
    } else {
        deleted_ids = to_delete
            .iter()
            .map(|(id, _)| id.as_str().to_string())
            .collect();
    }

    let total = deleted_ids.len();
    Response::success(
        req.id,
        serde_json::json!({
            "deleted": deleted_ids,
            "total": total,
            "dry_run": dry_run,
        }),
    )
}

/// Reindex persisted sessions into the kiln's NoteStore.
///
/// Params:
///   - `kiln` (string, required): Path to the kiln
///   - `force` (bool, optional): Re-index even if already present (default false)
///     Returns: { indexed: N, skipped: N, errors: N }
pub(super) async fn handle_session_reindex(req: Request, km: &Arc<KilnManager>) -> Response {
    let kiln_str = require_param!(req, "kiln", as_str);
    let force = optional_param!(req, "force", as_bool).unwrap_or(false);

    let kiln_path = Path::new(kiln_str);
    let sessions_path = FileSessionStorage::sessions_base(kiln_path);

    if !sessions_path.exists() {
        return Response::success(
            req.id,
            serde_json::json!({ "indexed": 0, "skipped": 0, "errors": 0 }),
        );
    }

    let ids = match crucible_observe::list_sessions(&sessions_path).await {
        Ok(ids) => ids,
        Err(e) => return internal_error(req.id, e),
    };

    let handle = match km.get_or_open(kiln_path).await {
        Ok(h) => h,
        Err(e) => return internal_error(req.id, e),
    };

    let note_store = handle.as_note_store();

    let mut indexed = 0u32;
    let mut skipped = 0u32;
    let mut errors = 0u32;

    for id in &ids {
        let session_dir = sessions_path.join(id.as_str());
        let path = format!("sessions/{}", id.as_str());

        if !force {
            match note_store.get(&path).await {
                Ok(Some(_)) => {
                    skipped += 1;
                    continue;
                }
                Ok(None) => {}
                Err(_) => {}
            }
        }

        let events = match crucible_observe::load_events(&session_dir).await {
            Ok(e) => e,
            Err(_) => {
                errors += 1;
                continue;
            }
        };

        let content = match crucible_observe::extract_session_content(id.as_str(), &events) {
            Some(c) => c,
            None => {
                skipped += 1;
                continue;
            }
        };

        let record = content.to_note_record(None);
        if note_store.upsert(record).await.is_err() {
            errors += 1;
            continue;
        }

        indexed += 1;
    }

    Response::success(
        req.id,
        serde_json::json!({
            "indexed": indexed,
            "skipped": skipped,
            "errors": errors,
        }),
    )
}

// =============================================================================
// MCP Server RPC Handlers
// =============================================================================
