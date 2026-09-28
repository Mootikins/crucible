use super::super::*;
use crate::agent_manager::commands::SlashRoute;
use crate::require_param;
use crate::rpc_client::{
    SessionConfigureAgentRequest, SessionIdRequest, SessionInjectContextRequest,
    SessionInteractionRespondRequest, SessionTestInteractionRequest,
};
use crate::rpc_helpers::typed_params;
use crucible_core::types::SendOutcome;

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
        // Everything else classified too. Matching one variant and blanketing
        // the rest left SessionNotFound, NoAgentConfigured and ConcurrentRequest
        // answering -32603 — the same three this file's sibling handler got
        // wrong, one function away.
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_send_message(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &crate::EventBus,
    admission: crate::server::diff::Admission<'_>,
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

    // A `/name` message that names a daemon command runs it. A mode switch
    // or a skill still starts a turn when there is text for one.
    let mut attached = Vec::new();
    let mut content = content;
    let routed_rest;
    match am.slash_route(session_id, content, event_tx).await {
        Ok(SlashRoute::Message) => {}
        Ok(SlashRoute::Mode { mode_id, rest }) => {
            if let Err(e) = am.set_mode(session_id, &mode_id, Some(event_tx)).await {
                return agent_error_to_response(req.id, e);
            }
            if rest.is_empty() {
                return command_reply(
                    req.id,
                    session_id,
                    &mode_id,
                    format!("Mode: {mode_id}").into(),
                );
            }
            routed_rest = rest;
            content = &routed_rest;
        }
        Ok(SlashRoute::Plugin { name, rest }) => {
            return run_plugin_command(req.id, am, session_id, &name, &rest).await;
        }
        // The turn keeps the text the user typed, so the transcript shows
        // the invocation; the instructions go with it as context.
        Ok(SlashRoute::Skill { instructions }) => attached.push(instructions),
        Err(e) => return agent_error_to_response(req.id, e),
    }

    let comments: Vec<crucible_core::diff::CommentRef> = match req.params.get("comments") {
        None | Some(serde_json::Value::Null) => Vec::new(),
        Some(value) => match serde_json::from_value(value.clone()) {
            Ok(comments) => comments,
            Err(e) => {
                return Response::error(req.id, INVALID_PARAMS, format!("Invalid comments: {e}"))
            }
        },
    };
    // The client sends references only. The daemon builds the block of each
    // comment, and refuses the message when a reference names no open comment.
    let review_context = if comments.is_empty()
        && crate::agent_manager::attachments::comment_mentions(content).is_empty()
    {
        None
    } else {
        let workspace = am
            .session_manager()
            .read_session(session_id)
            .await
            .ok()
            .flatten()
            .and_then(|session| session.workspace);
        match crate::server::diff_context::review_context(
            &admission,
            session_id,
            workspace.as_deref(),
            &comments,
            content,
        )
        .await
        {
            Ok(context) => context,
            Err((code, message)) => return Response::error(req.id, code, message),
        }
    };

    match am
        .send_message_with_context(
            session_id,
            content.to_string(),
            attached.into_iter().chain(review_context).collect(),
            event_tx,
            is_interactive,
            permission_override,
        )
        .await
    {
        Ok(message_id) => send_reply(req.id, session_id, SendOutcome::Turn { message_id }),
        // Classified rather than blanket-internal: this handler answered every
        // failure with -32603, so a missing session, an unconfigured agent and
        // a malformed id all read as daemon faults. crucible-web maps anything
        // that is not -32602 to HTTP 502.
        Err(e) => agent_error_to_response(req.id, e),
    }
}

/// The reply of `session.send_message`: the session and what happened.
fn send_reply(id: Option<RequestId>, session_id: &str, outcome: SendOutcome) -> Response {
    let mut reply = serde_json::to_value(outcome).expect("a send outcome serializes");
    reply["session_id"] = session_id.into();
    Response::success(id, reply)
}

fn command_reply(
    id: Option<RequestId>,
    session_id: &str,
    command: &str,
    result: serde_json::Value,
) -> Response {
    send_reply(
        id,
        session_id,
        SendOutcome::Command {
            command: command.to_string(),
            result,
        },
    )
}

/// Run a plugin command that a message named, with the rest of the message
/// as its input, as `plugin.run_command` does for a client's button.
async fn run_plugin_command(
    id: Option<RequestId>,
    am: &AgentManager,
    session_id: &str,
    name: &str,
    rest: &str,
) -> Response {
    let Some(registry) = am.plugin_registry().await else {
        return internal_error(id, "Plugin loader not initialized");
    };
    let args = if rest.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::json!({ "input": rest })
    };
    match registry.run_command_in(name, args, Some(session_id)).await {
        Ok(Some(result)) => command_reply(id, session_id, name, result),
        Ok(None) => internal_error(id, format!("Unknown plugin command: {name}")),
        // The plugin's message names the plugin and its reason.
        Err(e) => {
            tracing::warn!(command = %name, "{e:#}");
            Response::error(id, INTERNAL_ERROR, e.to_string())
        }
    }
}

/// Shared implementation for context injection -- used by both RPC handler and Lua bridge.
///
/// `plugin` names the plugin that injects; `None` for an RPC client. A plugin
/// may not write a user message: a person writes those.
pub(crate) async fn inject_context_impl(
    sm: &SessionManager,
    am: &AgentManager,
    event_tx: &crate::EventBus,
    session_id: &str,
    role: &str,
    content: &str,
    plugin: Option<&str>,
) -> Result<(), String> {
    if !matches!(role, "system" | "user" | "assistant") {
        return Err(format!(
            "Invalid role '{}': must be 'system', 'user', or 'assistant'",
            role
        ));
    }
    if let (Some(plugin), "user") = (plugin, role) {
        return Err(format!(
            "Plugin '{plugin}' cannot inject a user message; inject it as 'system'"
        ));
    }

    let session = sm
        .get_session(session_id)
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    if session
        .agent
        .as_ref()
        .is_some_and(|agent| agent.agent_type == "acp")
    {
        return Err(
            "Context injection requires an internal agent; ACP owns its conversation history"
                .into(),
        );
    }

    let log_event = match role {
        "system" => crate::observe::LogEvent::System {
            ts: chrono::Utc::now(),
            content: content.to_string(),
            tags: Vec::new(),
            injection: Some(("context".into(), plugin.unwrap_or("rpc").into())),
        },
        "user" => crate::observe::LogEvent::user(content),
        "assistant" => crate::observe::LogEvent::assistant(content),
        _ => unreachable!(),
    };

    let slot = am.slot(session_id);
    let mut input = slot.input.lock().await;
    // Rebuild before writing, otherwise the first turn replays this acceptance
    // and then drains the same message from the live queue a second time.
    am.get_or_rebuild_session_tree(session_id, &session.jsonl_path(sm.sessions_root()))
        .await;
    let announced = crate::observe::events::injection_payload(&log_event, None);
    input
        .accept(sm.storage().as_ref(), &session, log_event)
        .await
        .map_err(|e| e.to_string())?;
    drop(input);
    if let Some(payload) = announced {
        let _ = event_tx.emit(SessionEventMessage::typed(session_id, payload));
    }

    Ok(())
}

pub(crate) async fn handle_session_inject_context(
    req: Request,
    sm: &Arc<SessionManager>,
    am: &Arc<AgentManager>,
    event_tx: &crate::EventBus,
) -> Response {
    let params = match typed_params::<SessionInjectContextRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    match inject_context_impl(
        sm,
        am,
        event_tx,
        session_id,
        &params.role,
        &params.content,
        None,
    )
    .await
    {
        Ok(()) => Response::success(req.id, serde_json::json!({ "status": "ok" })),
        Err(msg)
            if msg.starts_with("Invalid role") || msg.starts_with("Context injection requires") =>
        {
            Response::error(req.id, INVALID_PARAMS, msg)
        }
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

/// `session.clear`: the user's clear of the model context. The transcript
/// stays; `context_cleared` names no plugin.
pub(crate) async fn handle_session_clear(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &crate::EventBus,
) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    match am
        .clear_session(&params.session_id, None, None, event_tx)
        .await
    {
        Ok(_) => Response::success(
            req.id,
            serde_json::json!({ "session_id": params.session_id }),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
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
    event_tx: &crate::EventBus,
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

    // Routed by WHICH REGISTRY HOLDS THE ID, not by the reply's own shape —
    // the two registries share a key type and this one wire method, so the
    // reply alone cannot say where it belongs. Matching on it stalled every
    // permission prompt an ACP host cancelled. A reply with no waiter in
    // either registry is not an error: the waiter may have timed out, and the
    // `interaction_completed` event below is still worth emitting so clients
    // can dismiss the modal.
    if let Err(e) = am.deliver_client_reply(session_id, request_id, response.clone()) {
        tracing::debug!(
            session_id = %session_id,
            request_id = %request_id,
            error = %e,
            "No waiter for this reply (may have timed out)"
        );
    }

    if !event_tx.emit(SessionEventMessage::interaction_completed(
        session_id, request_id, response,
    )) {
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
    event_tx: &crate::EventBus,
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
            // The typed request, so a client decodes the shape a real
            // permission prompt has.
            match serde_json::to_value(crucible_core::interaction::InteractionRequest::Permission(
                crucible_core::interaction::PermRequest::bash(action.split_whitespace()),
            )) {
                Ok(request) => request,
                Err(e) => return Response::error(req.id, INTERNAL_ERROR, e.to_string()),
            }
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

    let request = match serde_json::from_value(request) {
        Ok(request) => request,
        Err(e) => return Response::error(req.id, INTERNAL_ERROR, e.to_string()),
    };
    if !event_tx.emit(SessionEventMessage::typed(
        session_id.as_str(),
        crucible_core::protocol::TurnPayload::InteractionRequested {
            request_id: request_id.clone(),
            request,
        },
    )) {
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
        let (event_tx, mut events) = crate::EventBus::channel(8);

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
        let Ok(crucible_core::protocol::SessionEventPayload::Turn(
            crucible_core::protocol::TurnPayload::InteractionRequested {
                request: crucible_core::interaction::InteractionRequest::Permission(request),
                ..
            },
        )) = event.payload()
        else {
            panic!("a typed permission request: {event:?}");
        };
        assert_eq!(request.tokens(), ["rm", "-rf", "/tmp/example"]);
    }

    /// The `ask` branch's own optional field, and the default that applies
    /// when `type` is absent.
    #[tokio::test]
    async fn an_omitted_type_asks_the_question_the_caller_supplied() {
        let (event_tx, mut events) = crate::EventBus::channel(8);

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
        let (event_tx, _events) = crate::EventBus::channel(8);

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
