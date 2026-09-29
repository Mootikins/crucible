use super::super::*;
use crate::rpc_helpers::typed_params;
use crucible_core::protocol::requests::{
    Scoped, SessionPluginTurnLimitRequest, SessionSetContextStrategyRequest, SessionSetModeRequest,
    SessionSetPrecognitionRequest, SessionUndoRequest,
};

use crucible_core::session::ContextStrategy;

// Each handler deserializes the request type that `DaemonClient` serializes,
// from `crucible_core::protocol::requests`. The client and the server thus
// share one set of field names, and the compiler checks it.
//
// The getters are token-identical except for the knob's result field name and
// the `AgentManager` method, so one macro generates them. The literal JSON
// result name is always an explicit `$field` argument, never a concatenation,
// so every wire name stays greppable in this file.

/// Generate a `session.get_*` handler `(Request, &AgentManager) -> Response`.
///
/// The plain form serializes the returned value directly; the `display` form
/// serializes `value.to_string()` (for enum knobs stored as typed values but
/// exposed over the wire as their string spelling).
macro_rules! session_config_getter {
    ($fn_name:ident, $method:ident, $field:tt $(,)?) => {
        session_config_getter!(@impl $fn_name, $method, $field, |value| value);
    };
    ($fn_name:ident, $method:ident, $field:tt, display $(,)?) => {
        session_config_getter!(@impl $fn_name, $method, $field, |value| value.to_string());
    };
    (@impl $fn_name:ident, $method:ident, $field:tt, |$value:ident| $render:expr) => {
        pub(crate) async fn $fn_name(req: Request, am: &Arc<AgentManager>) -> Response {
            let params = match typed_params::<Scoped<()>>(&req) {
                Ok(p) => p,
                Err(response) => return *response,
            };
            let session_id = params.session_id.as_str();

            match am.$method(session_id) {
                Ok($value) => Response::success(
                    req.id,
                    serde_json::json!({
                        "session_id": session_id,
                        $field: $render,
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
    };
}

// ── Setters ─────────────────────────────────────────────────────────────────

pub(crate) async fn handle_session_set_mode(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &crate::EventBus,
) -> Response {
    let params = match typed_params::<SessionSetModeRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = params.session_id.as_str();
    let mode_id = params.mode_id.as_str();

    match am.set_mode(session_id, mode_id, Some(event_tx)).await {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "mode_id": mode_id,
                "set": true,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
            agent_not_configured(req.id, &id)
        }
        Err(crate::agent_manager::AgentError::NotSupported(msg)) => {
            Response::error(req.id, INVALID_PARAMS, msg)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_set_precognition(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &crate::EventBus,
) -> Response {
    let params = match typed_params::<SessionSetPrecognitionRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = params.session_id.as_str();
    let enabled = params.precognition_enabled;

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

pub(crate) async fn handle_session_set_plugin_turn_limit(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &crate::EventBus,
) -> Response {
    let params = match typed_params::<SessionPluginTurnLimitRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let limit = params.limit;
    match am
        .set_plugin_turn_limit(&params.session_id, limit, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(req.id, serde_json::json!({"limit": limit})),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

// ── Getters (uniform shape: fetch → echo, sync `AgentManager` accessors) ─────

session_config_getter!(
    handle_session_get_precognition,
    get_precognition,
    "precognition_enabled"
);
session_config_getter!(handle_session_get_mode, get_mode, "mode");
session_config_getter!(
    handle_session_get_context_strategy,
    get_context_strategy,
    "context_strategy",
    display
);
session_config_getter!(
    handle_session_get_plugin_turn_limit,
    get_plugin_turn_limit,
    "limit"
);

// ── Hand-written handlers (deviate from the uniform macro shape) ────────────
//
// `set_context_strategy` parses and validates the incoming string, and
// answers INVALID_PARAMS for a bad value.
//
// The A1 result-name parity gate in `tests/architecture_tests.rs` reads each
// getter's result field from whichever form (fn body or macro invocation) the
// knob uses. The shared request type covers the request direction.

pub(crate) async fn handle_session_set_context_strategy(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &crate::EventBus,
) -> Response {
    let params = match typed_params::<SessionSetContextStrategyRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = params.session_id.as_str();
    let strategy_str = params.context_strategy.as_str();

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

pub(crate) async fn handle_session_undo(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &crate::EventBus,
) -> Response {
    let params = match typed_params::<SessionUndoRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = params.session_id.as_str();
    let count = params.count.unwrap_or(1);

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
        Err(crate::agent_manager::AgentError::NotSupported(msg)) => {
            Response::error(req.id, INVALID_PARAMS, msg)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_can_undo(req: Request, am: &Arc<AgentManager>) -> Response {
    let params = match typed_params::<Scoped<()>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = params.session_id.as_str();

    match am.can_undo(session_id).await {
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
    let params = match typed_params::<Scoped<()>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = params.session_id.as_str();

    match am.undo_depth(session_id).await {
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

/// `session.cache_stats` — return the per-session prompt-cache aggregate.
/// `hit_rate` is `null` until at least one completion has reported cache
/// fields, distinguishing "never had a cache event" from "0%".
pub(crate) async fn handle_session_cache_stats(req: Request, am: &Arc<AgentManager>) -> Response {
    let params = match typed_params::<Scoped<()>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = params.session_id.as_str();
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
