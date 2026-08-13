use crate::assets::static_routes;
use crate::middleware::auth::{
    bearer_auth, host_guard, localhost_only_shell_auth, remote_shell_active, resolve_api_key,
    websocket_origin_guard, ApiKeyState, HostPolicy, ShellGateState,
};
use crate::routes::{
    agents_routes, auth_routes, canvas_routes, chat_routes, config_routes, fs_routes,
    health_routes, kiln_routes, layout_routes, mcp_routes, plugin_routes, project_routes,
    scm_routes, search_routes, session_routes_with, shell_routes, skills_routes, terminal_routes,
    webhook_routes, EndpointPolicy,
};
use crate::services::daemon;
use crate::{Result, WebError};
use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderValue, Method};
use axum::middleware;
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;

pub use crucible_core::config::{CliAppConfig, WebConfig};

const MAX_BODY_SIZE_10MB: usize = 10 * 1024 * 1024;

pub async fn start_server(
    web_config: &WebConfig,
    app_config: &CliAppConfig,
    standalone: bool,
) -> Result<()> {
    let mut state = daemon::init_daemon(app_config.clone()).await?;

    // A standalone instance (isolated debug/test daemon) must not share the
    // production layout file — its tab experiments would silently trash the
    // installed instance's restored workspace.
    if standalone {
        state.layout_path = Arc::new(daemon::standalone_layout_path());
        tracing::info!(path = %state.layout_path.display(), "Standalone: using isolated web layout");
    }

    // Pre-fill the slow catalog entries (agent/provider probes, ~0.5-1s each)
    // so the first splash render is served from cache.
    crate::services::catalog::warm(state.clone());

    // Wildcard CORS is dangerous here because `/api/shell/exec` can execute host shell commands.
    // Restricting origins prevents arbitrary websites from triggering command execution via browsers.
    // Shared allow-list: CORS uses it for HTTP, and the terminal WebSocket
    // guard reuses it (browsers bypass CORS on WS handshakes).
    let allowed_origins = Arc::new(build_cross_origin_allowlist(web_config));
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list((*allowed_origins).clone()))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let api_key = resolve_api_key(web_config.api_key.as_deref());
    if api_key.is_some() {
        tracing::info!("API Bearer token auth enabled for non-localhost requests");
    } else {
        tracing::warn!("API Bearer token auth is disabled — all requests will be accepted");
    }
    let api_key_state = Arc::new(ApiKeyState::new(
        api_key,
        HostPolicy::from_web_config(web_config),
    ));
    state.remote_shell =
        remote_shell_active(web_config.remote_shell, api_key_state.api_key.is_some());
    if state.remote_shell {
        tracing::warn!(
            "Remote shell/terminal access ENABLED ([web] remote_shell) — \
             authenticated non-localhost clients get a PTY"
        );
    } else if web_config.remote_shell {
        tracing::warn!(
            "remote_shell is set but NO API key is configured — keeping the \
             terminal loopback-only (fail closed)"
        );
    }
    let shell_gate = Arc::new(ShellGateState {
        allow_remote: state.remote_shell,
        // Carried so the gate can demand a credential of its own. It cannot
        // defer to `bearer_auth`: that layer waves loopback callers through,
        // and closing exactly that hole for the PTY is the point.
        credentials: state.remote_shell.then(|| (*api_key_state).clone()),
    });

    // API routes get Bearer auth + shell gets an additional localhost-only
    // check (relaxed only by the fail-closed remote-shell opt-in, which
    // requires an API key so bearer/cookie auth still gates every request).
    let api_routes = Router::new()
        .nest(
            "/api/shell",
            shell_routes().layer(middleware::from_fn_with_state(
                shell_gate.clone(),
                localhost_only_shell_auth,
            )),
        )
        // A PTY is full shell access: localhost gate + an Origin allow-list on
        // the WS upgrade to block Cross-Site WebSocket Hijacking (CORS doesn't
        // apply to WS handshakes).
        .nest(
            "/api/terminal",
            terminal_routes()
                .layer(middleware::from_fn_with_state(
                    shell_gate.clone(),
                    localhost_only_shell_auth,
                ))
                .layer(middleware::from_fn_with_state(
                    allowed_origins.clone(),
                    websocket_origin_guard,
                )),
        )
        .merge(agents_routes())
        .merge(chat_routes())
        .merge(config_routes())
        // Endpoint policy comes from the bind: a loopback bind keeps
        // `http://localhost:11434` (the local-Ollama path) working, a LAN or
        // wildcard bind refuses it. `session_routes_fail_closed()` is the harness form; this
        // must be the `_with` form or the default bind loses local Ollama.
        .merge(session_routes_with(EndpointPolicy::for_bind_host(
            &web_config.host,
        )))
        .merge(project_routes())
        .merge(scm_routes())
        .merge(fs_routes())
        .merge(search_routes())
        .merge(plugin_routes())
        .merge(mcp_routes())
        .merge(kiln_routes())
        .merge(canvas_routes())
        .merge(layout_routes())
        .merge(skills_routes())
        .merge(webhook_routes())
        .with_state(state)
        .layer(middleware::from_fn_with_state(
            api_key_state.clone(),
            bearer_auth,
        ));

    // Health, static, and the login bootstrap are public (no Bearer auth) —
    // login validates the key itself and mints the HttpOnly session cookie.
    let app = api_routes
        .merge(health_routes())
        .merge(auth_routes(api_key_state.clone()))
        .merge(static_routes(web_config.static_dir.as_deref()));

    let app = with_security_headers(with_app_wide_layers(app, api_key_state, cors));

    let addr: SocketAddr = format!("{}:{}", web_config.host, web_config.port)
        .parse()
        .map_err(|e| WebError::Config(format!("Invalid address: {e}")))?;

    tracing::info!("Starting web server on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(WebError::Io)?;

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .map_err(WebError::Io)?;

    Ok(())
}

/// The layers every route gets, whatever it is.
///
/// The Host check is here rather than on a route group because a DNS-rebound
/// page reaching *any* endpoint on the app's own origin is a same-origin
/// oracle: it used to be layered inside `bearer_auth` only, which left
/// `health_routes()` (a liveness/version probe) and the static bundle
/// answering `Host: evil.test`. One check, applied once, covering everything.
///
/// Outermost last, so the Host check runs before CORS and the body limit: a
/// request we do not answer to gets a bare 403, not a CORS-decorated one.
fn with_app_wide_layers(app: Router, api_key_state: Arc<ApiKeyState>, cors: CorsLayer) -> Router {
    app.layer(DefaultBodyLimit::max(MAX_BODY_SIZE_10MB))
        .layer(cors)
        .layer(middleware::from_fn_with_state(api_key_state, host_guard))
}

/// The bind-independent part of the app's Content-Security-Policy.
///
/// Written from what the SolidJS frontend actually does, not from aspiration —
/// a policy that breaks the UI on first load gets deleted, not tightened:
///
/// - `script-src 'self' 'wasm-unsafe-eval'` — the built bundle is module
///   scripts under `/assets`, `dist/index.html` carries no inline script, and
///   nothing in the bundle uses `eval`/`new Function` (checked). WASM is
///   needed: shiki's regex engine and the opt-in local Whisper both compile
///   it, and WebAssembly is blocked without this keyword.
/// - `style-src 'unsafe-inline'` is unavoidable today: CodeMirror (style-mod)
///   and xterm inject `<style>` elements at runtime, mermaid's rendered SVG
///   carries its own `<style>`, and katex/shiki emit `style` attributes. A
///   nonce cannot reach any of them. Removing this needs frontend work
///   (owned by W6), not a server change.
/// - `img-src`/`media-src` stay open to remote schemes because markdown
///   renders web images, and `frame-src` because a canvas link node embeds
///   arbitrary pages in a sandboxed iframe. These are the app's existing
///   posture; narrowing them is a product decision, not a fix.
/// - `object-src 'self'`, not `'none'`: a canvas file card embeds a kiln PDF
///   through `<object type="application/pdf">`. What `'self'` still admits is
///   `/api/file/raw`, and that route hands every document type back as an
///   inert `application/octet-stream` under `Content-Security-Policy: sandbox`
///   — so an injected `<object>` pointed at an agent-written `.svg` gets an
///   opaque origin, not this one.
/// - `connect-src 'self' https:` — `'self'` is what lets the terminal dial its
///   own WebSocket. CSP3 §6.7.2.7 matches a `ws`/`wss` URL against a `'self'`
///   whose scheme is `http`/`https` with the same host and port, and
///   `TerminalPanel` dials exactly `ws(s)://window.location.host`. Verified in
///   a real browser (Chromium 1217, Firefox 1495): with `connect-src 'self'`
///   the same-origin `ws://` handshake raises no violation, while a different
///   port on the same host is blocked — so it is `'self'` doing the matching,
///   not a blanket allowance. This is why the policy names no authority: a
///   proxied deployment (`allowed_hosts = ["crucible.example.com"]`, page on
///   `https://`, socket on `wss://` of the same authority) is covered by the
///   same `'self'`, with nothing to derive and nothing to fall out of step
///   with [`HostPolicy`]. `https:` is separate — the opt-in local
///   transcription model downloads from the Hugging Face CDN. Script execution
///   is unaffected by `connect-src`.
const CONTENT_SECURITY_POLICY: &str = concat!(
    "default-src 'self'; ",
    "base-uri 'self'; ",
    "object-src 'self'; ",
    "frame-ancestors 'none'; ",
    "form-action 'self'; ",
    "script-src 'self' 'wasm-unsafe-eval'; ",
    "worker-src 'self' blob:; ",
    "style-src 'self' 'unsafe-inline'; ",
    "img-src 'self' data: blob: https: http:; ",
    "font-src 'self' data:; ",
    "media-src 'self' data: blob:; ",
    "frame-src https: http:; ",
    "connect-src 'self' https:",
);

/// Apply the security headers to every response.
///
/// `if_not_present`, so a route that needs something stricter keeps it:
/// `/api/file/raw` sandboxes the documents it refuses to serve inline, and
/// that value must survive this layer.
fn with_security_headers(app: Router) -> Router {
    app.layer(SetResponseHeaderLayer::if_not_present(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY),
    ))
    .layer(SetResponseHeaderLayer::if_not_present(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    ))
    // The app is a local tool whose URLs name the operator's machine and
    // ports; nothing it links out to has any business learning them.
    .layer(SetResponseHeaderLayer::if_not_present(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    ))
}

/// Page origins allowed to talk to us *cross-origin*: the `CorsLayer` list, and
/// the static fallback the WebSocket Origin guard consults.
///
/// Not the list of authorities the server answers to — that is [`HostPolicy`],
/// and it is the only thing `Host` is checked against. The two are related but
/// not the same question, so this deliberately does not try to mirror it: a
/// deployment behind a proxy (`allowed_hosts`) is *same*-origin from the
/// browser's point of view, so CORS is never consulted and the WS guard takes
/// its verified-same-origin path. What is left here is the genuinely
/// cross-origin cases — the vite dev server, and whatever the operator names in
/// `CRUCIBLE_CORS_ORIGINS` — plus the app's own origin spellings, which cost
/// nothing and keep a hand-typed `curl -H Origin:` working.
fn build_cross_origin_allowlist(web_config: &WebConfig) -> Vec<HeaderValue> {
    let mut origins = Vec::new();

    let mut add_origin = |origin: &str| {
        let Ok(value) = HeaderValue::from_str(origin) else {
            tracing::warn!(origin, "Skipping invalid CORS origin");
            return;
        };

        if !origins.contains(&value) {
            origins.push(value);
        }
    };

    // Derived from HostPolicy rather than rebuilt, so `allowed_hosts` reaches
    // the CORS list from the same source that decides `Host`. Built
    // independently, the two disagreed: a configured public hostname passed
    // the Host check while the CSP still blocked the terminal, because
    // `connect-src` never learned about it. This covers the bind authority,
    // `127.0.0.1`, and `localhost` — browsers visiting as `http://localhost:PORT`
    // send exactly that Origin, and the bind host renders as 0.0.0.0/127.0.0.1,
    // never `localhost`.
    for authority in HostPolicy::from_web_config(web_config).allowed_authorities() {
        add_origin(&format!("http://{authority}"));
        add_origin(&format!("https://{authority}"));
    }

    if cfg!(debug_assertions) {
        add_origin("http://localhost:5173");
    }

    if let Ok(extra_origins) = std::env::var("CRUCIBLE_CORS_ORIGINS") {
        for origin in extra_origins
            .split(',')
            .map(str::trim)
            .filter(|o| !o.is_empty())
        {
            add_origin(origin);
        }
    }

    origins
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use tower::ServiceExt;

    #[test]
    fn test_max_body_size_is_10mb() {
        assert_eq!(MAX_BODY_SIZE_10MB, 10 * 1024 * 1024);
        assert_eq!(MAX_BODY_SIZE_10MB, 10_485_760);
    }

    /// A config differing from the default only in bind host/port — the two
    /// fields every test here cares about. Spelled with `..default()` so a new
    /// `WebConfig` field never breaks these tests into needing an unrelated
    /// value invented for them.
    fn web_config_bound_to(host: &str, port: u16) -> WebConfig {
        WebConfig {
            enabled: true,
            host: host.to_string(),
            port,
            ..WebConfig::default()
        }
    }

    #[test]
    fn the_cross_origin_allowlist_includes_expected_defaults() {
        let web_config = web_config_bound_to("localhost", 3000);

        let origins = build_cross_origin_allowlist(&web_config);
        let has_origin = |value: &str| {
            let value = HeaderValue::from_str(value).unwrap();
            origins.contains(&value)
        };

        assert!(has_origin("http://localhost:3000"));
        assert!(has_origin("http://127.0.0.1:3000"));
        if cfg!(debug_assertions) {
            assert!(has_origin("http://localhost:5173"));
        }
    }

    /// Regression: real binds are 0.0.0.0/127.0.0.1 — never the literal
    /// `localhost` — but browsers visiting `http://localhost:PORT` send that
    /// Origin. Missing it made the terminal WebSocket guard 403 every
    /// upgrade. (The old test used host="localhost", masking exactly this.)
    #[test]
    fn the_cross_origin_allowlist_allows_localhost_for_real_binds() {
        for host in ["0.0.0.0", "127.0.0.1"] {
            let web_config = web_config_bound_to(host, 3000);
            let origins = build_cross_origin_allowlist(&web_config);
            let localhost = HeaderValue::from_static("http://localhost:3000");
            assert!(
                origins.contains(&localhost),
                "bind host {host} must still allow the localhost Origin"
            );
        }
    }

    #[tokio::test]
    async fn cors_rejects_evil_origin_and_allows_localhost_origin() {
        let web_config = web_config_bound_to("localhost", 3000);

        let cors = CorsLayer::new()
            .allow_origin(AllowOrigin::list(build_cross_origin_allowlist(&web_config)))
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

        let app = Router::new()
            .route("/api/test", get(|| async { "ok" }))
            .layer(cors);

        let disallowed_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/api/test")
                    .header(header::ORIGIN, "https://evil.com")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(disallowed_response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none());

        let allowed_response = app
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/api/test")
                    .header(header::ORIGIN, "http://localhost:3000")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            allowed_response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&HeaderValue::from_static("http://localhost:3000"))
        );
    }

    // -- security headers ----------------------------------------------------

    /// Drive one GET through a minimal app carrying the security-header layer.
    async fn headers_for(app: Router) -> axum::http::HeaderMap {
        with_security_headers(app)
            .oneshot(
                Request::builder()
                    .uri("/api/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .headers()
            .clone()
    }

    fn header_str(headers: &axum::http::HeaderMap, name: axum::http::HeaderName) -> String {
        headers
            .get(&name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string()
    }

    #[tokio::test]
    async fn every_response_carries_the_security_headers() {
        let app = Router::new().route("/api/test", get(|| async { "ok" }));
        let headers = headers_for(app).await;

        assert_eq!(
            header_str(&headers, header::X_CONTENT_TYPE_OPTIONS),
            "nosniff"
        );
        assert_eq!(header_str(&headers, header::REFERRER_POLICY), "no-referrer");
        assert!(!header_str(&headers, header::CONTENT_SECURITY_POLICY).is_empty());
    }

    #[test]
    fn content_security_policy_refuses_inline_and_third_party_script() {
        // The X1 chain ends in script running on the app origin. Nothing but
        // the app's own bundle may execute: no 'unsafe-inline', no
        // 'unsafe-eval', no wildcard host.
        let csp = CONTENT_SECURITY_POLICY;

        assert!(
            csp.contains("script-src 'self' 'wasm-unsafe-eval'"),
            "csp: {csp}"
        );
        assert!(!csp.contains("'unsafe-inline' 'wasm"), "csp: {csp}");
        assert!(!csp.contains("'unsafe-eval'"), "csp: {csp}");
        assert!(csp.contains("base-uri 'self'"), "csp: {csp}");
        assert!(csp.contains("default-src 'self'"), "csp: {csp}");
        // Plugin documents stay same-origin-only: the canvas PDF card needs
        // `<object>`, but no remote host may supply one.
        assert!(csp.contains("object-src 'self'"), "csp: {csp}");
    }

    #[test]
    fn content_security_policy_forbids_framing_the_app() {
        // A framed loopback instance is authenticated (no cookie needed) and
        // therefore clickjackable.
        assert!(CONTENT_SECURITY_POLICY.contains("frame-ancestors 'none'"));
    }

    /// Rewritten from `content_security_policy_allows_the_terminal_websocket`,
    /// which asserted the enumerated `ws://…` origins. Those encoded a CSP2
    /// premise ("`connect-src 'self'` is NOT specified to cover a `ws:` URL
    /// from an `http:` page") that CSP3 §6.7.2.7 reversed, and the enumeration
    /// is what broke the terminal behind a proxy: `allowed_hosts` reached
    /// `HostPolicy` but never reached the CSP, so a page on
    /// `https://crucible.example.com` was refused its own `wss://` socket.
    #[test]
    fn the_csp_names_no_authority_so_it_cannot_disagree_with_the_host_policy() {
        let csp = CONTENT_SECURITY_POLICY;

        // `'self'` is what admits `ws(s)://window.location.host` — whatever
        // authority that turns out to be. Verified in Chromium and Firefox:
        // same-origin ws raises no violation under `connect-src 'self'`, a
        // different port on the same host still does.
        assert!(csp.contains("connect-src 'self' https:"), "csp: {csp}");
        // No enumerated origins: nothing here to fall out of step with the
        // authorities `HostPolicy` accepts, and no `ws:`/`wss:` scheme source
        // that would admit every host on the internet.
        assert!(!csp.contains("ws://"), "csp: {csp}");
        assert!(!csp.contains("wss://"), "csp: {csp}");
        assert!(
            !csp.contains("ws:;") && !csp.contains("wss:;"),
            "csp: {csp}"
        );
    }

    // -- the one Host check ---------------------------------------------------

    /// Regression: the Host check was layered on the API group only, so a
    /// DNS-rebound page still got answers from `health_routes()` and the
    /// static bundle — endpoints on the app's own origin, i.e. an oracle.
    #[tokio::test]
    async fn a_rebound_host_is_refused_on_public_routes_too() {
        let web_config = web_config_bound_to("127.0.0.1", 3000);
        let cors = CorsLayer::new()
            .allow_origin(AllowOrigin::list(build_cross_origin_allowlist(&web_config)));
        let state = Arc::new(ApiKeyState::new(
            None, // auth disabled: rebinding is most dangerous precisely here
            HostPolicy::from_web_config(&web_config),
        ));
        // `/health` and the static index are both public — neither sits behind
        // `bearer_auth`, which is where the Host check used to live.
        let app = with_app_wide_layers(
            Router::new()
                .route("/health", get(|| async { "ok" }))
                .route("/", get(|| async { "index.html" })),
            state,
            cors,
        );

        let status_for = |uri: &'static str, host: &'static str| {
            let app = app.clone();
            async move {
                app.oneshot(
                    Request::builder()
                        .uri(uri)
                        .header(header::HOST, host)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
            }
        };

        for uri in ["/health", "/"] {
            assert_eq!(
                status_for(uri, "evil.test").await,
                axum::http::StatusCode::FORBIDDEN,
                "{uri} must refuse a Host we do not answer to"
            );
            assert_eq!(
                status_for(uri, "127.0.0.1:3000").await,
                axum::http::StatusCode::OK,
                "{uri} must still serve the authority we bound"
            );
        }
    }

    #[tokio::test]
    async fn a_route_that_sets_a_stricter_policy_keeps_it() {
        // `/api/file/raw` sandboxes the documents it refuses to serve inline.
        // The global layer must not overwrite that with the app's own policy.
        let app = Router::new().route(
            "/api/test",
            get(|| async {
                (
                    [(
                        header::CONTENT_SECURITY_POLICY,
                        "sandbox; frame-ancestors 'none'",
                    )],
                    "ok",
                )
            }),
        );
        let headers = headers_for(app).await;

        assert_eq!(
            header_str(&headers, header::CONTENT_SECURITY_POLICY),
            "sandbox; frame-ancestors 'none'"
        );
    }
}
