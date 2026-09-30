use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

pub fn chat_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(send_message))
        .routes(routes!(interaction_respond))
        .routes(routes!(pending_interactions))
}

#[derive(Debug, Deserialize, ToSchema)]
struct SendMessageRequest {
    session_id: String,
    content: String,
    /// Stored review comments that the message attaches. The daemon builds
    /// the context of each one, and refuses an unknown or resolved comment.
    #[serde(default)]
    comments: Vec<crucible_core::diff::CommentRef>,
}

/// Start a turn in a session, or run the daemon command that the message
/// names.
///
/// A turn answers its `message_id`: the browser correlates the SSE events
/// that follow with it. A command that ran without a turn, such as a plugin
/// command, answers its result instead.
#[utoipa::path(
    post,
    path = "/api/chat/send",
    request_body = SendMessageRequest,
    responses(
        (status = 200, body = crucible_core::types::SendOutcome),
        (status = 400, description = "The message is empty and attaches no comment"),
        (status = 502, description = "The daemon could not accept the message"),
    )
)]
async fn send_message(
    State(state): State<AppState>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<crucible_core::types::SendOutcome>, WebError> {
    if req.content.trim().is_empty() && req.comments.is_empty() {
        return Err(WebError::Chat("Message cannot be empty".to_string()));
    }

    let outcome = state
        .daemon
        .session_send_message(&req.session_id, &req.content, &req.comments)
        .await
        .daemon_err()?;

    Ok(Json(outcome))
}

/// One interaction a session is waiting on an answer to.
///
/// `request` is the same typed, kind-tagged shape the SSE
/// `interaction_requested` frame carries — see
/// [`crucible_core::interaction::InteractionRequest`]. A permission
/// request's `pattern` is filled in here too, the one place this route
/// decides it, so the browser never re-derives it.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct PendingInteraction {
    /// The session that asked.
    session_id: String,
    /// The identifier an answer must carry back.
    request_id: String,
    request: crucible_core::interaction::InteractionRequest,
}

/// The interactions every session is waiting on, in one list.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct PendingInteractionsResponse {
    pending: Vec<PendingInteraction>,
}

/// Aggregate pending interactions across all sessions — the Inbox renders
/// both sources through one component.
#[utoipa::path(
    get,
    path = "/api/interactions/pending",
    responses(
        (status = 200, body = PendingInteractionsResponse),
        (status = 502, description = "The daemon could not list the pending interactions"),
    )
)]
async fn pending_interactions(
    State(state): State<AppState>,
) -> Result<Json<PendingInteractionsResponse>, WebError> {
    let raw = state
        .daemon
        .session_pending_interactions()
        .await
        .daemon_err()?;

    let pending: Vec<PendingInteraction> = raw["pending"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let request: crucible_core::interaction::InteractionRequest =
                        serde_json::from_value(item["request"].clone()).ok()?;
                    let request = match request {
                        crucible_core::interaction::InteractionRequest::Permission(perm) => {
                            crucible_core::interaction::InteractionRequest::Permission(
                                perm.with_suggested_pattern(),
                            )
                        }
                        other => other,
                    };
                    Some(PendingInteraction {
                        session_id: item["session_id"].as_str().unwrap_or_default().to_string(),
                        request_id: item["request_id"].as_str().unwrap_or_default().to_string(),
                        request,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(Json(PendingInteractionsResponse { pending }))
}

#[derive(Debug, Deserialize, ToSchema)]
struct InteractionResponseRequest {
    session_id: String,
    request_id: String,
    /// The answer, in the kind-tagged shape `InteractionResponse` takes.
    ///
    /// Open, because the vocabulary belongs to the daemon's interaction types
    /// and an unreadable answer must come back as this route's 400 naming the
    /// deserialiser's own complaint, not as axum's plain-text rejection.
    #[schema(value_type = serde_json::Value)]
    response: serde_json::Value,
}

/// The answer this route gives when the daemon took the response.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct InteractionRespondResponse {
    /// Always `true`. A refusal is an error status, not a `false`.
    ok: bool,
}

/// Infer the `kind` tag for a bare response object.
///
/// `InteractionResponse` is a `kind`-tagged enum. The frontend now always
/// states its kind (`lib/types.ts`), so this is the compatibility path for
/// clients that do not — and it must stay one, because inference cannot cover
/// the full set: a panel result and an ask response both carry `selected`, and
/// an edit response's `{modified}` and a bare `cancelled` have no
/// discriminating field at all. Anything it cannot name is passed through
/// unchanged so the deserializer reports the real error rather than this
/// function guessing wrong.
fn tag_interaction_response(mut value: serde_json::Value) -> serde_json::Value {
    if value.get("kind").is_some() {
        return value;
    }
    let kind = if value.get("allowed").is_some() {
        "permission"
    } else if value.get("selected").is_some() {
        "ask"
    } else if value.get("selected_index").is_some() || value.get("other").is_some() {
        "popup"
    } else {
        return value;
    };
    if let Some(obj) = value.as_object_mut() {
        obj.insert("kind".into(), serde_json::json!(kind));
    }
    value
}

/// Answer one pending interaction.
#[utoipa::path(
    post,
    path = "/api/interaction/respond",
    request_body = InteractionResponseRequest,
    responses(
        (status = 200, body = InteractionRespondResponse),
        (status = 400, description = "The response does not name an interaction kind the daemon knows"),
        (status = 502, description = "The daemon could not take the response"),
    )
)]
async fn interaction_respond(
    State(state): State<AppState>,
    Json(req): Json<InteractionResponseRequest>,
) -> Result<Json<InteractionRespondResponse>, WebError> {
    let response: crucible_core::interaction::InteractionResponse =
        serde_json::from_value(tag_interaction_response(req.response))
            .map_err(|e| WebError::Chat(format!("Invalid interaction response: {e}")))?;

    state
        .daemon
        .session_interaction_respond(&req.session_id, &req.request_id, response)
        .await
        .daemon_err()?;

    Ok(Json(InteractionRespondResponse { ok: true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::request_json;
    use crucible_core::interaction::InteractionResponse;
    use crucible_core::protocol::rpc::RpcMethod;

    #[tokio::test]
    async fn send_message_answers_the_declared_shape() {
        let (status, json) = request_json(
            "POST",
            "/api/chat/send",
            Some(serde_json::json!({ "session_id": "s-1", "content": "hello" })),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);

        let parsed: crucible_core::types::SendOutcome =
            serde_json::from_value(json.clone()).unwrap_or_else(|e| panic!("{e}: {json}"));
        assert_eq!(
            parsed,
            crucible_core::types::SendOutcome::Turn {
                message_id: "msg-001".into()
            }
        );
    }

    /// The route forwards the comment references as the client sent them.
    /// The daemon, not the route, builds their context.
    #[tokio::test]
    async fn send_message_forwards_the_attached_comments() {
        use crate::test_support::{build_state, build_test_app, start_mock_daemon};
        use tower::ServiceExt;

        let comments = serde_json::json!([{
            "id": "c1",
            "source": { "kind": "session_record", "session": "s-1" },
        }]);
        let (mock, client) = start_mock_daemon().await;
        let app = build_test_app(build_state(client));
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/chat/send")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::json!({
                            "session_id": "s-1",
                            "content": "",
                            "comments": comments,
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::OK,
            "a message with only a comment is not empty"
        );
        let params = mock.received_params(RpcMethod::SessionSendMessage).unwrap();
        assert_eq!(params["comments"], comments);

        let (status, _) = request_json(
            "POST",
            "/api/chat/send",
            Some(serde_json::json!({ "session_id": "s-1", "content": " " })),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn interaction_respond_answers_the_declared_shape() {
        let (status, json) = request_json(
            "POST",
            "/api/interaction/respond",
            Some(serde_json::json!({
                "session_id": "s-1",
                "request_id": "r-1",
                "response": { "kind": "permission", "allowed": true, "scope": "once" },
            })),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);

        let parsed: InteractionRespondResponse =
            serde_json::from_value(json.clone()).unwrap_or_else(|e| panic!("{e}: {json}"));
        assert!(parsed.ok);
    }

    /// The daemon names the session and the request; this route decodes the
    /// request into the typed, kind-tagged `InteractionRequest` and fills in
    /// the permission's suggested `pattern`.
    #[tokio::test]
    async fn pending_interactions_answers_the_declared_shape() {
        use crucible_core::interaction::{InteractionRequest, PermAction};

        let (status, json) = request_json("GET", "/api/interactions/pending", None).await;
        assert_eq!(status, axum::http::StatusCode::OK);

        let parsed: PendingInteractionsResponse =
            serde_json::from_value(json.clone()).unwrap_or_else(|e| panic!("{e}: {json}"));
        assert_eq!(parsed.pending.len(), 1);
        let entry = &parsed.pending[0];
        assert_eq!(entry.session_id, "session-001");
        assert_eq!(entry.request_id, "req-001");
        match &entry.request {
            InteractionRequest::Permission(perm) => {
                assert_eq!(
                    perm.action,
                    PermAction::Bash {
                        tokens: vec!["ls".into()]
                    }
                );
                assert_eq!(
                    perm.pattern.as_deref(),
                    Some("ls"),
                    "the route fills the suggested pattern: {json}"
                );
            }
            other => panic!("expected a permission request, got {other:?}: {json}"),
        }
    }

    /// The exact objects the frontend POSTs (PermResponse/AskResponse/
    /// PopupResponse in web/src/lib/types.ts) must deserialize into the
    /// kind-tagged InteractionResponse after tagging.
    /// The four kinds inference cannot reach arrive tagged, and must survive
    /// the tagging pass untouched. `panel` is the one that would be actively
    /// mis-tagged as `ask` without its own `kind`, because both carry
    /// `selected` — which is why the frontend states it.
    #[test]
    fn explicitly_tagged_responses_pass_through() {
        let cases = [
            serde_json::json!({ "kind": "panel", "selected": [1], "cancelled": false }),
            serde_json::json!({ "kind": "edit", "modified": "new text" }),
            serde_json::json!({
                "kind": "ask_batch",
                "id": "8a2f5c1e-0000-4000-8000-000000000000",
                "answers": [],
                "cancelled": false
            }),
            serde_json::json!({ "kind": "cancelled" }),
        ];
        for case in cases {
            let kind = case["kind"].as_str().unwrap().to_string();
            let tagged = tag_interaction_response(case);
            assert_eq!(tagged["kind"], kind, "tagging changed an explicit kind");
            serde_json::from_value::<InteractionResponse>(tagged)
                .unwrap_or_else(|e| panic!("{kind} did not deserialize: {e}"));
        }
    }

    /// A panel result reaching the inference path would be read as an ask
    /// response. Pinned so nobody "helpfully" drops `kind` from the frontend.
    #[test]
    fn an_untagged_panel_result_is_indistinguishable_from_an_ask() {
        let bare = serde_json::json!({ "selected": [1], "cancelled": true });
        let tagged = tag_interaction_response(bare);
        assert_eq!(
            tagged["kind"], "ask",
            "inference guesses ask for a bare selected-carrying object; \
             panel responses must therefore carry their own kind"
        );
    }

    #[test]
    fn bare_frontend_responses_deserialize_after_tagging() {
        let perm = serde_json::json!({ "allowed": true, "scope": "once" });
        let tagged = tag_interaction_response(perm);
        let parsed: InteractionResponse = serde_json::from_value(tagged).expect("permission");
        assert!(matches!(parsed, InteractionResponse::Permission(p) if p.allowed));

        let ask = serde_json::json!({ "selected": [1] });
        let parsed: InteractionResponse =
            serde_json::from_value(tag_interaction_response(ask)).expect("ask");
        assert!(matches!(parsed, InteractionResponse::Ask(a) if a.selected == vec![1]));

        let popup = serde_json::json!({ "selected_index": 0 });
        let parsed: InteractionResponse =
            serde_json::from_value(tag_interaction_response(popup)).expect("popup");
        assert!(matches!(parsed, InteractionResponse::Popup(_)));
    }

    #[test]
    fn already_tagged_responses_pass_through() {
        let tagged = serde_json::json!({ "kind": "permission", "allowed": false, "scope": "once" });
        let out = tag_interaction_response(tagged.clone());
        assert_eq!(out, tagged);
    }

    // ── Cross-language drift guards ───────────────────────────────────
    //
    // These two moved here from the deleted `crucible-web/src/events.rs`
    // when `ChatEvent` was deleted (see step 11 of the Simplification
    // Plan). Neither depends on `ChatEvent`: one guards the stop-reason
    // wording, the other the side-channel SSE names, and both still hold
    // under the SSE route sending the daemon's own `{event, data}` pair.

    /// Every `.ts` and `.tsx` file the frontend ships.
    ///
    /// A walk rather than `include_str!`, because the gate below must refuse a
    /// wording wherever a future edit puts it, not only in the one file that
    /// held the old copy.
    fn frontend_sources() -> Vec<(String, String)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web/src");
        walkdir::WalkDir::new(&root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .filter(|e| {
                matches!(
                    e.path().extension().and_then(|x| x.to_str()),
                    Some("ts" | "tsx")
                )
            })
            .filter_map(|e| {
                let name = e.path().strip_prefix(&root).ok()?.display().to_string();
                Some((name, std::fs::read_to_string(e.path()).ok()?))
            })
            .collect()
    }

    /// **The H3 gate.** `StopReason::user_notice` is the only wording. The page
    /// held a second one (`stopReasonNotice` in `lib/stop-reason.ts`) and the
    /// two had already drifted — a capital letter and a trailing full stop — so
    /// the comparison ignores case and a trailing stop. Any frontend file that
    /// spells a notice again fails here.
    ///
    /// The notices come from the running enum through [`StopReason::ALL`], not
    /// from a list in this file, and the walk must find sources: an empty parse
    /// is a gate that passes because it looked at nothing.
    #[test]
    fn the_frontend_words_no_stop_reason_notice() {
        use crucible_core::turn::StopReason;

        let sources = frontend_sources();
        assert!(
            !sources.is_empty(),
            "walked no frontend sources — the path moved, fix this test"
        );

        let notices: Vec<&str> = StopReason::ALL
            .iter()
            .filter_map(StopReason::user_notice)
            .collect();
        assert!(
            !notices.is_empty(),
            "no reason words a notice — `user_notice` changed, fix this test"
        );

        let normalize = |s: &str| s.to_lowercase().replace('.', "");
        let mut offenders = Vec::new();
        for (name, body) in &sources {
            let flat = normalize(body);
            for notice in &notices {
                if flat.contains(&normalize(notice)) {
                    offenders.push(format!("web/src/{name}: {notice:?}"));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "the daemon words the stop-reason note; the page must draw \
             `stop_notice` instead of spelling it again:\n  {}",
            offenders.join("\n  ")
        );
    }

    // ── The side-channel SSE names ───────────────────────────────────

    /// The publication and surface streams do not travel the chat channel, so
    /// a gate that compares the chat vocabulary alone cannot see them. That is
    /// why a fresh literal for one of these two names passed review before
    /// this test existed.
    ///
    /// Each name comes from the compiled const, which `event_payload!`
    /// expands from the same literal as the daemon's serde rename. So the
    /// chain is: rename and core const → route const → browser listener.
    #[test]
    fn every_side_channel_event_name_has_a_frontend_listener() {
        let sources = frontend_sources();
        assert!(
            !sources.is_empty(),
            "walked no frontend sources — the path moved, fix this test"
        );

        let declared = [
            crate::routes::PublicationChangedEvent::EVENT_NAME,
            crate::routes::SurfaceChangedEvent::EVENT_NAME,
            crate::routes::ProposalChangedEvent::EVENT_NAME,
        ];

        // `SIDE_CHANNEL_EVENTS` in `lib/api.ts` types each listener table as
        // `Record<name, listener>`, so tsc proves a listener for each name in
        // the table. This test proves the table holds exactly the names the
        // routes send.
        let api = sources
            .iter()
            .find(|(name, _)| name == "lib/api.ts")
            .map(|(_, body)| body.as_str())
            .expect("web/src/lib/api.ts is missing — the path moved, fix this test");
        let start = api
            .find("const SIDE_CHANNEL_EVENTS = {")
            .expect("lib/api.ts declares no SIDE_CHANNEL_EVENTS table");
        let table = &api[start..];
        let table = &table[..table
            .find("} as const;")
            .expect("SIDE_CHANNEL_EVENTS does not end with `} as const;`")];
        let listed: std::collections::BTreeSet<&str> =
            table.split('\'').skip(1).step_by(2).collect();
        let declared: std::collections::BTreeSet<&str> = declared.into_iter().collect();
        assert_eq!(
            listed, declared,
            "SIDE_CHANNEL_EVENTS in web/src/lib/api.ts must name exactly the \
             side-channel events that the daemon sends"
        );
    }
}
