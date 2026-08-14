use super::session_commands::{execute_command, list_commands};
use super::session_status::session_status;
use crate::routes::helpers::ModelsResponse;
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;

// =========================================================================
// Typed Response Structs
// =========================================================================

/// Standard acknowledgment response for successful mutations.
#[derive(Debug, Serialize)]
pub(super) struct OkResponse {
    ok: bool,
}

impl OkResponse {
    pub(super) fn success() -> Json<Self> {
        Json(Self { ok: true })
    }
}

/// Response for session archive/unarchive status changes.
#[derive(Debug, Serialize)]
struct ArchiveResponse {
    archived: bool,
}

/// Response for session deletion.
#[derive(Debug, Serialize)]
struct DeleteResponse {
    deleted: bool,
}

/// Response for session cancellation.
#[derive(Debug, Serialize)]
struct CancelledResponse {
    cancelled: bool,
}

/// Response for title operations.
#[derive(Debug, Serialize)]
struct TitleResponse {
    title: String,
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

/// Session routes for a harness with no bind address — the fail-closed policy.
///
/// Named for what it is rather than offered as `session_routes()`, because an
/// argument-free constructor next to [`session_routes_with`] reads like the
/// default: `start_server` called it, and a default `cru web` silently refused
/// `http://localhost:11434` — the local-Ollama path — until someone noticed.
/// Production callers have a bind address and must pass it.
pub fn session_routes_fail_closed() -> Router<AppState> {
    session_routes_with(EndpointPolicy::for_bind_host(UNKNOWN_BIND))
}

/// Session routes carrying `policy`, which `create_session` reads when
/// validating a custom provider endpoint.
pub fn session_routes_with(policy: EndpointPolicy) -> Router<AppState> {
    Router::new()
        .route("/api/session", post(create_session))
        .route("/api/session/list", get(list_sessions))
        .route("/api/sessions/search", get(search_sessions))
        .route("/api/session/{id}", get(get_session).delete(delete_session))
        .route("/api/session/{id}/history", get(get_session_history))
        .route("/api/session/{id}/pause", post(pause_session))
        .route("/api/session/{id}/resume", post(resume_session))
        .route("/api/session/{id}/end", post(end_session))
        .route("/api/session/{id}/archive", post(archive_session))
        .route("/api/session/{id}/unarchive", post(unarchive_session))
        .route("/api/session/{id}/cancel", post(cancel_session))
        .route("/api/session/{id}/models", get(list_models))
        .route("/api/session/{id}/model", post(switch_model))
        .route("/api/session/{id}/modes", get(list_modes))
        .route("/api/session/{id}/status", get(session_status))
        .route("/api/session/{id}/kilns/connect", post(connect_kiln))
        .route("/api/session/{id}/kilns/disconnect", post(disconnect_kiln))
        .route("/api/session/{id}/workspace", put(set_workspace))
        .route("/api/session/{id}/mode", post(set_mode).get(get_mode))
        .route("/api/session/{id}/title", put(set_session_title))
        .route("/api/session/{id}/auto-title", post(auto_title))
        .route("/api/providers", get(list_providers))
        // Config knobs register themselves in `session_config`, next to their
        // handlers: fifteen route pairs is 60 lines that pushed this file past the
        // 1000-line budget, and the group has no reason to be spelled out here.
        .merge(super::session_config::config_routes())
        // Review lives inside this group, not beside it: bearer auth, the host
        // guard, the CORS allowlist, the body limit and the security headers
        // are applied to the session router, and a separate group is how the
        // review surface would quietly stop inheriting them.
        .route("/api/session/{id}/review/hunks", get(review::list_hunks))
        .route("/api/session/{id}/review/rebase", post(review::rebase))
        .route("/api/session/{id}/review/state", post(review::set_state))
        .route("/api/session/{id}/review/comment", post(review::comment))
        .route(
            "/api/session/{id}/review/comment/{comment_id}/resolve",
            post(review::resolve_comment),
        )
        .route("/api/session/{id}/export", post(export_session))
        .route("/api/session/{id}/command", post(execute_command))
        // Session-independent: the command set is static, so the composer can
        // fetch it once instead of per session.
        .route("/api/commands", get(list_commands))
        .layer(Extension(policy))
}
#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    #[serde(default = "default_session_type")]
    session_type: String,
    /// Kiln for the session; omitted → daemon default (home kiln).
    kiln: Option<PathBuf>,
    /// Additional knowledge kilns to attach at creation.
    #[serde(default)]
    connect_kilns: Vec<PathBuf>,
    workspace: Option<PathBuf>,
    /// LLM provider (e.g., "ollama", "openai", "anthropic")
    provider: Option<String>,
    /// Model name (e.g., "llama3.2", "gpt-4o", "claude-3-5-sonnet")
    model: Option<String>,
    /// Custom endpoint URL (optional, for self-hosted models)
    endpoint: Option<String>,
    /// "internal" (default) or "acp"
    agent_type: Option<String>,
    /// ACP agent profile name (e.g. "claude", "opencode"); required when agent_type == "acp"
    agent_name: Option<String>,
    /// Isolation override: absent → resolve normally; `false` → no container
    /// even if the project has one; `true`, a profile name or an environment
    /// object → override. Forwarded to the daemon untouched — the vocabulary
    /// belongs to the plugin that resolves it, and an unknown profile comes
    /// back as `-32602`, which `daemon_err` turns into a 422.
    isolation: Option<serde_json::Value>,
}

fn default_session_type() -> String {
    "chat".to_string()
}

/// Escape hatch for a NON-loopback bind: an operator who deliberately exposes
/// `cru web` on a LAN and still wants sessions pointed at the server's own
/// Ollama. It only ever adds permission — with a loopback bind it is redundant,
/// because [`EndpointPolicy`] already allows loopback there.
const ALLOW_LOOPBACK_ENDPOINTS_ENV: &str = "CRUCIBLE_WEB_ALLOW_LOOPBACK_ENDPOINTS";

/// Split from the env read so the parsing rule is testable without mutating
/// process env (which races under parallel test runs).
fn loopback_opt_in(raw: Option<&str>) -> bool {
    raw.map(str::trim)
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

fn loopback_env_override() -> bool {
    loopback_opt_in(std::env::var(ALLOW_LOOPBACK_ENDPOINTS_ENV).ok().as_deref())
}

/// Whether this server may hand a provider an endpoint on the machine's own
/// loopback — `http://localhost:11434`, the Ollama default and the product's
/// headline local-LLM path.
///
/// Decided by the bind address, per §W4 of the hardening plan ("Loopback stays
/// allowed only when the bind is loopback"). A loopback bind means the only
/// browser that can reach this server is already on this machine, so naming
/// this machine's loopback grants it nothing it did not already have. A LAN or
/// public bind is the different case: there the browser is a confused deputy
/// for everything the *server* can reach, and the server's own loopback
/// services are exactly what the browser cannot reach on its own.
///
/// Nothing else about the endpoint check is configurable — link-local, private,
/// CGNAT and the metadata address stay refused under every policy.
#[derive(Clone, Copy, Debug)]
pub struct EndpointPolicy {
    allow_loopback: bool,
}

impl EndpointPolicy {
    /// The policy for a server bound to `bind_host`, plus the env escape hatch.
    pub fn for_bind_host(bind_host: &str) -> Self {
        Self::from_bind(bind_host, loopback_env_override())
    }

    /// Split from the env read so the rule is testable without mutating process
    /// env.
    fn from_bind(bind_host: &str, env_override: bool) -> Self {
        Self {
            allow_loopback: bind_is_loopback(bind_host) || env_override,
        }
    }
}

/// What [`session_routes_fail_closed`] passes: no bind address is known, so the bind cannot
/// be shown to be loopback and the policy fails closed.
const UNKNOWN_BIND: &str = "";

/// Whether binding to `bind_host` means "this machine only".
///
/// `localhost` is loopback by RFC 6761 and is what `[server] host` carries by
/// default; `0.0.0.0` and `::` are unspecified, not loopback, and a name we
/// cannot resolve here is treated as reachable from elsewhere.
fn bind_is_loopback(bind_host: &str) -> bool {
    let host = bind_host
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// The IPv4 address an IPv6 address actually reaches, if any.
///
/// `::ffff:a.b.c.d` (v4-mapped), `::a.b.c.d` (v4-compatible),
/// `::ffff:0:a.b.c.d` (v4-translated), `2002:a.b.c.d::/16` (6to4) and
/// `64:ff9b::a.b.c.d` (NAT64) are all ways of writing an IPv4 destination, so
/// they have to be judged as that IPv4 address rather than as an opaque v6
/// literal.
fn embedded_ipv4(v6: Ipv6Addr) -> Option<Ipv4Addr> {
    fn from_halves(a: u16, b: u16) -> Option<Ipv4Addr> {
        Some(Ipv4Addr::from(((a as u32) << 16) | b as u32))
    }
    match v6.segments() {
        [0x2002, a, b, ..] => from_halves(a, b),
        // NAT64 well-known prefix 64:ff9b::/96. The local-use prefix
        // 64:ff9b:1::/48 and the other RFC 6052 embeddings scatter the IPv4
        // bytes across the address; they are not decoded here, and are instead
        // refused wholesale by the 2000::/3 allow-list in `is_internal_target`.
        [0x0064, 0xff9b, 0, 0, 0, 0, a, b] => from_halves(a, b),
        // ::ffff:0:a.b.c.d (v4-translated, RFC 2765)
        [0, 0, 0, 0, 0xffff, 0, a, b] => from_halves(a, b),
        // ::ffff:a.b.c.d and ::a.b.c.d
        _ => v6.to_ipv4(),
    }
}

/// Whether an address is somewhere `cru web` must never be talked into dialing:
/// anything that is not a globally routable unicast destination. Written as a
/// deny of everything non-global rather than a list of "private" ranges, so
/// oddities (0.0.0.0/8, CGNAT, 240/4, multicast) fail closed too.
///
/// This is the ONE place the decision is made — literals and resolved addresses
/// both come through here.
fn is_internal_target(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                // 169.254.0.0/16 — includes the cloud metadata address 169.254.169.254
                || v4.is_link_local()
                || v4.is_multicast()
                // 0.0.0.0/8 "this host" (0.x reaches localhost on Linux); also
                // subsumes `is_unspecified`, as `a >= 240` below subsumes
                // `is_broadcast` — spelling either out again would pad the
                // deny-list with clauses that can never fire.
                || a == 0
                || (a == 100 && (64..128).contains(&b)) // 100.64.0.0/10 CGNAT
                || (a == 192 && b == 0 && v4.octets()[2] == 0) // 192.0.0.0/24 IETF assignments
                || (a == 198 && b & 0xfe == 18) // 198.18.0.0/15 benchmarking
                || a >= 240 // 240.0.0.0/4 reserved
        }
        IpAddr::V6(v6) => {
            // An address that encodes an IPv4 destination is that destination.
            if let Some(v4) = embedded_ipv4(v6) {
                return is_internal_target(IpAddr::V4(v4));
            }
            let segments = v6.segments();
            // Allow-list rather than a list of "private" prefixes: only global
            // unicast (2000::/3) is a public destination. ::1, ::, fc00::/7
            // unique-local, fe80::/10 link-local, fec0::/10 site-local,
            // ff00::/8 multicast and every other reserved prefix fall outside
            // it and are refused without having to be enumerated — including
            // the RFC 6052 NAT64 encodings this code does not decode.
            segments[0] & 0xe000 != 0x2000 || segments[..2] == [0x2001, 0x0db8] // 2001:db8::/32 documentation
        }
    }
}

fn is_loopback_target(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback() || embedded_ipv4(v6).is_some_and(|v4| v4.is_loopback()),
    }
}

fn reject_internal_target(ip: IpAddr, host: &str, allow_loopback: bool) -> Result<(), WebError> {
    if is_internal_target(ip) && !(allow_loopback && is_loopback_target(ip)) {
        let hint = if is_loopback_target(ip) {
            format!(
                " (loopback endpoints are allowed only on a loopback bind, or with \
                 {ALLOW_LOOPBACK_ENDPOINTS_ENV}=1)"
            )
        } else {
            String::new()
        };
        return Err(WebError::Validation(format!(
            "Endpoint must not target a private/internal address: {host} → {ip}{hint}"
        )));
    }
    Ok(())
}

type ResolvedAddrs = std::io::Result<Vec<IpAddr>>;

async fn resolve_host(host: String, port: u16) -> ResolvedAddrs {
    Ok(tokio::net::lookup_host((host.as_str(), port))
        .await?
        .map(|addr| addr.ip())
        .collect())
}

/// Validate that an endpoint URL is safe to hand to a provider (no SSRF into
/// the machine's own networks).
///
/// What this guarantees: the endpoint's scheme is http(s), and every address
/// the host maps to **at validation time** is a globally routable unicast
/// address. Literal hosts in any encoding the URL parser normalizes (decimal
/// `2130706433`, IPv4-mapped/6to4/NAT64 IPv6) are judged as the address they
/// actually reach, and a hostname is resolved and judged on its answers — all
/// of them, so one internal record in an otherwise public answer set refuses.
///
/// What this does NOT guarantee: that the connection later made to this
/// endpoint goes to a checked address. Resolution here and resolution in the
/// dialer are two separate lookups, so a short-TTL or round-robin record can
/// answer with a public address now and 169.254.169.254 when the provider
/// connects — DNS rebinding, unfixable at this layer. Closing it requires the
/// component that dials (the daemon's provider client) to pin or re-check the
/// address it connects to. Treat this as raising the cost of the attack, not as
/// a boundary. Note also that this check lives in the web layer only: the same
/// endpoint reaches the daemon unvalidated from the TUI or a direct RPC client.
async fn validate_endpoint(endpoint: &str, policy: EndpointPolicy) -> Result<(), WebError> {
    validate_endpoint_with(endpoint, policy.allow_loopback, resolve_host).await
}

async fn validate_endpoint_with<F, Fut>(
    endpoint: &str,
    allow_loopback: bool,
    resolve: F,
) -> Result<(), WebError>
where
    F: FnOnce(String, u16) -> Fut,
    Fut: std::future::Future<Output = ResolvedAddrs>,
{
    let url = reqwest::Url::parse(endpoint)
        .map_err(|e| WebError::Validation(format!("Invalid endpoint URL: {e}")))?;

    match url.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(WebError::Validation(format!(
                "Unsupported URL scheme: {scheme}"
            )));
        }
    }

    let host = url
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or_else(|| WebError::Validation("Endpoint URL must have a host".to_string()))?
        .to_string();

    // `host_str` is the *normalized* host: `http://2130706433` and
    // `http://0x7f.1` are already "127.0.0.1" here, and an IPv6 literal keeps
    // its brackets.
    let literal = host.trim_start_matches('[').trim_end_matches(']').parse();

    let addrs = match literal {
        Ok(ip) => vec![ip],
        Err(_) => {
            let port = url.port_or_known_default().unwrap_or(80);
            // Fail closed: an unresolvable host is not a safe host, it is an
            // unknown one.
            let addrs = resolve(host.clone(), port).await.map_err(|e| {
                WebError::Validation(format!("Endpoint host {host} could not be resolved: {e}"))
            })?;
            if addrs.is_empty() {
                return Err(WebError::Validation(format!(
                    "Endpoint host {host} resolved to no addresses"
                )));
            }
            addrs
        }
    };

    for ip in addrs {
        reject_internal_target(ip, &host, allow_loopback)?;
    }

    Ok(())
}

/// Map a `session.create` daemon error to an HTTP status. An `INVALID_PARAMS`
/// error (JSON-RPC code `-32602` — e.g. an unknown ACP profile or an
/// unparseable provider override, both now resolved daemon-side) is a client
/// error (422), preserving the pre-consolidation behavior where the web
/// validated the profile itself. Anything else is a daemon/transport failure
/// (502).
async fn create_session(
    State(state): State<AppState>,
    Extension(endpoint_policy): Extension<EndpointPolicy>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    if let Some(ref endpoint) = req.endpoint {
        validate_endpoint(endpoint, endpoint_policy).await?;
    }

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
    // of "what is the default agent". No kiln → omitted from the wire so the
    // daemon resolves its default (home kiln).
    let agent_spec = crucible_daemon::rpc_client::SessionAgentSpec {
        agent_name: req.agent_name.clone(),
        // The web's own request type still spells a card `agent_name` (the
        // deprecated alias the daemon keeps for exactly this caller). Setting
        // both fields is INVALID_PARAMS, so this stays None until the web
        // frontend grows a card selector of its own.
        agent_card: None,
        provider: req.provider.clone(),
        provider_key: None,
        model: req.model.clone(),
        endpoint: req.endpoint.clone(),
    };

    let params = crucible_daemon::rpc_client::SessionCreateParams {
        session_type: req.session_type.clone(),
        kiln: req.kiln.clone(),
        workspace: req.workspace.clone(),
        connect_kilns: req.connect_kilns.clone(),
        recording_mode: None,
        recording_path: None,
        agent_type: req.agent_type.clone(),
        isolation: req.isolation.clone(),
    };

    let result = state
        .daemon
        .session_create_with_agent(params, agent_spec)
        .await
        .daemon_err()?;

    // A create response without a usable session_id (protocol drift) would
    // otherwise let subscribe run against an empty id and surface as a confusing
    // downstream error; fail loudly here instead.
    let session_id = result["session_id"].as_str().unwrap_or("");
    if session_id.is_empty() {
        return Err(WebError::Daemon(
            "daemon returned no session_id from session.create".to_string(),
        ));
    }

    state
        .daemon
        .session_subscribe(&[session_id])
        .await
        .daemon_err()?;

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct ListSessionsQuery {
    kiln: Option<PathBuf>,
    workspace: Option<PathBuf>,
    #[serde(rename = "type")]
    session_type: Option<String>,
    state: Option<String>,
    #[serde(default)]
    include_archived: Option<bool>,
}

async fn list_sessions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListSessionsQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .session_list(
            query.kiln.as_deref(),
            query.workspace.as_deref(),
            query.session_type.as_deref(),
            query.state.as_deref(),
            query.include_archived,
        )
        .await
        .daemon_err()?;

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct SearchSessionsQuery {
    q: String,
    kiln: Option<PathBuf>,
    limit: Option<usize>,
}

async fn search_sessions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<SearchSessionsQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let results = state
        .daemon
        .session_search(&query.q, query.kiln.as_deref(), query.limit.or(Some(20)))
        .await
        .daemon_err()?;

    Ok(Json(results))
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state.daemon.session_get(&id).await.daemon_err()?;

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    kiln: PathBuf,
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn get_session_history(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .session_resume_from_storage(&id, &query.kiln, query.limit, query.offset)
        .await
        .daemon_err()?;

    Ok(Json(result))
}

async fn pause_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state.daemon.session_pause(&id).await.daemon_err()?;

    Ok(Json(result))
}

async fn resume_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    // Transparent resume: sessions are always resumable. Try the warm path
    // (session still resident and merely paused); on any failure — ended,
    // evicted, or not in memory — fall back to reloading it from its kiln's
    // storage so an idle session is never a dead end for the UI.
    let result = match state.daemon.session_resume(&id).await {
        Ok(result) => result,
        Err(_) => {
            let kiln = resolve_session_kiln(&state, &id).await?;
            state
                .daemon
                .session_resume_from_storage(&id, std::path::Path::new(&kiln), None, None)
                .await
                .map_err(|e| map_session_not_found(e, &id))?
        }
    };

    let session_id = id.as_str();
    state
        .daemon
        .session_subscribe(&[session_id])
        .await
        .daemon_err()?;

    Ok(Json(result))
}

async fn end_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state.daemon.session_end(&id).await.daemon_err()?;

    state.events.remove_session(&id).await;

    Ok(Json(result))
}

async fn archive_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ArchiveResponse>, WebError> {
    let kiln = resolve_session_kiln(&state, &id).await?;
    state
        .daemon
        .session_archive(&id, std::path::Path::new(&kiln))
        .await
        .map_err(|e| map_session_not_found(e, &id))?;
    state.events.remove_session(&id).await;
    Ok(Json(ArchiveResponse { archived: true }))
}

async fn unarchive_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ArchiveResponse>, WebError> {
    let kiln = resolve_session_kiln(&state, &id).await?;
    state
        .daemon
        .session_unarchive(&id, std::path::Path::new(&kiln))
        .await
        .map_err(|e| map_session_not_found(e, &id))?;
    Ok(Json(ArchiveResponse { archived: false }))
}

async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, WebError> {
    let kiln = resolve_session_kiln(&state, &id).await?;
    state
        .daemon
        .session_delete(&id, std::path::Path::new(&kiln))
        .await
        .map_err(|e| map_session_not_found(e, &id))?;
    state.events.remove_session(&id).await;
    Ok(Json(DeleteResponse { deleted: true }))
}

async fn resolve_session_kiln(state: &AppState, session_id: &str) -> Result<String, WebError> {
    match state.daemon.session_get(session_id).await {
        Ok(session) => {
            return session
                .get("kiln")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
                .ok_or_else(|| WebError::Validation("Session has no kiln path".to_string()));
        }
        Err(e) => {
            let message = e.to_string();
            if !message.contains("Session not found") {
                return Err(WebError::Daemon(message));
            }
        }
    }

    let sessions = state
        .daemon
        .session_list(None, None, None, None, Some(true))
        .await
        .daemon_err()?;

    let kiln = sessions
        .get("sessions")
        .and_then(|value| value.as_array())
        .and_then(|items| {
            items.iter().find_map(|item| {
                let id = item.get("session_id").and_then(|value| value.as_str())?;
                if id == session_id {
                    item.get("kiln")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                } else {
                    None
                }
            })
        });

    kiln.ok_or_else(|| WebError::NotFound(format!("Session not found: {session_id}")))
}

async fn cancel_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CancelledResponse>, WebError> {
    let cancelled = state.daemon.session_cancel(&id).await.daemon_err()?;
    Ok(Json(CancelledResponse { cancelled }))
}

async fn list_models(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ModelsResponse>, WebError> {
    let models = state.daemon.session_list_models(&id).await.daemon_err()?;
    Ok(Json(ModelsResponse { models }))
}

/// The session's modes, forwarded from the daemon unchanged.
///
/// The web layer deliberately adds nothing here: mode labels and ordering are
/// the daemon's, so the TUI and the browser cannot drift into showing
/// different names for the same mode.
async fn list_modes(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<crucible_core::types::mode::SessionModes>, WebError> {
    let modes = state.daemon.session_list_modes(&id).await.daemon_err()?;
    Ok(Json(modes))
}

#[derive(Debug, Deserialize)]
struct SwitchModelRequest {
    model_id: String,
}

async fn switch_model(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SwitchModelRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_switch_model(&id, &req.model_id)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

#[derive(Debug, Deserialize)]
struct SessionKilnRequest {
    kiln: PathBuf,
}

/// Updated session scope, echoed by kiln/workspace mutations.
async fn connect_kiln(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SessionKilnRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let scope = state
        .daemon
        .session_connect_kiln(&id, &req.kiln)
        .await
        .daemon_err()?;
    Ok(Json(scope))
}

async fn disconnect_kiln(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SessionKilnRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let scope = state
        .daemon
        .session_disconnect_kiln(&id, &req.kiln)
        .await
        .daemon_err()?;
    Ok(Json(scope))
}

#[derive(Debug, Deserialize)]
struct SetWorkspaceRequest {
    /// Omitted/null → detach (workspace falls back to the kiln).
    workspace: Option<PathBuf>,
}

async fn set_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetWorkspaceRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let scope = state
        .daemon
        .session_set_workspace(&id, req.workspace.as_deref())
        .await
        .daemon_err()?;
    Ok(Json(scope))
}

#[derive(Debug, Deserialize)]
struct SetModeRequest {
    mode: String,
}

/// Set the session mode (normal/plan/auto). The daemon persists it on the
/// agent config and applies it to the live handle; confirmation reaches the
/// UI as a `mode_changed` SSE event.
async fn set_mode(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetModeRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_mode(&id, &req.mode)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

#[derive(Debug, Serialize)]
struct ModeResponse {
    mode: Option<String>,
}

/// Read the session mode. `session.get_mode` has existed all along with no web
/// reader, so the panel could set a mode and then render whatever it last
/// guessed. Exempt from gate A2e by design (`mode` is not a `config/` knob), so
/// nothing would have failed if this stayed missing.
async fn get_mode(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ModeResponse>, WebError> {
    let mode = state.daemon.session_get_mode(&id).await.daemon_err()?;
    Ok(Json(ModeResponse { mode }))
}

#[derive(Debug, Deserialize)]
struct SetTitleRequest {
    title: String,
}

async fn set_session_title(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetTitleRequest>,
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
    // Get session metadata to find kiln path
    let session = state.daemon.session_get(&id).await.daemon_err()?;
    let kiln_str = session.get("kiln").and_then(|v| v.as_str()).unwrap_or("");

    if kiln_str.is_empty() {
        return Err(WebError::Validation(
            "Session has no kiln path, cannot export".to_string(),
        ));
    }

    let kiln = std::path::Path::new(kiln_str);

    // Build session directory path (mirrors FileSessionStorage::session_dir_by_id)
    let session_dir = if crucible_core::config::is_crucible_home(kiln) {
        kiln.join("sessions").join(&id)
    } else {
        kiln.join(".crucible").join("sessions").join(&id)
    };

    // Try to render markdown from persisted session events
    let markdown = match state
        .daemon
        .session_render_markdown(&session_dir, Some(true), None, Some(true), None)
        .await
    {
        Ok(md) => md,
        Err(_) => {
            // Fallback: construct basic markdown from session metadata
            let title = session
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled Session");
            let started_at = session
                .get("started_at")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let model = session
                .get("agent_model")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let state_str = session
                .get("state")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

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

#[derive(Debug, Deserialize)]
struct ListProvidersQuery {
    kiln: Option<PathBuf>,
}

/// Served through the SWR catalog cache — provider probing takes ~0.7s and
/// must not gate every splash render. Shape: `{providers: [ProviderInfo]}`.
async fn list_providers(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListProvidersQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let providers = crate::services::catalog::providers_value(&state, query.kiln.as_deref())
        .await
        .daemon_err()?;
    Ok(Json(serde_json::json!({ "providers": providers })))
}

mod review;

#[cfg(test)]
mod tests;
