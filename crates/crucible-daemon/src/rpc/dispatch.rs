//! RPC dispatch and method registration
//!
//! Provides a dispatcher that can be unit-tested without socket I/O.
//! The actual handler implementations remain in server.rs for now,
//! but this module provides the infrastructure for testable dispatch.

use crate::protocol::{Request, RequestId, Response, RpcError, INTERNAL_ERROR, METHOD_NOT_FOUND};
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
    "kiln.set_classification",
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
    "session.archive",
    "session.unarchive",
    "session.delete",
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
    "session.set_max_iterations",
    "session.get_max_iterations",
    "session.set_execution_timeout",
    "session.get_execution_timeout",
    "session.set_context_budget",
    "session.get_context_budget",
    "session.set_context_strategy",
    "session.get_context_strategy",
    "session.set_context_window",
    "session.get_context_window",
    "session.set_output_validation",
    "session.get_output_validation",
    "session.set_validation_retries",
    "session.get_validation_retries",
    "session.set_system_prompt",
    "session.get_system_prompt",
    "session.set_precognition",
    "session.get_precognition",
    "session.set_precognition_results",
    "session.get_precognition_results",
    "session.inject_context",
    "session.test_interaction",
    "session.fork",
    "session.set_title",
    "session.search",
    "session.load_events",
    "session.list_persisted",
    "session.render_markdown",
    "session.export_to_file",
    "session.replay",
    "session.cleanup",
    "session.reindex",
    "session.undo",
    "session.can_undo",
    "session.undo_depth",
    "plugin.reload",
    "plugin.list",
    "lua.init_session",
    "lua.register_hooks",
    "lua.execute_hook",
    "lua.shutdown_session",
    "lua.discover_plugins",
    "lua.plugin_health",
    "lua.generate_stubs",
    "lua.run_plugin_tests",
    "lua.register_commands",
    "lua.eval",
    "project.register",
    "project.unregister",
    "project.list",
    "project.get",
    "storage.verify",
    "storage.cleanup",
    "storage.backup",
    "storage.restore",
    "mcp.start",
    "mcp.stop",
    "mcp.status",
    "skills.list",
    "skills.get",
    "skills.search",
    "agents.list_profiles",
    "agents.resolve_profile",
    "models.list",
    "providers.list",
    "subagent.collect",
    "webhook.receive",
    "suggest_links",
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

fn map_server_resp(resp: Response) -> RpcResult<serde_json::Value> {
    match resp.error {
        Some(err) => Err(err),
        None => Ok(resp.result.unwrap_or(serde_json::Value::Null)),
    }
}

pub struct RpcDispatcher {
    ctx: RpcContext,
}

impl RpcDispatcher {
    pub fn new(ctx: RpcContext) -> Self {
        Self { ctx }
    }

    #[allow(dead_code)] // accessor for direct RpcContext access by handlers
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

            // Session config get/set handlers — each pair delegates to
            // server::session::handle_session_{set,get}_<name> with uniform signatures.
            "session.set_thinking_budget"
            | "session.set_temperature"
            | "session.set_max_tokens"
            | "session.set_max_iterations"
            | "session.set_execution_timeout"
            | "session.set_context_budget"
            | "session.set_context_strategy"
            | "session.set_context_window"
            | "session.set_output_validation"
            | "session.set_validation_retries"
            | "session.set_system_prompt"
            | "session.set_precognition"
            | "session.set_precognition_results" => {
                to_response(id, self.dispatch_session_config_setter(&req).await)
            }
            "session.get_thinking_budget"
            | "session.get_temperature"
            | "session.get_max_tokens"
            | "session.get_max_iterations"
            | "session.get_execution_timeout"
            | "session.get_context_budget"
            | "session.get_context_strategy"
            | "session.get_context_window"
            | "session.get_output_validation"
            | "session.get_validation_retries"
            | "session.get_system_prompt"
            | "session.get_precognition"
            | "session.get_precognition_results" => {
                to_response(id, self.dispatch_session_config_getter(&req).await)
            }
            // Kiln CRUD handlers
            "kiln.open" => to_response(id, self.handle_kiln_open(&req).await),
            "kiln.close" => to_response(id, self.handle_kiln_close(&req).await),
            "kiln.list" => to_response(id, self.handle_kiln_list(&req).await),
            "kiln.set_classification" => {
                to_response(id, self.handle_kiln_set_classification(&req).await)
            }

            // Note search and retrieval handlers
            "search_vectors" => to_response(id, self.handle_search_vectors(&req).await),
            "list_notes" => to_response(id, self.handle_list_notes(&req).await),
            "get_note_by_name" => to_response(id, self.handle_get_note_by_name(&req).await),
            "suggest_links" => to_response(id, self.handle_suggest_links(&req).await),

            // Note CRUD handlers
            "note.upsert" => to_response(id, self.handle_note_upsert(&req).await),
            "note.get" => to_response(id, self.handle_note_get(&req).await),
            "note.delete" => to_response(id, self.handle_note_delete(&req).await),
            "note.list" => to_response(id, self.handle_note_list(&req).await),

            // Processing handlers
            "process_file" => to_response(id, self.handle_process_file(&req).await),
            "process_batch" => to_response(id, self.handle_process_batch(&req).await),

            // Models handler
            "models.list" => to_response(id, self.handle_models_list(&req).await),
            "providers.list" => to_response(id, self.handle_providers_list(&req).await),

            // Session lifecycle handlers
            "session.create" => to_response(id, self.handle_session_create(&req).await),
            "session.list" => to_response(id, self.handle_session_list(&req).await),
            "session.get" => to_response(id, self.handle_session_get(&req).await),
            "session.pause" => to_response(id, self.handle_session_pause(&req).await),
            "session.resume" => to_response(id, self.handle_session_resume(&req).await),
            "session.resume_from_storage" => {
                to_response(id, self.handle_session_resume_from_storage(&req).await)
            }
            "session.end" => to_response(id, self.handle_session_end(&req).await),
            "session.archive" => to_response(id, self.handle_session_archive(&req).await),
            "session.unarchive" => to_response(id, self.handle_session_unarchive(&req).await),
            "session.delete" => to_response(id, self.handle_session_delete(&req).await),
            "session.compact" => to_response(id, self.handle_session_compact(&req).await),
            "session.fork" => to_response(id, self.handle_session_fork(&req).await),

            // Session utility handlers
            "session.search" => to_response(id, self.handle_session_search(&req).await),
            "session.load_events" => to_response(id, self.handle_session_load_events(&req).await),
            "session.list_persisted" => {
                to_response(id, self.handle_session_list_persisted(&req).await)
            }
            "session.render_markdown" => {
                to_response(id, self.handle_session_render_markdown(&req).await)
            }
            "session.export_to_file" => {
                to_response(id, self.handle_session_export_to_file(&req).await)
            }
            "session.cleanup" => to_response(id, self.handle_session_cleanup(&req).await),
            "session.reindex" => to_response(id, self.handle_session_reindex(&req).await),

            // Agent operation handlers
            "session.configure_agent" => {
                to_response(id, self.handle_session_configure_agent(&req).await)
            }
            "session.send_message" => to_response(id, self.handle_session_send_message(&req).await),
            "session.inject_context" => {
                to_response(id, self.handle_session_inject_context(&req).await)
            }
            "session.cancel" => to_response(id, self.handle_session_cancel(&req).await),
            "session.interaction_respond" => {
                to_response(id, self.handle_session_interaction_respond(&req).await)
            }
            "session.switch_model" => to_response(id, self.handle_session_switch_model(&req).await),
            "session.list_models" => to_response(id, self.handle_session_list_models(&req).await),
            "session.add_notification" => {
                to_response(id, self.handle_session_add_notification(&req).await)
            }
            "session.list_notifications" => {
                to_response(id, self.handle_session_list_notifications(&req).await)
            }
            "session.dismiss_notification" => {
                to_response(id, self.handle_session_dismiss_notification(&req).await)
            }
            "session.test_interaction" => {
                to_response(id, self.handle_session_test_interaction(&req).await)
            }
            "session.replay" => to_response(id, self.handle_session_replay(&req).await),

            // Undo handlers
            "session.undo" => to_response(id, self.handle_session_undo(&req).await),
            "session.can_undo" => to_response(id, self.handle_session_can_undo(&req).await),
            "session.undo_depth" => to_response(id, self.handle_session_undo_depth(&req).await),

            // Lua RPC handlers
            "lua.init_session" => to_response(id, self.handle_lua_init_session(&req).await),
            "lua.register_hooks" => to_response(id, self.handle_lua_register_hooks(&req).await),
            "lua.execute_hook" => to_response(id, self.handle_lua_execute_hook(&req).await),
            "lua.shutdown_session" => to_response(id, self.handle_lua_shutdown_session(&req).await),
            "lua.discover_plugins" => to_response(id, self.handle_lua_discover_plugins(&req).await),
            "lua.plugin_health" => to_response(id, self.handle_lua_plugin_health(&req).await),
            "lua.generate_stubs" => to_response(id, self.handle_lua_generate_stubs(&req).await),
            "lua.run_plugin_tests" => to_response(id, self.handle_lua_run_plugin_tests(&req).await),
            "lua.register_commands" => {
                to_response(id, self.handle_lua_register_commands(&req).await)
            }
            "lua.eval" => to_response(id, self.handle_lua_eval(&req).await),

            // Plugin RPC handlers
            "plugin.reload" => to_response(id, self.handle_plugin_reload(&req).await),
            "plugin.list" => to_response(id, self.handle_plugin_list(&req).await),

            // Project RPC handlers
            "project.register" => to_response(id, self.handle_project_register(&req).await),
            "project.unregister" => to_response(id, self.handle_project_unregister(&req).await),
            "project.list" => to_response(id, self.handle_project_list(&req).await),
            "project.get" => to_response(id, self.handle_project_get(&req).await),

            // Storage RPC handlers
            "storage.verify" => to_response(id, self.handle_storage_verify(&req).await),
            "storage.cleanup" => to_response(id, self.handle_storage_cleanup(&req).await),
            "storage.backup" => to_response(id, self.handle_storage_backup(&req).await),
            "storage.restore" => to_response(id, self.handle_storage_restore(&req).await),

            // MCP RPC handlers
            "mcp.start" => to_response(id, self.handle_mcp_start(&req).await),
            "mcp.stop" => to_response(id, self.handle_mcp_stop(&req).await),
            "mcp.status" => to_response(id, self.handle_mcp_status(&req).await),

            // Skills RPC handlers
            "skills.list" => to_response(id, self.handle_skills_list(&req).await),
            "skills.get" => to_response(id, self.handle_skills_get(&req).await),
            "skills.search" => to_response(id, self.handle_skills_search(&req).await),

            // Agents RPC handlers
            "agents.list_profiles" => to_response(id, self.handle_agents_list_profiles(&req).await),
            "agents.resolve_profile" => {
                to_response(id, self.handle_agents_resolve_profile(&req).await)
            }

            // Subagent RPC handlers
            "subagent.collect" => to_response(id, self.handle_subagent_collect(&req).await),

            // Webhook RPC handler
            "webhook.receive" => to_response(id, self.handle_webhook_receive(&req)),

            _ => Response::error(
                id,
                METHOD_NOT_FOUND,
                format!("Method not found: '{}'", req.method),
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
            "build_sha": option_env!("CRUCIBLE_BUILD_SHA").unwrap_or("dev"),
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

    /// Route a `session.set_*` method to the corresponding server handler.
    ///
    /// All session config setters share the signature `(Request, &AgentManager, &Sender) -> Response`.
    /// This avoids 13 near-identical one-line forwarding methods.
    async fn dispatch_session_config_setter(&self, req: &Request) -> RpcResult<serde_json::Value> {
        use crate::server::session;
        let resp = match req.method.as_str() {
            "session.set_thinking_budget" => {
                session::handle_session_set_thinking_budget(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.event_tx,
                )
                .await
            }
            "session.set_temperature" => {
                session::handle_session_set_temperature(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.event_tx,
                )
                .await
            }
            "session.set_max_tokens" => {
                session::handle_session_set_max_tokens(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.event_tx,
                )
                .await
            }
            "session.set_max_iterations" => {
                session::handle_session_set_max_iterations(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.event_tx,
                )
                .await
            }
            "session.set_execution_timeout" => {
                session::handle_session_set_execution_timeout(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.event_tx,
                )
                .await
            }
            "session.set_context_budget" => {
                session::handle_session_set_context_budget(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.event_tx,
                )
                .await
            }
            "session.set_context_strategy" => {
                session::handle_session_set_context_strategy(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.event_tx,
                )
                .await
            }
            "session.set_context_window" => {
                session::handle_session_set_context_window(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.event_tx,
                )
                .await
            }
            "session.set_output_validation" => {
                session::handle_session_set_output_validation(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.event_tx,
                )
                .await
            }
            "session.set_validation_retries" => {
                session::handle_session_set_validation_retries(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.event_tx,
                )
                .await
            }
            "session.set_system_prompt" => {
                session::handle_session_set_system_prompt(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.event_tx,
                )
                .await
            }
            "session.set_precognition" => {
                session::handle_session_set_precognition(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.event_tx,
                )
                .await
            }
            "session.set_precognition_results" => {
                session::handle_session_set_precognition_results(
                    req.clone(),
                    &self.ctx.agents,
                    &self.ctx.event_tx,
                )
                .await
            }
            _ => unreachable!("dispatch match already filtered to known setter methods"),
        };
        map_server_resp(resp)
    }

    /// Route a `session.get_*` method to the corresponding server handler.
    ///
    /// All session config getters share the signature `(Request, &AgentManager) -> Response`.
    async fn dispatch_session_config_getter(&self, req: &Request) -> RpcResult<serde_json::Value> {
        use crate::server::session;
        let resp = match req.method.as_str() {
            "session.get_thinking_budget" => {
                session::handle_session_get_thinking_budget(req.clone(), &self.ctx.agents).await
            }
            "session.get_temperature" => {
                session::handle_session_get_temperature(req.clone(), &self.ctx.agents).await
            }
            "session.get_max_tokens" => {
                session::handle_session_get_max_tokens(req.clone(), &self.ctx.agents).await
            }
            "session.get_max_iterations" => {
                session::handle_session_get_max_iterations(req.clone(), &self.ctx.agents).await
            }
            "session.get_execution_timeout" => {
                session::handle_session_get_execution_timeout(req.clone(), &self.ctx.agents).await
            }
            "session.get_context_budget" => {
                session::handle_session_get_context_budget(req.clone(), &self.ctx.agents).await
            }
            "session.get_context_strategy" => {
                session::handle_session_get_context_strategy(req.clone(), &self.ctx.agents).await
            }
            "session.get_context_window" => {
                session::handle_session_get_context_window(req.clone(), &self.ctx.agents).await
            }
            "session.get_output_validation" => {
                session::handle_session_get_output_validation(req.clone(), &self.ctx.agents).await
            }
            "session.get_validation_retries" => {
                session::handle_session_get_validation_retries(req.clone(), &self.ctx.agents).await
            }
            "session.get_system_prompt" => {
                session::handle_session_get_system_prompt(req.clone(), &self.ctx.agents).await
            }
            "session.get_precognition" => {
                session::handle_session_get_precognition(req.clone(), &self.ctx.agents).await
            }
            "session.get_precognition_results" => {
                session::handle_session_get_precognition_results(req.clone(), &self.ctx.agents)
                    .await
            }
            _ => unreachable!("dispatch match already filtered to known getter methods"),
        };
        map_server_resp(resp)
    }

    async fn handle_kiln_open(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::kiln::handle_kiln_open(
            req.clone(),
            &self.ctx.kiln,
            &self.ctx.plugin_loader,
            &self.ctx.event_tx,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_kiln_close(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::kiln::handle_kiln_close(req.clone(), &self.ctx.kiln).await;
        map_server_resp(resp)
    }

    async fn handle_kiln_list(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::kiln::handle_kiln_list(req.clone(), &self.ctx.kiln).await;
        map_server_resp(resp)
    }

    async fn handle_kiln_set_classification(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::kiln::handle_kiln_set_classification(req.clone(), &self.ctx.kiln).await;
        map_server_resp(resp)
    }

    async fn handle_search_vectors(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::kiln::handle_search_vectors(req.clone(), &self.ctx.kiln).await;
        map_server_resp(resp)
    }

    async fn handle_list_notes(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::kiln::handle_list_notes(req.clone(), &self.ctx.kiln).await;
        map_server_resp(resp)
    }

    async fn handle_get_note_by_name(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::kiln::handle_get_note_by_name(req.clone(), &self.ctx.kiln).await;
        map_server_resp(resp)
    }

    async fn handle_suggest_links(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::kiln::handle_suggest_links(req.clone(), &self.ctx.kiln).await;
        map_server_resp(resp)
    }

    async fn handle_note_upsert(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::kiln::handle_note_upsert(req.clone(), &self.ctx.kiln).await;
        map_server_resp(resp)
    }

    async fn handle_note_get(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::kiln::handle_note_get(req.clone(), &self.ctx.kiln).await;
        map_server_resp(resp)
    }

    async fn handle_note_delete(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::kiln::handle_note_delete(req.clone(), &self.ctx.kiln).await;
        map_server_resp(resp)
    }

    async fn handle_note_list(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::kiln::handle_note_list(req.clone(), &self.ctx.kiln).await;
        map_server_resp(resp)
    }

    async fn handle_process_file(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::kiln::handle_process_file(req.clone(), &self.ctx.kiln).await;
        map_server_resp(resp)
    }

    async fn handle_process_batch(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::kiln::handle_process_batch(
            req.clone(),
            &self.ctx.kiln,
            &self.ctx.event_tx,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_models_list(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_models_list(req.clone(), &self.ctx.agents).await;
        map_server_resp(resp)
    }

    async fn handle_providers_list(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::session::handle_providers_list(req.clone(), &self.ctx.agents).await;
        map_server_resp(resp)
    }

    // ── Session lifecycle wrappers ────────────────────────────────────────────

    async fn handle_session_create(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_create(
            req.clone(),
            &self.ctx.sessions,
            &self.ctx.project_manager,
            &self.ctx.llm_config,
            &self.ctx.kiln,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_list(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_list(
            req.clone(),
            &self.ctx.sessions,
            &self.ctx.kiln,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_get(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::session::handle_session_get(req.clone(), &self.ctx.sessions).await;
        map_server_resp(resp)
    }

    async fn handle_session_pause(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::session::handle_session_pause(req.clone(), &self.ctx.sessions).await;
        map_server_resp(resp)
    }

    async fn handle_session_resume(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::session::handle_session_resume(req.clone(), &self.ctx.sessions).await;
        map_server_resp(resp)
    }

    async fn handle_session_resume_from_storage(
        &self,
        req: &Request,
    ) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_resume_from_storage(
            req.clone(),
            &self.ctx.sessions,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_end(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_end(
            req.clone(),
            &self.ctx.sessions,
            &self.ctx.agents,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_archive(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_archive(
            req.clone(),
            &self.ctx.sessions,
            &self.ctx.agents,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_unarchive(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_unarchive(
            req.clone(),
            &self.ctx.sessions,
            &self.ctx.agents,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_delete(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_delete(
            req.clone(),
            &self.ctx.sessions,
            &self.ctx.agents,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_compact(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::session::handle_session_compact(req.clone(), &self.ctx.sessions).await;
        map_server_resp(resp)
    }

    async fn handle_session_fork(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_fork(
            req.clone(),
            &self.ctx.sessions,
            &self.ctx.agents,
        )
        .await;
        map_server_resp(resp)
    }

    // ── Session utility wrappers ─────────────────────────────────────────────

    async fn handle_session_search(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::session::handle_session_search(req.clone(), &self.ctx.sessions).await;
        map_server_resp(resp)
    }

    async fn handle_session_load_events(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::observe::handle_session_load_events(req.clone()).await;
        map_server_resp(resp)
    }

    async fn handle_session_list_persisted(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::observe::handle_session_list_persisted(req.clone()).await;
        map_server_resp(resp)
    }

    async fn handle_session_render_markdown(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::observe::handle_session_render_markdown(req.clone()).await;
        map_server_resp(resp)
    }

    async fn handle_session_export_to_file(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::observe::handle_session_export_to_file(req.clone()).await;
        map_server_resp(resp)
    }

    async fn handle_session_cleanup(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::observe::handle_session_cleanup(req.clone()).await;
        map_server_resp(resp)
    }

    async fn handle_session_reindex(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::observe::handle_session_reindex(req.clone(), &self.ctx.kiln).await;
        map_server_resp(resp)
    }

    // ── Agent operation wrappers ─────────────────────────────────────────────

    async fn handle_session_configure_agent(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::session::handle_session_configure_agent(req.clone(), &self.ctx.agents)
                .await;
        map_server_resp(resp)
    }

    async fn handle_session_send_message(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_send_message(
            req.clone(),
            &self.ctx.agents,
            &self.ctx.event_tx,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_inject_context(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_inject_context(
            req.clone(),
            &self.ctx.sessions,
            &self.ctx.event_tx,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_cancel(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::session::handle_session_cancel(req.clone(), &self.ctx.agents).await;
        map_server_resp(resp)
    }

    async fn handle_session_interaction_respond(
        &self,
        req: &Request,
    ) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_interaction_respond(
            req.clone(),
            &self.ctx.agents,
            &self.ctx.event_tx,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_switch_model(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_switch_model(
            req.clone(),
            &self.ctx.agents,
            &self.ctx.event_tx,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_list_models(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::session::handle_session_list_models(req.clone(), &self.ctx.agents).await;
        map_server_resp(resp)
    }

    async fn handle_session_add_notification(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_add_notification(
            req.clone(),
            &self.ctx.agents,
            &self.ctx.event_tx,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_list_notifications(
        &self,
        req: &Request,
    ) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_list_notifications(
            req.clone(),
            &self.ctx.agents,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_dismiss_notification(
        &self,
        req: &Request,
    ) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_dismiss_notification(
            req.clone(),
            &self.ctx.agents,
            &self.ctx.event_tx,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_test_interaction(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_test_interaction(
            req.clone(),
            &self.ctx.event_tx,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_replay(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_replay(
            req.clone(),
            &self.ctx.sessions,
            &self.ctx.event_tx,
        )
        .await;
        map_server_resp(resp)
    }

    // ── Undo RPC wrappers ────────────────────────────────────────────────

    async fn handle_session_undo(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_undo(
            req.clone(),
            &self.ctx.agents,
            &self.ctx.event_tx,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_can_undo(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::session::handle_session_can_undo(req.clone(), &self.ctx.agents).await;
        map_server_resp(resp)
    }

    async fn handle_session_undo_depth(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::session::handle_session_undo_depth(req.clone(), &self.ctx.agents).await;
        map_server_resp(resp)
    }

    // ── Lua RPC wrappers ─────────────────────────────────────────────────

    async fn handle_lua_init_session(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::lua::handle_lua_init_session(req.clone(), &self.ctx.lua_sessions).await;
        map_server_resp(resp)
    }

    async fn handle_lua_register_hooks(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::lua::handle_lua_register_hooks(req.clone(), &self.ctx.lua_sessions)
                .await;
        map_server_resp(resp)
    }

    async fn handle_lua_execute_hook(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::lua::handle_lua_execute_hook(req.clone(), &self.ctx.lua_sessions).await;
        map_server_resp(resp)
    }

    async fn handle_lua_shutdown_session(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::lua::handle_lua_shutdown_session(req.clone(), &self.ctx.lua_sessions)
                .await;
        map_server_resp(resp)
    }

    async fn handle_lua_discover_plugins(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::lua::handle_lua_discover_plugins(req.clone()).await;
        map_server_resp(resp)
    }

    async fn handle_lua_plugin_health(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::lua::handle_lua_plugin_health(req.clone()).await;
        map_server_resp(resp)
    }

    async fn handle_lua_generate_stubs(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::lua::handle_lua_generate_stubs(req.clone()).await;
        map_server_resp(resp)
    }

    async fn handle_lua_run_plugin_tests(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::lua::handle_lua_run_plugin_tests(req.clone()).await;
        map_server_resp(resp)
    }

    async fn handle_lua_register_commands(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::lua::handle_lua_register_commands(req.clone(), &self.ctx.lua_sessions)
                .await;
        map_server_resp(resp)
    }

    // SAFETY: lua.eval executes arbitrary code in the daemon's Lua VM.
    // This is safe because the daemon socket is protected by filesystem permissions
    // (same-user access only). If the daemon is ever exposed over TCP, this
    // endpoint MUST require authentication.
    async fn handle_lua_eval(&self, req: &Request) -> RpcResult<serde_json::Value> {
        use crate::rpc::params::parse_params;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            code: String,
        }

        let params: Params = parse_params(req)?;
        let loader_guard = self.ctx.plugin_loader.lock().await;
        match loader_guard.as_ref() {
            Some(loader) => match loader.eval(&params.code).await {
                Ok(result) => Ok(serde_json::json!({ "result": result })),
                Err(e) => Err(RpcError {
                    code: INTERNAL_ERROR,
                    message: e.to_string(),
                    data: None,
                }),
            },
            None => Err(RpcError {
                code: INTERNAL_ERROR,
                message: "Lua runtime not initialized".to_string(),
                data: None,
            }),
        }
    }

    // ── Plugin RPC wrappers ──────────────────────────────────────────────

    async fn handle_plugin_reload(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::plugins::handle_plugin_reload(req.clone(), &self.ctx.plugin_loader)
                .await;
        map_server_resp(resp)
    }

    async fn handle_plugin_list(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::plugins::handle_plugin_list(req.clone(), &self.ctx.plugin_loader).await;
        map_server_resp(resp)
    }

    // ── Project RPC wrappers ────────────────────────────────────────────

    async fn handle_project_register(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::plugins::handle_project_register(req.clone(), &self.ctx.project_manager)
                .await;
        map_server_resp(resp)
    }

    async fn handle_project_unregister(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::plugins::handle_project_unregister(
            req.clone(),
            &self.ctx.project_manager,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_project_list(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::plugins::handle_project_list(req.clone(), &self.ctx.project_manager)
                .await;
        map_server_resp(resp)
    }

    async fn handle_project_get(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::plugins::handle_project_get(req.clone(), &self.ctx.project_manager)
                .await;
        map_server_resp(resp)
    }

    // ── Storage RPC wrappers ────────────────────────────────────────────

    async fn handle_storage_verify(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::storage::handle_storage_verify(req.clone()).await;
        map_server_resp(resp)
    }

    async fn handle_storage_cleanup(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::storage::handle_storage_cleanup(req.clone()).await;
        map_server_resp(resp)
    }

    async fn handle_storage_backup(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::storage::handle_storage_backup(req.clone()).await;
        map_server_resp(resp)
    }

    async fn handle_storage_restore(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::storage::handle_storage_restore(req.clone()).await;
        map_server_resp(resp)
    }

    // ── MCP RPC wrappers ────────────────────────────────────────────────

    async fn handle_mcp_start(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::platform::handle_mcp_start(
            req.clone(),
            &self.ctx.kiln,
            &self.ctx.mcp_server_manager,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_mcp_stop(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::platform::handle_mcp_stop(req.clone(), &self.ctx.mcp_server_manager)
                .await;
        map_server_resp(resp)
    }

    async fn handle_mcp_status(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::platform::handle_mcp_status(req.clone(), &self.ctx.mcp_server_manager)
                .await;
        map_server_resp(resp)
    }

    // ── Skills RPC wrappers ─────────────────────────────────────────────

    async fn handle_skills_list(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::platform::handle_skills_list(req.clone()).await;
        map_server_resp(resp)
    }

    async fn handle_skills_get(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::platform::handle_skills_get(req.clone()).await;
        map_server_resp(resp)
    }

    async fn handle_skills_search(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::platform::handle_skills_search(req.clone()).await;
        map_server_resp(resp)
    }

    // ── Agents RPC wrappers ─────────────────────────────────────────────

    async fn handle_agents_list_profiles(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::platform::handle_agents_list_profiles(req.clone(), &self.ctx.agents)
                .await;
        map_server_resp(resp)
    }

    async fn handle_agents_resolve_profile(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::platform::handle_agents_resolve_profile(req.clone(), &self.ctx.agents)
                .await;
        map_server_resp(resp)
    }

    // ── Subagent RPC handlers ─────────────────────────────────────────────

    async fn handle_subagent_collect(&self, req: &Request) -> RpcResult<serde_json::Value> {
        use crate::rpc::params::parse_params;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            job_ids: Vec<String>,
            #[serde(default = "default_collect_timeout")]
            timeout_secs: f64,
        }

        fn default_collect_timeout() -> f64 {
            120.0
        }

        let p: Params = parse_params(req)?;
        let manager = self.ctx.agents.background_manager();
        let timeout = std::time::Duration::from_secs_f64(p.timeout_secs);
        let results = manager.collect_jobs(&p.job_ids, timeout).await;

        Ok(serde_json::json!({ "results": results }))
    }

    // ── Webhook RPC handler ─────────────────────────────────────────────

    fn handle_webhook_receive(&self, req: &Request) -> RpcResult<serde_json::Value> {
        use crate::rpc::params::parse_params;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            name: String,
            headers: serde_json::Map<String, serde_json::Value>,
            body: String,
        }

        let p: Params = parse_params(req)?;

        let event = crate::protocol::SessionEventMessage::new(
            "__webhook__",
            "webhook:received",
            serde_json::json!({
                "name": p.name,
                "headers": p.headers,
                "body": p.body,
            }),
        );

        // Best-effort broadcast — no subscribers is fine
        let _ = self.ctx.event_tx.send(event);

        Ok(serde_json::json!({ "status": "ok" }))
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
        use crate::agent_manager::{AgentManager, AgentManagerParams};
        use crate::background_manager::BackgroundJobManager;

        use crate::kiln_manager::KilnManager;
        use crate::mcp_server::McpServerManager;
        use crate::project_manager::ProjectManager;
        use crate::session_manager::SessionManager;
        use crate::subscription::SubscriptionManager;
        use crate::tools::workspace::WorkspaceTools;
        use dashmap::DashMap;
        use tokio::sync::broadcast;

        let (event_tx, _) = broadcast::channel(16);
        let (shutdown_tx, _) = broadcast::channel(1);
        let kiln_manager = Arc::new(KilnManager::new());
        let session_manager = Arc::new(SessionManager::new());
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx.clone()));
        let agent_manager = Arc::new(AgentManager::new(AgentManagerParams {
            kiln_manager: kiln_manager.clone(),
            session_manager: session_manager.clone(),
            background_manager,
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            permission_config: None,
            plugin_loader: None,
            workspace_tools: Arc::new(WorkspaceTools::new(std::path::PathBuf::from("/tmp"))),
        }));

        RpcContext::new(
            kiln_manager,
            session_manager,
            agent_manager,
            Arc::new(SubscriptionManager::new()),
            event_tx,
            shutdown_tx,
            Arc::new(ProjectManager::new(std::path::PathBuf::from(
                "/tmp/projects.json",
            ))),
            Arc::new(DashMap::new()),
            Arc::new(tokio::sync::Mutex::new(None)),
            None,
            Arc::new(McpServerManager::new()),
        )
    }

    #[test]
    fn methods_list_includes_core_methods() {
        assert!(METHODS.contains(&"ping"));
        assert!(METHODS.contains(&"daemon.capabilities"));
        assert!(METHODS.contains(&"session.subscribe"));
        assert!(METHODS.contains(&"session.set_thinking_budget"));
        assert!(METHODS.contains(&"subagent.collect"));
    }

    #[test]
    fn methods_count() {
        assert_eq!(METHODS.len(), 112, "Update when adding RPC methods");
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
