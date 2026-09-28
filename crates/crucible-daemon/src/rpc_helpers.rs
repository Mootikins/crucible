//! Typed parameter extraction for RPC handlers.
//!
//! Each handler deserializes the request type that `DaemonClient` serializes,
//! from `crucible_core::protocol::requests`. The client and the server thus use
//! one type, and a field name cannot differ between them.

/// Deserialize the whole `params` object into the request type the client
/// serializes, or the `INVALID_PARAMS` response to return in its place.
///
/// One call replaces the field-name literals that the server otherwise
/// repeats by hand. The `require_param!` and `optional_param!` macros did that
/// before, and gate A6 (`tests/architecture_tests/wire_types.rs`) now forbids
/// it. A missing or invalid field answers `INVALID_PARAMS`, as the macros did.
///
/// Unknown fields are tolerated deliberately (no `deny_unknown_fields`) so a
/// newer client can talk to an older daemon.
///
/// Called with an explicit turbofish at every site — `typed_params::<T>(&req)` —
/// because A6 reads the type out of the source.
///
/// The error is boxed: `Response` is 216 bytes, so an unboxed `Err` variant
/// makes every `Ok` this large too (`clippy::result_large_err`), on the path
/// that always succeeds.
pub fn typed_params<T: serde::de::DeserializeOwned>(
    req: &crate::protocol::Request,
) -> Result<T, Box<crate::protocol::Response>> {
    serde_json::from_value(req.params.clone()).map_err(|e| {
        Box::new(crate::protocol::Response::error(
            req.id.clone(),
            crate::protocol::INVALID_PARAMS,
            format!("invalid params: {e}"),
        ))
    })
}

/// Validate a `session_id` a request struct has already deserialized.
///
/// `typed_params` reads the field *name* once. This function applies the
/// rule of one path component to its *value*, and answers `INVALID_PARAMS`,
/// so a handler refuses `../../Documents`. Serde refuses an absent or
/// non-string `session_id` before this function runs.
///
/// The error is boxed for the same reason [`typed_params`] boxes its own:
/// `Response` is 216 bytes and an unboxed `Err` makes every `Ok` that large.
pub fn session_id_field(
    raw: &str,
    req: &crate::protocol::Request,
) -> Result<crucible_core::session::SessionId, Box<crate::protocol::Response>> {
    crucible_core::session::SessionId::parse(raw).map_err(|e| {
        Box::new(crate::protocol::Response::error(
            req.id.clone(),
            crate::protocol::INVALID_PARAMS,
            format!("Invalid 'session_id' parameter: {e}"),
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::typed_params;
    use crate::protocol::{Request, RequestId, INVALID_PARAMS};
    use crucible_core::protocol::requests::SessionUndoRequest;
    use serde_json::json;

    fn request(params: serde_json::Value) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: "session.undo".to_string(),
            params,
        }
    }

    #[test]
    fn a_request_with_every_field_parses() {
        let params = json!({"session_id": "s", "count": 2});
        let parsed = typed_params::<SessionUndoRequest>(&request(params))
            .expect("a complete request parses");
        assert_eq!(parsed.session_id, "s");
        assert_eq!(parsed.count, Some(2));
    }

    /// A missing or invalid field answers INVALID_PARAMS, as `require_param!`
    /// did before the typed requests replaced it.
    #[test]
    fn a_missing_or_invalid_field_answers_invalid_params() {
        for params in [
            json!({}),
            json!({"session_id": 7}),
            json!({"session_id": "s", "count": "two"}),
        ] {
            let response = *typed_params::<SessionUndoRequest>(&request(params.clone()))
                .expect_err("the request must be refused");
            let error = response.error.expect("an error response");
            assert_eq!(error.code, INVALID_PARAMS, "{params}");
            assert!(response.result.is_none(), "{params}");
        }
    }
}
