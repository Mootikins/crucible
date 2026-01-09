//! RPC parameter extraction macros
//!
//! Reduces boilerplate in server.rs handlers by providing macros for
//! extracting and validating JSON-RPC parameters.
//!
//! These macros are for internal use within the crucible-daemon crate.
//! They are made available to sibling modules via `#[macro_use]` in lib.rs.

/// Extract a required string parameter from a request.
///
/// Returns the parameter value as `&str`, or returns early with an error Response
/// if the parameter is missing or not a string.
///
/// # Example
///
/// ```ignore
/// let path = require_str_param!(req, "path");
/// // `path` is now &str, or function returned early with error
/// ```
#[macro_export]
macro_rules! require_str_param {
    ($req:expr, $name:literal) => {
        match $req.params.get($name).and_then(|v| v.as_str()) {
            Some(v) => v,
            None => {
                return $crate::protocol::Response::error(
                    $req.id.clone(),
                    $crate::protocol::INVALID_PARAMS,
                    concat!("Missing or invalid '", $name, "' parameter"),
                )
            }
        }
    };
}

/// Extract an optional string parameter from a request.
///
/// Returns `Option<&str>`. Returns `None` if the parameter is missing or not a string.
///
/// # Example
///
/// ```ignore
/// let filter = optional_str_param!(req, "filter");
/// // `filter` is Option<&str>
/// ```
#[macro_export]
macro_rules! optional_str_param {
    ($req:expr, $name:literal) => {
        $req.params.get($name).and_then(|v| v.as_str())
    };
}

/// Extract an optional u64 parameter from a request.
///
/// Returns `Option<u64>`. Returns `None` if the parameter is missing or not a number.
///
/// # Example
///
/// ```ignore
/// let limit = optional_u64_param!(req, "limit").unwrap_or(20);
/// // `limit` is u64, defaulting to 20
/// ```
#[macro_export]
macro_rules! optional_u64_param {
    ($req:expr, $name:literal) => {
        $req.params.get($name).and_then(|v| v.as_u64())
    };
}

/// Extract a required array parameter from a request.
///
/// Returns the parameter value as `&Vec<serde_json::Value>`, or returns early
/// with an error Response if the parameter is missing or not an array.
///
/// # Example
///
/// ```ignore
/// let items = require_array_param!(req, "items");
/// // `items` is &Vec<Value>, or function returned early with error
/// ```
#[macro_export]
macro_rules! require_array_param {
    ($req:expr, $name:literal) => {
        match $req.params.get($name).and_then(|v| v.as_array()) {
            Some(v) => v,
            None => {
                return $crate::protocol::Response::error(
                    $req.id.clone(),
                    $crate::protocol::INVALID_PARAMS,
                    concat!("Missing or invalid '", $name, "' parameter"),
                )
            }
        }
    };
}

// Re-export macros for use in sibling modules
pub(crate) use optional_str_param;
pub(crate) use optional_u64_param;
pub(crate) use require_array_param;
pub(crate) use require_str_param;

#[cfg(test)]
mod tests {
    use crate::protocol::{Request, RequestId, Response, INVALID_PARAMS};
    use serde_json::json;

    // Helper to create a test request
    fn make_request(params: serde_json::Value) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: "test".to_string(),
            params,
        }
    }

    // Test functions that use the macros (simulating handlers)
    fn extract_required_str(req: Request) -> Response {
        let value = require_str_param!(req, "name");
        Response::success(req.id, value)
    }

    fn extract_optional_str(req: Request) -> Response {
        let value = optional_str_param!(req, "filter");
        Response::success(req.id, value.unwrap_or("default"))
    }

    fn extract_optional_u64(req: Request) -> Response {
        let value = optional_u64_param!(req, "limit");
        Response::success(req.id, value.unwrap_or(10))
    }

    fn extract_required_array(req: Request) -> Response {
        let arr = require_array_param!(req, "items");
        Response::success(req.id, arr.len())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // require_str_param! tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn require_str_param_success() {
        let req = make_request(json!({"name": "hello"}));
        let resp = extract_required_str(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), "hello");
    }

    #[test]
    fn require_str_param_missing() {
        let req = make_request(json!({}));
        let resp = extract_required_str(req);

        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, INVALID_PARAMS);
        assert!(err.message.contains("'name'"));
    }

    #[test]
    fn require_str_param_wrong_type() {
        let req = make_request(json!({"name": 123}));
        let resp = extract_required_str(req);

        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, INVALID_PARAMS);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // optional_str_param! tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn optional_str_param_present() {
        let req = make_request(json!({"filter": "active"}));
        let resp = extract_optional_str(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), "active");
    }

    #[test]
    fn optional_str_param_missing() {
        let req = make_request(json!({}));
        let resp = extract_optional_str(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), "default");
    }

    #[test]
    fn optional_str_param_wrong_type() {
        let req = make_request(json!({"filter": 123}));
        let resp = extract_optional_str(req);

        // Wrong type is treated as missing for optional params
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), "default");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // optional_u64_param! tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn optional_u64_param_present() {
        let req = make_request(json!({"limit": 50}));
        let resp = extract_optional_u64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 50);
    }

    #[test]
    fn optional_u64_param_missing() {
        let req = make_request(json!({}));
        let resp = extract_optional_u64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 10); // default value
    }

    #[test]
    fn optional_u64_param_wrong_type() {
        let req = make_request(json!({"limit": "not a number"}));
        let resp = extract_optional_u64(req);

        // Wrong type is treated as missing for optional params
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 10); // default value
    }

    // ─────────────────────────────────────────────────────────────────────────
    // require_array_param! tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn require_array_param_success() {
        let req = make_request(json!({"items": [1, 2, 3]}));
        let resp = extract_required_array(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 3);
    }

    #[test]
    fn require_array_param_empty_array() {
        let req = make_request(json!({"items": []}));
        let resp = extract_required_array(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 0);
    }

    #[test]
    fn require_array_param_missing() {
        let req = make_request(json!({}));
        let resp = extract_required_array(req);

        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, INVALID_PARAMS);
        assert!(err.message.contains("'items'"));
    }

    #[test]
    fn require_array_param_wrong_type() {
        let req = make_request(json!({"items": "not an array"}));
        let resp = extract_required_array(req);

        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, INVALID_PARAMS);
    }
}
