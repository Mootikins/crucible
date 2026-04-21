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

        self.write_permission_response(json_response).await
    }

    async fn write_permission_response(&mut self, payload: serde_json::Value) -> Result<()> {
        if self.agent_stdin.is_none() && self.boxed_writer.is_none() {
            tracing::warn!("Agent stdin unavailable; cannot send permission response");
            return Ok(());
        }
        self.write_request(&payload).await
    }

    pub(super) fn parse_request_id(&self, value: &serde_json::Value) -> Option<u64> {
        match value {
            serde_json::Value::Number(n) => n.as_u64(),
            serde_json::Value::String(s) => s.parse::<u64>().ok(),
            _ => None,
        }
    }
}
