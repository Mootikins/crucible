//! `GET /api/sessions/search` kiln-scope parsing.
//!
//! Split out of `tests.rs` to stay under the 1000-line module budget enforced
//! by `no_new_oversized_modules`.

/// A `kiln` the registry could never have issued is refused, not dropped.
///
/// The daemon distinguishes "said nothing" from "said something unresolvable"
/// deliberately (`server/session/scope.rs`), because an all-dropped set is a
/// request that asked to narrow. Dropping every name here collapsed the two:
/// the search ran unscoped, found nothing, and answered `{matches: [], total: 0}`
/// — "searched everything" where the honest answer is a 422.
#[tokio::test]
async fn search_refuses_a_kiln_that_is_not_a_usable_name() {
    let (status, json) =
        crate::test_support::request_json("GET", "/api/sessions/search?q=x&kiln=Bad%20Name", None)
            .await;

    assert_eq!(
        status,
        axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "{json}"
    );
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("Bad Name"),
        "the refusal names the value it refused: {json}"
    );
}

/// A partially resolvable set keeps the members that parse — dropping is safe
/// because the non-empty remainder still narrows. Only the all-dropped case
/// can widen, and that is the only case refused.
#[tokio::test]
async fn search_keeps_the_usable_names_when_only_some_are_refused() {
    let (status, json) = crate::test_support::request_json(
        "GET",
        "/api/sessions/search?q=x&kiln=Bad%20Name&kiln=notes",
        None,
    )
    .await;

    assert_eq!(status, axum::http::StatusCode::OK, "{json}");
}

/// No `kiln` key at all is the caller saying nothing, which is not an error.
#[tokio::test]
async fn search_without_a_kiln_is_not_refused() {
    let (status, json) =
        crate::test_support::request_json("GET", "/api/sessions/search?q=x", None).await;

    assert_eq!(status, axum::http::StatusCode::OK, "{json}");
}
