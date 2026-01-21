//! RPC parameter extraction macros
//!
//! Reduces boilerplate in RPC handlers by providing macros for extracting
//! and validating JSON-RPC parameters.
//!
//! These macros use `#[macro_export]` for availability across the crate.
//! They return early with error responses when parameters are missing or invalid.

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

/// Extract a required f64 parameter from a request.
///
/// Returns the parameter value as `f64`, or returns early with an error Response
/// if the parameter is missing or not a number.
///
/// # Example
///
/// ```ignore
/// let temperature = require_f64_param!(req, "temperature");
/// // `temperature` is f64, or function returned early with error
/// ```
#[macro_export]
macro_rules! require_f64_param {
    ($req:expr, $name:literal) => {
        match $req.params.get($name).and_then(|v| v.as_f64()) {
            Some(v) => v,
            None => {
                return $crate::protocol::Response::error(
                    $req.id.clone(),
                    $crate::protocol::INVALID_PARAMS,
                    concat!(
                        "Missing or invalid '",
                        $name,
                        "' parameter (expected number)"
                    ),
                )
            }
        }
    };
}

/// Extract an optional f64 parameter from a request.
///
/// Returns `Option<f64>`. Returns `None` if the parameter is missing or not a number.
///
/// # Example
///
/// ```ignore
/// let temperature = optional_f64_param!(req, "temperature").unwrap_or(0.7);
/// // `temperature` is f64, defaulting to 0.7
/// ```
#[macro_export]
macro_rules! optional_f64_param {
    ($req:expr, $name:literal) => {
        $req.params.get($name).and_then(|v| v.as_f64())
    };
}

/// Extract a required i64 parameter from a request.
///
/// Returns the parameter value as `i64`, or returns early with an error Response
/// if the parameter is missing or not a number.
///
/// # Example
///
/// ```ignore
/// let budget = require_i64_param!(req, "thinking_budget");
/// // `budget` is i64, or function returned early with error
/// ```
#[macro_export]
macro_rules! require_i64_param {
    ($req:expr, $name:literal) => {
        match $req.params.get($name).and_then(|v| v.as_i64()) {
            Some(v) => v,
            None => {
                return $crate::protocol::Response::error(
                    $req.id.clone(),
                    $crate::protocol::INVALID_PARAMS,
                    concat!(
                        "Missing or invalid '",
                        $name,
                        "' parameter (expected integer)"
                    ),
                )
            }
        }
    };
}

/// Extract an optional i64 parameter from a request.
///
/// Returns `Option<i64>`. Returns `None` if the parameter is missing or not a number.
///
/// # Example
///
/// ```ignore
/// let budget = optional_i64_param!(req, "thinking_budget");
/// // `budget` is Option<i64>
/// ```
#[macro_export]
macro_rules! optional_i64_param {
    ($req:expr, $name:literal) => {
        $req.params.get($name).and_then(|v| v.as_i64())
    };
}

/// Extract a required bool parameter from a request.
///
/// Returns the parameter value as `bool`, or returns early with an error Response
/// if the parameter is missing or not a boolean.
///
/// # Example
///
/// ```ignore
/// let enabled = require_bool_param!(req, "enabled");
/// // `enabled` is bool, or function returned early with error
/// ```
#[macro_export]
macro_rules! require_bool_param {
    ($req:expr, $name:literal) => {
        match $req.params.get($name).and_then(|v| v.as_bool()) {
            Some(v) => v,
            None => {
                return $crate::protocol::Response::error(
                    $req.id.clone(),
                    $crate::protocol::INVALID_PARAMS,
                    concat!(
                        "Missing or invalid '",
                        $name,
                        "' parameter (expected boolean)"
                    ),
                )
            }
        }
    };
}

/// Extract an optional bool parameter from a request.
///
/// Returns `Option<bool>`. Returns `None` if the parameter is missing or not a boolean.
///
/// # Example
///
/// ```ignore
/// let verbose = optional_bool_param!(req, "verbose").unwrap_or(false);
/// // `verbose` is bool, defaulting to false
/// ```
#[macro_export]
macro_rules! optional_bool_param {
    ($req:expr, $name:literal) => {
        $req.params.get($name).and_then(|v| v.as_bool())
    };
}

// Re-export macros for use in sibling modules via `use crate::rpc_helpers::*`
pub use crate::optional_bool_param;
pub use crate::optional_f64_param;
pub use crate::optional_i64_param;
pub use crate::optional_str_param;
pub use crate::optional_u64_param;
pub use crate::require_array_param;
pub use crate::require_bool_param;
pub use crate::require_f64_param;
pub use crate::require_i64_param;
pub use crate::require_str_param;

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

    // ─────────────────────────────────────────────────────────────────────────
    // require_f64_param! tests
    // ─────────────────────────────────────────────────────────────────────────

    fn extract_required_f64(req: Request) -> Response {
        let value = require_f64_param!(req, "temperature");
        Response::success(req.id, value)
    }

    fn extract_optional_f64(req: Request) -> Response {
        let value = optional_f64_param!(req, "temperature");
        Response::success(req.id, value.unwrap_or(0.7))
    }

    #[test]
    fn require_f64_param_success() {
        let req = make_request(json!({"temperature": 0.5}));
        let resp = extract_required_f64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 0.5);
    }

    #[test]
    fn require_f64_param_integer_coerced() {
        let req = make_request(json!({"temperature": 1}));
        let resp = extract_required_f64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 1.0);
    }

    #[test]
    fn require_f64_param_missing() {
        let req = make_request(json!({}));
        let resp = extract_required_f64(req);

        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, INVALID_PARAMS);
        assert!(err.message.contains("'temperature'"));
    }

    #[test]
    fn optional_f64_param_present() {
        let req = make_request(json!({"temperature": 1.5}));
        let resp = extract_optional_f64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 1.5);
    }

    #[test]
    fn optional_f64_param_missing() {
        let req = make_request(json!({}));
        let resp = extract_optional_f64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 0.7);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // require_i64_param! tests
    // ─────────────────────────────────────────────────────────────────────────

    fn extract_required_i64(req: Request) -> Response {
        let value = require_i64_param!(req, "budget");
        Response::success(req.id, value)
    }

    fn extract_optional_i64(req: Request) -> Response {
        let value = optional_i64_param!(req, "budget");
        Response::success(req.id, value.unwrap_or(-1))
    }

    #[test]
    fn require_i64_param_success() {
        let req = make_request(json!({"budget": 1024}));
        let resp = extract_required_i64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 1024);
    }

    #[test]
    fn require_i64_param_negative() {
        let req = make_request(json!({"budget": -1}));
        let resp = extract_required_i64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), -1);
    }

    #[test]
    fn require_i64_param_missing() {
        let req = make_request(json!({}));
        let resp = extract_required_i64(req);

        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, INVALID_PARAMS);
    }

    #[test]
    fn optional_i64_param_present() {
        let req = make_request(json!({"budget": 2048}));
        let resp = extract_optional_i64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 2048);
    }

    #[test]
    fn optional_i64_param_missing() {
        let req = make_request(json!({}));
        let resp = extract_optional_i64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), -1);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // require_bool_param! tests
    // ─────────────────────────────────────────────────────────────────────────

    fn extract_required_bool(req: Request) -> Response {
        let value = require_bool_param!(req, "enabled");
        Response::success(req.id, value)
    }

    fn extract_optional_bool(req: Request) -> Response {
        let value = optional_bool_param!(req, "enabled");
        Response::success(req.id, value.unwrap_or(false))
    }

    #[test]
    fn require_bool_param_true() {
        let req = make_request(json!({"enabled": true}));
        let resp = extract_required_bool(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), true);
    }

    #[test]
    fn require_bool_param_false() {
        let req = make_request(json!({"enabled": false}));
        let resp = extract_required_bool(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), false);
    }

    #[test]
    fn require_bool_param_missing() {
        let req = make_request(json!({}));
        let resp = extract_required_bool(req);

        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, INVALID_PARAMS);
    }

    #[test]
    fn optional_bool_param_present() {
        let req = make_request(json!({"enabled": true}));
        let resp = extract_optional_bool(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), true);
    }

    #[test]
    fn optional_bool_param_missing() {
        let req = make_request(json!({}));
        let resp = extract_optional_bool(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), false);
    }
}
