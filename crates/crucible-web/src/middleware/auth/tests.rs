//! Tests for the auth middleware: the Host check, the loopback bypass, the
//! session token, and the forwarded-scheme reading behind the `Secure`
//! cookie attribute. Split out of `mod.rs` to keep it under the size budget.

use super::*;
use axum::http::HeaderValue;
use axum::routing::get;
use axum::{middleware, Router};
use tower::ServiceExt;

/// The port every bearer test's [`HostPolicy`] is built for.
const TEST_PORT: u16 = 3000;

/// `new_at(.., None)`, never `new`: `new` reads and rewrites the real
/// `~/.config/crucible/sessions.json`.
fn bearer_state(api_key: Option<String>) -> Arc<ApiKeyState> {
    Arc::new(ApiKeyState::new_at(
        api_key,
        HostPolicy::from_bind("127.0.0.1", TEST_PORT, &[]).expect("bind-derived policy"),
        None,
    ))
}

fn router_with_state(state: Arc<ApiKeyState>) -> Router {
    Router::new()
        .route("/api/test", get(|| async { "ok" }))
        .layer(middleware::from_fn_with_state(state, bearer_auth))
}

fn test_router_with_bearer(api_key: Option<String>) -> Router {
    router_with_state(bearer_state(api_key))
}

/// A request addressed to this server's own authority — what every real
/// client sends. Tests exercising Host validation set their own.
fn local_request() -> axum::http::request::Builder {
    Request::builder()
        .uri("/api/test")
        .header(header::HOST, format!("127.0.0.1:{TEST_PORT}"))
}

#[tokio::test]
async fn bearer_auth_accepts_a_minted_session_cookie() {
    // Rewritten from the old `bearer_auth_accepts_valid_session_cookie`,
    // which put the API key in the cookie — the defect, not the contract.
    let state = bearer_state(Some("secret-key".to_string()));
    let token = state.sessions.issue("secret-key");
    let app = router_with_state(state);

    let req = local_request()
        .header("cookie", format!("other=1; {AUTH_COOKIE}={token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_shadowing_cookie_cannot_lock_out_a_valid_session() {
    // Cookies are not isolated by port, so anything else on a sibling
    // localhost port can set its own `crucible_auth` for `Path=/`. Reading
    // only the first value would hand that page a logout button for the
    // real session. The second header also covers the mundane case: HTTP/2
    // may split `Cookie` across several headers.
    let state = bearer_state(Some("secret-key".to_string()));
    let token = state.sessions.issue("secret-key");
    let app = router_with_state(state);

    let req = local_request()
        .header("cookie", format!("{AUTH_COOKIE}=shadow; other=1"))
        .header("cookie", format!("{AUTH_COOKIE}={token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn bearer_auth_rejects_wrong_session_cookie() {
    let app = test_router_with_bearer(Some("secret-key".to_string()));

    let req = local_request()
        .header("cookie", format!("{AUTH_COOKIE}=wrong"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bearer_auth_header_wins_over_cookie() {
    // A wrong header must not fall through to a valid cookie —
    // explicit credentials fail loudly.
    let state = bearer_state(Some("secret-key".to_string()));
    let token = state.sessions.issue("secret-key");
    let app = router_with_state(state);

    let req = local_request()
        .header("cookie", format!("{AUTH_COOKIE}={token}"))
        .header("authorization", "Bearer wrong")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bearer_auth_no_longer_accepts_url_tokens() {
    // Regression: tokens in URLs leak via history/logs/referrers. The old
    // ?access_token= fallback must stay dead.
    let app = test_router_with_bearer(Some("secret-key".to_string()));

    let req = Request::builder()
        .uri("/api/test?access_token=secret-key&token=secret-key")
        .header(header::HOST, format!("127.0.0.1:{TEST_PORT}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bearer_auth_passes_when_no_key_configured() {
    let app = test_router_with_bearer(None);

    let req = local_request().body(Body::empty()).unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn bearer_auth_passes_with_valid_token() {
    let app = test_router_with_bearer(Some("secret-key".to_string()));

    let mut req = local_request()
        .header("authorization", "Bearer secret-key")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 1], 5000))));

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn bearer_auth_rejects_invalid_token() {
    let app = test_router_with_bearer(Some("secret-key".to_string()));

    let mut req = local_request()
        .header("authorization", "Bearer wrong-key")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 1], 5000))));

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bearer_auth_rejects_missing_header() {
    let app = test_router_with_bearer(Some("secret-key".to_string()));

    let mut req = local_request().body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 1], 5000))));

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bearer_auth_bypasses_for_localhost() {
    let app = test_router_with_bearer(Some("secret-key".to_string()));

    let mut req = local_request().body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

/// Every `X-Forwarded-For` shape both loopback gates must agree on, with
/// the single answer they must both give.
///
/// `bearer_auth` and `localhost_only_shell_auth` ask the same question —
/// *is the caller loopback?* — about the same request. Two answers to one
/// question is a bypass waiting for whichever middleware is more generous,
/// so this table drives both.
const FORWARDED_FOR_CASES: &[(&[&str], bool)] = &[
    // No proxy in front of us: the peer really is the client.
    (&[], true),
    (&["127.0.0.1"], true),
    (&["::1"], true),
    (&["127.0.0.1:4321"], true),
    // A named client hop — the whole point of the header.
    (&["203.0.113.9"], false),
    // Unparseable, empty, or obfuscated hops prove nothing. Failing open
    // here hands the bypass to anyone who can make the header unreadable.
    (&["not-an-ip"], false),
    (&[""], false),
    (&["_hidden"], false),
    (&["unknown"], false),
    // A proxy that *appends* leaves the client's own spoofed hop in front.
    (&["127.0.0.1, 203.0.113.9"], false),
    (&["203.0.113.9, 127.0.0.1"], false),
    // Split across headers (HTTP/2, or a smuggling attempt).
    (&["127.0.0.1", "203.0.113.9"], false),
];

fn request_forwarded_from(peer: [u8; 4], forwarded: &[&str]) -> Request<Body> {
    let mut req = local_request().body(Body::empty()).unwrap();
    for value in forwarded {
        req.headers_mut()
            .append("x-forwarded-for", HeaderValue::from_str(value).unwrap());
    }
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from((peer, 5000))));
    req
}

#[tokio::test]
async fn the_localhost_bypass_is_refused_unless_the_whole_forwarded_chain_is_loopback() {
    for (forwarded, trusted) in FORWARDED_FOR_CASES {
        let app = test_router_with_bearer(Some("secret-key".to_string()));
        let status = app
            .oneshot(request_forwarded_from([127, 0, 0, 1], forwarded))
            .await
            .unwrap()
            .status();
        let expected = if *trusted {
            StatusCode::OK
        } else {
            StatusCode::UNAUTHORIZED
        };
        assert_eq!(
            status,
            expected,
            "X-Forwarded-For {forwarded:?} must{} grant the localhost bypass",
            if *trusted { "" } else { " not" }
        );
    }
}

#[tokio::test]
async fn both_loopback_gates_answer_the_same_way_for_the_same_request() {
    // The two middlewares used to implement this question separately, and
    // disagreed: `bearer_auth` failed open on an unparseable hop while the
    // shell gate failed closed. Same header, same request, opposite trust.
    let shell_gate = Arc::new(ShellGateState {
        allow_remote: false,
        credentials: None,
    });
    for (forwarded, trusted) in FORWARDED_FOR_CASES {
        let shell = Router::new()
            .route("/api/test", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                shell_gate.clone(),
                localhost_only_shell_auth,
            ));
        let status = shell
            .oneshot(request_forwarded_from([127, 0, 0, 1], forwarded))
            .await
            .unwrap()
            .status();
        let expected = if *trusted {
            StatusCode::OK
        } else {
            StatusCode::FORBIDDEN
        };
        assert_eq!(
            status, expected,
            "the shell gate must treat X-Forwarded-For {forwarded:?} exactly as bearer_auth does"
        );
    }
}

#[tokio::test]
async fn a_loopback_forwarded_chain_cannot_vouch_for_a_remote_peer() {
    // The header only ever narrows trust: a request that did not arrive
    // from loopback is not local however friendly its X-Forwarded-For is.
    let app = test_router_with_bearer(Some("secret-key".to_string()));
    let status = app
        .oneshot(request_forwarded_from([10, 0, 0, 1], &["127.0.0.1"]))
        .await
        .unwrap()
        .status();
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bearer_auth_bypasses_for_ipv6_localhost() {
    let app = test_router_with_bearer(Some("secret-key".to_string()));

    let mut req = Request::builder()
        .uri("/api/test")
        .header(header::HOST, format!("[::1]:{TEST_PORT}"))
        .body(Body::empty())
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo("[::1]:5000".parse::<SocketAddr>().unwrap()));

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// --- Host validation (DNS rebinding), through the real middleware ---

#[tokio::test]
async fn rebound_host_is_refused_before_the_loopback_bypass_applies() {
    // DNS rebinding: evil.test resolves to 127.0.0.1, so the connection
    // really does arrive from loopback and Origin really does equal Host.
    // Neither fact makes the request local — only the Host authority does.
    let app = test_router_with_bearer(Some("secret-key".to_string()));

    let mut req = Request::builder()
        .uri("/api/test")
        .header(header::HOST, "evil.test")
        .header(header::ORIGIN, "http://evil.test")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

/// One request from another machine, addressed by a name this server was
/// never told about — the LAN client's ordinary case.
fn remote_request(host: &str) -> axum::http::request::Builder {
    Request::builder()
        .uri("/api/test")
        .header(header::HOST, host)
}

fn from_remote_peer(builder: axum::http::request::Builder) -> Request<Body> {
    let mut req = builder.body(Body::empty()).unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 1], 5000))));
    req
}

#[tokio::test]
async fn an_authenticated_client_on_another_machine_may_use_any_name_for_this_server() {
    // Every FQDN the operator's network resolves to this box — `node7`,
    // `node7.lan`, a tailnet name, a CNAME — without enumerating any of
    // them. Safe because the request came from another machine and so has
    // no loopback bypass to reach: it authenticated to get here.
    let app = test_router_with_bearer(Some("secret-key".to_string()));
    let req = from_remote_peer(
        remote_request("node7.tail1234.ts.net:3000").header("authorization", "Bearer secret-key"),
    );

    assert_eq!(app.oneshot(req).await.unwrap().status(), StatusCode::OK);
}

#[tokio::test]
async fn an_unknown_name_from_another_machine_still_has_to_authenticate() {
    // The relaxation moves the refusal from the Host check to the key
    // check; it does not remove one. A 403 here would mean the name was
    // the gate, a 200 would mean nothing was.
    let app = test_router_with_bearer(Some("secret-key".to_string()));
    let req = from_remote_peer(remote_request("evil.test:3000"));

    assert_eq!(
        app.oneshot(req).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn a_request_with_no_peer_address_is_held_to_the_host_list() {
    // The relaxation turns on "this came from another machine", so an
    // absent `ConnectInfo` has to read as "unknown", not as "remote".
    // Read as remote it would be handed to anything that reaches the
    // server without one — including the unauthenticated login bootstrap,
    // where a rebound page could then guess keys and read every answer.
    let app = test_router_with_bearer(Some("secret-key".to_string()));
    let req = remote_request("evil.test:3000")
        .body(Body::empty())
        .unwrap();

    assert_eq!(
        app.oneshot(req).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn with_auth_disabled_every_client_is_held_to_the_host_list() {
    // `api_key = ""`: there is no check behind the Host check, so the Host
    // check is the whole defence and keeps its strict reading.
    let app = test_router_with_bearer(None);
    let req = from_remote_peer(remote_request("evil.test:3000"));

    assert_eq!(
        app.oneshot(req).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}

#[test]
fn a_name_admitted_only_by_the_remote_relaxation_is_not_marked_verified() {
    // `HostVerified` means "this authority is provably one of ours", and
    // the WebSocket guard reads it as licence to treat Origin == Host as
    // same-origin. The relaxation declines to make that claim, so the
    // marker must be absent and the guard left with its static allow-list.
    let state = bearer_state(Some("secret-key".to_string()));
    let mut relaxed = from_remote_peer(remote_request("node7.lan:3000"));
    assert!(enforce_host(&state, &mut relaxed).is_none());
    assert!(relaxed.extensions().get::<HostVerified>().is_none());

    let mut ours = from_remote_peer(remote_request(&format!("127.0.0.1:{TEST_PORT}")));
    assert!(enforce_host(&state, &mut ours).is_none());
    assert!(ours.extensions().get::<HostVerified>().is_some());
}

/// Drive one Host value through the real middleware from a loopback peer —
/// the DNS-rebinding vantage point, where every other gate is already open.
async fn status_for_host(host: &str) -> StatusCode {
    let app = test_router_with_bearer(Some("secret-key".to_string()));
    let mut req = Request::builder()
        .uri("/api/test")
        .header(header::HOST, host)
        .body(Body::empty())
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));
    app.oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn every_spelling_of_a_rebound_host_is_refused() {
    // The variations that make a naive string allowlist leak: a trailing
    // dot (same name to a resolver), mixed case, an appended expected
    // port, and a name that merely *contains* an expected authority.
    for host in [
        "evil.test",
        "evil.test.",
        "EVIL.test",
        "evil.test:3000",
        "evil.test.:3000",
        "127.0.0.1.evil.test:3000",
        "localhost.evil.test:3000",
        "user@127.0.0.1:3000",
        "127.0.0.1:3000@evil.test",
        "127.0.0.1:3000/../evil.test",
    ] {
        assert_eq!(
            status_for_host(host).await,
            StatusCode::FORBIDDEN,
            "Host {host:?} must not be accepted"
        );
    }
}

#[tokio::test]
async fn a_port_mismatch_is_refused() {
    // The rebound page controls the port it navigates to; only the port we
    // actually bound is ours.
    assert_eq!(
        status_for_host("127.0.0.1:3001").await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(status_for_host("127.0.0.1").await, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_missing_or_duplicated_host_is_refused() {
    let app = test_router_with_bearer(Some("secret-key".to_string()));
    let mut req = Request::builder()
        .uri("/api/test")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));
    assert_eq!(
        app.oneshot(req).await.unwrap().status(),
        StatusCode::FORBIDDEN,
        "no Host at all must fail closed"
    );

    // Two Host headers: whichever one we picked, a layer behind us could
    // pick the other. Refuse rather than choose.
    let app = test_router_with_bearer(Some("secret-key".to_string()));
    let mut req = Request::builder()
        .uri("/api/test")
        .header(header::HOST, "127.0.0.1:3000")
        .body(Body::empty())
        .unwrap();
    req.headers_mut()
        .append(header::HOST, HeaderValue::from_static("evil.test"));
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));
    assert_eq!(
        app.oneshot(req).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn an_absolute_form_target_may_not_disagree_with_host() {
    // HTTP/1.1 absolute-form (and HTTP/2 `:authority`) puts an authority on
    // the URI. If it disagrees with Host, one of the two is a smuggled
    // value — refuse instead of picking a winner.
    let app = test_router_with_bearer(Some("secret-key".to_string()));
    let mut req = Request::builder()
        .uri("http://evil.test/api/test")
        .header(header::HOST, "127.0.0.1:3000")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 5000))));
    assert_eq!(
        app.oneshot(req).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn the_bind_authority_and_its_loopback_spellings_are_accepted() {
    // The legitimate cases the guard must not break.
    for host in [
        "127.0.0.1:3000",
        "localhost:3000",
        "LOCALHOST:3000",
        "localhost.:3000",
        "[::1]:3000",
        "[0:0:0:0:0:0:0:1]:3000",
    ] {
        assert_eq!(
            status_for_host(host).await,
            StatusCode::OK,
            "Host {host:?} is this server and must be accepted"
        );
    }
}

// --- Forwarded scheme (what may carry `Secure` on the cookie) ---

/// Every `X-Forwarded-Proto` shape, with the one answer each may give.
///
/// Ambiguity resolves to "not TLS": the loss is a cookie without `Secure`,
/// where the opposite mistake is a `Secure` cookie the browser drops on a
/// plain-HTTP deployment, i.e. a login that stops working without saying so.
const FORWARDED_PROTO_CASES: &[(&[&str], bool)] = &[
    (&["https"], true),
    // Real proxies vary in the case and padding they write the scheme with.
    (&["HTTPS"], true),
    (&[" https "], true),
    // The deployment as it is today: nothing in front of us at all.
    (&[], false),
    (&["http"], false),
    (&[""], false),
    (&["wss"], false),
    // Chained proxies: the LEFTMOST hop is the browser's, the same
    // convention X-Forwarded-For follows. `https, http` is the ordinary
    // shape of browser --https--> edge --http--> local proxy, and is
    // exactly the deployment the attribute exists for — demanding every
    // hop be https answered "not TLS" for it.
    (&["https, https"], true),
    (&["https, http"], true),
    (&["https,http"], true),
    // …and the client's own hop being plaintext settles it, whatever an
    // internal hop upgraded to afterwards.
    (&["http, https"], false),
    // Two headers — a layer behind us could read the other one.
    (&["https", "https"], false),
    (&["https", "http"], false),
];

#[test]
fn a_forwarded_scheme_counts_only_from_a_proxy_on_this_machine() {
    for (forwarded, is_tls) in FORWARDED_PROTO_CASES {
        let mut headers = HeaderMap::new();
        for value in *forwarded {
            headers.append("x-forwarded-proto", HeaderValue::from_str(value).unwrap());
        }

        let local = Some(SocketAddr::from(([127, 0, 0, 1], 5000)));
        assert_eq!(
            forwarded_scheme_is_tls(&headers, local),
            *is_tls,
            "X-Forwarded-Proto {forwarded:?} from a loopback proxy"
        );

        // Only a proxy on this machine can be in front of us: this server's
        // local port is what it dials. From anywhere else — or from a peer
        // we cannot name — the header is a client talking about itself.
        for peer in [Some(SocketAddr::from(([10, 0, 0, 1], 5000))), None] {
            assert!(
                !forwarded_scheme_is_tls(&headers, peer),
                "X-Forwarded-Proto {forwarded:?} from {peer:?} must not claim TLS"
            );
        }
    }
}

// --- Session token (the cookie is not the key) ---

#[tokio::test]
async fn the_api_key_itself_is_never_accepted_as_a_session_cookie() {
    // The cookie must carry a derived, revocable token; presenting the
    // long-lived credential in the cookie slot must fail.
    let app = test_router_with_bearer(Some("secret-key".to_string()));

    let mut req = Request::builder()
        .uri("/api/test")
        .header(header::HOST, "127.0.0.1:3000")
        .header("cookie", format!("{AUTH_COOKIE}=secret-key"))
        .body(Body::empty())
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([10, 0, 0, 1], 5000))));

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
