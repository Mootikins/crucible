//! RPC dispatch and method registration
//!
//! Provides a dispatcher that can be unit-tested without socket I/O.
//! The actual handler implementations remain in server.rs for now,
//! but this module provides the infrastructure for testable dispatch.

use crate::protocol::{Request, RequestId, Response, RpcError, METHOD_NOT_FOUND};
use crate::rpc::context::RpcContext;
use crate::subscription::ClientId;

pub type RpcResult<T> = Result<T, RpcError>;

pub const METHODS: &[&str] = &[
    "ping",
    "daemon.capabilities",
    "shutdown",
    "kiln.open",
    "kiln.close",
    "kiln.list",
    "search_vectors",
    "list_notes",
    "get_note_by_name",
    "note.upsert",
    "note.get",
    "note.delete",
    "note.list",
    "process_file",
    "process_batch",
    "session.create",
    "session.list",
    "session.get",
    "session.pause",
    "session.resume",
    "session.resume_from_storage",
    "session.end",
    "session.compact",
    "session.subscribe",
    "session.unsubscribe",
    "session.configure_agent",
    "session.send_message",
    "session.cancel",
    "session.switch_model",
    "session.list_models",
    "session.set_thinking_budget",
    "session.get_thinking_budget",
    "session.add_notification",
    "session.list_notifications",
    "session.dismiss_notification",
    "session.interaction_respond",
    "session.set_temperature",
    "session.get_temperature",
    "session.set_max_tokens",
    "session.get_max_tokens",
    "session.test_interaction",
    "session.set_title",
    "plugin.reload",
    "plugin.list",
    "project.register",
    "project.unregister",
    "project.list",
    "project.get",
];

fn to_response(id: Option<RequestId>, result: RpcResult<serde_json::Value>) -> Response {
    match result {
        Ok(v) => Response::success(id, v),
        Err(e) => Response {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(e),
        },
    }
}

pub struct RpcDispatcher {
    ctx: RpcContext,
}

impl RpcDispatcher {
    pub fn new(ctx: RpcContext) -> Self {
        Self { ctx }
    }

    #[allow(dead_code)]
    pub fn context(&self) -> &RpcContext {
        &self.ctx
    }

    pub async fn dispatch(&self, client_id: ClientId, req: Request) -> Response {
        let id = req.id.clone();
        tracing::debug!("RPC dispatch: method={:?}, id={:?}", req.method, id);

        match req.method.as_str() {
            "ping" => to_response(id, self.handle_ping()),
            "daemon.capabilities" => to_response(id, self.handle_capabilities()),
            "shutdown" => to_response(id, self.handle_shutdown()),

            // Subscription handlers (need client_id)
            "session.subscribe" => to_response(id, self.handle_subscribe(client_id, &req)),
            "session.unsubscribe" => to_response(id, self.handle_unsubscribe(client_id, &req)),

            // Session title handler
            "session.set_title" => to_response(id, self.handle_set_title(&req).await),

            // For other methods, we return METHOD_NOT_FOUND here.
            // In production, server.rs will handle these until we migrate them.
            // This allows incremental migration.
            _ => Response::error(
                id,
                METHOD_NOT_FOUND,
                format!("Method '{}' not yet migrated to new dispatcher", req.method),
            ),
        }
    }

    fn handle_ping(&self) -> RpcResult<serde_json::Value> {
        Ok(serde_json::json!("pong"))
    }

    fn handle_shutdown(&self) -> RpcResult<serde_json::Value> {
        tracing::info!("Shutdown requested via RPC");
        let _ = self.ctx.shutdown_tx.send(());
        Ok(serde_json::json!("shutting down"))
    }

    fn handle_capabilities(&self) -> RpcResult<serde_json::Value> {
        Ok(serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "build_sha": env!("CRUCIBLE_BUILD_SHA"),
            "protocol_version": "1.0",
            "capabilities": {
                "kilns": true,
                "sessions": true,
                "agents": true,
                "events": true,
                "thinking_budget": true,
                "model_switching": true,
            },
            "methods": METHODS,
        }))
    }

    fn handle_subscribe(&self, client_id: ClientId, req: &Request) -> RpcResult<serde_json::Value> {
        use crate::rpc::params::parse_params;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            session_ids: Vec<String>,
        }
        let p: Params = parse_params(req)?;

        for session_id in &p.session_ids {
            if session_id == "*" {
                self.ctx.subscriptions.subscribe_all(client_id);
            } else {
                self.ctx.subscriptions.subscribe(client_id, session_id);
            }
        }

        Ok(serde_json::json!({
            "subscribed": p.session_ids,
            "client_id": format!("{:?}", client_id),
        }))
    }

    fn handle_unsubscribe(
        &self,
        client_id: ClientId,
        req: &Request,
    ) -> RpcResult<serde_json::Value> {
        use crate::rpc::params::parse_params;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            session_ids: Vec<String>,
        }
        let p: Params = parse_params(req)?;

        for session_id in &p.session_ids {
            self.ctx.subscriptions.unsubscribe(client_id, session_id);
        }

        Ok(serde_json::json!({
            "unsubscribed": p.session_ids,
            "client_id": format!("{:?}", client_id),
        }))
    }

    async fn handle_set_title(&self, req: &Request) -> RpcResult<serde_json::Value> {
        use crate::rpc::params::parse_params;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            session_id: String,
            title: String,
        }
        let p: Params = parse_params(req)?;

        self.ctx
            .sessions
            .set_title(&p.session_id, p.title.clone())
            .await
            .map_err(|e| RpcError {
                code: crate::protocol::INVALID_PARAMS,
                message: format!("Failed to set title: {}", e),
                data: None,
            })?;

        Ok(serde_json::json!({
            "session_id": p.session_id,
            "title": p.title,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::RequestId;
    use crate::rpc::RpcContext;
    use std::sync::Arc;

    fn make_request(method: &str, params: serde_json::Value) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: method.to_string(),
            params,
        }
    }

    fn test_context() -> RpcContext {
        use crate::agent_manager::AgentManager;
        use crate::background_manager::BackgroundJobManager;
        use crate::kiln_manager::KilnManager;
        use crate::session_manager::SessionManager;
        use crate::subscription::SubscriptionManager;
        use tokio::sync::broadcast;

        let (event_tx, _) = broadcast::channel(16);
        let (shutdown_tx, _) = broadcast::channel(1);
        let session_manager = Arc::new(SessionManager::new());
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx.clone()));
        let agent_manager = Arc::new(AgentManager::new(
            session_manager.clone(),
            background_manager,
            None,
            crucible_config::ProvidersConfig::default(),
        ));

        RpcContext::new(
            Arc::new(KilnManager::new()),
            session_manager,
            agent_manager,
            Arc::new(SubscriptionManager::new()),
            event_tx,
            shutdown_tx,
        )
    }

    #[test]
    fn methods_list_includes_core_methods() {
        assert!(METHODS.contains(&"ping"));
        assert!(METHODS.contains(&"daemon.capabilities"));
        assert!(METHODS.contains(&"session.subscribe"));
        assert!(METHODS.contains(&"session.set_thinking_budget"));
    }

    #[test]
    fn methods_count() {
        assert_eq!(METHODS.len(), 48, "Update when adding RPC methods");
    }

    #[tokio::test]
    async fn dispatch_ping_no_socket() {
        let dispatcher = RpcDispatcher::new(test_context());
        let req = make_request("ping", serde_json::json!({}));

        let resp = dispatcher.dispatch(ClientId::new(), req).await;

        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap(), "pong");
    }

    #[tokio::test]
    async fn dispatch_capabilities_returns_methods_list() {
        let dispatcher = RpcDispatcher::new(test_context());
        let req = make_request("daemon.capabilities", serde_json::json!({}));

        let resp = dispatcher.dispatch(ClientId::new(), req).await;

        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let methods = result["methods"].as_array().unwrap();
        assert!(methods.iter().any(|m| m == "ping"));
        assert!(methods.iter().any(|m| m == "session.set_thinking_budget"));
    }

    #[tokio::test]
    async fn dispatch_unknown_method_returns_error() {
        let dispatcher = RpcDispatcher::new(test_context());
        let req = make_request("nonexistent.method", serde_json::json!({}));

        let resp = dispatcher.dispatch(ClientId::new(), req).await;

        assert!(resp.error.is_some());
        let err = resp.error.unwrap();
        assert_eq!(err.code, METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn dispatch_subscribe_tracks_subscription() {
        let ctx = test_context();
        let dispatcher = RpcDispatcher::new(ctx);
        let client_id = ClientId::new();
        let req = make_request(
            "session.subscribe",
            serde_json::json!({
                "session_ids": ["session-123"]
            }),
        );

        let resp = dispatcher.dispatch(client_id, req).await;

        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        let subscribed = result["subscribed"].as_array().unwrap();
        assert_eq!(subscribed.len(), 1);
        assert_eq!(subscribed[0], "session-123");
    }
}
