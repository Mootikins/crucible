use super::super::*;
use crate::{optional_param, require_param};

use crucible_core::session::{ContextStrategy, OutputValidation};

// The session config-knob handlers are token-identical except for the knob's
// wire field name, the `AgentManager` method, and how the value is extracted.
// These two macros generate one handler per knob from a one-line declaration.
//
// Wire-contract invariant (the project's known RPC-parity failure mode is a
// client/server field-name mismatch): the literal JSON field name is always an
// explicit `$field` argument at the declaration site — never derived by
// concatenation — so every wire name stays greppable in this file. The value
// extracted from the request (`$extract`) is echoed back verbatim under that
// same field, preserving the exact request/response shape of the old handlers.

/// Generate a `session.set_*` handler `(Request, &AgentManager, &Sender) -> Response`.
///
/// `$req` is threaded through as an explicit identifier so the caller-supplied
/// `$extract` (which uses `require_param!`/`optional_param!` and may early-return
/// on a missing/invalid param) shares hygiene with the generated `req` binding.
macro_rules! session_config_setter {
    ($fn_name:ident, $req:ident, $method:ident, $field:tt, $extract:expr $(,)?) => {
        pub(crate) async fn $fn_name(
            $req: Request,
            am: &Arc<AgentManager>,
            event_tx: &broadcast::Sender<SessionEventMessage>,
        ) -> Response {
            let session_id = require_param!($req, "session_id", as_str);
            let value = $extract;

            match am.$method(session_id, value, Some(event_tx)).await {
                Ok(()) => Response::success(
                    $req.id,
                    serde_json::json!({
                        "session_id": session_id,
                        $field: value,
                    }),
                ),
                Err(e) => agent_error_to_response($req.id, e),
            }
        }
    };
}

/// Generate a `session.get_*` handler `(Request, &AgentManager) -> Response`.
///
/// The plain form serializes the returned value directly; the `display` form
/// serializes `value.to_string()` (for enum knobs stored as typed values but
/// exposed over the wire as their string spelling).
macro_rules! session_config_getter {
    ($fn_name:ident, $method:ident, $field:tt $(,)?) => {
        pub(crate) async fn $fn_name(req: Request, am: &Arc<AgentManager>) -> Response {
            let session_id = require_param!(req, "session_id", as_str);

            match am.$method(session_id) {
                Ok(value) => Response::success(
                    req.id,
                    serde_json::json!({
                        "session_id": session_id,
                        $field: value,
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
    ($fn_name:ident, $method:ident, $field:tt, display $(,)?) => {
        pub(crate) async fn $fn_name(req: Request, am: &Arc<AgentManager>) -> Response {
            let session_id = require_param!(req, "session_id", as_str);

            match am.$method(session_id) {
                Ok(value) => Response::success(
                    req.id,
                    serde_json::json!({
                        "session_id": session_id,
                        $field: value.to_string(),
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

// ── Setters (uniform shape: extract → set → echo the value back) ────────────

session_config_setter!(
    handle_session_set_system_prompt,
    req,
    set_system_prompt,
    "system_prompt",
    require_param!(req, "system_prompt", as_str)
);

session_config_setter!(
    handle_session_set_precognition_results,
    req,
    set_precognition_results,
    "precognition_results",
    optional_param!(req, "precognition_results", as_u64).unwrap_or(5) as usize
);

session_config_setter!(
    handle_session_set_temperature,
    req,
    set_temperature,
    "temperature",
    require_param!(req, "temperature", as_f64)
);

// max_tokens can be null to clear the limit, so we use optional.
session_config_setter!(
    handle_session_set_max_tokens,
    req,
    set_max_tokens,
    "max_tokens",
    optional_param!(req, "max_tokens", as_u64).map(|v| v as u32)
);

// max_iterations can be null to clear the limit (unlimited), so we use optional.
session_config_setter!(
    handle_session_set_max_iterations,
    req,
    set_max_iterations,
    "max_iterations",
    optional_param!(req, "max_iterations", as_u64).map(|v| v as u32)
);

// timeout_secs can be null to clear the timeout, so we use optional.
session_config_setter!(
    handle_session_set_execution_timeout,
    req,
    set_execution_timeout,
    "timeout_secs",
    optional_param!(req, "timeout_secs", as_u64)
);

session_config_setter!(
    handle_session_set_context_window,
    req,
    set_context_window,
    "context_window",
    optional_param!(req, "context_window", as_u64).map(|v| v as usize)
);

session_config_setter!(
    handle_session_set_validation_retries,
    req,
    set_validation_retries,
    "validation_retries",
    require_param!(req, "validation_retries", as_u64) as u32
);

session_config_setter!(
    handle_session_set_autocompact_threshold,
    req,
    set_autocompact_threshold,
    "autocompact_threshold",
    optional_param!(req, "autocompact_threshold", as_f64).map(|v| v as f32)
);

// ── Getters (uniform shape: fetch → echo, sync `AgentManager` accessors) ─────

session_config_getter!(
    handle_session_get_system_prompt,
    get_system_prompt,
    "system_prompt"
);
session_config_getter!(
    handle_session_get_precognition_results,
    get_precognition_results,
    "precognition_results"
);
session_config_getter!(handle_session_get_temperature, get_temperature, "temperature");
session_config_getter!(handle_session_get_max_tokens, get_max_tokens, "max_tokens");
session_config_getter!(
    handle_session_get_max_iterations,
    get_max_iterations,
    "max_iterations"
);
session_config_getter!(
    handle_session_get_execution_timeout,
    get_execution_timeout,
    "timeout_secs"
);
session_config_getter!(
    handle_session_get_context_window,
    get_context_window,
    "context_window"
);
session_config_getter!(
    handle_session_get_validation_retries,
    get_validation_retries,
    "validation_retries"
);
session_config_getter!(
    handle_session_get_autocompact_threshold,
    get_autocompact_threshold,
    "autocompact_threshold"
);
session_config_getter!(
    handle_session_get_context_strategy,
    get_context_strategy,
    "context_strategy",
    display
);
session_config_getter!(
    handle_session_get_output_validation,
    get_output_validation,
    "output_validation",
    display
);

// ── Hand-written handlers ───────────────────────────────────────────────────
//
// Two reasons a handler stays hand-written rather than macro-generated:
//
//  1. It deviates from the uniform shape — `set_thinking_budget` echoes back a
//     different value than it stores; `set_context_strategy` /
//     `set_output_validation` parse-and-validate the incoming string.
//
//  2. It is one of the methods sampled by the A1 field-name parity gate in
//     `tests/architecture_tests.rs`. That gate source-scans this file for
//     literal `fn handle_session_{get,set}_<suffix>(` signatures and diffs the
//     param/response field names against the client — a defense against the
//     project's known client/server field-name-mismatch bug class. Macro
//     invocations have no such literal signature for it to find, so the three
//     sampled methods (`thinking_budget`, `context_budget`, `precognition`)
//     keep their get/set handlers spelled out here. (Teaching that gate to read
//     the macro invocations would let these collapse too — see hand-off report.)

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

pub(crate) async fn handle_session_set_precognition(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let enabled = optional_param!(req, "precognition_enabled", as_bool).unwrap_or(true);

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
    let session_id = require_param!(req, "session_id", as_str);

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
