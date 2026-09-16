use super::session_commands::{
    __path_execute_command, __path_list_commands, execute_command, list_commands,
};
use super::session_status::{__path_session_status, session_status};
use crate::routes::helpers::ModelsResponse;
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
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

/// Read a present `null` as a present-and-null value rather than as an absent
/// key.
///
/// `Option<Option<T>>` alone cannot tell the two apart on the way in: serde
/// reads a `null` into the outer `None`, which then writes no key at all. The
/// three session shapes differ by exactly that distinction, so it has to
/// survive the round trip.
fn present_or_null<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// One session, as every session route answers it.
///
/// Three daemon methods build three different objects for one entity.
/// `session.create` sends no `started_at` and no `title`. `session.get` nests
/// the model under `agent` and sends no `event_count`, `last_activity` or
/// `archived`. `session.list` sends the model flat and sends all three. This
/// declares the union once, so a client reads one type instead of reconciling
/// three. The divergence is a daemon bug (`server/session/list.rs:155` against
/// `:302`); this route does not fix it.
///
/// A field that one shape omits and another sends as `null` carries
/// `Option<Option<T>>`. The outer level is "the daemon wrote no key", the
/// inner one is "the key is null". Both spellings then reach the browser as
/// the daemon wrote them, which is what keeps this projection lossless.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(super) struct SessionRow {
    /// The session id. The wire name is `session_id`, never `id`.
    session_id: String,
    /// The session type prefix, such as `chat`.
    #[serde(rename = "type")]
    session_type: String,
    /// Every kiln the session can query, by registry name.
    kilns: Vec<String>,
    /// The session's working directory. `null` is a session with no workspace.
    workspace: Option<String>,
    /// The session state, such as `active` or `paused`.
    state: String,
    /// When the session started. `session.create` does not send it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    started_at: Option<String>,
    /// The session title. `session.create` does not send it.
    #[serde(
        default,
        deserialize_with = "present_or_null",
        skip_serializing_if = "Option::is_none"
    )]
    title: Option<Option<String>>,
    /// The resolved model. `session.get` does not send it; read `agent.model`
    /// there.
    #[serde(
        default,
        deserialize_with = "present_or_null",
        skip_serializing_if = "Option::is_none"
    )]
    agent_model: Option<Option<String>>,
    /// The last event's time. Only `session.list` sends it.
    #[serde(
        default,
        deserialize_with = "present_or_null",
        skip_serializing_if = "Option::is_none"
    )]
    last_activity: Option<Option<String>>,
    /// How many events the transcript holds. Only `session.list` sends it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    event_count: Option<u64>,
    /// Whether the session is archived. Only `session.list` sends it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    archived: Option<bool>,
    /// The parent of a delegated session. `session.create` does not send it.
    #[serde(
        default,
        deserialize_with = "present_or_null",
        skip_serializing_if = "Option::is_none"
    )]
    parent_session_id: Option<Option<String>>,
    /// The session this one continues. Only `session.get` sends it.
    #[serde(
        default,
        deserialize_with = "present_or_null",
        skip_serializing_if = "Option::is_none"
    )]
    continued_from: Option<Option<String>>,
    /// The session's agent record. Only `session.get` sends it.
    #[serde(
        default,
        deserialize_with = "present_or_null",
        skip_serializing_if = "Option::is_none"
    )]
    agent: Option<Option<SessionAgentRow>>,
    /// How the session records its transcript. `session.get` sends it only
    /// when the session has a recording mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recording_mode: Option<String>,
}

/// The agent record `session.get` nests inside a session.
///
/// Two fields are named because a client reads them: `model`, which
/// `session.list` sends flat as `agent_model`, and `mode`. Every other field
/// of the daemon's record rides in `rest`, so this projection narrows what the
/// document describes without dropping anything from the reply.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(super) struct SessionAgentRow {
    /// The model this session's agent runs.
    model: String,
    /// The session mode id. Absent when the session is in the normal mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    /// Every other field of the daemon's agent record, carried verbatim.
    #[serde(flatten)]
    #[schema(value_type = Object)]
    rest: serde_json::Map<String, serde_json::Value>,
}

/// What `GET /api/session/list` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct SessionListResponse {
    sessions: Vec<SessionRow>,
    /// How many sessions the reply carries.
    total: usize,
}

/// One transcript line that matched a session search.
///
/// Not a session: the daemon answers the line it matched on, so a caller that
/// wants the session reads `session_id` and asks for it.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct SessionSearchMatch {
    session_id: String,
    /// The 1-based line of the transcript. `0` marks a title match on a
    /// session whose transcript has not reached disk yet.
    line: u64,
    /// The matched line, truncated to 100 characters.
    context: String,
}

/// What `GET /api/sessions/search` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct SessionSearchResponse {
    matches: Vec<SessionSearchMatch>,
    /// How many matches the reply carries.
    total: usize,
    /// Why the search looked at nothing, when it looked at nothing.
    ///
    /// The daemon writes it for a search with no kiln scope
    /// (`server/session/list.rs:206`), and only then. An unscoped search is
    /// the one case where an empty result is not a statement about the
    /// corpus, so the sentence has to reach the caller.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

/// One persisted session event, as `session.resume_from_storage` replays it.
///
/// The same envelope the SSE stream carries, but this route replays whatever
/// the transcript holds — including an event name a newer daemon minted — so
/// `data` stays untyped here rather than narrowing to [`crate::events::ChatEvent`].
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct SessionHistoryEvent {
    /// The envelope kind. Always `event`.
    #[serde(rename = "type")]
    message_type: String,
    session_id: String,
    /// The event name, such as `user_message` or `text_delta`.
    event: String,
    /// The event payload. Its shape follows `event`.
    data: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seq: Option<u64>,
}

/// What `GET /api/session/{id}/history` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct SessionHistoryResponse {
    session_id: String,
    /// The session type prefix.
    #[serde(rename = "type")]
    session_type: String,
    state: String,
    kilns: Vec<String>,
    /// The page of events the query asked for.
    history: Vec<SessionHistoryEvent>,
    /// How many events the whole transcript holds, for paging.
    total_events: usize,
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
/// that resumed the session.
///
/// The warm path answers the state change. The cold path reloads the session
/// from the store and answers its history, because that call is also what
/// `GET /api/session/{id}/history` serves.
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

/// Session routes for a harness with no bind address — the fail-closed policy.
///
/// Named for what it is rather than offered as `session_routes()`, because an
/// argument-free constructor next to [`session_routes_with`] reads like the
/// default: `start_server` called it, and a default `cru web` silently refused
/// `http://localhost:11434` — the local-Ollama path — until someone noticed.
/// Production callers have a bind address and must pass it.
pub fn session_routes_fail_closed() -> OpenApiRouter<AppState> {
    session_routes_with(EndpointPolicy::for_bind_host(UNKNOWN_BIND))
}

/// Session routes carrying `policy`, which `create_session` reads when
/// validating a custom provider endpoint.
pub fn session_routes_with(policy: EndpointPolicy) -> OpenApiRouter<AppState> {
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
        .routes(routes!(switch_model))
        .routes(routes!(list_modes))
        .routes(routes!(list_knobs))
        .routes(routes!(session_status))
        .routes(routes!(connect_kiln))
        .routes(routes!(disconnect_kiln))
        .routes(routes!(set_workspace))
        .routes(routes!(set_mode, get_mode))
        .routes(routes!(set_session_title))
        .routes(routes!(auto_title))
        .routes(routes!(list_providers))
        // Config knobs register themselves in `session_config`, next to their
        // handlers: fifteen route pairs is 60 lines that pushed this file past the
        // 1000-line budget, and the group has no reason to be spelled out here.
        .merge(super::session_config::config_routes())
        // Review lives inside this group, not beside it: bearer auth, the host
        // guard, the CORS allowlist, the body limit and the security headers
        // are applied to the session router, and a separate group is how the
        // review surface would quietly stop inheriting them.
        .routes(routes!(review::list_hunks))
        .routes(routes!(review::rebase))
        .routes(routes!(review::set_state))
        .routes(routes!(review::set_states))
        .routes(routes!(review::undo_reject))
        .routes(routes!(review::comment))
        .routes(routes!(review::resolve_comment))
        .routes(routes!(export_session))
        .routes(routes!(execute_command))
        // Session-independent: the command set is static, so the composer can
        // fetch it once instead of per session.
        .routes(routes!(list_commands))
        .layer(Extension(policy))
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
    /// Custom endpoint URL (optional, for self-hosted models)
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
    #[schema(value_type = Option<Object>)]
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
#[utoipa::path(
    post,
    path = "/api/session",
    request_body = CreateSessionRequest,
    responses(
        (status = 200, body = SessionRow),
        (status = 422, description = "The request named an endpoint, an agent type or a card the server refuses"),
        (status = 502, description = "The daemon could not create the session"),
    )
)]
async fn create_session(
    State(state): State<AppState>,
    Extension(endpoint_policy): Extension<EndpointPolicy>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<SessionRow>, WebError> {
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
    // of "what is the default agent". Kilns are forwarded verbatim, empty set
    // included — see `CreateSessionRequest::kilns`.
    let agent_spec = crucible_daemon::rpc_client::SessionAgentSpec {
        agent_name: req.agent_name.clone(),
        agent_card: req.agent_card.clone(),
        provider: req.provider.clone(),
        provider_key: None,
        model: req.model.clone(),
        endpoint: req.endpoint.clone(),
        ..Default::default()
    };

    let params = crucible_daemon::rpc_client::SessionCreateParams {
        session_type: req.session_type.clone(),
        kilns: req.kilns.clone(),
        workspace: req.workspace.clone(),
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

    Ok(Json(daemon_shape(result, "session.create")?))
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
        (status = 200, body = SessionListResponse),
        (status = 422, description = "The `kiln` query carried a path rather than a registry name"),
        (status = 502, description = "The daemon could not list the sessions"),
    )
)]
async fn list_sessions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListSessionsQuery>,
) -> Result<Json<SessionListResponse>, WebError> {
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

    Ok(Json(daemon_shape(result, "session.list")?))
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

    Ok(Json(daemon_shape(results, "session.search")?))
}

#[utoipa::path(
    get,
    path = "/api/session/{id}",
    params(("id" = String, Path, description = "The session to read")),
    responses(
        (status = 200, body = SessionRow),
        (status = 502, description = "The daemon could not read the session"),
    )
)]
async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SessionRow>, WebError> {
    let result = state.daemon.session_get(&id).await.daemon_err()?;

    Ok(Json(daemon_shape(result, "session.get")?))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct HistoryQuery {
    /// How many events to return.
    limit: Option<usize>,
    /// How many events to skip.
    offset: Option<usize>,
}

#[utoipa::path(
    get,
    path = "/api/session/{id}/history",
    params(("id" = String, Path, description = "The session to replay"), HistoryQuery),
    responses(
        (status = 200, body = SessionHistoryResponse),
        (status = 502, description = "The daemon could not read the transcript"),
    )
)]
async fn get_session_history(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<HistoryQuery>,
) -> Result<Json<SessionHistoryResponse>, WebError> {
    let result = state
        .daemon
        .session_resume_from_storage(&id, query.limit, query.offset)
        .await
        .daemon_err()?;

    Ok(Json(daemon_shape(result, "session.resume_from_storage")?))
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
    // Transparent resume: sessions are always resumable. Try the warm path
    // (session still resident and merely paused); on any failure — ended,
    // evicted, or not in memory — fall back to reloading it from the daemon's
    // session store so an idle session is never a dead end for the UI.
    let reply = match state.daemon.session_resume(&id).await {
        Ok(result) => ResumeSessionResponse::Live(daemon_shape(result, "session.resume")?),
        Err(_) => {
            let result = state
                .daemon
                .session_resume_from_storage(&id, None, None)
                .await
                .map_err(|e| map_session_not_found(e, &id))?;
            ResumeSessionResponse::Restored(daemon_shape(result, "session.resume_from_storage")?)
        }
    };

    let session_id = id.as_str();
    state
        .daemon
        .session_subscribe(&[session_id])
        .await
        .daemon_err()?;

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

    state.events.remove_session(&id).await;

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
    state.events.remove_session(&id).await;
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
    state.events.remove_session(&id).await;
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

/// How much review a mode asks for before the agent writes.
///
/// Mirrors `crucible_core::types::mode::ReviewPolicy`, which carries no
/// schema: `crucible-core` takes no utoipa dependency, and a client that
/// renders a mode chip has to know the three values it can read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum ReviewPolicyRow {
    /// No gate at all.
    None,
    /// The review queue surfaces at turn end; nothing ever blocks.
    PostTurn,
    /// A writing tool call waits while its target has unreviewed hunks.
    PreWrite,
}

impl From<crucible_core::types::mode::ReviewPolicy> for ReviewPolicyRow {
    fn from(policy: crucible_core::types::mode::ReviewPolicy) -> Self {
        use crucible_core::types::mode::ReviewPolicy;
        match policy {
            ReviewPolicy::None => Self::None,
            ReviewPolicy::PostTurn => Self::PostTurn,
            ReviewPolicy::PreWrite => Self::PreWrite,
        }
    }
}

/// One mode a session may enter.
///
/// Mirrors `crucible_core::types::mode::ModeDescriptor` field for field, for
/// the reason [`ReviewPolicyRow`] gives.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct ModeRow {
    /// The mode id, such as `plan`.
    id: String,
    /// The label to draw.
    name: String,
    description: Option<String>,
    /// An emoji or an icon name.
    icon: Option<String>,
    /// A hex colour.
    color: Option<String>,
    /// The review this mode asks for, already degraded to what this session's
    /// agent can enforce.
    review_policy: ReviewPolicyRow,
}

impl From<crucible_core::types::mode::ModeDescriptor> for ModeRow {
    fn from(mode: crucible_core::types::mode::ModeDescriptor) -> Self {
        Self {
            id: mode.id,
            name: mode.name,
            description: mode.description,
            icon: mode.icon,
            color: mode.color,
            review_policy: mode.review_policy.into(),
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
/// [`ReviewPolicyRow`] gives.
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

#[derive(Debug, Deserialize, ToSchema)]
struct SwitchModelRequest {
    model_id: String,
}

#[utoipa::path(
    post,
    path = "/api/session/{id}/model",
    params(("id" = String, Path, description = "The session whose model to switch")),
    request_body = SwitchModelRequest,
    responses(
        (status = 200, body = OkResponse),
        (status = 502, description = "The daemon could not switch the model"),
    )
)]
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

/// The body of `POST /connect_kiln` and `POST /disconnect_kiln`.
///
/// Named without the `Session` prefix its siblings drop, because the daemon's
/// RPC client declares a `SessionKilnRequest` that is a different shape going
/// the other way: that one is `Serialize` and carries `session_id`, this one is
/// `Deserialize` and takes the id from the URL path.
#[derive(Debug, Deserialize, ToSchema)]
struct KilnRequest {
    /// The kiln's registry NAME, validated on the way in — a browser that sent
    /// a path gets a 422 rather than a session attached to a directory the
    /// registration floor never saw.
    #[schema(value_type = String)]
    kiln: crucible_core::config::KilnName,
}

/// Updated session scope, echoed by kiln/workspace mutations.
#[utoipa::path(
    post,
    path = "/api/session/{id}/kilns/connect",
    params(("id" = String, Path, description = "The session to attach the kiln to")),
    request_body = KilnRequest,
    responses(
        (status = 200, body = SessionScopeResponse),
        (status = 422, description = "The body named a path rather than a registry name"),
        (status = 502, description = "The daemon could not attach the kiln"),
    )
)]
async fn connect_kiln(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<KilnRequest>,
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
    request_body = KilnRequest,
    responses(
        (status = 200, body = SessionScopeResponse),
        (status = 422, description = "The body named a path rather than a registry name"),
        (status = 502, description = "The daemon could not detach the kiln"),
    )
)]
async fn disconnect_kiln(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<KilnRequest>,
) -> Result<Json<SessionScopeResponse>, WebError> {
    let scope = state
        .daemon
        .session_disconnect_kiln(&id, &req.kiln)
        .await
        .daemon_err()?;
    Ok(Json(daemon_shape(scope, "session.disconnect_kiln")?))
}

#[derive(Debug, Deserialize, ToSchema)]
struct SetWorkspaceRequest {
    /// Omitted/null → detach: the session is then left with no workspace.
    #[schema(value_type = Option<String>)]
    workspace: Option<PathBuf>,
}

#[utoipa::path(
    put,
    path = "/api/session/{id}/workspace",
    params(("id" = String, Path, description = "The session whose workspace to set")),
    request_body = SetWorkspaceRequest,
    responses(
        (status = 200, body = SessionScopeResponse),
        (status = 502, description = "The daemon could not set the workspace"),
    )
)]
async fn set_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetWorkspaceRequest>,
) -> Result<Json<SessionScopeResponse>, WebError> {
    let scope = state
        .daemon
        .session_set_workspace(&id, req.workspace.as_deref())
        .await
        .daemon_err()?;
    Ok(Json(daemon_shape(scope, "session.set_workspace")?))
}

#[derive(Debug, Deserialize, ToSchema)]
struct SetModeRequest {
    mode: String,
}

/// Set the session mode (normal/plan/auto). The daemon persists it on the
/// agent config and applies it to the live handle; confirmation reaches the
/// UI as a `mode_changed` SSE event.
#[utoipa::path(
    post,
    path = "/api/session/{id}/mode",
    params(("id" = String, Path, description = "The session whose mode to set")),
    request_body = SetModeRequest,
    responses(
        (status = 200, body = OkResponse),
        (status = 502, description = "The daemon could not set the mode"),
    )
)]
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

#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct ModeResponse {
    /// The session's mode id. `null` is the normal mode.
    mode: Option<String>,
}

/// Read the session mode. `session.get_mode` has existed all along with no web
/// reader, so the panel could set a mode and then render whatever it last
/// guessed. Exempt from gate A2e by design (`mode` is not a `config/` knob), so
/// nothing would have failed if this stayed missing.
#[utoipa::path(
    get,
    path = "/api/session/{id}/mode",
    params(("id" = String, Path, description = "The session whose mode to read")),
    responses(
        (status = 200, body = ModeResponse),
        (status = 502, description = "The daemon could not read the mode"),
    )
)]
async fn get_mode(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ModeResponse>, WebError> {
    let mode = state.daemon.session_get_mode(&id).await.daemon_err()?;
    Ok(Json(ModeResponse { mode }))
}

#[derive(Debug, Deserialize, ToSchema)]
struct SetTitleRequest {
    title: String,
}

#[utoipa::path(
    put,
    path = "/api/session/{id}/title",
    params(("id" = String, Path, description = "The session to rename")),
    request_body = SetTitleRequest,
    responses(
        (status = 200, body = OkResponse),
        (status = 502, description = "The daemon could not set the title"),
    )
)]
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

mod review;

#[cfg(test)]
mod search_scope_tests;
#[cfg(test)]
mod shape_tests;
#[cfg(test)]
mod tests;
