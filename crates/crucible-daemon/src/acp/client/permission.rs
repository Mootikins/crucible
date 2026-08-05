use agent_client_protocol::{
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
};

use super::CrucibleAcpClient;
use crate::acp::Result;

impl CrucibleAcpClient {
    pub(super) async fn respond_to_permission_request(
        &mut self,
        request_id: u64,
        request: RequestPermissionRequest,
    ) -> Result<()> {
        let outcome = if let Some(handler) = self.permission_handler.clone() {
            handler(request).await
        } else {
            tracing::warn!(
                request_id,
                "No ACP permission handler configured; cancelling request"
            );
            RequestPermissionOutcome::Cancelled
        };

        let response = RequestPermissionResponse::new(outcome);

        let result_value = serde_json::to_value(response)?;
        let json_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": result_value
        });

        self.write_agent_response(json_response).await
    }

    /// Answer an inbound *request* whose method we do not implement.
    ///
    /// A frame carrying an `id` is a request: the agent blocks until it gets a
    /// response. Dropping it hangs the turn until a read timeout fires, so an
    /// unknown method has to come back as JSON-RPC `-32601` instead.
    pub(super) async fn respond_method_not_found(
        &mut self,
        request_id: u64,
        method: &str,
    ) -> Result<()> {
        tracing::debug!(request_id, method, "Refusing unimplemented ACP method");

        self.write_agent_response(serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {
                "code": -32601,
                "message": format!("Method not found: {method}"),
            }
        }))
        .await
    }

    pub(super) fn parse_request_id(&self, value: &serde_json::Value) -> Option<u64> {
        match value {
            serde_json::Value::Number(n) => n.as_u64(),
            serde_json::Value::String(s) => s.parse::<u64>().ok(),
            _ => None,
        }
    }
}
