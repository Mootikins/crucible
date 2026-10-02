//! Compatibility for modern clients probing our initialize-era MCP wire.

use rmcp::model::{
    ClientJsonRpcMessage, ClientRequest, ErrorCode, ErrorData, ServerJsonRpcMessage,
};
use rmcp::transport::Transport;
use rmcp::RoleServer;
use std::future::Future;

/// rmcp 2.2 requires initialize first. Claude's newer MCP client first sends
/// server/discover and needs MethodNotFound to fall back on the same connection.
/// Keep this external-wire shim until rmcp supports that negotiation itself.
pub(super) struct LegacyStdio<T>(pub(super) T);

impl<T: Transport<RoleServer>> Transport<RoleServer> for LegacyStdio<T> {
    type Error = T::Error;

    fn send(
        &mut self,
        message: ServerJsonRpcMessage,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        self.0.send(message)
    }

    async fn receive(&mut self) -> Option<ClientJsonRpcMessage> {
        loop {
            let message = self.0.receive().await?;
            if let ClientJsonRpcMessage::Request(request) = &message {
                if matches!(&request.request, ClientRequest::CustomRequest(custom) if custom.method == "server/discover")
                {
                    let reply = ServerJsonRpcMessage::error(
                        ErrorData::new(ErrorCode::METHOD_NOT_FOUND, "Method not found", None),
                        Some(request.id.clone()),
                    );
                    if let Err(error) = self.0.send(reply).await {
                        tracing::warn!(%error, "MCP discovery fallback reply failed");
                        return None;
                    }
                    continue;
                }
            }
            return Some(message);
        }
    }

    fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        self.0.close()
    }
}
