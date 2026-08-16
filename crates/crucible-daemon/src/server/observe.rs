use super::*;
use crate::require_session_id;
use crate::server::session::scope::caller_kiln_scope;
use crate::session_manager::{KilnFilter, KilnScope};
use crucible_core::session::SessionSummary;

/// Load a persisted session's events.
///
/// Params:
///   - `session_id` (string, required): The session id.
///
/// Keyed on the id rather than a directory: sessions live under the daemon's
/// own root now, so only the daemon can spell the path, and a client that
/// guessed one was guessing the layout.
pub(crate) async fn handle_session_load_events(req: Request, sessions_root: &Path) -> Response {
    let session_id = require_session_id!(req);
    let session_dir = session_id.dir_under(sessions_root);

    match crate::observe::load_events(&session_dir).await {
        Ok(events) => match serde_json::to_value(&events) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => internal_error(req.id, e),
        },
        Err(e) => internal_error(req.id, e),
    }
}

/// The sessions within a caller's reach, newest first.
///
/// `scope` is `None` for the deliberately unbounded sweep; otherwise a session
/// qualifies iff its kiln set overlaps the caller's — [`KilnScope::overlaps`],
/// the one predicate `session.search` and `session.list` also answer to.
/// `KilnFilter` cannot express it (it tests one kiln at a time), so the listing
/// is unfiltered and the overlap is applied here.
///
/// Two things ride on going through the [`SessionManager`] rather than reading
/// the sessions root directly:
///
/// 1. **Scope.** The flat root holds every session on the box, so kiln-set
///    overlap is what bounds a handler now that the per-kiln directory does
///    not.
/// 2. **It sees the sessions that exist.** `observe::list_sessions` keeps only
///    directory names `SessionId::parse` accepts (`chat-20260104-1530-a1b2`),
///    while `Session::new` mints ids as `chat-2026-01-04T1530-a1b2c3`. The two
///    schemes have never agreed, so a directory walk filtered through
///    `SessionId` matches nothing a running daemon writes.
async fn scoped_sessions(sm: &SessionManager, scope: Option<&KilnScope>) -> Vec<SessionSummary> {
    let mut sessions = sm
        .list_sessions_filtered_async(KilnFilter::Any, None, None, None, true)
        .await;
    if let Some(scope) = scope {
        sessions.retain(|s| scope.overlaps(&s.kilns));
    }
    sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at));
    sessions
}

/// List persisted sessions.
///
/// Params:
///   - `kilns` (array of strings, optional): The **caller's whole kiln set**,
///     not directories to scan
///   - `kiln` (string, optional): the single-member spelling of `kilns`
///   - `session_type` (string, optional): Filter by type ("chat", "agent", etc.)
///   - `limit` (u64, optional): Max sessions to return (default 50, newest first)
///     Returns: { sessions: [...], total: N }
///
/// `title` here is not metadata — it is the first 50 characters of the
/// session's first user message, built below. Listing the whole root would
/// therefore hand back the opening line of every conversation on the box, so
/// the result is scoped to sessions whose kiln set overlaps the caller's.
/// With no kilns there is no scope at all, and the answer is empty rather than
/// everything — the same fail-closed shape `session.search` uses until the
/// Phase 2 privacy predicate lands.
pub(crate) async fn handle_session_list_persisted(
    req: Request,
    sm: &Arc<SessionManager>,
) -> Response {
    let session_type_filter = optional_param!(req, "session_type", as_str)
        .and_then(|t| t.parse::<crate::observe::SessionType>().ok());
    let limit = optional_param!(req, "limit", as_u64).unwrap_or(50) as usize;

    let scope = caller_kiln_scope(&req);
    if scope.is_empty() {
        return Response::success(
            req.id,
            serde_json::json!({
                "sessions": [],
                "total": 0,
                "note": "Specify 'kilns' to scope the listing to sessions that share one"
            }),
        );
    }

    let mut sessions = scoped_sessions(sm, Some(&scope)).await;
    if let Some(filter_type) = session_type_filter {
        sessions.retain(|s| s.session_type == filter_type);
    }
    sessions.truncate(limit);

    let mut session_entries = Vec::new();
    for summary in &sessions {
        let session_dir = sm.session_dir(&summary.id);
        let events = crate::observe::load_events(&session_dir)
            .await
            .unwrap_or_default();
        let msg_count = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    crate::observe::LogEvent::User { .. }
                        | crate::observe::LogEvent::Assistant { .. }
                )
            })
            .count();

        let title = events
            .iter()
            .find_map(|e| match e {
                crate::observe::LogEvent::User { content, .. } => {
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
            "id": summary.id,
            "session_type": format!("{}", summary.session_type),
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
///   - `session_id` (string, required): The session id.
///   - `include_timestamps` (bool, optional): Include timestamps (default false)
///   - `include_tokens` (bool, optional): Include token stats (default true)
///   - `include_tools` (bool, optional): Include tool details (default true)
///   - `max_content_length` (u64, optional): Truncation limit (default 0 = no limit)
///     Returns: { markdown: "..." }
pub(crate) async fn handle_session_render_markdown(req: Request, sessions_root: &Path) -> Response {
    let session_id = require_session_id!(req);
    let session_dir = session_id.dir_under(sessions_root);
    let include_timestamps = optional_param!(req, "include_timestamps", as_bool).unwrap_or(false);
    let include_tokens = optional_param!(req, "include_tokens", as_bool).unwrap_or(true);
    let include_tools = optional_param!(req, "include_tools", as_bool).unwrap_or(true);
    let max_content_length =
        optional_param!(req, "max_content_length", as_u64).unwrap_or(0) as usize;

    let events = match crate::observe::load_events(&session_dir).await {
        Ok(e) => e,
        Err(e) => return internal_error(req.id, e),
    };

    let options = crate::observe::RenderOptions {
        include_timestamps,
        include_tokens,
        include_tools,
        max_content_length,
    };

    let md = crate::observe::render_to_markdown(&events, &options);

    Response::success(req.id, serde_json::json!({ "markdown": md }))
}

/// Export a session to a markdown file.
///
/// Params:
///   - `session_id` (string, required): The session id.
///   - `output_path` (string, optional): Output file path (default: the
///     session's own `session.md`)
///   - `include_timestamps` (bool, optional): Include timestamps (default false)
///     Returns: { status: "ok", output_path: "..." }
pub(crate) async fn handle_session_export_to_file(req: Request, sessions_root: &Path) -> Response {
    let session_id = require_session_id!(req);
    let output_path = optional_param!(req, "output_path", as_str);
    let timestamps = optional_param!(req, "include_timestamps", as_bool).unwrap_or(false);

    let session_dir = session_id.dir_under(sessions_root);

    let events = match crate::observe::load_events(&session_dir).await {
        Ok(e) => e,
        Err(e) => return internal_error(req.id, e),
    };

    let options = crate::observe::RenderOptions {
        include_timestamps: timestamps,
        ..Default::default()
    };

    let md = crate::observe::render_to_markdown(&events, &options);

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
///   - `older_than_days` (u64, required): Delete sessions older than N days
///   - `kilns` (array of strings, optional): The **caller's whole kiln set**.
///     Only sessions whose own set overlaps it are eligible for deletion.
///   - `kiln` (string, optional): the single-member spelling of `kilns`
///   - `all_kilns` (bool, optional): Sweep the whole backlog instead (default false)
///   - `dry_run` (bool, optional): If true, just report what would be deleted (default false)
///     Returns: { deleted: [...], total: N, dry_run: bool, scope: "..." }
///
/// One flat root now holds every session on the box, so an unscoped sweep here
/// is a machine-wide delete with no confirmation and no undo. A scope is
/// therefore mandatory: it bounds the blast radius to sessions that share a
/// kiln with the caller, and widening to the whole backlog has to be asked for
/// by name. `all_kilns` is also the only way a kiln-less session — a legitimate
/// state per §4.1 — is ever collected, since it overlaps nothing.
pub(crate) async fn handle_session_cleanup(req: Request, sm: &Arc<SessionManager>) -> Response {
    let older_than_days = require_param!(req, "older_than_days", as_u64);
    let dry_run = optional_param!(req, "dry_run", as_bool).unwrap_or(false);
    let all_kilns = optional_param!(req, "all_kilns", as_bool).unwrap_or(false);
    let requested = caller_kiln_scope(&req);

    // `all_kilns` wins over a named scope deliberately: it is the more explicit
    // of the two, and a caller that sent both meant the wider one.
    let scope = match (all_kilns, requested.is_empty()) {
        (true, _) => None,
        (false, false) => Some(requested),
        (false, true) => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                "session.cleanup needs a scope: pass 'kilns' to delete only sessions that \
                 share one, or 'all_kilns': true to sweep every session on this machine",
            )
        }
    };

    let scope_label = match &scope {
        Some(scope) => scope
            .kilns()
            .iter()
            .map(|k| k.to_string_lossy())
            .collect::<Vec<_>>()
            .join(", "),
        None => "all_kilns".to_string(),
    };

    if !sm.sessions_root().exists() {
        return Response::success(
            req.id,
            serde_json::json!({
                "deleted": [],
                "total": 0,
                "dry_run": dry_run,
                "scope": scope_label,
            }),
        );
    }

    let candidates = scoped_sessions(sm, scope.as_ref()).await;

    let cutoff = chrono::Utc::now() - chrono::Duration::days(older_than_days as i64);

    let mut to_delete = Vec::new();

    for summary in candidates {
        let session_dir = sm.session_dir(&summary.id);
        let events = crate::observe::load_events(&session_dir)
            .await
            .unwrap_or_default();

        let latest = events.iter().map(|e| e.timestamp()).max();
        if let Some(ts) = latest {
            if ts < cutoff {
                to_delete.push(summary.id);
            }
        }
    }

    let mut deleted_ids = Vec::new();
    if !dry_run {
        for id in &to_delete {
            // Not `remove_dir_all(dir)`: the sweep is unattended and has no
            // undo, so the path is re-derived from the validated id and
            // asserted equal to it after symlink resolution. See
            // `remove_session_dir`.
            if let Err(e) = crate::session_manager::remove_session_dir(sm.sessions_root(), id).await
            {
                warn!(
                    session_id = %id,
                    error = %e,
                    "Failed to delete session directory"
                );
            } else {
                deleted_ids.push(id.clone());
            }
        }
    } else {
        deleted_ids.clone_from(&to_delete);
    }

    let total = deleted_ids.len();
    Response::success(
        req.id,
        serde_json::json!({
            "deleted": deleted_ids,
            "total": total,
            "dry_run": dry_run,
            "scope": scope_label,
        }),
    )
}

// =============================================================================
// MCP Server RPC Handlers
// =============================================================================
