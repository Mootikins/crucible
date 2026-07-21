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
    "embed.query",
    "list_notes",
    "get_note_by_name",
    "get_backlinks",
    "kiln.graph",
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
    "session.connect_kiln",
    "session.disconnect_kiln",
    "session.set_workspace",
    "session.set_mode",
    "session.get_mode",
    "session.list_models",
    "session.set_thinking_budget",
    "session.get_thinking_budget",
    "session.cache_stats",
    "session.set_autocompact_threshold",
    "session.get_autocompact_threshold",
    "session.add_notification",
    "session.list_notifications",
    "session.dismiss_notification",
    "session.interaction_respond",
    "session.pending_interactions",
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
    "session.generate_title",
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
    "plugin.install",
    "plugin.remove",
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
    "config.get",
    "config.set",
    "project.register",
    "project.unregister",
    "project.list",
    "project.get",
    "fs.list_dir",
    "fs.move",
    "fs.mkdir",
    "fs.trash",
    "note.rename",
    "note.move",
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
    "workflow.start",
    "workflow.approve_gate",
    "workflow.status",
    "workflow.cancel",
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

// Route a filtered `session.set_*` / `session.get_*` method string to its
// server handler. Every method literal stays paired with its handler at the
// call site (greppable, wire-name-explicit); the shared call shape lives here.
macro_rules! dispatch_session_setter {
    ($req:expr, $agents:expr, $event_tx:expr, { $($method:literal => $handler:ident),+ $(,)? }) => {
        match $req.method.as_str() {
            $(
                $method => {
                    crate::server::session::$handler($req.clone(), $agents, $event_tx).await
                }
            )+
            _ => unreachable!("dispatch match already filtered to known setter methods"),
        }
    };
}

macro_rules! dispatch_session_getter {
    ($req:expr, $agents:expr, { $($method:literal => $handler:ident),+ $(,)? }) => {
        match $req.method.as_str() {
            $(
                $method => crate::server::session::$handler($req.clone(), $agents).await,
            )+
            _ => unreachable!("dispatch match already filtered to known getter methods"),
        }
    };
}

pub struct RpcDispatcher {
    ctx: RpcContext,
}

impl RpcDispatcher {
    pub fn new(ctx: RpcContext) -> Self {
        Self { ctx }
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
            "session.generate_title" => to_response(id, self.handle_generate_title(&req).await),

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
            | "session.set_precognition_results"
            | "session.set_autocompact_threshold" => {
                to_response(id, self.dispatch_session_config_setter(&req).await)
            }
            "session.get_thinking_budget"
            | "session.get_temperature"
            | "session.get_mode"
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
            | "session.get_precognition_results"
            | "session.get_autocompact_threshold" => {
                to_response(id, self.dispatch_session_config_getter(&req).await)
            }
            "session.cache_stats" => to_response(id, self.handle_session_cache_stats(&req).await),
            // Kiln CRUD handlers
            "kiln.open" => to_response(id, self.handle_kiln_open(&req).await),
            "kiln.close" => to_response(id, self.handle_kiln_close(&req).await),
            "kiln.list" => to_response(id, self.handle_kiln_list(&req).await),
            "kiln.set_classification" => {
                to_response(id, self.handle_kiln_set_classification(&req).await)
            }

            // Note search and retrieval handlers
            "search_vectors" => to_response(id, self.handle_search_vectors(&req).await),
            "embed.query" => to_response(id, self.handle_embed_query(&req).await),
            "list_notes" => to_response(id, self.handle_list_notes(&req).await),
            "get_note_by_name" => to_response(id, self.handle_get_note_by_name(&req).await),
            "get_backlinks" => to_response(id, self.handle_get_backlinks(&req).await),
            "kiln.graph" => to_response(id, self.handle_kiln_graph(&req).await),
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
            "session.pending_interactions" => {
                to_response(id, self.handle_session_pending_interactions(&req).await)
            }
            "session.switch_model" => to_response(id, self.handle_session_switch_model(&req).await),
            "session.connect_kiln" => to_response(id, self.handle_session_connect_kiln(&req).await),
            "session.disconnect_kiln" => {
                to_response(id, self.handle_session_disconnect_kiln(&req).await)
            }
            "session.set_workspace" => {
                to_response(id, self.handle_session_set_workspace(&req).await)
            }
            "session.set_mode" => to_response(id, self.handle_session_set_mode(&req).await),
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

            // App-config store (the same store `cru.config.*` reads in Lua)
            "config.get" => to_response(id, self.handle_config_get(&req)),
            "config.set" => to_response(id, self.handle_config_set(&req)),

            // Plugin RPC handlers
            "plugin.reload" => to_response(id, self.handle_plugin_reload(&req).await),
            "plugin.list" => to_response(id, self.handle_plugin_list(&req).await),
            "plugin.install" => to_response(id, self.handle_plugin_install(&req).await),
            "plugin.remove" => to_response(id, self.handle_plugin_remove(&req).await),

            // Project RPC handlers
            "project.register" => to_response(id, self.handle_project_register(&req).await),
            "project.unregister" => to_response(id, self.handle_project_unregister(&req).await),
            "project.list" => to_response(id, self.handle_project_list(&req).await),
            "project.get" => to_response(id, self.handle_project_get(&req).await),
            "fs.list_dir" => to_response(id, self.handle_fs_list_dir(&req).await),
            "fs.move" => to_response(id, self.handle_fs_move(&req).await),
            "fs.mkdir" => to_response(id, self.handle_fs_mkdir(&req).await),
            "fs.trash" => to_response(id, self.handle_fs_trash(&req).await),
            "note.rename" | "note.move" => to_response(id, self.handle_note_rename(&req).await),

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

            // Workflow execution (Phase 3a)
            "workflow.start" => to_response(
                id,
                crate::rpc::workflow_handlers::handle_workflow_start(&self.ctx, &req).await,
            ),
            "workflow.approve_gate" => to_response(
                id,
                crate::rpc::workflow_handlers::handle_workflow_approve_gate(&self.ctx, &req).await,
            ),
            "workflow.status" => to_response(
                id,
                crate::rpc::workflow_handlers::handle_workflow_status(&self.ctx, &req).await,
            ),
            "workflow.cancel" => to_response(
                id,
                crate::rpc::workflow_handlers::handle_workflow_cancel(&self.ctx, &req).await,
            ),

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

    async fn handle_generate_title(&self, req: &Request) -> RpcResult<serde_json::Value> {
        use crate::rpc::params::parse_params;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            session_id: String,
        }
        let p: Params = parse_params(req)?;

        let title = self
            .ctx
            .agents
            .generate_session_title(&p.session_id, &self.ctx.event_tx)
            .await
            .map_err(|e| RpcError {
                code: crate::protocol::INVALID_PARAMS,
                message: format!("Failed to generate title: {}", e),
                data: None,
            })?;

        Ok(serde_json::json!({
            "session_id": p.session_id,
            "title": title,
        }))
    }

    /// Route a `session.set_*` method to the corresponding server handler.
    ///
    /// All session config setters share the signature `(Request, &AgentManager, &Sender) -> Response`.
    /// This avoids 13 near-identical one-line forwarding methods.
    async fn dispatch_session_config_setter(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = dispatch_session_setter!(req, &self.ctx.agents, &self.ctx.event_tx, {
            "session.set_thinking_budget" => handle_session_set_thinking_budget,
            "session.set_temperature" => handle_session_set_temperature,
            "session.set_max_tokens" => handle_session_set_max_tokens,
            "session.set_max_iterations" => handle_session_set_max_iterations,
            "session.set_execution_timeout" => handle_session_set_execution_timeout,
            "session.set_context_budget" => handle_session_set_context_budget,
            "session.set_context_strategy" => handle_session_set_context_strategy,
            "session.set_context_window" => handle_session_set_context_window,
            "session.set_output_validation" => handle_session_set_output_validation,
            "session.set_validation_retries" => handle_session_set_validation_retries,
            "session.set_system_prompt" => handle_session_set_system_prompt,
            "session.set_precognition" => handle_session_set_precognition,
            "session.set_precognition_results" => handle_session_set_precognition_results,
            "session.set_autocompact_threshold" => handle_session_set_autocompact_threshold,
        });
        map_server_resp(resp)
    }

    /// Route a `session.get_*` method to the corresponding server handler.
    ///
    /// All session config getters share the signature `(Request, &AgentManager) -> Response`.
    async fn dispatch_session_config_getter(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = dispatch_session_getter!(req, &self.ctx.agents, {
            "session.get_thinking_budget" => handle_session_get_thinking_budget,
            "session.get_temperature" => handle_session_get_temperature,
            "session.get_mode" => handle_session_get_mode,
            "session.get_max_tokens" => handle_session_get_max_tokens,
            "session.get_max_iterations" => handle_session_get_max_iterations,
            "session.get_execution_timeout" => handle_session_get_execution_timeout,
            "session.get_context_budget" => handle_session_get_context_budget,
            "session.get_context_strategy" => handle_session_get_context_strategy,
            "session.get_context_window" => handle_session_get_context_window,
            "session.get_output_validation" => handle_session_get_output_validation,
            "session.get_validation_retries" => handle_session_get_validation_retries,
            "session.get_system_prompt" => handle_session_get_system_prompt,
            "session.get_precognition" => handle_session_get_precognition,
            "session.get_precognition_results" => handle_session_get_precognition_results,
            "session.get_autocompact_threshold" => handle_session_get_autocompact_threshold,
        });
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
        let resp =
            crate::server::kiln::handle_kiln_list(req.clone(), &self.ctx.kiln, &self.ctx.data_home)
                .await;
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

    async fn handle_embed_query(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::kiln::handle_embed_query(req.clone(), &self.ctx.kiln).await;
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

    async fn handle_get_backlinks(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::kiln::handle_get_backlinks(req.clone(), &self.ctx.kiln).await;
        map_server_resp(resp)
    }

    async fn handle_kiln_graph(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::kiln::handle_kiln_graph(req.clone(), &self.ctx.kiln).await;
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
            &self.ctx.data_home,
            &self.ctx.llm_config,
            &self.ctx.kiln,
            &self.ctx.event_tx,
            &self.ctx.agents,
            self.ctx.mcp_config.as_ref(),
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_list(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_list(
            req.clone(),
            &self.ctx.sessions,
            &self.ctx.kiln,
            &self.ctx.data_home,
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
        // Fire on_session_end Lua hooks before ending the session.
        // Plugins use this for cleanup (e.g., releasing resources, stopping
        // services) and for agent-learning extraction (session digest, entity
        // memory). Before firing, enrich the Session userdata with kiln_path,
        // agent_name, and end_reason so handlers don't need extra RPC
        // round-trips to do their work.
        let session_id = req
            .params
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !session_id.is_empty() {
            if let Some(state) = self.ctx.lua_sessions.get(session_id) {
                let state = state.value().clone();
                let mut state = state.lock().await;
                // Daemon-side idempotency: `lua.shutdown_session` also fires
                // these hooks. Whichever path reaches us first sets the flag;
                // the second is a no-op.
                if state.end_hooks_fired {
                    tracing::debug!(
                        session_id = %session_id,
                        "on_session_end hooks already fired; skipping"
                    );
                } else {
                    if let Err(e) = state.executor.sync_session_end_hooks() {
                        tracing::warn!(session_id = %session_id, error = %e, "Failed to sync session_end hooks");
                    }
                    if let Some(session) = state.executor.session_manager().get_current() {
                        if let Err(e) = state.executor.fire_session_end_hooks(&session) {
                            tracing::warn!(session_id = %session_id, error = %e, "Failed to fire session_end hooks");
                        }
                    }
                    state.end_hooks_fired = true;
                }
            }
        }

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

    async fn handle_session_cache_stats(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::session::handle_session_cache_stats(req.clone(), &self.ctx.agents).await;
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

    async fn handle_session_pending_interactions(
        &self,
        req: &Request,
    ) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_pending_interactions(
            req.clone(),
            &self.ctx.agents,
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

    async fn handle_session_connect_kiln(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_connect_kiln(
            req.clone(),
            &self.ctx.sessions,
            &self.ctx.agents,
            &self.ctx.kiln,
            &self.ctx.llm_config,
            &self.ctx.event_tx,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_disconnect_kiln(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_disconnect_kiln(
            req.clone(),
            &self.ctx.agents,
            &self.ctx.event_tx,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_set_workspace(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_set_workspace(
            req.clone(),
            &self.ctx.sessions,
            &self.ctx.agents,
            &self.ctx.project_manager,
            &self.ctx.llm_config,
            &self.ctx.event_tx,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_session_set_mode(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::session::handle_session_set_mode(
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

    /// Read from the app-config store — the same store `cru.config.get`
    /// exposes to Lua (seeded from TOML at daemon startup, merged by
    /// `cru.config.set` / `config.set`). With `key`: one top-level value
    /// (null if absent); without: the whole object.
    fn handle_config_get(&self, req: &Request) -> RpcResult<serde_json::Value> {
        use crate::rpc::params::parse_params;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            #[serde(default)]
            key: Option<String>,
        }

        let params: Params = parse_params(req)?;
        let config = crucible_lua::get_app_config();
        Ok(match params.key {
            Some(key) => {
                let value = config
                    .as_ref()
                    .and_then(|c| c.get(&key))
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                serde_json::json!({ "value": value })
            }
            None => serde_json::json!({ "config": config }),
        })
    }

    /// Merge top-level values into the app-config store (same semantics as
    /// Lua's `cru.config.set`). Typed transport for `:set` forwarding — the
    /// TUI must never build Lua source from user input.
    fn handle_config_set(&self, req: &Request) -> RpcResult<serde_json::Value> {
        use crate::rpc::params::parse_params;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            values: serde_json::Map<String, serde_json::Value>,
        }

        let params: Params = parse_params(req)?;
        crucible_lua::merge_app_config(serde_json::Value::Object(params.values));
        Ok(serde_json::json!({ "ok": true }))
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

    async fn handle_plugin_install(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::plugins::handle_plugin_install(req.clone()).await;
        map_server_resp(resp)
    }

    async fn handle_plugin_remove(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::plugins::handle_plugin_remove(req.clone()).await;
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

    async fn handle_fs_list_dir(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::fs::handle_fs_list_dir(req.clone(), &self.ctx.project_manager).await;
        map_server_resp(resp)
    }

    async fn handle_fs_move(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::fs::handle_fs_move(
            req.clone(),
            &self.ctx.project_manager,
            &self.ctx.kiln,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_fs_mkdir(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::fs::handle_fs_mkdir(
            req.clone(),
            &self.ctx.project_manager,
            &self.ctx.kiln,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_fs_trash(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp = crate::server::fs::handle_fs_trash(
            req.clone(),
            &self.ctx.project_manager,
            &self.ctx.kiln,
        )
        .await;
        map_server_resp(resp)
    }

    async fn handle_note_rename(&self, req: &Request) -> RpcResult<serde_json::Value> {
        let resp =
            crate::server::note_refactor::handle_note_rename(req.clone(), &self.ctx.kiln).await;
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
            None,
            std::path::PathBuf::from("/tmp"),
        )
    }

    #[test]
    fn methods_list_includes_core_methods() {
        assert!(METHODS.contains(&"ping"));
        assert!(METHODS.contains(&"daemon.capabilities"));
        assert!(METHODS.contains(&"session.subscribe"));
        assert!(METHODS.contains(&"session.set_thinking_budget"));
        assert!(METHODS.contains(&"session.cache_stats"));
        assert!(METHODS.contains(&"subagent.collect"));
    }

    #[test]
    fn methods_has_no_duplicates() {
        let unique: std::collections::HashSet<_> = METHODS.iter().collect();
        assert_eq!(unique.len(), METHODS.len(), "duplicate entry in METHODS");
    }

    // METHODS is hand-maintained while the dispatch arms are the source of truth;
    // daemon.capabilities returns METHODS, so any drift silently hides methods
    // from capability-detecting clients (this happened with plugin.install/remove).
    #[test]
    fn methods_matches_dispatch_arms() {
        let src = include_str!("dispatch.rs");
        let start = src
            .find("match req.method.as_str()")
            .expect("dispatch match not found");
        let end = src[start..]
            .find("_ => Response::error")
            .expect("dispatch default arm not found")
            + start;
        let region = &src[start..end];

        let mut dispatched = std::collections::BTreeSet::new();
        let mut rest = region;
        while let Some(open) = rest.find('"') {
            let after = &rest[open + 1..];
            let Some(close) = after.find('"') else { break };
            let lit = &after[..close];
            if !lit.is_empty()
                && lit
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c == '.')
            {
                dispatched.insert(lit);
            }
            rest = &after[close + 1..];
        }

        let advertised: std::collections::BTreeSet<_> = METHODS.iter().copied().collect();
        let unadvertised: Vec<_> = dispatched.difference(&advertised).collect();
        let unreachable: Vec<_> = advertised.difference(&dispatched).collect();
        assert!(
            unadvertised.is_empty(),
            "dispatched but missing from METHODS: {unadvertised:?}"
        );
        assert!(
            unreachable.is_empty(),
            "in METHODS but no dispatch arm: {unreachable:?}"
        );
    }

    /// `config.set` merges into the same store `cru.config.get` reads (the
    /// crucible-lua app-config store), and `config.get` reads it back — the
    /// :set/:lua shared-store bridge.
    ///
    /// NOTE: that store is process-global. Under nextest each test gets its
    /// own process, but under plain `cargo test` (the justfile fallback)
    /// tests in this binary share it — so config tests here must use
    /// test-unique keys and never reset or read the whole store expecting
    /// exclusivity.
    #[tokio::test]
    async fn dispatch_config_set_then_get_round_trips() {
        let dispatcher = RpcDispatcher::new(test_context());

        let set_req = make_request(
            "config.set",
            serde_json::json!({ "values": { "myplugin.debug": true, "answer": 42 } }),
        );
        let resp = dispatcher.dispatch(ClientId::new(), set_req).await;
        assert!(resp.error.is_none(), "config.set failed: {:?}", resp.error);

        let get_req = make_request("config.get", serde_json::json!({ "key": "myplugin.debug" }));
        let resp = dispatcher.dispatch(ClientId::new(), get_req).await;
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap()["value"], serde_json::json!(true));

        // No key → the whole (merged) config object.
        let all_req = make_request("config.get", serde_json::json!({}));
        let resp = dispatcher.dispatch(ClientId::new(), all_req).await;
        let config = resp.result.unwrap();
        assert_eq!(config["config"]["answer"], serde_json::json!(42));
    }

    #[tokio::test]
    async fn dispatch_config_get_missing_key_returns_null() {
        let dispatcher = RpcDispatcher::new(test_context());
        let req = make_request(
            "config.get",
            serde_json::json!({ "key": "no.such.key.xyz" }),
        );
        let resp = dispatcher.dispatch(ClientId::new(), req).await;
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap()["value"], serde_json::Value::Null);
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

    /// Regression: `session.end` and `lua.shutdown_session` both
    /// fire `on_session_end` hooks. The CLI chat REPL invokes both — once
    /// when the user runs `:end` and again when the REPL exits — so an
    /// `on_session_end` handler was being fired twice per session lifecycle.
    /// Non-idempotent hooks (LLM calls, file writes) would have run twice.
    ///
    /// Fix: the daemon tracks per-session `end_hooks_fired` in
    /// `LuaSessionState`. The second caller short-circuits.
    #[tokio::test]
    async fn end_then_shutdown_fires_on_session_end_hook_exactly_once() {
        use crate::server::LuaSessionState;
        use crucible_core::session::SessionType;
        use crucible_lua::{LuaExecutor, LuaScriptHandlerRegistry, Session as LuaSession};
        use tempfile::TempDir;

        let tempdir = TempDir::new().unwrap();
        let kiln_root = tempdir.path().to_path_buf();
        let ctx = test_context();

        // Create a real daemon-side session so handle_session_end can find it.
        let session = ctx
            .sessions
            .create_session(
                SessionType::Chat,
                kiln_root.clone(),
                Some(kiln_root.clone()),
                Vec::new(),
                None,
            )
            .await
            .expect("create session");
        let session_id = session.id.clone();

        // Build a Lua session with a hook that increments a Lua global counter.
        let mut executor = LuaExecutor::new().expect("lua executor");
        executor
            .lua()
            .load(
                r#"
                _G.test_end_hook_count = 0
                crucible.on_session_end(function(_session)
                    _G.test_end_hook_count = _G.test_end_hook_count + 1
                end)
                "#,
            )
            .exec()
            .expect("install end hook");
        executor.sync_session_end_hooks().expect("sync end hooks");

        // Bind a LuaSession into the executor's session manager so the
        // hook dispatcher has a target.
        let lua_session = LuaSession::new("chat".to_string());
        executor.session_manager().set_current(lua_session);

        let lua = executor.lua().clone();
        let state = LuaSessionState {
            executor,
            registry: LuaScriptHandlerRegistry::new(),
            end_hooks_fired: false,
        };
        ctx.lua_sessions
            .insert(session_id.clone(), Arc::new(tokio::sync::Mutex::new(state)));

        let dispatcher = RpcDispatcher::new(ctx);

        // First: session.end (User reason)
        let resp1 = dispatcher
            .dispatch(
                ClientId::new(),
                make_request(
                    "session.end",
                    serde_json::json!({ "session_id": session_id }),
                ),
            )
            .await;
        assert!(
            resp1.error.is_none(),
            "session.end failed: {:?}",
            resp1.error
        );

        // Second: lua.shutdown_session (Shutdown reason) — pre-fix this
        // re-fires the hook against the same Lua session.
        let resp2 = dispatcher
            .dispatch(
                ClientId::new(),
                make_request(
                    "lua.shutdown_session",
                    serde_json::json!({ "session_id": session_id }),
                ),
            )
            .await;
        assert!(
            resp2.error.is_none(),
            "lua.shutdown_session failed: {:?}",
            resp2.error
        );

        // Read back the Lua counter. `lua.shutdown_session` removes the
        // session from `lua_sessions`, so we use the cloned Lua handle.
        let count: i64 = lua
            .globals()
            .get("test_end_hook_count")
            .expect("read counter");
        assert_eq!(
            count, 1,
            "on_session_end fired {count} times; expected exactly 1 \
             (session.end and lua.shutdown_session must not both fire)"
        );
    }
}
