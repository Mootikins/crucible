//! Authentication and address validation for the web server.
//!
//! [`bearer_auth`] is the single gate every API route passes through. It runs
//! the [`HostPolicy`] check first, because every relaxation after it — auth
//! disabled, loopback caller, same-origin WebSocket — assumes the request was
//! addressed to *us* rather than to a name that merely resolves to us.
//!
//! That check is strict for a loopback caller, who has those relaxations to
//! reach, and gives way to the key check for a caller on another machine, who
//! has none — see [`host_list_is_not_this_requests_defence`].

mod api_key;
mod host;
mod session;
mod shell;

pub use api_key::{
    api_key_path, generate_and_persist_key, resolve_api_key, resolve_api_key_at, verify_api_key,
};
pub use host::{local_names, HostPolicy, HostVerified, InvalidAllowedHost};
pub use session::{sessions_path, SessionStore, SESSION_TTL};
pub use shell::{
    localhost_only_shell_auth, remote_shell_active, websocket_origin_guard, ShellGateState,
};

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

/// Session cookie carrying a derived session token for browser clients (set by
/// `POST /api/auth/login`). HttpOnly, so page JS never touches it, and it
/// rides along on EventSource/SSE requests that cannot set headers.
pub const AUTH_COOKIE: &str = "crucible_auth";

/// Shared state for [`bearer_auth`]: the expected API key, the authorities the
/// server answers to, and the live browser sessions.
#[derive(Clone)]
pub struct ApiKeyState {
    /// The expected API key. `None` means auth is disabled.
    pub api_key: Option<String>,
    /// Authorities accepted in `Host`. Enforced even when `api_key` is `None`:
    /// rebinding is most dangerous precisely when auth is off.
    pub host_policy: HostPolicy,
    /// Sessions minted by `POST /api/auth/login`.
    pub sessions: SessionStore,
}

impl ApiKeyState {
    /// The server's state: sessions are restored from, and written back to,
    /// [`sessions_path`] so a `cru web` restart does not log every browser out.
    pub fn new(api_key: Option<String>, host_policy: HostPolicy) -> Self {
        Self::new_at(api_key, host_policy, sessions_path())
    }

    /// [`ApiKeyState::new`] with an injectable session-file path.
    ///
    /// Tests MUST use this with a `TempDir`-rooted path or `None`: the default
    /// path is the operator's real `~/.config/crucible/sessions.json`, and the
    /// store both READS the live credentials there and WRITES back to them.
    pub fn new_at(
        api_key: Option<String>,
        host_policy: HostPolicy,
        sessions_path: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            api_key,
            host_policy,
            sessions: SessionStore::persistent(sessions_path),
        }
    }
}

/// The one Host check. Returns a refusal to send, or marks the request
/// [`HostVerified`] and returns `None`.
///
/// Reached two ways: [`bearer_auth`] runs it first and unconditionally for the
/// API, and [`host_guard`] is the same check on its own for everything mounted
/// outside that layer — the login bootstrap, `/health`, and the static SPA.
/// A rebound page reaching an unauthenticated endpoint on the app's own origin
/// is a same-origin oracle even when it cannot get past auth, so coverage has
/// to be the whole surface rather than the authenticated part of it.
///
/// `/health` and the static SPA shell sit outside `bearer_auth` entirely, so
/// they get this check only from the [`host_guard`] that `server.rs` wraps the
/// merged app in. Nothing else covers them.
fn enforce_host(state: &ApiKeyState, request: &mut Request<Body>) -> Option<Response> {
    if state.host_policy.accepts(request) {
        request.extensions_mut().insert(HostVerified);
        return None;
    }
    if host_list_is_not_this_requests_defence(state, request) {
        return None;
    }
    tracing::warn!(
        host = ?request.headers().get(header::HOST),
        "Rejecting request addressed to an unexpected Host"
    );
    Some(forbidden_host_response())
}

/// Whether the Host allow-list is the wrong tool for *this* request, and the
/// key check behind it is the right one.
///
/// What rebinding is *for* is the loopback bypass. A page on `evil.test` whose
/// record is rebound to this machine runs in a browser ON it, so its requests
/// arrive from 127.0.0.1, skip auth entirely, and the Host allow-list is the
/// only thing left refusing them. That is worth every name it costs.
///
/// A request from ANOTHER machine has no such shortcut to reach: it presents
/// the API key or [`bearer_auth`] answers 401. Holding it to a list of names
/// buys nothing there, and costs every FQDN the operator's network resolves to
/// this box — `node7`, `node7.lan`, a tailnet name, a CNAME, a name only
/// the phone's resolver knows. None of them can be enumerated up front, which
/// is how a wildcard bind that worked by IP came to 403 by name and read as
/// though it had never left loopback.
///
/// Conditioned on a key being configured. Under `api_key = ""` there is no
/// check behind this one, so the allow-list *is* the defence and keeps its
/// strict reading — the same fail-closed shape as [`remote_shell_active`].
///
/// Fails closed on the peer, and note that this is the mirror image of
/// [`caller_is_loopback`]: there, an unknown peer is safely "not local", while
/// here an unknown peer must not be "remote" — that reading would hand the
/// relaxation to a request whose origin we cannot establish. So the address
/// has to be present and has to be non-loopback. A proxy on this machine is
/// therefore NOT covered (its connections arrive from 127.0.0.1); a public name
/// it forwards still belongs in `allowed_hosts`.
///
/// The request is deliberately NOT marked [`HostVerified`]: that marker means
/// "this authority is provably one of ours", which is the claim this path
/// declines to make, and the WebSocket guard reads it as licence to treat
/// `Origin == Host` as same-origin.
fn host_list_is_not_this_requests_defence(state: &ApiKeyState, request: &Request<Body>) -> bool {
    let peer_is_remote = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .is_some_and(|peer| !peer.0.ip().is_loopback());

    state.api_key.is_some() && peer_is_remote && !caller_is_loopback(request)
}

/// [`enforce_host`] on its own, for routes mounted outside [`bearer_auth`].
pub async fn host_guard(
    state: axum::extract::State<Arc<ApiKeyState>>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    match enforce_host(&state, &mut request) {
        Some(refusal) => refusal,
        None => next.run(request).await,
    }
}

/// Axum middleware that enforces `Authorization: Bearer <key>` on API routes.
///
/// Refuses any request whose `Host` is not an authority this server answers to
/// — unless the request came from another machine, where the key check below
/// is the stronger gate ([`host_list_is_not_this_requests_defence`]). The Host
/// check is first, because every relaxation below it — auth disabled, loopback
/// caller, same-origin WebSocket — assumes the request was addressed to *us*
/// rather than to a name that merely resolves to us.
///
/// Past the Host check, auth is bypassed when:
/// - No API key is configured (auth disabled)
/// - The caller is loopback by [`caller_is_loopback`]'s fail-closed reading —
///   the same one [`localhost_only_shell_auth`] uses
pub async fn bearer_auth(
    state: axum::extract::State<Arc<ApiKeyState>>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    if let Some(refusal) = enforce_host(&state, &mut request) {
        return refusal;
    }

    // No key configured — auth disabled.
    let expected_key = match &state.api_key {
        Some(key) => key,
        None => return next.run(request).await,
    };

    // Localhost bypass: loopback callers skip the token check.
    if caller_is_loopback(&request) {
        return next.run(request).await;
    }

    // Check Authorization header for a valid Bearer token.
    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    if let Some(header) = auth_header {
        return match header.strip_prefix("Bearer ") {
            Some(token) if constant_time_eq(token.as_bytes(), expected_key.as_bytes()) => {
                next.run(request).await
            }
            _ => unauthorized_response(),
        };
    }

    // Browser clients authenticate with the HttpOnly session cookie set by
    // POST /api/auth/login (EventSource cannot set headers; cookies ride along
    // automatically). Header, when present, always wins above. Tokens are
    // deliberately NOT accepted in the URL — query strings leak through
    // browser history, server logs, and referrers. The cookie carries a minted
    // session token, never the API key: the key would then be unrevocable
    // short of rotating it for every client at once.
    let session_ok = auth_cookie_values(request.headers())
        .any(|token| state.sessions.verify(token, expected_key));
    if session_ok {
        return next.run(request).await;
    }

    unauthorized_response()
}

/// Whether the request's *client* — not merely its TCP peer — is loopback.
///
/// The one answer to that question. `bearer_auth` grants an unauthenticated
/// bypass on it and [`localhost_only_shell_auth`] hands out a PTY on it; two
/// implementations of one question means the weaker one is the real policy, so
/// there is exactly this one.
///
/// Fails closed at every step. Loopback is claimed only when the peer address
/// is loopback *and* every hop named by `X-Forwarded-For` is loopback too:
///
/// - No `X-Forwarded-For` at all — nothing is in front of us, so the peer is
///   the client.
/// - A hop we cannot read as an address (unparseable, empty, an obfuscated
///   `_hidden`/`unknown` token, non-ASCII) proves nothing, and behind a proxy
///   the connection always arrives from loopback — so trusting the peer when
///   the header is unreadable would hand the bypass to any remote client that
///   can make one token unparseable.
/// - *Every* hop, not just the first: a proxy that appends rather than
///   replaces leaves the client's own spoofed `127.0.0.1` at the head of the
///   chain, and reading only the head would take the spoof at face value.
pub(super) fn caller_is_loopback<B>(request: &Request<B>) -> bool {
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .is_some_and(|peer| peer.0.ip().is_loopback())
        && forwarded_chain_is_loopback(request.headers())
}

fn forwarded_chain_is_loopback(headers: &HeaderMap) -> bool {
    headers.get_all("x-forwarded-for").iter().all(|value| {
        value.to_str().is_ok_and(|value| {
            value
                .split(',')
                .all(|hop| parse_forwarded_hop(hop.trim()).is_some_and(|ip| ip.is_loopback()))
        })
    })
}

/// Whether the *browser's* leg of this request was TLS — the one question
/// `Secure` on the session cookie may be conditioned on.
///
/// Nothing in this process terminates TLS: `cru web` serves plain HTTP from a
/// `TcpListener`, and encrypted deployments put a terminating proxy in front
/// (`cru tunnel` wrapping cloudflared / tailscale funnel, or an operator's own
/// reverse proxy). So the connection's own scheme is *always* `http` and reading
/// it would mean never minting a `Secure` cookie at all, while the request URI's
/// scheme is no better than the client: absent for origin-form HTTP/1.1, and
/// over h2 it is whatever the client wrote in `:scheme`. The only real evidence
/// is `X-Forwarded-Proto`, from the hop that actually did terminate.
///
/// That header is attacker-supplied unless something in front of us set it, so
/// it counts only when the connection arrived from loopback — the shape every
/// supported terminating proxy has, because it dials this server's local port.
/// Note that is *peer* loopback, not [`caller_is_loopback`]: a tunnel's
/// `X-Forwarded-For` names the real remote client, which is the point of the
/// header, so demanding a loopback forwarded chain here would refuse precisely
/// the deployment this exists for.
///
/// The weaker test is sound because the answer only decorates the `Set-Cookie`
/// in the response to *this very request*. A local process that spoofs the
/// header gets a cookie its own browser then refuses to store; it cannot reach a
/// session any other client holds, since `Set-Cookie` travels to the requester
/// alone. `caller_is_loopback` gates an authentication bypass and needs the
/// strict reading; a cookie attribute does not.
///
/// Within a single header, the **leftmost** hop is the answer: by the same
/// convention `X-Forwarded-For` follows (and RFC 7239 formalises), the first
/// value describes the *client's* connection and later ones describe hops
/// downstream of it. So a legitimate chained deployment —
/// browser --https--> edge --http--> local proxy --> here — arrives as
/// `https, http`, and demanding every hop be `https` would answer "not TLS" for
/// exactly the deployment this exists for, leaving the cookie non-`Secure` over
/// HTTPS with nothing to say so.
///
/// Still fails closed on what is genuinely ambiguous — two separate header
/// instances (a layer behind us could read the other one), an unreadable value,
/// a non-loopback peer — because guessing `Secure` wrong breaks login
/// *silently*: browsers drop a `Secure` cookie that arrives over `http`. Each
/// such refusal is logged, since the residual is a silent security downgrade and
/// an operator otherwise has no way to find it.
///
/// Named for what it reads rather than what it concludes: this is a header
/// believed on peer-loopback alone, sound for decorating a `Set-Cookie` and not
/// sound for authorization. A future "require TLS for the PTY" must not reach
/// for it.
pub(crate) fn forwarded_scheme_is_tls(headers: &HeaderMap, peer: Option<SocketAddr>) -> bool {
    let mut forwarded = headers.get_all("x-forwarded-proto").iter();
    let (Some(proto), None) = (forwarded.next(), forwarded.next()) else {
        // Two instances is a misconfiguration worth naming; none is the ordinary
        // plain-HTTP case and says nothing.
        if headers.get_all("x-forwarded-proto").iter().count() > 1 {
            tracing::warn!(
                "Ignoring X-Forwarded-Proto: the request carries more than one, so which hop \
                 terminated TLS is ambiguous. The session cookie will not be marked Secure."
            );
        }
        return false;
    };
    if !peer.is_some_and(|addr| addr.ip().is_loopback()) {
        tracing::debug!(
            ?peer,
            "Ignoring X-Forwarded-Proto from a non-loopback peer; a terminating proxy dials \
             this server's local port. The session cookie will not be marked Secure."
        );
        return false;
    }
    let Ok(proto) = proto.to_str() else {
        tracing::warn!(
            "Ignoring an X-Forwarded-Proto that is not valid UTF-8. The session cookie will \
             not be marked Secure."
        );
        return false;
    };
    let client_hop = proto.split(',').next().unwrap_or("").trim();
    if client_hop.eq_ignore_ascii_case("https") {
        return true;
    }
    tracing::debug!(
        forwarded_proto = proto,
        "X-Forwarded-Proto reports the client's own hop as plaintext; the session cookie will \
         not be marked Secure."
    );
    false
}

/// A single `X-Forwarded-For` hop: a bare address, or one carrying a port
/// (some proxies emit `127.0.0.1:54321`).
fn parse_forwarded_hop(hop: &str) -> Option<IpAddr> {
    hop.parse::<IpAddr>()
        .ok()
        .or_else(|| hop.parse::<SocketAddr>().ok().map(|addr| addr.ip()))
}

/// Every [`AUTH_COOKIE`] value the request carries.
///
/// Every one, not the first. Cookies are not isolated by port, so any other
/// service — or an XSS — on a sibling `localhost` port can set its own
/// `crucible_auth` for `Path=/`, and the browser then sends both. Reading only
/// the first would let that page lock the real session out. Multiple `Cookie`
/// headers are read for the same reason plus a mundane one: HTTP/2 is entitled
/// to split them.
pub(crate) fn auth_cookie_values(headers: &HeaderMap) -> impl Iterator<Item = &str> + '_ {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|raw| raw.split(';'))
        .filter_map(|pair| {
            let (name, value) = pair.trim().split_once('=')?;
            (name == AUTH_COOKIE).then_some(value)
        })
}

/// Constant-time byte comparison to prevent timing side-channel attacks
/// on bearer token validation.
///
/// Note: The early return on length mismatch leaks the key length via timing.
/// This is acceptable because the auto-generated API key is always 32 chars
/// (high entropy), making length-based attacks impractical.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

fn forbidden_host_response() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": {
                "code": StatusCode::FORBIDDEN.as_u16(),
                "message": "Request Host is not an address this server answers to",
            }
        })),
    )
        .into_response()
}

fn unauthorized_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({
            "error": {
                "code": StatusCode::UNAUTHORIZED.as_u16(),
                "message": "Missing or invalid Authorization: Bearer <key>",
            }
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests;
