use super::session_commands::{
    __path_execute_command, __path_list_commands, execute_command, list_commands,
};
use super::session_status::{
    __path_dismiss_session_notification, __path_session_notifications, __path_session_status,
    dismiss_session_notification, session_notifications, session_status,
};
use crate::routes::helpers::ModelsResponse;
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    Json,
};
use crucible_core::protocol::requests::{
    NamedKiln, Page, SessionCreateRequest, Title, WorkspaceChoice,
};
use crucible_core::session::SessionSearchResponse;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

// =========================================================================
// Typed Response Structs
// =========================================================================

/// Standard acknowledgment response for successful mutations.
// `pub(crate)`, not `pub(super)`: the plugin option endpoint answers this too,
// as one arm of an untagged union. One `{"ok": true}` shape, one schema in the
// document. The doc comment above is published to the browser, so the reason
// stays here rather than there.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct OkResponse {
    pub(crate) ok: bool,
}

impl OkResponse {
    pub(crate) fn success() -> Json<Self> {
        Json(Self::ok())
    }

    /// The bare value, for a caller that wraps it in something other than
    /// [`Json`].
    pub(crate) fn ok() -> Self {
        Self { ok: true }
    }
}

/// Response for session archive/unarchive status changes.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct ArchiveResponse {
    archived: bool,
}

/// Response for session deletion.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct DeleteResponse {
    deleted: bool,
}

/// Response for session cancellation.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct CancelledResponse {
    cancelled: bool,
}

/// Response for title operations.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct TitleResponse {
    title: String,
}

// `session.create`, `session.list` and `session.get` all answer the core
// `SessionSummary` (or `SessionListReply`, a `Vec<SessionSummary>` with a
// count). The daemon used to build three shapes by hand for one entity, and
// this route used to declare a fourth — a hand-written union, `SessionRow`
// — to read all three. There is one shape now, so this route returns it
// unchanged: no row, no `Option<Option<T>>` present-or-null trick.

/// What `GET /api/session/{id}/history` answers.
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

/// What `session.pause`, `session.resume` and `session.end` answer.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct SessionLifecycleResponse {
    session_id: String,
    /// The state the session left. `session.end` does not send it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous_state: Option<String>,
    /// The state the session is in now.
    state: String,
    /// The session's kilns. Only `session.end` sends them.
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

/// The session scope that a kiln or workspace mutation echoes.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct SessionScopeResponse {
    session_id: String,
    /// Every kiln the session can query, by registry name.
    kilns: Vec<String>,
    /// The session's working directory. `null` is a session with no workspace.
    workspace: Option<String>,
}

/// One LLM provider the daemon found.
///
/// Mirrors `crucible_core::types::ProviderInfo` field for field. It is
/// declared here rather than re-exported because `crucible-core` takes no
/// utoipa dependency, and a schema is what puts the fields in the document.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct ProviderRow {
    name: String,
    /// The backend behind the provider, such as `ollama` or `openai`.
    provider_type: String,
    /// Whether the provider answered its probe.
    available: bool,
    default_model: Option<String>,
    models: Vec<String>,
    endpoint: Option<String>,
    /// Why the provider is unavailable, when it is.
    reason: Option<String>,
    /// Whether the provider runs on this machine.
    is_local: bool,
}

/// What `GET /api/providers` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct ProvidersResponse {
    providers: Vec<ProviderRow>,
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
        .routes(routes!(list_sessions))
        .routes(routes!(search_sessions))
        .routes(routes!(get_session, delete_session))
        .routes(routes!(get_session_history))
        .routes(routes!(pause_session))
        .routes(routes!(resume_session))
        .routes(routes!(end_session))
        .routes(routes!(archive_session))
        .routes(routes!(unarchive_session))
        .routes(routes!(cancel_session))
        .routes(routes!(list_models))
        .routes(routes!(set_knob))
        .routes(routes!(get_knob))
        .routes(routes!(list_modes))
        .routes(routes!(list_knobs))
        .routes(routes!(session_status))
        .routes(routes!(session_notifications))
        .routes(routes!(dismiss_session_notification))
        .routes(routes!(connect_kiln))
        .routes(routes!(disconnect_kiln))
        .routes(routes!(set_workspace))
        .routes(routes!(set_session_title))
        .routes(routes!(auto_title))
        .routes(routes!(list_providers))
        // Config knobs register themselves in `session_config`, next to their
        // handlers: the group belongs with the knobs it serves, and has no
        // reason to be spelled out here.
        .merge(super::session_config::config_routes())
        .routes(routes!(export_session))
        .routes(routes!(execute_command))
        .routes(routes!(list_commands))
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

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct ListSessionsQuery {
    /// The kiln's registry NAME. A query string carrying a path is a 422 —
    /// which is the honest answer, because a path names no kiln.
    #[param(value_type = Option<String>)]
    kiln: Option<crucible_core::config::KilnName>,
    #[param(value_type = Option<String>)]
    workspace: Option<PathBuf>,
    #[serde(rename = "type")]
    session_type: Option<String>,
    state: Option<String>,
    #[serde(default)]
    include_archived: Option<bool>,
}

#[utoipa::path(
    get,
    path = "/api/session/list",
    params(ListSessionsQuery),
    responses(
        (status = 200, body = crucible_core::protocol::requests::SessionListReply),
        (status = 422, description = "The `kiln` query carried a path rather than a registry name"),
        (status = 502, description = "The daemon could not list the sessions"),
    )
)]
async fn list_sessions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListSessionsQuery>,
) -> Result<Json<crucible_core::protocol::requests::SessionListReply>, WebError> {
    let result = state
        .daemon
        .session_list(
            query.kiln.as_ref(),
            query.workspace.as_deref(),
            query.session_type.as_deref(),
            query.state.as_deref(),
            query.include_archived,
        )
        .await
        .daemon_err()?;

    Ok(Json(result))
}

/// `GET /api/sessions/search?q=…&kiln=…&kiln=…&limit=…`
///
/// `kiln` repeats. Search scope is kiln-set *overlap*, so the caller states
/// every kiln it is cleared for rather than one member standing in for the
/// rest — one member matches only the sessions sharing that one. Parsed from
/// the raw pairs because `serde_urlencoded`, which `Query` uses, cannot
/// deserialize a repeated key into a sequence.
#[utoipa::path(
    get,
    path = "/api/sessions/search",
    params(
        ("q" = String, Query, description = "The substring to match, case-insensitive"),
        ("kiln" = Option<Vec<String>>, Query, description = "The caller's whole kiln set, one `kiln` key per member"),
        ("limit" = Option<usize>, Query, description = "How many matches to return. The default is 20"),
    ),
    responses(
        (status = 200, body = SessionSearchResponse),
        (status = 422, description = "No `q`, or every `kiln` named an unusable name"),
        (status = 502, description = "The daemon could not run the search"),
    )
)]
async fn search_sessions(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<Vec<(String, String)>>,
) -> Result<Json<SessionSearchResponse>, WebError> {
    let mut query = None;
    // Names, parsed rather than accepted. The daemon draws a deliberate
    // distinction at `server/session/scope.rs`: "no kiln key at all" is an
    // empty scope each handler interprets for itself, while "named kilns, none
    // of which resolve" is an INVALID_PARAMS naming the refused values —
    // because an all-dropped set is a request that asked to NARROW and would
    // otherwise be answered as though it had said nothing.
    //
    // This route has to draw the same line or it collapses the two: dropping
    // every name silently turns `?q=x&kiln=..%2Fescape` into "searched
    // everything, found nothing" instead of a 422. Partial drops are safe and
    // stay silent, for the daemon's reason — the surviving members still narrow.
    let mut kilns: Vec<crucible_core::config::KilnName> = Vec::new();
    let mut refused: Vec<String> = Vec::new();
    let mut limit = None;
    for (key, value) in params {
        match key.as_str() {
            "q" => query = Some(value),
            "kiln" => match crucible_core::config::KilnName::parse(&value) {
                Ok(name) => kilns.push(name),
                Err(_) => refused.push(value),
            },
            "limit" => limit = value.parse::<usize>().ok(),
            _ => {}
        }
    }
    if kilns.is_empty() && !refused.is_empty() {
        return Err(WebError::Validation(format!(
            "None of the kilns named in this request are usable names: {}. Kilns are addressed \
             by the name of their `[kilns]` entry, not by path.",
            refused
                .iter()
                .map(|v| format!("{v:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let query = query.ok_or_else(|| WebError::Validation("Missing 'q' parameter".into()))?;

    let results = state
        .daemon
        .session_search(&query, &kilns, limit.or(Some(20)))
        .await
        .daemon_err()?;

    Ok(Json(results))
}

#[utoipa::path(
    get,
    path = "/api/session/{id}",
    params(("id" = String, Path, description = "The session to read")),
    responses(
        (status = 200, body = crucible_core::session::SessionDetail),
        (status = 502, description = "The daemon could not read the session"),
    )
)]
async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<crucible_core::session::SessionDetail>, WebError> {
    let result = state.daemon.session_get(&id).await.daemon_err()?;

    Ok(Json(result))
}

#[utoipa::path(
    get,
    path = "/api/session/{id}/history",
    params(("id" = String, Path, description = "The session to replay"), Page),
    responses(
        (status = 200, body = SessionHistoryResponse),
        (status = 502, description = "The daemon could not read the transcript"),
    )
)]
async fn get_session_history(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Query(page): axum::extract::Query<Page>,
) -> Result<Json<SessionHistoryResponse>, WebError> {
    // A read, so the session stays as it is. `session.resume_from_storage`
    // would make an ended session live and run its start checks.
    let result = state.daemon.session_history(&id, page).await.daemon_err()?;

    Ok(Json(daemon_shape(result, "session.history")?))
}

#[utoipa::path(
    post,
    path = "/api/session/{id}/pause",
    params(("id" = String, Path, description = "The session to pause")),
    responses(
        (status = 200, body = SessionLifecycleResponse),
        (status = 502, description = "The daemon could not pause the session"),
    )
)]
async fn pause_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionLifecycleResponse>, WebError> {
    let result = state.daemon.session_pause(&id).await.daemon_err()?;

    Ok(Json(daemon_shape(result, "session.pause")?))
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
    post,
    path = "/api/session/{id}/unarchive",
    params(("id" = String, Path, description = "The session to unarchive")),
    responses(
        (status = 200, body = ArchiveResponse),
        (status = 404, description = "No session of that id"),
        (status = 502, description = "The daemon could not unarchive the session"),
    )
)]
async fn unarchive_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ArchiveResponse>, WebError> {
    state
        .daemon
        .session_unarchive(&id)
        .await
        .map_err(|e| map_session_not_found(e, &id))?;
    Ok(Json(ArchiveResponse { archived: false }))
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
    path = "/api/session/{id}/cancel",
    params(("id" = String, Path, description = "The session whose turn to cancel")),
    responses(
        (status = 200, body = CancelledResponse),
        (status = 502, description = "The daemon could not cancel the turn"),
    )
)]
async fn cancel_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CancelledResponse>, WebError> {
    let cancelled = state.daemon.session_cancel(&id).await.daemon_err()?;
    Ok(Json(CancelledResponse { cancelled }))
}

#[utoipa::path(
    get,
    path = "/api/session/{id}/models",
    params(("id" = String, Path, description = "The session whose models to list")),
    responses(
        (status = 200, body = ModelsResponse),
        (status = 502, description = "The daemon could not list the models"),
    )
)]
async fn list_models(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ModelsResponse>, WebError> {
    let models = state.daemon.session_list_models(&id).await.daemon_err()?;
    Ok(Json(ModelsResponse { models }))
}

/// What a note write in a mode does.
///
/// Mirrors `crucible_core::types::mode::WriteMode`, which carries no
/// schema: `crucible-core` takes no utoipa dependency, and a client that
/// shows a mode has to know the values it can read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum WriteModeRow {
    /// The note tools write the file.
    Apply,
    /// The note tools record a proposal, and the file does not change.
    Propose,
}

impl From<crucible_core::types::WriteMode> for WriteModeRow {
    fn from(writes: crucible_core::types::WriteMode) -> Self {
        use crucible_core::types::WriteMode;
        match writes {
            WriteMode::Apply => Self::Apply,
            WriteMode::Propose => Self::Propose,
        }
    }
}

/// One mode a session may enter.
///
/// Mirrors `crucible_core::types::mode::ModeDescriptor` field for field, for
/// the reason [`WriteModeRow`] gives.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct ModeRow {
    /// The mode id, such as `plan`.
    id: String,
    /// The label to draw.
    name: String,
    description: Option<String>,
    /// An emoji or an icon name.
    icon: Option<String>,
    /// What a note write in this mode does, already degraded to what this
    /// session's agent can hold back.
    writes: WriteModeRow,
}

impl From<crucible_core::types::mode::ModeDescriptor> for ModeRow {
    fn from(mode: crucible_core::types::mode::ModeDescriptor) -> Self {
        Self {
            id: mode.id,
            name: mode.name,
            description: mode.description,
            icon: mode.icon,
            writes: mode.writes.into(),
        }
    }
}

/// What `GET /api/session/{id}/modes` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct SessionModesResponse {
    /// The mode the session is in. Always one of `modes`.
    current_mode_id: String,
    /// Every mode the session may switch to, in declaration order.
    modes: Vec<ModeRow>,
}

impl From<crucible_core::types::mode::SessionModes> for SessionModesResponse {
    fn from(modes: crucible_core::types::mode::SessionModes) -> Self {
        Self {
            current_mode_id: modes.current_mode_id,
            modes: modes.modes.into_iter().map(ModeRow::from).collect(),
        }
    }
}

/// The session's modes, forwarded from the daemon unchanged.
///
/// The web layer deliberately adds nothing here: mode labels and ordering are
/// the daemon's, so the TUI and the browser cannot drift into showing
/// different names for the same mode.
#[utoipa::path(
    get,
    path = "/api/session/{id}/modes",
    params(("id" = String, Path, description = "The session whose modes to list")),
    responses(
        (status = 200, body = SessionModesResponse),
        (status = 502, description = "The daemon could not list the modes"),
    )
)]
async fn list_modes(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionModesResponse>, WebError> {
    let modes = state.daemon.session_list_modes(&id).await.daemon_err()?;
    Ok(Json(modes.into()))
}

/// Which settings this session can change.
///
/// The browser drew a fixed set of controls, which was wrong for every ACP
/// session: the protocol has no temperature and no token cap, so the panel
/// offered a slider for each that changed nothing. As with modes, the web
/// layer adds nothing — the answer is the daemon's, so the TUI and the
/// browser cannot disagree about what a session can do.
#[utoipa::path(
    get,
    path = "/api/session/{id}/knobs",
    params(("id" = String, Path, description = "The session whose settings to describe")),
    responses(
        (status = 200, body = SessionKnobsResponse),
        (status = 502, description = "The daemon could not describe the settings"),
    )
)]
async fn list_knobs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionKnobsResponse>, WebError> {
    let knobs = state.daemon.session_list_knobs(&id).await.daemon_err()?;
    Ok(Json(knobs.into()))
}

/// One setting and whether this session can change it.
///
/// Mirrors `crucible_core::types::KnobDescriptor`, for the reason
/// [`WriteModeRow`] gives.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct KnobRow {
    /// The knob id, such as `context_strategy`.
    id: String,
    /// `false` means the control should not be offered: the daemon refuses
    /// the call.
    supported: bool,
}

/// What `GET /api/session/{id}/knobs` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct SessionKnobsResponse {
    /// Every knob Crucible has, answered for. A client that finds an id
    /// missing is talking to an older daemon.
    knobs: Vec<KnobRow>,
}

impl From<crucible_core::types::SessionKnobSupport> for SessionKnobsResponse {
    fn from(support: crucible_core::types::SessionKnobSupport) -> Self {
        Self {
            knobs: support
                .knobs
                .into_iter()
                .map(|knob| KnobRow {
                    id: knob.id,
                    supported: knob.supported,
                })
                .collect(),
        }
    }
}

/// Write one session knob — model, mode, context strategy, precognition or
/// plugin turn limit. The body names its own knob (`KnobValue`'s tag), so
/// one route serves all five: a knob added later needs no sibling route.
#[utoipa::path(
    put,
    path = "/api/session/{id}/knob",
    params(("id" = String, Path, description = "The session to configure")),
    request_body = crucible_core::types::KnobValue,
    responses(
        (status = 200, body = OkResponse),
        (status = 422, description = "The session cannot carry the knob, or the value is invalid"),
        (status = 502, description = "The daemon could not store the value"),
    )
)]
async fn set_knob(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(value): Json<crucible_core::types::KnobValue>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_knob_set(&id, value)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

/// Read one session knob, in the same [`crucible_core::types::KnobValue`]
/// shape [`set_knob`] writes.
#[utoipa::path(
    get,
    path = "/api/session/{id}/knob/{knob}",
    params(
        ("id" = String, Path, description = "The session to read"),
        ("knob" = String, Path, description = "The knob to read: model, mode, context_strategy, precognition or plugin_turn_limit"),
    ),
    responses(
        (status = 200, body = crucible_core::types::KnobValue),
        (status = 422, description = "The session cannot carry the knob"),
        (status = 502, description = "The daemon could not read the value"),
    )
)]
async fn get_knob(
    State(state): State<AppState>,
    Path((id, knob)): Path<(String, crucible_core::types::SessionKnob)>,
) -> Result<Json<crucible_core::types::KnobValue>, WebError> {
    let value = state
        .daemon
        .session_knob_get(&id, knob)
        .await
        .daemon_err()?;
    Ok(Json(value))
}

/// Updated session scope, echoed by kiln/workspace mutations.
#[utoipa::path(
    post,
    path = "/api/session/{id}/kilns/connect",
    params(("id" = String, Path, description = "The session to attach the kiln to")),
    request_body = NamedKiln,
    responses(
        (status = 200, body = SessionScopeResponse),
        (status = 422, description = "The body named a path rather than a registry name"),
        (status = 502, description = "The daemon could not attach the kiln"),
    )
)]
async fn connect_kiln(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<NamedKiln>,
) -> Result<Json<SessionScopeResponse>, WebError> {
    let scope = state
        .daemon
        .session_connect_kiln(&id, &req.kiln)
        .await
        .daemon_err()?;
    Ok(Json(daemon_shape(scope, "session.connect_kiln")?))
}

#[utoipa::path(
    post,
    path = "/api/session/{id}/kilns/disconnect",
    params(("id" = String, Path, description = "The session to detach the kiln from")),
    request_body = NamedKiln,
    responses(
        (status = 200, body = SessionScopeResponse),
        (status = 422, description = "The body named a path rather than a registry name"),
        (status = 502, description = "The daemon could not detach the kiln"),
    )
)]
async fn disconnect_kiln(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<NamedKiln>,
) -> Result<Json<SessionScopeResponse>, WebError> {
    let scope = state
        .daemon
        .session_disconnect_kiln(&id, &req.kiln)
        .await
        .daemon_err()?;
    Ok(Json(daemon_shape(scope, "session.disconnect_kiln")?))
}

#[utoipa::path(
    put,
    path = "/api/session/{id}/workspace",
    params(("id" = String, Path, description = "The session whose workspace to set")),
    request_body = WorkspaceChoice,
    responses(
        (status = 200, body = SessionScopeResponse),
        (status = 502, description = "The daemon could not set the workspace"),
    )
)]
async fn set_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<WorkspaceChoice>,
) -> Result<Json<SessionScopeResponse>, WebError> {
    let scope = state
        .daemon
        .session_set_workspace(&id, req.workspace.as_deref().map(std::path::Path::new))
        .await
        .daemon_err()?;
    Ok(Json(daemon_shape(scope, "session.set_workspace")?))
}

// Mode is a knob now: `PUT /api/session/{id}/knob` with
// `{"knob": "mode", "value": "plan"}`, and `GET
// /api/session/{id}/knob/mode` to read it. See `set_knob`/`get_knob` above.

#[utoipa::path(
    put,
    path = "/api/session/{id}/title",
    params(("id" = String, Path, description = "The session to rename")),
    request_body = Title,
    responses(
        (status = 200, body = OkResponse),
        (status = 502, description = "The daemon could not set the title"),
    )
)]
async fn set_session_title(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<Title>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_title(&id, &req.title)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

/// Auto-generate a title for a session from its conversation history.
///
/// Delegates to the daemon's `session.generate_title`, which produces a
/// topic-based title via the session's own LLM provider (falling back to
/// first-message truncation daemon-side). Idempotent: an already-titled
/// session returns its existing title.
#[utoipa::path(
    post,
    path = "/api/session/{id}/auto-title",
    params(("id" = String, Path, description = "The session to title")),
    responses(
        (status = 200, body = TitleResponse),
        (status = 502, description = "The daemon could not generate a title"),
    )
)]
async fn auto_title(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TitleResponse>, WebError> {
    let result = state
        .daemon
        .session_generate_title(&id)
        .await
        .daemon_err()?;

    let title = result
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Untitled Session")
        .to_string();

    Ok(Json(TitleResponse { title }))
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

/// Served through the SWR catalog cache — provider probing takes ~0.7s and
/// must not gate every splash render. Shape: `{providers: [ProviderInfo]}`.
///
/// Takes no `kiln` parameter. It used to accept `kiln: Option<PathBuf>` and
/// forward the raw directory to the daemon, which fed it to
/// `find_workspace_and_resolve_classification` — so an arbitrary directory
/// could influence which providers a caller was told about, an input door
/// standing outside the registry floor every other kiln input now passes
/// through. Nothing ever sent it (`listProviders()` takes no argument), so
/// converting it to a name would have preserved a door for no caller.
#[utoipa::path(
    get,
    path = "/api/providers",
    responses(
        (status = 200, body = ProvidersResponse),
        (status = 502, description = "The daemon could not list the providers"),
    )
)]
async fn list_providers(
    State(state): State<AppState>,
) -> Result<Json<ProvidersResponse>, WebError> {
    let providers = crate::services::catalog::providers_value(&state)
        .await
        .daemon_err()?;
    Ok(Json(ProvidersResponse {
        providers: daemon_shape(providers, "providers.list")?,
    }))
}

#[cfg(test)]
mod search_scope_tests;
#[cfg(test)]
mod shape_tests;
#[cfg(test)]
mod tests;
