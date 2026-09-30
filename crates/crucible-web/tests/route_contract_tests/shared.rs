//! The test daemons of the route contract tests.
//!
//! The mock daemon and the real in-process daemon live in
//! `crucible_web::test_support`, which the `test-utils` self dev-dependency
//! exposes. This module only re-exports them and the test router. A copy of
//! the mock lived here once and drifted from the library copy. Do not make a
//! copy again.

pub(super) use crucible_web::test_support::{
    build_state, start_mock_daemon, start_mock_daemon_with_errors, start_real_daemon_with_kilns,
    MockErrors,
};

pub(super) use crucible_web::test_support::build_test_app;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

/// Calls one `POST /api/rpc/{method}` row on a built test app and reads the
/// JSON body back. Every route this pass moved onto the one RPC route shares
/// this helper, rather than each test file copying its own.
pub(super) async fn call_rpc(app: axum::Router, method: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/rpc/{method}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}
