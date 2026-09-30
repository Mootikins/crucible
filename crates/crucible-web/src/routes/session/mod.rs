//! `POST /api/session` (create), `POST /api/session/{id}/resume`,
//! `POST /api/session/{id}/export`, `POST /api/session/{id}/end`,
//! `POST /api/session/{id}/archive` and `DELETE /api/session/{id}`.
//!
//! Every other session route that only forwarded one RPC row moved onto
//! `rpc(method, params)` and `POST /api/rpc/{method}`
//! ([[Simplification Plan#Step 19]] item 4/9). These six stay, each for its
//! own reason:
//! - `create_session` validates `agent_type` before the daemon ever sees the
//!   request. The daemon's own `session.create` treats an unrecognized
//!   `agent_type` (anything other than `"acp"`) as `"internal"` rather than
//!   refusing it, so this check is web-only — not yet a decision the daemon
//!   makes once for every caller (a candidate for a follow-up, per
//!   AGENTS.md's "keep a behavior in the daemon" rule).
//! - `resume_session` composes two daemon calls when
//!   `session.resume` reports `resumed_from_storage`: a plain forward would
//!   need the same conditional follow-up call in the browser instead.
//! - `export_session` composes `session.get` and `session.render_markdown`,
//!   with a fallback markdown built from session metadata when rendering
//!   fails.
//! - `end_session`, `archive_session` and `delete_session` each release this
//!   web process's own `EventBroker` entry for the session
//!   (`ReconnectingDaemon::close_event_streams`) after the daemon call
//!   succeeds. The broker is this process's local SSE fan-out state, not a
//!   daemon concept and not something the browser can reach on its own, so a
//!   plain `rpc()` forward would leave a dangling upstream subscription for
//!   a session that just ended, archived or was deleted.

use super::session_commands::{__path_execute_command, execute_command};
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    Json,
};
use crucible_core::protocol::requests::SessionCreateRequest;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

// =========================================================================
// Typed Response Structs
// =========================================================================

/// What `GET /api/session/{id}/history` answers (`session.history`'s own
/// reply shape). Kept here, not only in core, because [`resume_session`]
/// decodes a `session.history` reply through it on the cold path.
///
/// `history` is the core [`crucible_core::protocol::SessionEventMessage`] —
/// the same envelope the SSE stream carries — replayed whatever the
/// transcript holds, including an event name a newer daemon minted. Its
/// `data` stays untyped for exactly that reason: this build cannot type an
/// event it does not know.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct SessionHistoryResponse {
    session_id: String,
    /// The session type prefix.
    #[serde(rename = "type")]
    session_type: String,
    state: String,
    kilns: Vec<String>,
    /// The page of events the query asked for.
    history: Vec<crucible_core::protocol::SessionEventMessage>,
    /// How many events the whole transcript holds, for paging.
    total_events: usize,
    /// The whole log, folded into what a client draws. The daemon folds it;
    /// a client renders it and does not fold the events again.
    #[serde(default)]
    transcript: crucible_core::transcript::Transcript,
}

/// What `session.resume`'s warm path answers (`SessionTransitionReply`'s own
/// shape, decoded here for [`resume_session`]).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct SessionLifecycleResponse {
    session_id: String,
    /// The state the session left.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous_state: Option<String>,
    /// The state the session is in now.
    state: String,
    /// The session's kilns, when the daemon sends them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kilns: Option<Vec<String>>,
}

/// What `POST /api/session/{id}/resume` answers, which depends on the path
/// that resumed the session — a decision `session.resume` itself makes and
/// reports (`SessionTransitionReply::resumed_from_storage`), not something
/// this route infers from which daemon call happened to succeed.
///
/// The warm path answers the state change alone. The cold path (the session
/// was not held in memory, or was held but not `Paused` — most commonly
/// `Ended`) also answers the full history, read with `session.history`
/// after the daemon's own resume, because the browser's view of a session it
/// did not just have open may be stale.
///
/// **`Restored` must stay first.** The daemon sends no tag, so the variants
/// are told apart by their fields, and `Live`'s required fields
/// (`session_id`, `state`) are a subset of `Restored`'s. An untagged enum
/// takes the first variant that fits, so with the order reversed every
/// restored history would read back as a bare state change and every event
/// would be dropped without an error. `a_restored_payload_does_not_read_as_a_live_one`
/// holds the order.
///
/// `deny_unknown_fields` would be the other way to separate them, and it is
/// not used here: these are daemon replies, and refusing a field a newer
/// daemon added would turn an extension into a 502 on three healthy routes.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
enum ResumeSessionResponse {
    /// The session came back from the store.
    Restored(Box<SessionHistoryResponse>),
    /// The session was resident and merely paused.
    Live(SessionLifecycleResponse),
}

/// Response for session archive/unarchive status changes. `unarchive_session`
/// is a pure forward now, but `archive_session` still builds this reply
/// itself (see the module doc) for the `close_event_streams` side effect, so
/// the type stays.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct ArchiveResponse {
    archived: bool,
}

/// Response for session deletion.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct DeleteResponse {
    deleted: bool,
}

/// Read a daemon reply into the shape this route promises.
///
/// The daemon answers `serde_json::Value`, so the route is where the wire
/// shape is decided. A reply that does not fit is a protocol failure between
/// two Crucible processes rather than a client error, so it answers 502 like
/// every other daemon fault.
pub(crate) fn daemon_shape<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
    method: &str,
) -> Result<T, WebError> {
    serde_json::from_value(value).map_err(|e| {
        WebError::Daemon(format!(
            "{method} answered a shape this route cannot read: {e}"
        ))
    })
}

// =========================================================================
// Route Helpers
// =========================================================================

/// Map daemon errors for session operations, converting "Session not found" to 404.
fn map_session_not_found(err: impl std::fmt::Display, id: &str) -> WebError {
    let message = err.to_string();
    if message.contains("Session not found") {
        WebError::NotFound(format!("Session not found: {id}"))
    } else {
        WebError::Daemon(message)
    }
}

/// Session routes.
pub fn session_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create_session))
        .routes(routes!(resume_session))
        .routes(routes!(export_session))
        .routes(routes!(end_session))
        .routes(routes!(archive_session))
        .routes(routes!(delete_session))
        .routes(routes!(execute_command))
}

#[derive(Debug, Deserialize, ToSchema)]
struct CreateSessionRequest {
    #[serde(default = "default_session_type")]
    session_type: String,
    /// The session's kiln set by registry NAME — flat, no member privileged.
    /// Empty or omitted is a literal empty set, NOT a request for a default:
    /// it creates a session with no corpus attached. The daemon stopped
    /// substituting its data root here because that root is the parent of the
    /// sessions store, so "default" quietly put every transcript in scope.
    #[serde(default)]
    #[schema(value_type = Vec<String>)]
    kilns: Vec<crucible_core::config::KilnName>,
    #[schema(value_type = Option<String>)]
    workspace: Option<PathBuf>,
    /// LLM provider (e.g., "ollama", "openai", "anthropic")
    provider: Option<String>,
    /// Model name (e.g., "llama3.2", "gpt-4o", "claude-3-5-sonnet")
    model: Option<String>,
    /// Custom endpoint URL (optional, for self-hosted models). The daemon
    /// refuses one that targets an internal address it does not have
    /// configured.
    endpoint: Option<String>,
    /// "internal" (default) or "acp"
    agent_type: Option<String>,
    /// ACP agent profile name (e.g. "claude", "opencode"); required when agent_type == "acp"
    agent_name: Option<String>,
    /// Internal-agent card name; never resolved in the web layer.
    agent_card: Option<String>,
    /// Isolation override: absent → resolve normally; `false` → no container
    /// even if the project has one; `true`, a profile name or an environment
    /// object → override. Forwarded to the daemon untouched — the vocabulary
    /// belongs to the plugin that resolves it, and an unknown profile comes
    /// back as `-32602`, which `daemon_err` turns into a 422.
    #[schema(value_type = Option<serde_json::Value>)]
    isolation: Option<serde_json::Value>,
}

fn default_session_type() -> String {
    "chat".to_string()
}

/// Map a `session.create` daemon error to an HTTP status. An `INVALID_PARAMS`
/// error (JSON-RPC code `-32602` — e.g. an unknown ACP profile or an
/// unparseable provider override, both now resolved daemon-side) is a client
/// error (422), preserving the pre-consolidation behavior where the web
/// validated the profile itself. Anything else is a daemon/transport failure
/// (502).
///
/// The daemon also checks a custom `endpoint` (no internal addresses; see
/// `crucible_daemon::provider::endpoint_check`) and refuses it with `-32602`,
/// so a refused endpoint is a 422 here. The web keeps no copy of the check.
#[utoipa::path(
    post,
    path = "/api/session",
    request_body = CreateSessionRequest,
    responses(
        (status = 200, body = crucible_core::session::SessionSummary),
        (status = 422, description = "The request named an endpoint, an agent type or a card the server refuses"),
        (status = 502, description = "The daemon could not create the session"),
    )
)]
async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<crucible_core::session::SessionSummary>, WebError> {
    // Validate agent_type up front: an unrecognized value (e.g. "ACP",
    // "internal-x") must be rejected, not silently forwarded to the daemon as a
    // junk string while taking the internal branch.
    match req.agent_type.as_deref() {
        None | Some("internal") | Some("acp") => {}
        Some(other) => {
            return Err(WebError::Validation(format!(
                "Invalid agent_type: {other:?} (expected \"internal\" or \"acp\")"
            )));
        }
    }

    let is_acp = req.agent_type.as_deref() == Some("acp");
    if is_acp && req.agent_name.as_deref().unwrap_or("").is_empty() {
        return Err(WebError::Validation(
            "agent_name is required when agent_type is \"acp\"".to_string(),
        ));
    }

    // Hand the agent spec to the daemon, which owns default-agent resolution:
    // it resolves the ACP profile (unknown ⇒ INVALID_PARAMS, and no session is
    // created — see `daemon_err`) or builds config-derived internal
    // defaults, configures the session's agent as part of create, and returns
    // the resolved model in `agent_model`. The web no longer keeps its own copy
    // of "what is the default agent". Kilns are forwarded verbatim, empty set
    // included — see `CreateSessionRequest::kilns`.
    let result = state
        .daemon
        .session_create(SessionCreateRequest {
            session_type: req.session_type.clone(),
            kilns: SessionCreateRequest::kiln_set(req.kilns.clone()),
            workspace: req
                .workspace
                .as_deref()
                .map(|p| p.to_string_lossy().into_owned()),
            agent_type: req.agent_type.clone(),
            isolation: req.isolation.clone(),
            configure_agent: true,
            agent_name: req.agent_name.clone(),
            agent_card: req.agent_card.clone(),
            provider: req.provider.clone(),
            model: req.model.clone(),
            endpoint: req.endpoint.clone(),
            ..Default::default()
        })
        .await
        .daemon_err()?;

    // A create response without a usable session_id (protocol drift) would
    // otherwise leave the browser with no stream it can open; fail loudly here.
    if result.id.as_str().is_empty() {
        return Err(WebError::Daemon(
            "daemon returned no session_id from session.create".to_string(),
        ));
    }

    Ok(Json(result))
}

#[utoipa::path(
    post,
    path = "/api/session/{id}/resume",
    params(("id" = String, Path, description = "The session to resume")),
    responses(
        (status = 200, body = ResumeSessionResponse),
        (status = 404, description = "No session of that id"),
        (status = 502, description = "The daemon could not resume the session"),
    )
)]
async fn resume_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ResumeSessionResponse>, WebError> {
    // Transparent resume: sessions are always resumable. `session.resume`
    // itself decides whether the session was resident and merely paused, or
    // needed reloading from the daemon's session store — the daemon's own
    // policy, shared by every caller (see `SessionTransitionReply::resumed_from_storage`).
    // This route only decides what a resumed browser needs NEXT: a stored
    // resume means the browser's own view may be stale, so it also reads the
    // full transcript (a read-only `session.history` call); a warm resume
    // needs no such call, because the browser's live view never lapsed.
    let raw = state
        .daemon
        .session_resume(&id)
        .await
        .map_err(|e| map_session_not_found(e, &id))?;

    let reply = if raw["resumed_from_storage"].as_bool().unwrap_or(false) {
        let history = state
            .daemon
            .session_history(&id, Default::default())
            .await
            .map_err(|e| map_session_not_found(e, &id))?;
        ResumeSessionResponse::Restored(Box::new(daemon_shape(history, "session.history")?))
    } else {
        ResumeSessionResponse::Live(daemon_shape(raw, "session.resume")?)
    };

    Ok(Json(reply))
}

#[utoipa::path(
    post,
    path = "/api/session/{id}/end",
    params(("id" = String, Path, description = "The session to end")),
    responses(
        (status = 200, body = SessionLifecycleResponse),
        (status = 502, description = "The daemon could not end the session"),
    )
)]
async fn end_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionLifecycleResponse>, WebError> {
    let result = state.daemon.session_end(&id).await.daemon_err()?;

    state.daemon.close_event_streams(&id).await;

    Ok(Json(daemon_shape(result, "session.end")?))
}

#[utoipa::path(
    post,
    path = "/api/session/{id}/archive",
    params(("id" = String, Path, description = "The session to archive")),
    responses(
        (status = 200, body = ArchiveResponse),
        (status = 404, description = "No session of that id"),
        (status = 502, description = "The daemon could not archive the session"),
    )
)]
async fn archive_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ArchiveResponse>, WebError> {
    state
        .daemon
        .session_archive(&id)
        .await
        .map_err(|e| map_session_not_found(e, &id))?;
    state.daemon.close_event_streams(&id).await;
    Ok(Json(ArchiveResponse { archived: true }))
}

#[utoipa::path(
    delete,
    path = "/api/session/{id}",
    params(("id" = String, Path, description = "The session to delete")),
    responses(
        (status = 200, body = DeleteResponse),
        (status = 404, description = "No session of that id"),
        (status = 502, description = "The daemon could not delete the session"),
    )
)]
async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, WebError> {
    state
        .daemon
        .session_delete(&id)
        .await
        .map_err(|e| map_session_not_found(e, &id))?;
    state.daemon.close_event_streams(&id).await;
    Ok(Json(DeleteResponse { deleted: true }))
}

#[utoipa::path(
    post,
    path = "/api/session/{id}/export",
    params(("id" = String, Path, description = "The session to export")),
    responses(
        (status = 200, content_type = "text/markdown; charset=utf-8", body = String),
        (status = 502, description = "The daemon could not read the session"),
    )
)]
async fn export_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<
    (
        [(
            axum::http::header::HeaderName,
            axum::http::header::HeaderValue,
        ); 1],
        String,
    ),
    WebError,
> {
    // Metadata only — the daemon resolves the session's own directory from the
    // id, so the web no longer keeps a copy of the storage layout (it kept two
    // arms of one, and both were wrong once sessions left kilns).
    let session = state.daemon.session_get(&id).await.daemon_err()?;

    // Try to render markdown from persisted session events
    let markdown = match state
        .daemon
        .session_render_markdown(&id, Some(true), None, Some(true), None)
        .await
    {
        Ok(md) => md,
        Err(_) => {
            // Fallback: construct basic markdown from session metadata
            let title = session.title.as_deref().unwrap_or("Untitled Session");
            let started_at = session.started_at.to_rfc3339();
            let model = session.agent_model.as_deref().unwrap_or("unknown");
            let state_str = session.state.to_string();

            format!(
                "# {}\n\n- **Date**: {}\n- **Model**: {}\n- **State**: {}\n\n---\n\n*Session events are not yet persisted. Export will be available after the session is paused or ended.*\n",
                title, started_at, model, state_str
            )
        }
    };

    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            axum::http::header::HeaderValue::from_static("text/markdown; charset=utf-8"),
        )],
        markdown,
    ))
}

#[cfg(test)]
mod tests;
