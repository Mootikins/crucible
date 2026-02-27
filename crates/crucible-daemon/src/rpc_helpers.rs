//! RPC parameter extraction macros
//!
//! Reduces boilerplate in RPC handlers by providing macros for extracting
//! and validating JSON-RPC parameters.
//!
//! These macros use `#[macro_export]` for availability across the crate.
//! They return early with error responses when parameters are missing or invalid.

/// Extract a required parameter from a request using a custom conversion method.
///
/// The conversion method should be a method on `serde_json::Value` that returns `Option<T>`.
/// Returns the parameter value as `T`, or returns early with an error Response
/// if the parameter is missing or the conversion fails.
///
/// # Example
///
/// ```text
/// let path = require_param!(req, "path", as_str);
/// // `path` is now &str, or function returned early with error
/// ```
#[macro_export]
macro_rules! require_param {
    ($req:expr, $name:literal, $method:ident) => {
        match $req.params.get($name).and_then(|v| v.$method()) {
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

/// Extract an optional parameter from a request using a custom conversion method.
///
/// The conversion method should be a method on `serde_json::Value` that returns `Option<T>`.
/// Returns `Option<T>`. Returns `None` if the parameter is missing or the conversion fails.
///
/// # Example
///
/// ```text
/// let filter = optional_param!(req, "filter", as_str);
/// // `filter` is Option<&str>
/// ```
#[macro_export]
macro_rules! optional_param {
    ($req:expr, $name:literal, $method:ident) => {
        $req.params.get($name).and_then(|v| v.$method())
    };
}

// Re-export macros for use in sibling modules via `use crate::rpc_helpers::*`
// These are preemptive exports - not all are used yet but will be as handlers grow
#[allow(unused_imports)]
pub use crate::{optional_param, require_param};

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
        let value = require_param!(req, "name", as_str);
        Response::success(req.id, value)
    }

    fn extract_optional_str(req: Request) -> Response {
        let value = optional_param!(req, "filter", as_str);
        Response::success(req.id, value.unwrap_or("default"))
    }

    fn extract_optional_u64(req: Request) -> Response {
        let value = optional_param!(req, "limit", as_u64);
        Response::success(req.id, value.unwrap_or(10))
    }

    fn extract_required_array(req: Request) -> Response {
        let arr = require_param!(req, "items", as_array);
        Response::success(req.id, arr.len())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // require_param! tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn require_param_str_success() {
        let req = make_request(json!({"name": "hello"}));
        let resp = extract_required_str(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), "hello");
    }

    #[test]
    fn require_param_str_missing() {
        let req = make_request(json!({}));
        let resp = extract_required_str(req);

        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, INVALID_PARAMS);
        assert!(err.message.contains("'name'"));
    }

    #[test]
    fn require_param_str_wrong_type() {
        let req = make_request(json!({"name": 123}));
        let resp = extract_required_str(req);

        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, INVALID_PARAMS);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // optional_param! tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn optional_param_str_present() {
        let req = make_request(json!({"filter": "active"}));
        let resp = extract_optional_str(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), "active");
    }

    #[test]
    fn optional_param_str_missing() {
        let req = make_request(json!({}));
        let resp = extract_optional_str(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), "default");
    }

    #[test]
    fn optional_param_str_wrong_type() {
        let req = make_request(json!({"filter": 123}));
        let resp = extract_optional_str(req);

        // Wrong type is treated as missing for optional params
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), "default");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // optional_param! with u64 tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn optional_param_u64_present() {
        let req = make_request(json!({"limit": 50}));
        let resp = extract_optional_u64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 50);
    }

    #[test]
    fn optional_param_u64_missing() {
        let req = make_request(json!({}));
        let resp = extract_optional_u64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 10); // default value
    }

    #[test]
    fn optional_param_u64_wrong_type() {
        let req = make_request(json!({"limit": "not a number"}));
        let resp = extract_optional_u64(req);

        // Wrong type is treated as missing for optional params
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 10); // default value
    }

    // ─────────────────────────────────────────────────────────────────────────
    // require_param! with array tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn require_param_array_success() {
        let req = make_request(json!({"items": [1, 2, 3]}));
        let resp = extract_required_array(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 3);
    }

    #[test]
    fn require_param_array_empty_array() {
        let req = make_request(json!({"items": []}));
        let resp = extract_required_array(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 0);
    }

    #[test]
    fn require_param_array_missing() {
        let req = make_request(json!({}));
        let resp = extract_required_array(req);

        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, INVALID_PARAMS);
        assert!(err.message.contains("'items'"));
    }

    #[test]
    fn require_param_array_wrong_type() {
        let req = make_request(json!({"items": "not an array"}));
        let resp = extract_required_array(req);

        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, INVALID_PARAMS);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // require_param! with f64 tests
    // ─────────────────────────────────────────────────────────────────────────

    fn extract_required_f64(req: Request) -> Response {
        let value = require_param!(req, "temperature", as_f64);
        Response::success(req.id, value)
    }

    fn extract_optional_f64(req: Request) -> Response {
        let value = optional_param!(req, "temperature", as_f64);
        Response::success(req.id, value.unwrap_or(0.7))
    }

    #[test]
    fn require_param_f64_success() {
        let req = make_request(json!({"temperature": 0.5}));
        let resp = extract_required_f64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 0.5);
    }

    #[test]
    fn require_param_f64_integer_coerced() {
        let req = make_request(json!({"temperature": 1}));
        let resp = extract_required_f64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 1.0);
    }

    #[test]
    fn require_param_f64_missing() {
        let req = make_request(json!({}));
        let resp = extract_required_f64(req);

        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, INVALID_PARAMS);
        assert!(err.message.contains("'temperature'"));
    }

    #[test]
    fn optional_param_f64_present() {
        let req = make_request(json!({"temperature": 1.5}));
        let resp = extract_optional_f64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 1.5);
    }

    #[test]
    fn optional_param_f64_missing() {
        let req = make_request(json!({}));
        let resp = extract_optional_f64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 0.7);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // require_param! with i64 tests
    // ─────────────────────────────────────────────────────────────────────────

    fn extract_required_i64(req: Request) -> Response {
        let value = require_param!(req, "budget", as_i64);
        Response::success(req.id, value)
    }

    fn extract_optional_i64(req: Request) -> Response {
        let value = optional_param!(req, "budget", as_i64);
        Response::success(req.id, value.unwrap_or(-1))
    }

    #[test]
    fn require_param_i64_success() {
        let req = make_request(json!({"budget": 1024}));
        let resp = extract_required_i64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 1024);
    }

    #[test]
    fn require_param_i64_negative() {
        let req = make_request(json!({"budget": -1}));
        let resp = extract_required_i64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), -1);
    }

    #[test]
    fn require_param_i64_missing() {
        let req = make_request(json!({}));
        let resp = extract_required_i64(req);

        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, INVALID_PARAMS);
    }

    #[test]
    fn optional_param_i64_present() {
        let req = make_request(json!({"budget": 2048}));
        let resp = extract_optional_i64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 2048);
    }

    #[test]
    fn optional_param_i64_missing() {
        let req = make_request(json!({}));
        let resp = extract_optional_i64(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), -1);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // require_param! with bool tests
    // ─────────────────────────────────────────────────────────────────────────

    fn extract_required_bool(req: Request) -> Response {
        let value = require_param!(req, "enabled", as_bool);
        Response::success(req.id, value)
    }

    fn extract_optional_bool(req: Request) -> Response {
        let value = optional_param!(req, "enabled", as_bool);
        Response::success(req.id, value.unwrap_or(false))
    }

    #[test]
    fn require_param_bool_true() {
        let req = make_request(json!({"enabled": true}));
        let resp = extract_required_bool(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), true);
    }

    #[test]
    fn require_param_bool_false() {
        let req = make_request(json!({"enabled": false}));
        let resp = extract_required_bool(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), false);
    }

    #[test]
    fn require_param_bool_missing() {
        let req = make_request(json!({}));
        let resp = extract_required_bool(req);

        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, INVALID_PARAMS);
    }

    #[test]
    fn optional_param_bool_present() {
        let req = make_request(json!({"enabled": true}));
        let resp = extract_optional_bool(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), true);
    }

    #[test]
    fn optional_param_bool_missing() {
        let req = make_request(json!({}));
        let resp = extract_optional_bool(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), false);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // require_param! with object tests
    // ─────────────────────────────────────────────────────────────────────────

    fn extract_required_obj(req: Request) -> Response {
        let obj = require_param!(req, "config", as_object);
        Response::success(req.id, obj.len())
    }

    #[test]
    fn require_param_obj_success() {
        let req = make_request(json!({"config": {"key": "value", "nested": {"a": 1}}}));
        let resp = extract_required_obj(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 2); // 2 keys in the object
    }

    #[test]
    fn require_param_obj_empty_object() {
        let req = make_request(json!({"config": {}}));
        let resp = extract_required_obj(req);

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), 0); // empty object has 0 keys
    }

    #[test]
    fn require_param_obj_missing() {
        let req = make_request(json!({}));
        let resp = extract_required_obj(req);

        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, INVALID_PARAMS);
        assert!(err.message.contains("'config'"));
    }

    #[test]
    fn require_param_obj_wrong_type() {
        let req = make_request(json!({"config": "not an object"}));
        let resp = extract_required_obj(req);

        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, INVALID_PARAMS);
    }
}
