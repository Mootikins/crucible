//! Typed parameter parsing for RPC handlers

use crate::protocol::{Request, RpcError, INVALID_PARAMS};
use serde::de::DeserializeOwned;

pub fn parse_params<T: DeserializeOwned>(req: &Request) -> Result<T, RpcError> {
    serde_json::from_value(req.params.clone()).map_err(|e| RpcError {
        code: INVALID_PARAMS,
        message: format!("Invalid params: {}", e),
        data: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::RequestId;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct TestParams {
        name: String,
        count: Option<i32>,
    }

    fn make_request(params: serde_json::Value) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: "test".to_string(),
            params,
        }
    }

    #[test]
    fn parse_valid_params() {
        let req = make_request(serde_json::json!({"name": "test", "count": 5}));
        let params: TestParams = parse_params(&req).unwrap();
        assert_eq!(params.name, "test");
        assert_eq!(params.count, Some(5));
    }

    #[test]
    fn parse_missing_optional() {
        let req = make_request(serde_json::json!({"name": "test"}));
        let params: TestParams = parse_params(&req).unwrap();
        assert_eq!(params.name, "test");
        assert_eq!(params.count, None);
    }

    #[test]
    fn parse_missing_required_fails() {
        let req = make_request(serde_json::json!({}));
        let err = parse_params::<TestParams>(&req).unwrap_err();
        assert_eq!(err.code, INVALID_PARAMS);
        assert!(err.message.contains("Invalid params"));
    }

    #[test]
    fn parse_wrong_type_fails() {
        let req = make_request(serde_json::json!({"name": 123}));
        let err = parse_params::<TestParams>(&req).unwrap_err();
        assert_eq!(err.code, INVALID_PARAMS);
    }
}
