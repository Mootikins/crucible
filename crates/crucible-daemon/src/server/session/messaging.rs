use super::super::*;
use crate::require_param;
use crate::rpc_client::{
    SessionConfigureAgentRequest, SessionIdRequest, SessionInjectContextRequest,
    SessionInteractionRespondRequest, SessionTestInteractionRequest,
};
use crate::rpc_helpers::typed_params;

pub(crate) async fn handle_session_configure_agent(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let params = match typed_params::<SessionConfigureAgentRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    let agent: crucible_core::session::SessionAgent = match serde_json::from_value(params.agent) {
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
        // How `configure_agent`'s trust gate refuses a provider the session's
        // attached kilns do not clear. Caller-fixable, so it must not read as a
        // daemon fault: crucible-web maps -32602 to 422 and everything else to
        // 502. Same classification `scope_error` gives the variant.
        Err(e @ AgentError::InvalidConfig(_)) => {
            Response::error(req.id, INVALID_PARAMS, e.to_string())
        }
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
            s.parse::<crucible_core::config::components::permissions::PermissionMode>()
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
    let storage = FileSessionStorage::new(sm.sessions_root().to_path_buf())
        .with_registry(sm.kiln_registry().clone());
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
    let params = match typed_params::<SessionInjectContextRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    match inject_context_impl(sm, event_tx, session_id, &params.role, &params.content).await {
        Ok(()) => Response::success(req.id, serde_json::json!({ "status": "ok" })),
        Err(msg) if msg.starts_with("Invalid role") => Response::error(req.id, INVALID_PARAMS, msg),
        Err(msg) if msg.starts_with("Session not found") => session_not_found(req.id, session_id),
        Err(msg) => Response::error(req.id, INTERNAL_ERROR, msg),
    }
}

pub(crate) async fn handle_session_cancel(req: Request, am: &Arc<AgentManager>) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    let cancelled = am.cancel(session_id).await;
    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "cancelled": cancelled,
        }),
    )
}

/// Every pending interaction across every session — the aggregate the web
/// Inbox polls so sessions without an open browser tab still surface.
pub(crate) async fn handle_session_pending_interactions(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let permissions =
        am.list_all_pending_permissions()
            .into_iter()
            .map(|(session_id, request_id, request)| {
                (
                    session_id,
                    request_id,
                    crucible_core::interaction::InteractionRequest::Permission(request),
                )
            });
    // Both registries, one list: a client asking what it owes an answer to
    // does not care which map the request came out of.
    let pending: Vec<serde_json::Value> = permissions
        .chain(am.list_all_pending_interactions())
        .map(|(session_id, request_id, request)| {
            serde_json::json!({
                "session_id": session_id,
                "request_id": request_id,
                "request": request,
            })
        })
        .collect();

    Response::success(req.id, serde_json::json!({ "pending": pending }))
}

pub(crate) async fn handle_session_interaction_respond(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let params = match typed_params::<SessionInteractionRespondRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let (session_id, request_id) = (&params.session_id, &params.request_id);

    let response: crucible_core::interaction::InteractionResponse =
        match serde_json::from_value(params.response) {
            Ok(r) => r,
            Err(e) => {
                return Response::error(
                    req.id,
                    INVALID_PARAMS,
                    format!("Invalid interaction response: {}", e),
                )
            }
        };

    // Permission responses go to the permission registry; everything else —
    // including a `Cancelled` answering a question rather than a prompt — goes
    // to the interaction registry. A response with no waiter in either is not
    // an error: the waiter may have timed out, and the `interaction_completed`
    // event below is still worth emitting so clients can dismiss the modal.
    match &response {
        crucible_core::interaction::InteractionResponse::Permission(perm_response) => {
            if let Err(e) = am.respond_to_permission(session_id, request_id, perm_response.clone())
            {
                tracing::warn!(
                    session_id = %session_id,
                    request_id = %request_id,
                    error = %e,
                    "Failed to send permission response to channel (may have timed out)"
                );
            }
        }
        other => {
            if let Err(e) = am.respond_to_interaction(session_id, request_id, other.clone()) {
                tracing::debug!(
                    session_id = %session_id,
                    request_id = %request_id,
                    error = %e,
                    "No waiter for interaction response (may have timed out)"
                );
            }
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
    let params = match typed_params::<SessionTestInteractionRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    let interaction_type = params.interaction_type.as_deref().unwrap_or("ask");
    let request_id = format!("test-{}", uuid::Uuid::new_v4());

    let request = match interaction_type {
        "ask" => {
            let question = params
                .question
                .as_deref()
                .unwrap_or("Test question: Which option do you prefer?");

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
            let action = params.action.as_deref().unwrap_or("rm -rf /tmp/test");

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

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::protocol::RequestId;

    fn request(params: serde_json::Value) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: "session.test_interaction".to_string(),
            params,
        }
    }

    /// `SessionTestInteractionRequest` renames `interaction_type` to the wire
    /// name `type`. A rename that did not take would silently fall back to the
    /// `ask` default and this method would stop being able to emit a
    /// permission prompt at all.
    #[tokio::test]
    async fn the_renamed_type_field_and_its_payload_reach_the_emitted_event() {
        let (event_tx, mut events) = broadcast::channel(8);

        let resp = handle_session_test_interaction(
            request(serde_json::json!({
                "session_id": "sess",
                "type": "permission",
                "action": "rm -rf /tmp/example",
            })),
            &event_tx,
        )
        .await;

        assert!(resp.error.is_none(), "{:?}", resp.error);
        assert_eq!(resp.result.expect("success")["type"], "permission");

        let event = events.try_recv().expect("an interaction_requested event");
        assert_eq!(event.data["request"]["kind"], "permission");
        assert_eq!(
            event.data["request"]["Bash"]["command"],
            "rm -rf /tmp/example"
        );
    }

    /// The `ask` branch's own optional field, and the default that applies
    /// when `type` is absent.
    #[tokio::test]
    async fn an_omitted_type_asks_the_question_the_caller_supplied() {
        let (event_tx, mut events) = broadcast::channel(8);

        let resp = handle_session_test_interaction(
            request(serde_json::json!({
                "session_id": "sess",
                "question": "Ship it?",
            })),
            &event_tx,
        )
        .await;

        assert_eq!(resp.result.expect("success")["type"], "ask");
        let event = events.try_recv().expect("an interaction_requested event");
        assert_eq!(event.data["request"]["question"], "Ship it?");
    }

    /// A `type` outside the two the handler knows is still refused by name,
    /// not swallowed by the request struct.
    #[tokio::test]
    async fn an_unknown_type_is_refused_and_named() {
        let (event_tx, _events) = broadcast::channel(8);

        let resp = handle_session_test_interaction(
            request(serde_json::json!({ "session_id": "sess", "type": "toast" })),
            &event_tx,
        )
        .await;

        let err = resp
            .error
            .expect("an unknown interaction type must be refused");
        assert_eq!(err.code, INVALID_PARAMS);
        assert!(err.message.contains("toast"), "{}", err.message);
    }
}
