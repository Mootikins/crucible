use super::*;

use crucible_core::config::McpConfig;
use crucible_core::protocol::session_events::{
    ContextLimitResolvedPayload, ContextLimitSource, KilnNotesIndexedPayload,
    McpServersReadyPayload, PluginsDiscoveredPayload, ProvidersListedPayload,
    SessionInitializedPayload, WorkspaceIndexedPayload,
};

mod create;
mod lifecycle;
mod list;
mod messaging;
mod models;
mod notifications;
mod params;

pub(crate) use create::handle_session_create;
// Re-exported for tests.rs (accessed via `use session::*`).
#[allow(unused_imports)]
pub(crate) use create::{
    resolve_kiln_classification_for_create, resolve_provider_trust_level_for_create,
    validate_trust_level,
};
pub(crate) use lifecycle::{
    handle_session_archive, handle_session_compact, handle_session_delete, handle_session_end,
    handle_session_pause, handle_session_replay, handle_session_resume,
    handle_session_resume_from_storage, handle_session_unarchive,
};
pub(crate) use list::{handle_session_get, handle_session_list, handle_session_search};
pub(crate) use messaging::{
    handle_session_cancel, handle_session_configure_agent, handle_session_inject_context,
    handle_session_interaction_respond, handle_session_pending_interactions,
    handle_session_send_message, handle_session_test_interaction, inject_context_impl,
};
pub(crate) use models::{
    handle_models_list, handle_providers_list, handle_session_fork, handle_session_list_models,
    handle_session_switch_model,
};
pub(crate) use notifications::{
    handle_session_add_notification, handle_session_dismiss_notification,
    handle_session_list_notifications,
};
pub(crate) use params::{
    handle_session_cache_stats, handle_session_can_undo, handle_session_get_autocompact_threshold,
    handle_session_get_context_budget, handle_session_get_context_strategy,
    handle_session_get_context_window, handle_session_get_execution_timeout,
    handle_session_get_max_iterations, handle_session_get_max_tokens, handle_session_get_mode,
    handle_session_get_output_validation, handle_session_get_precognition,
    handle_session_get_precognition_results, handle_session_get_system_prompt,
    handle_session_get_temperature, handle_session_get_thinking_budget,
    handle_session_get_validation_retries, handle_session_set_autocompact_threshold,
    handle_session_set_context_budget, handle_session_set_context_strategy,
    handle_session_set_context_window, handle_session_set_execution_timeout,
    handle_session_set_max_iterations, handle_session_set_max_tokens, handle_session_set_mode,
    handle_session_set_output_validation, handle_session_set_precognition,
    handle_session_set_precognition_results, handle_session_set_system_prompt,
    handle_session_set_temperature, handle_session_set_thinking_budget,
    handle_session_set_validation_retries, handle_session_undo, handle_session_undo_depth,
};

/// Serialize `payload` and broadcast it as a setup event for `session_id`.
///
/// Failures are logged but never propagate: the setup task is best-effort and
/// must not break session creation. "No subscribers" is normal at startup
/// (the CLI subscribes slightly after `session.create` returns) and is
/// logged at `debug`, not `warn`.
fn emit_setup_event<P: serde::Serialize>(
    event_tx: &broadcast::Sender<SessionEventMessage>,
    session_id: &str,
    event_type: &str,
    payload: P,
) {
    let data = match serde_json::to_value(payload) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(event_type, error = %e, "failed to serialize setup event payload");
            return;
        }
    };
    let msg = SessionEventMessage::new(session_id.to_string(), event_type.to_string(), data);
    if !crate::event_emitter::emit_event(event_tx, msg) {
        tracing::debug!(event_type, session_id, "no subscribers for setup event");
    }
}

/// Spawn the post-`session.create` setup task.
///
/// Fires seven events:
///   1. `session_initialized` (always, first)
///   2. `workspace_indexed`, `kiln_notes_indexed`, `plugins_discovered`,
///      `mcp_servers_ready` (concurrent, in any order)
///   3. For internal agents only: `providers_listed`,
///      `context_limit_resolved` (the latter may be skipped if no endpoint)
///
/// All work is best-effort. Index/discovery failures are logged and the
/// corresponding event is skipped; they never bubble up.
fn spawn_setup_task(
    session: &crucible_core::session::Session,
    agent_type: String,
    event_tx: broadcast::Sender<SessionEventMessage>,
    am: Arc<AgentManager>,
    mcp_config: Option<McpConfig>,
) {
    let sid = session.id.clone();
    let workspace_path = session.workspace.clone();
    let kiln_path = session.kiln.clone();
    // Agent config is populated by a later `session.configure_agent` call, so
    // at create time we almost always observe `None` here and the event
    // carries empty strings. Task 1.3 (CLI) will still render progressively;
    // a future `agent_configured` event can refresh these fields on clients
    // that care. `mode` has no daemon-side representation yet; emit "normal"
    // as a placeholder that matches the default TUI mode.
    let model = session
        .agent
        .as_ref()
        .map(|a| a.model.clone())
        .unwrap_or_default();
    let agent_name = session.agent.as_ref().and_then(|a| a.agent_name.clone());
    let endpoint = session.agent.as_ref().and_then(|a| a.endpoint.clone());
    let mode = "normal".to_string();

    tokio::spawn(async move {
        // 1. session_initialized (always first)
        emit_setup_event(
            &event_tx,
            &sid,
            "session_initialized",
            SessionInitializedPayload {
                model: model.clone(),
                mode,
                agent_name,
                kiln_path: kiln_path.clone(),
                workspace_path: workspace_path.clone(),
            },
        );

        // 2. Concurrent: workspace + kiln indexers.
        let (files_res, notes_res) = tokio::join!(
            tokio::task::spawn_blocking({
                let ws = workspace_path.clone();
                move || crate::workspace::indexer::index_workspace_files(&ws)
            }),
            tokio::task::spawn_blocking({
                let k = kiln_path.clone();
                move || crate::workspace::indexer::index_kiln_notes(&k)
            }),
        );
        match files_res {
            Ok(files) => emit_setup_event(
                &event_tx,
                &sid,
                "workspace_indexed",
                WorkspaceIndexedPayload { files },
            ),
            Err(e) => {
                tracing::warn!(error = %e, "workspace indexer task failed; skipping event")
            }
        }
        match notes_res {
            Ok(notes) => emit_setup_event(
                &event_tx,
                &sid,
                "kiln_notes_indexed",
                KilnNotesIndexedPayload { notes },
            ),
            Err(e) => {
                tracing::warn!(error = %e, "kiln notes indexer task failed; skipping event")
            }
        }

        // Plugin discovery.
        //
        // `PluginManager::initialize` is moderately expensive (touches
        // disk). Run it on a blocking thread so we don't starve the tokio
        // reactor when many setup tasks run concurrently.
        let kiln_for_plugins = kiln_path.clone();
        let plugins_res = tokio::task::spawn_blocking(move || {
            super::lua::discover_plugins_for_kiln(&kiln_for_plugins)
        })
        .await;
        match plugins_res {
            Ok(Ok(plugins)) => emit_setup_event(
                &event_tx,
                &sid,
                "plugins_discovered",
                PluginsDiscoveredPayload { plugins },
            ),
            Ok(Err(e)) => tracing::warn!(error = %e, "plugin discovery failed; skipping event"),
            Err(e) => tracing::warn!(error = %e, "plugin discovery task panicked; skipping event"),
        }

        // MCP config projection. Always emit — an empty list is meaningful.
        let servers = crate::mcp::config::read_mcp_servers(mcp_config.as_ref());
        emit_setup_event(
            &event_tx,
            &sid,
            "mcp_servers_ready",
            McpServersReadyPayload { servers },
        );

        // 3. LLM-specific: only for internal agents. ACP sessions have no
        // provider-backed context window and the "providers" concept does
        // not apply to them.
        if agent_type == "internal" {
            let providers = am.list_providers(None).await;
            emit_setup_event(
                &event_tx,
                &sid,
                "providers_listed",
                ProvidersListedPayload { providers },
            );

            if let Some(endpoint) = endpoint {
                if !model.is_empty() {
                    if let Some(limit) =
                        crate::agent_manager::context_length::fetch_model_context_length(
                            &endpoint, &model,
                        )
                        .await
                    {
                        emit_setup_event(
                            &event_tx,
                            &sid,
                            "context_limit_resolved",
                            ContextLimitResolvedPayload {
                                limit,
                                source: ContextLimitSource::ProviderApi,
                            },
                        );
                    }
                }
            }
        }
    });
}
