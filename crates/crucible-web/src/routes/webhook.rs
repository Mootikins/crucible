use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Extension, Json, Router,
};
use crucible_daemon::webhook::{
    default_secrets_path, Signature, WebhookSecrets, SIGNATURE_HEADERS,
};
use std::collections::HashMap;
use std::sync::Arc;

/// Webhook ingress: `POST /api/webhook/{name}` turns a request into a
/// `webhook:received` event on every plugin's stream.
///
/// **This is the enforcement point for webhook sender authentication.** The
/// route sits inside the bearer-auth layer, but that layer waves loopback
/// callers through — so on the machine running `cru web` any page the operator
/// visits can reach this endpoint cross-origin with no credential of its own.
/// Every delivery therefore carries a per-webhook HMAC over the raw body
/// (schemes and headers in [`crucible_daemon::webhook`]: Crucible's and
/// Stripe's timestamped `t=…,v1=…`, or GitHub's `X-Hub-Signature-256`), and a
/// webhook with no configured secret is refused rather than allowed. Nothing
/// downstream re-checks.
///
/// Secrets live in `~/.config/crucible/webhooks.toml`, minted by
/// [`crucible_daemon::webhook::mint_secret`]:
///
/// ```toml
/// [webhooks.ci]
/// secret = "at-least-16-bytes-of-secret"
/// ```
pub fn webhook_routes() -> Router<AppState> {
    webhook_routes_with_secrets(Arc::new(WebhookSecrets::load(
        default_secrets_path().as_deref(),
    )))
}

/// [`webhook_routes`] with the secret store injected — tests must use this, so
/// they never read the developer's real `webhooks.toml`.
fn webhook_routes_with_secrets(secrets: Arc<WebhookSecrets>) -> Router<AppState> {
    Router::new()
        .route("/api/webhook/{name}", post(handle_webhook))
        .layer(Extension(secrets))
}

/// Credential headers stripped before the delivery is broadcast. The header map
/// goes onto the plugin event stream verbatim, and the caller's own credentials
/// do not belong in a plugin's inbox.
const REDACTED_HEADERS: [&str; 3] = ["authorization", "cookie", "proxy-authorization"];

/// ...and neither does the delivery's signature, in whichever header it came.
/// Derived from [`SIGNATURE_HEADERS`] rather than restated, so a new accepted
/// header cannot be forgotten here.
fn is_redacted(name: &str) -> bool {
    REDACTED_HEADERS.contains(&name) || SIGNATURE_HEADERS.contains(&name)
}

// `Response` is 128 bytes, so an unboxed `Err` makes every `Ok` that large
// too. `rpc_helpers::typed_params` boxes for exactly this reason and this
// cannot: axum resolves a handler through `IntoResponse` on the error type,
// which `Response` implements and `Box<Response>` does not, so boxing here
// stops the route compiling at all.
#[allow(clippy::result_large_err)]
async fn handle_webhook(
    State(state): State<AppState>,
    Extension(secrets): Extension<Arc<WebhookSecrets>>,
    Path(name): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, Response> {
    // A CORS-safelisted content type (text/plain, form-urlencoded, multipart)
    // makes a cross-origin POST a *simple* request: the browser sends it with
    // no preflight and no opportunity to refuse. Demanding application/json
    // forces the preflight, which the CORS layer answers only for the app's own
    // origins. Defence in depth behind the signature, never instead of it.
    if !is_json_content_type(&headers) {
        return Err(refuse(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Webhook deliveries must be sent as application/json",
        ));
    }

    // Signed over the bytes as received — `body` is still `Bytes` here, so
    // nothing has re-encoded the document between signing and verification.
    let signature =
        Signature::from_headers(|header| headers.get(header).and_then(|v| v.to_str().ok()));
    if let Err(err) = secrets.verify(&name, signature, &body) {
        // One status and one message for every reason, so the endpoint is not
        // an oracle for which webhook names are configured. The reason goes to
        // the operator's log instead.
        tracing::warn!(webhook = %name, reason = %err, "Refusing webhook delivery");
        return Err(refuse(
            StatusCode::UNAUTHORIZED,
            "Missing or invalid webhook signature",
        ));
    }

    // Only after the signature verifies: the RPC carries the body as a JSON
    // string, and `application/json` is UTF-8 by definition, so anything else
    // is a malformed delivery.
    let body = String::from_utf8(body.to_vec()).map_err(|_| {
        refuse(
            StatusCode::BAD_REQUEST,
            "Webhook body is not valid UTF-8 JSON",
        )
    })?;

    let header_map: HashMap<String, String> = headers
        .iter()
        .filter(|(k, _)| !is_redacted(k.as_str()))
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
        .collect();

    let result = state
        .daemon
        .webhook_receive(name, header_map, body)
        .await
        .daemon_err()
        .map_err(WebError::into_response)?;

    Ok(Json(result))
}

/// `Content-Type` is exactly `application/json`, parameters allowed.
/// Fails closed on a missing or unreadable header.
fn is_json_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|essence| essence.trim().eq_ignore_ascii_case("application/json"))
        })
}

/// Same error envelope as [`WebError`], for the two statuses it has no variant
/// for.
fn refuse(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(serde_json::json!({
            "error": { "code": status.as_u16(), "message": message }
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{build_mock_state, start_mock_daemon};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use crucible_daemon::webhook::{
        sign, sign_body_only, WebhookSecrets, GITHUB_SIGNATURE_HEADER, SIGNATURE_HEADER,
        STRIPE_SIGNATURE_HEADER,
    };
    use std::sync::Arc;
    use tower::ServiceExt;

    const SECRET: &str = "correct-horse-battery-staple";
    const BODY: &str = r#"{"event":"push"}"#;

    fn secrets() -> Arc<WebhookSecrets> {
        Arc::new(WebhookSecrets::new(HashMap::from([(
            "ci".to_string(),
            SECRET.to_string(),
        )])))
    }

    /// Drive one webhook POST through a mock-daemon-backed router. `signature`
    /// is `(header name, value)`, so every accepted sender shape goes through
    /// the same header plumbing a real delivery does.
    async fn post(
        secrets: Arc<WebhookSecrets>,
        content_type: Option<&str>,
        signature: Option<(&str, &str)>,
        body: &str,
    ) -> StatusCode {
        let (_mock, client) = start_mock_daemon().await;
        let app = webhook_routes_with_secrets(secrets).with_state(build_mock_state(client));

        let mut builder = Request::builder().method("POST").uri("/api/webhook/ci");
        if let Some(content_type) = content_type {
            builder = builder.header("content-type", content_type);
        }
        if let Some((header, value)) = signature {
            builder = builder.header(header, value);
        }
        let request = builder.body(Body::from(body.to_string())).unwrap();

        app.oneshot(request).await.unwrap().status()
    }

    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    #[tokio::test]
    async fn correctly_signed_webhook_is_accepted() {
        let signature = sign(SECRET, now(), BODY.as_bytes());
        let status = post(
            secrets(),
            Some("application/json"),
            Some((SIGNATURE_HEADER, &signature)),
            BODY,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn unsigned_webhook_is_refused() {
        // The CSRF primitive: any page the operator visits can POST here, and
        // the web server's loopback bypass means no bearer token is needed.
        let status = post(secrets(), Some("application/json"), None, BODY).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrongly_signed_webhook_is_refused() {
        let signature = sign("not-the-configured-secret", now(), BODY.as_bytes());
        let status = post(
            secrets(),
            Some("application/json"),
            Some((SIGNATURE_HEADER, &signature)),
            BODY,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn github_shaped_delivery_is_accepted() {
        // Exactly what an off-the-shelf GitHub webhook sends: its own header,
        // its own body-only signature. If this is refused the endpoint cannot
        // be pointed at GitHub at all.
        let signature = sign_body_only(SECRET, BODY.as_bytes());
        let status = post(
            secrets(),
            Some("application/json"),
            Some((GITHUB_SIGNATURE_HEADER, &signature)),
            BODY,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn wrongly_signed_github_shaped_delivery_is_refused() {
        let signature = sign_body_only("not-the-configured-secret", BODY.as_bytes());
        let status = post(
            secrets(),
            Some("application/json"),
            Some((GITHUB_SIGNATURE_HEADER, &signature)),
            BODY,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn stripe_shaped_delivery_is_accepted() {
        // Stripe's header, Stripe's wire format, and the `v0=` field its
        // parsers ignore — the scheme was copied from Stripe, so this must
        // work unmodified.
        let signature = format!("{},v0=deadbeef", sign(SECRET, now(), BODY.as_bytes()));
        let status = post(
            secrets(),
            Some("application/json"),
            Some((STRIPE_SIGNATURE_HEADER, &signature)),
            BODY,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn webhook_signed_over_a_different_body_is_refused() {
        let signature = sign(SECRET, now(), BODY.as_bytes());
        let status = post(
            secrets(),
            Some("application/json"),
            Some((SIGNATURE_HEADER, &signature)),
            r#"{"event":"tampered"}"#,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn webhook_with_a_replayed_old_timestamp_is_refused() {
        let stale = now() - 3600;
        let signature = sign(SECRET, stale, BODY.as_bytes());
        let status = post(
            secrets(),
            Some("application/json"),
            Some((SIGNATURE_HEADER, &signature)),
            BODY,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn webhook_without_a_configured_secret_is_refused() {
        // Out of the box nothing is configured; that must mean closed, not open.
        let signature = sign(SECRET, now(), BODY.as_bytes());
        let status = post(
            Arc::new(WebhookSecrets::default()),
            Some("application/json"),
            Some((SIGNATURE_HEADER, &signature)),
            BODY,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn webhook_never_accepts_a_cors_simple_request_content_type() {
        // text/plain, form-urlencoded and multipart are CORS-safelisted: a
        // cross-origin `fetch` with one of them is sent with no preflight and
        // no consent. Requiring application/json forces the preflight, which
        // the CORS layer then answers for the app's own origins only.
        let signature = sign(SECRET, now(), BODY.as_bytes());
        for content_type in [
            "text/plain",
            "text/plain;charset=UTF-8",
            "application/x-www-form-urlencoded",
            "multipart/form-data; boundary=x",
        ] {
            let status = post(
                secrets(),
                Some(content_type),
                Some((SIGNATURE_HEADER, &signature)),
                BODY,
            )
            .await;
            assert_eq!(
                status,
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "{content_type} must not be accepted"
            );
        }

        // A missing Content-Type is equally not-a-preflight-forcing request.
        let status = post(secrets(), None, Some((SIGNATURE_HEADER, &signature)), BODY).await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn json_content_type_parameters_are_tolerated() {
        let signature = sign(SECRET, now(), BODY.as_bytes());
        let status = post(
            secrets(),
            Some("Application/JSON; charset=utf-8"),
            Some((SIGNATURE_HEADER, &signature)),
            BODY,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn forwarded_headers_never_include_caller_credentials_or_signatures() {
        // The delivery's headers are broadcast verbatim onto the plugin event
        // stream; neither the operator's API key nor any signature — in any of
        // the headers we accept one from — may ride along.
        let (mock, client) = start_mock_daemon().await;
        let app = webhook_routes_with_secrets(secrets()).with_state(build_mock_state(client));

        let signature = sign(SECRET, now(), BODY.as_bytes());
        let request = Request::builder()
            .method("POST")
            .uri("/api/webhook/ci")
            .header("content-type", "application/json")
            .header(SIGNATURE_HEADER, &signature)
            .header(STRIPE_SIGNATURE_HEADER, &signature)
            .header(
                GITHUB_SIGNATURE_HEADER,
                sign_body_only(SECRET, BODY.as_bytes()),
            )
            .header("authorization", "Bearer the-api-key")
            .header("cookie", "crucible_auth=the-api-key")
            .header("x-github-event", "push")
            .body(Body::from(BODY.to_string()))
            .unwrap();
        assert_eq!(app.oneshot(request).await.unwrap().status(), StatusCode::OK);

        let forwarded = mock
            .received_params("webhook.receive")
            .expect("webhook.receive was called");
        let headers = &forwarded["headers"];
        assert!(headers.get("authorization").is_none(), "{headers}");
        assert!(headers.get("cookie").is_none(), "{headers}");
        for header in SIGNATURE_HEADERS {
            assert!(headers.get(header).is_none(), "{header}: {headers}");
        }
        assert_eq!(headers["x-github-event"], "push");
    }

    #[tokio::test]
    async fn raw_body_is_forwarded_byte_for_byte() {
        let (mock, client) = start_mock_daemon().await;
        let app = webhook_routes_with_secrets(secrets()).with_state(build_mock_state(client));

        // Whitespace and escapes that a reserialize would normalise away —
        // the daemon must see exactly what was signed.
        let body = "{\n  \"a\" :\t1,\n  \"b\": \"\\u00e9\"\n}";
        let signature = sign(SECRET, now(), body.as_bytes());
        let request = Request::builder()
            .method("POST")
            .uri("/api/webhook/ci")
            .header("content-type", "application/json")
            .header(SIGNATURE_HEADER, &signature)
            .body(Body::from(body.to_string()))
            .unwrap();
        assert_eq!(app.oneshot(request).await.unwrap().status(), StatusCode::OK);

        let forwarded = mock
            .received_params("webhook.receive")
            .expect("webhook.receive was called");
        assert_eq!(forwarded["body"], body);
    }
}
