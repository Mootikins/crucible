use super::super::*;
use crate::require_param;

pub(crate) async fn handle_session_configure_agent(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    let agent_json = match req.params.get("agent") {
        Some(v) => v.clone(),
        None => {
            return Response::error(req.id, INVALID_PARAMS, "Missing 'agent' parameter");
        }
    };

    let agent: crucible_core::session::SessionAgent = match serde_json::from_value(agent_json) {
        Ok(a) => a,
        Err(e) => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!("Invalid agent config: {}", e),
            );
        }
    };

    match am.configure_agent(session_id, agent).await {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "configured": true,
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_send_message(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let content = require_param!(req, "content", as_str);
    let is_interactive = req
        .params
        .get("is_interactive")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let permission_override = req
        .params
        .get("permission_mode")
        .and_then(|v| v.as_str())
        .and_then(|s| {
            s.parse::<crucible_config::components::permissions::PermissionMode>()
                .ok()
        });

    match am
        .send_message(
            session_id,
            content.to_string(),
            event_tx,
            is_interactive,
            permission_override,
        )
        .await
    {
        Ok(message_id) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "message_id": message_id,
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

/// Shared implementation for context injection -- used by both RPC handler and Lua bridge.
pub(crate) async fn inject_context_impl(
    sm: &SessionManager,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    session_id: &str,
    role: &str,
    content: &str,
) -> Result<(), String> {
    if !matches!(role, "system" | "user" | "assistant") {
        return Err(format!(
            "Invalid role '{}': must be 'system', 'user', or 'assistant'",
            role
        ));
    }

    let session = sm
        .get_session(session_id)
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    let log_event = match role {
        "system" => crate::observe::LogEvent::system(content),
        "user" => crate::observe::LogEvent::user(content),
        "assistant" => crate::observe::LogEvent::assistant(content),
        _ => unreachable!(),
    };

    let event_json = serde_json::to_string(&log_event).map_err(|e| e.to_string())?;
    let storage = FileSessionStorage::new();
    storage
        .append_event(&session, &event_json)
        .await
        .map_err(|e| e.to_string())?;

    let _ = emit_event(
        event_tx,
        SessionEventMessage::new(
            session_id,
            "context_injected",
            serde_json::json!({
                "role": role,
                "content": content,
            }),
        ),
    );

    Ok(())
}

pub(crate) async fn handle_session_inject_context(
    req: Request,
    sm: &Arc<SessionManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let role = require_param!(req, "role", as_str);
    let content = require_param!(req, "content", as_str);

    match inject_context_impl(sm, event_tx, session_id, role, content).await {
        Ok(()) => Response::success(req.id, serde_json::json!({ "status": "ok" })),
        Err(msg) if msg.starts_with("Invalid role") => Response::error(req.id, INVALID_PARAMS, msg),
        Err(msg) if msg.starts_with("Session not found") => session_not_found(req.id, session_id),
        Err(msg) => Response::error(req.id, INTERNAL_ERROR, msg),
    }
}

pub(crate) async fn handle_session_cancel(req: Request, am: &Arc<AgentManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    let cancelled = am.cancel(session_id).await;
    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "cancelled": cancelled,
        }),
    )
}

pub(crate) async fn handle_session_interaction_respond(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let request_id = require_param!(req, "request_id", as_str);
    let response_obj = require_param!(req, "response", as_object);

    let response: crucible_core::interaction::InteractionResponse =
        match serde_json::from_value(serde_json::Value::Object(response_obj.clone())) {
            Ok(r) => r,
            Err(e) => {
                return Response::error(
                    req.id,
                    INVALID_PARAMS,
                    format!("Invalid interaction response: {}", e),
                )
            }
        };

    if let crucible_core::interaction::InteractionResponse::Permission(perm_response) = &response {
        if let Err(e) = am.respond_to_permission(session_id, request_id, perm_response.clone()) {
            tracing::warn!(
                session_id = %session_id,
                request_id = %request_id,
                error = %e,
                "Failed to send permission response to channel (may have timed out)"
            );
        }
    }

    if !emit_event(
        event_tx,
        SessionEventMessage::new(
            session_id,
            "interaction_completed",
            serde_json::json!({
                "request_id": request_id,
                "response": response,
            }),
        ),
    ) {
        tracing::debug!("Failed to emit interaction_completed event (no subscribers)");
    }

    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "request_id": request_id,
        }),
    )
}

pub(crate) async fn handle_session_test_interaction(
    req: Request,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    let get_str = |key: &str| -> Option<&str> { req.params.get(key)?.as_str() };

    let interaction_type = get_str("type").unwrap_or("ask");
    let request_id = format!("test-{}", uuid::Uuid::new_v4());

    let request = match interaction_type {
        "ask" => {
            let question =
                get_str("question").unwrap_or("Test question: Which option do you prefer?");

            // InteractionRequest uses #[serde(tag = "kind")] internally-tagged format
            serde_json::json!({
                "kind": "ask",
                "question": question,
                "choices": ["Option A", "Option B", "Option C"],
                "allow_other": true,
                "multi_select": false
            })
        }
        "permission" => {
            let action = get_str("action").unwrap_or("rm -rf /tmp/test");

            // PermRequest uses externally-tagged format for its inner Bash/Read/Write/Tool
            serde_json::json!({
                "kind": "permission",
                "Bash": {
                    "command": action
                }
            })
        }
        _ => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!(
                    "Unknown interaction type: {}. Use 'ask' or 'permission'",
                    interaction_type
                ),
            )
        }
    };

    if !emit_event(
        event_tx,
        SessionEventMessage::new(
            session_id.to_string(),
            "interaction_requested",
            serde_json::json!({
                "request_id": request_id,
                "request": request,
            }),
        ),
    ) {
        tracing::debug!("Failed to emit interaction_requested event (no subscribers)");
    }

    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "request_id": request_id,
            "type": interaction_type,
        }),
    )
}
