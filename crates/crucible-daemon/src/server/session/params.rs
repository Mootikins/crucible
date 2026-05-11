use super::super::*;
use crate::{optional_param, require_param};

use crucible_core::session::{ContextStrategy, OutputValidation};

pub(crate) async fn handle_session_set_thinking_budget(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let budget = optional_param!(req, "thinking_budget", as_i64);

    // When budget is None, clear the thinking budget override
    let effective_budget = budget.unwrap_or(0);

    match am
        .set_thinking_budget(session_id, effective_budget, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "thinking_budget": budget,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_thinking_budget(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_thinking_budget(session_id) {
        Ok(budget) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "thinking_budget": budget,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_set_system_prompt(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let prompt = require_param!(req, "system_prompt", as_str);

    match am
        .set_system_prompt(session_id, prompt, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "system_prompt": prompt,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_system_prompt(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_system_prompt(session_id) {
        Ok(prompt) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "system_prompt": prompt,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_set_precognition(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let enabled = optional_param!(req, "enabled", as_bool).unwrap_or(true);

    match am
        .set_precognition(session_id, enabled, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "precognition_enabled": enabled,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_precognition(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_precognition(session_id) {
        Ok(enabled) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "precognition_enabled": enabled,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_set_precognition_results(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let count = optional_param!(req, "precognition_results", as_u64).unwrap_or(5) as usize;

    match am
        .set_precognition_results(session_id, count, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "precognition_results": count,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_precognition_results(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_precognition_results(session_id) {
        Ok(count) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "precognition_results": count,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_set_temperature(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let temperature = require_param!(req, "temperature", as_f64);

    match am
        .set_temperature(session_id, temperature, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "temperature": temperature,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_temperature(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_temperature(session_id) {
        Ok(temperature) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "temperature": temperature,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_set_max_tokens(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    // max_tokens can be null to clear the limit, so we use optional
    let max_tokens = optional_param!(req, "max_tokens", as_u64).map(|v| v as u32);

    match am
        .set_max_tokens(session_id, max_tokens, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "max_tokens": max_tokens,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_max_tokens(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_max_tokens(session_id) {
        Ok(max_tokens) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "max_tokens": max_tokens,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_set_max_iterations(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    // max_iterations can be null to clear the limit (unlimited), so we use optional
    let max_iterations = optional_param!(req, "max_iterations", as_u64).map(|v| v as u32);

    match am
        .set_max_iterations(session_id, max_iterations, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "max_iterations": max_iterations,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_max_iterations(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_max_iterations(session_id) {
        Ok(max_iterations) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "max_iterations": max_iterations,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_set_execution_timeout(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    // timeout_secs can be null to clear the timeout, so we use optional
    let timeout_secs = optional_param!(req, "timeout_secs", as_u64);

    match am
        .set_execution_timeout(session_id, timeout_secs, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "timeout_secs": timeout_secs,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_execution_timeout(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_execution_timeout(session_id) {
        Ok(timeout_secs) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "timeout_secs": timeout_secs,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_set_context_budget(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let context_budget = optional_param!(req, "context_budget", as_u64).map(|v| v as usize);

    match am
        .set_context_budget(session_id, context_budget, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "context_budget": context_budget,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_context_budget(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_context_budget(session_id) {
        Ok(context_budget) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "context_budget": context_budget,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_set_context_strategy(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let strategy_str = require_param!(req, "context_strategy", as_str);

    let strategy = match strategy_str.parse::<ContextStrategy>() {
        Ok(s) => s,
        Err(e) => return Response::error(req.id, INVALID_PARAMS, e),
    };

    match am
        .set_context_strategy(session_id, strategy, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "context_strategy": strategy_str,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_context_strategy(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_context_strategy(session_id) {
        Ok(strategy) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "context_strategy": strategy.to_string(),
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_set_context_window(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let context_window = optional_param!(req, "context_window", as_u64).map(|v| v as usize);

    match am
        .set_context_window(session_id, context_window, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "context_window": context_window,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_context_window(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_context_window(session_id) {
        Ok(context_window) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "context_window": context_window,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_set_output_validation(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let validation_str = require_param!(req, "output_validation", as_str);

    let validation = match validation_str.parse::<OutputValidation>() {
        Ok(v) => v,
        Err(e) => return Response::error(req.id, INVALID_PARAMS, e),
    };

    match am
        .set_output_validation(session_id, validation, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "output_validation": validation_str,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_output_validation(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_output_validation(session_id) {
        Ok(validation) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "output_validation": validation.to_string(),
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_set_validation_retries(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let retries = require_param!(req, "validation_retries", as_u64) as u32;

    match am
        .set_validation_retries(session_id, retries, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "validation_retries": retries,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_validation_retries(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_validation_retries(session_id) {
        Ok(retries) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "validation_retries": retries,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_undo(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let count = optional_param!(req, "count", as_u64).unwrap_or(1) as usize;

    match am.undo(session_id, count, Some(event_tx)).await {
        Ok(summaries) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "undone": summaries,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::ConcurrentRequest(id)) => Response::error(
            req.id,
            INVALID_PARAMS,
            format!("Cannot undo while a request is in progress for session: {id}"),
        ),
        Err(crate::agent_manager::AgentError::NotSupported(msg))
        | Err(crate::agent_manager::AgentError::Chat(
            crucible_core::traits::chat::ChatError::NotSupported(msg),
        )) => Response::error(req.id, crate::protocol::METHOD_NOT_FOUND, msg),
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_can_undo(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.can_undo(session_id) {
        Ok(can_undo) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "can_undo": can_undo,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_undo_depth(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.undo_depth(session_id) {
        Ok(depth) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "undo_depth": depth,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_set_autocompact_threshold(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let threshold = optional_param!(req, "autocompact_threshold", as_f64).map(|v| v as f32);

    match am
        .set_autocompact_threshold(session_id, threshold, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "autocompact_threshold": threshold,
            }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_autocompact_threshold(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.get_autocompact_threshold(session_id) {
        Ok(threshold) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "autocompact_threshold": threshold,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

/// `session.cache_stats` — return the per-session prompt-cache aggregate.
/// `hit_rate` is `null` until at least one completion has reported cache
/// fields, distinguishing "never had a cache event" from "0%".
/// `session.set_grammar` — attach a GBNF grammar.
///
/// Body: `{ "session_id": "...", "content": "...", "name": "..." }`.
/// `name` is optional. Backend-unsupported attach hard-errors with the
/// daemon `NotSupported` shape (METHOD_NOT_FOUND code so callers can
/// distinguish "wrong feature" from "wrong session").
pub(crate) async fn handle_session_set_grammar(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let content = require_param!(req, "content", as_str);
    let name = optional_param!(req, "name", as_str).map(|s| s.to_string());

    let grammar = match name {
        Some(n) => crucible_core::types::Grammar::named(n, content),
        None => crucible_core::types::Grammar::new(content),
    };

    match am.set_grammar(session_id, grammar, Some(event_tx)).await {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "attached": true,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NotSupported(msg))
        | Err(crate::agent_manager::AgentError::Chat(
            crucible_core::traits::chat::ChatError::NotSupported(msg),
        )) => Response::error(req.id, crate::protocol::METHOD_NOT_FOUND, msg),
        Err(e) => internal_error(req.id, e),
    }
}

/// `session.clear_grammar` — detach any attached grammar. Idempotent.
pub(crate) async fn handle_session_clear_grammar(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    match am.clear_grammar(session_id, Some(event_tx)).await {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "attached": false,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

/// `session.get_grammar` — read the currently-attached grammar (or null).
pub(crate) async fn handle_session_get_grammar(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    match am.get_grammar(session_id) {
        Ok(opt) => {
            let body = match opt {
                Some(g) => serde_json::json!({
                    "session_id": session_id,
                    "grammar": {
                        "content": g.content,
                        "name": g.name,
                    },
                }),
                None => serde_json::json!({
                    "session_id": session_id,
                    "grammar": null,
                }),
            };
            Response::success(req.id, body)
        }
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_cache_stats(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let stats = am.get_cache_stats(session_id);
    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "hits": stats.hits,
            "misses": stats.misses,
            "read_tokens": stats.read_tokens,
            "creation_tokens": stats.creation_tokens,
            "prompt_tokens": stats.prompt_tokens,
            "completion_tokens": stats.completion_tokens,
            "hit_rate": stats.hit_rate(),
        }),
    )
}
