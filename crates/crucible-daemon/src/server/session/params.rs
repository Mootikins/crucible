use super::super::*;
use crate::rpc_helpers::typed_params;
use crucible_core::protocol::requests::{KnobRef, Scoped, UndoCount};
use crucible_core::types::{AcpKnob, KnobValue, SessionKnob};

use crucible_core::session::ContextStrategy;

// One handler writes every knob and one handler reads every knob: both
// deserialize `crucible_core::types::KnobValue` (set) or
// `crucible_core::protocol::requests::KnobRef` (get), from the same wire
// types `DaemonClient` builds. `KnobValue::knob` names which knob a set
// carries, so a single early check can refuse a knob an ACP session's agent
// does not carry, before dispatching to that knob's own apply/read logic.

/// Refuse a knob classified `AcpKnob::Absent` for an ACP session, or `None` to
/// proceed. Checked once, here, rather than inside every per-knob branch —
/// `update_agent_config_and_emit` already refuses this for the write side,
/// so this mainly closes the read side, which had no such check before.
fn refuse_if_absent_on_acp(
    req_id: Option<RequestId>,
    am: &Arc<AgentManager>,
    session_id: &str,
    knob: SessionKnob,
) -> Option<Response> {
    let is_acp = am
        .get_session_with_agent(session_id)
        .map(|(_, agent_config)| agent_config.agent_type == "acp")
        .unwrap_or(false);
    if is_acp && knob.on_acp() == AcpKnob::Absent {
        return Some(Response::error(
            req_id,
            INVALID_PARAMS,
            format!(
                "the ACP agent for this session has no '{}' setting",
                knob.id()
            ),
        ));
    }
    None
}

/// `session.knob.set` — validate the value once, then dispatch to the same
/// per-knob apply logic the daemon always had.
pub(crate) async fn handle_session_knob_set(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &crate::EventBus,
) -> Response {
    let params = match typed_params::<Scoped<KnobValue>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = params.session_id.clone();
    let value = params.body;

    if let Some(refusal) = refuse_if_absent_on_acp(req.id.clone(), am, &session_id, value.knob()) {
        return refusal;
    }

    match value {
        KnobValue::Model(model_id) => {
            match am
                .switch_model(&session_id, &model_id, Some(event_tx))
                .await
            {
                Ok(()) => knob_set_response(req.id, &session_id, SessionKnob::Model),
                Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
                    session_not_found(req.id, &id)
                }
                Err(crate::agent_manager::AgentError::NoAgentConfigured(id)) => {
                    agent_not_configured(req.id, &id)
                }
                Err(crate::agent_manager::AgentError::ConcurrentRequest(id)) => Response::error(
                    req.id,
                    INVALID_PARAMS,
                    format!(
                        "Cannot switch model while request is in progress for session: {}",
                        id
                    ),
                ),
                Err(crate::agent_manager::AgentError::InvalidModelId(msg)) => {
                    Response::error(req.id, INVALID_PARAMS, msg)
                }
                Err(e) => internal_error(req.id, e),
            }
        }
        KnobValue::Mode(mode_id) => {
            let Some(mode_id) = mode_id else {
                return Response::error(
                    req.id,
                    INVALID_PARAMS,
                    "mode requires a mode id".to_string(),
                );
            };
            match am.set_mode(&session_id, &mode_id, Some(event_tx)).await {
                Ok(()) => knob_set_response(req.id, &session_id, SessionKnob::Mode),
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
        KnobValue::ContextStrategy(strategy_str) => {
            let strategy = match strategy_str.parse::<ContextStrategy>() {
                Ok(s) => s,
                Err(e) => return Response::error(req.id, INVALID_PARAMS, e),
            };
            match am
                .set_context_strategy(&session_id, strategy, Some(event_tx))
                .await
            {
                Ok(()) => knob_set_response(req.id, &session_id, SessionKnob::ContextStrategy),
                Err(e) => agent_error_to_response(req.id, e),
            }
        }
        KnobValue::Precognition(enabled) => match am
            .set_precognition(&session_id, enabled, Some(event_tx))
            .await
        {
            Ok(()) => knob_set_response(req.id, &session_id, SessionKnob::Precognition),
            Err(e) => agent_error_to_response(req.id, e),
        },
        KnobValue::PluginTurnLimit(limit) => match am
            .set_plugin_turn_limit(&session_id, limit, Some(event_tx))
            .await
        {
            Ok(()) => knob_set_response(req.id, &session_id, SessionKnob::PluginTurnLimit),
            Err(e) => agent_error_to_response(req.id, e),
        },
    }
}

/// The uniform `session.knob.set` success body: the session, the knob that
/// changed, and nothing else — the value is not echoed back, because the
/// caller already knows what it sent, and `session.knob.get` is the read
/// path for a value some other client changed.
fn knob_set_response(req_id: Option<RequestId>, session_id: &str, knob: SessionKnob) -> Response {
    Response::success(
        req_id,
        serde_json::json!({
            "session_id": session_id,
            "knob": knob.id(),
            "set": true,
        }),
    )
}

/// `session.knob.get` — read one knob's value, in the same
/// [`KnobValue`] shape `session.knob.set` writes.
pub(crate) async fn handle_session_knob_get(req: Request, am: &Arc<AgentManager>) -> Response {
    let params = match typed_params::<Scoped<KnobRef>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = params.session_id.clone();
    let knob = params.body.knob;

    if let Some(refusal) = refuse_if_absent_on_acp(req.id.clone(), am, &session_id, knob) {
        return refusal;
    }

    let value = match knob {
        SessionKnob::Model => am.get_model(&session_id).map(KnobValue::Model),
        SessionKnob::Mode => am.get_mode(&session_id).map(KnobValue::Mode),
        SessionKnob::ContextStrategy => am
            .get_context_strategy(&session_id)
            .map(|s| KnobValue::ContextStrategy(s.to_string())),
        SessionKnob::Precognition => am
            .get_precognition(&session_id)
            .map(KnobValue::Precognition),
        SessionKnob::PluginTurnLimit => am
            .get_plugin_turn_limit(&session_id)
            .map(KnobValue::PluginTurnLimit),
    };

    match value {
        Ok(value) => match serde_json::to_value(&value) {
            Ok(mut json) => {
                if let serde_json::Value::Object(map) = &mut json {
                    map.insert(
                        "session_id".to_string(),
                        serde_json::Value::String(session_id),
                    );
                }
                Response::success(req.id, json)
            }
            Err(e) => internal_error(req.id, e),
        },
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
    event_tx: &crate::EventBus,
) -> Response {
    let params = match typed_params::<Scoped<UndoCount>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = params.session_id.as_str();
    let count = params.body.count.unwrap_or(1);

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
