use agent_client_protocol::schema::v1::{
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
};

use super::CrucibleAcpClient;
use crate::acp::Result;

impl CrucibleAcpClient {
    /// Answer an inbound `session/request_permission` frame.
    ///
    /// The agent blocks its turn until the reply comes, so every request gets
    /// one. Params that the client cannot read get `-32602`. A frame with no
    /// `id` is a notification, and JSON-RPC forbids a reply to it.
    pub(super) async fn answer_permission_frame(
        &mut self,
        frame: &serde_json::Value,
    ) -> Result<()> {
        let Some(request_id) = frame.get("id") else {
            tracing::debug!("Ignoring session/request_permission sent as a notification");
            return Ok(());
        };
        let params = frame
            .get("params")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        match serde_json::from_value::<RequestPermissionRequest>(params) {
            Ok(request) => {
                self.respond_to_permission_request(request_id, request)
                    .await
            }
            Err(error) => {
                tracing::warn!(%request_id, %error, "Unreadable session/request_permission params");
                self.write_agent_response(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "error": {
                        "code": -32602,
                        "message": format!("Invalid params for session/request_permission: {error}"),
                    }
                }))
                .await
            }
        }
    }

    /// Ask the permission handler for an outcome and send it back.
    ///
    /// The id is echoed as the raw JSON value, for the same reason as in
    /// [`Self::respond_method_not_found`].
    async fn respond_to_permission_request(
        &mut self,
        request_id: &serde_json::Value,
        request: RequestPermissionRequest,
    ) -> Result<()> {
        let outcome = if let Some(handler) = self.permission_handler.clone() {
            handler(request).await
        } else {
            tracing::warn!(
                %request_id,
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
    ///
    /// The id is echoed as the raw JSON value rather than parsed. JSON-RPC ids
    /// are strings, numbers or null, and the response id must match the request
    /// id in type as well as value — an agent keyed on `"req-7"` does not
    /// recognise a reply addressed to `7`. Narrowing to `u64` first also threw
    /// away every id it could not represent, which put string and negative ids
    /// straight back into the hang this reply exists to prevent.
    pub(super) async fn respond_method_not_found(
        &mut self,
        request_id: &serde_json::Value,
        method: &str,
    ) -> Result<()> {
        tracing::debug!(%request_id, method, "Refusing unimplemented ACP method");

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
}
