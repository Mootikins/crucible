//! `turn_finished`: the one event that ends a whole turn for the clients.
//!
//! A turn ENDS. A `turn:complete` handler that wants more work asks for a NEW
//! turn, and that turn opens with its own `user_message` marked
//! `origin: plugin`. These tests pin both halves.

use super::*;
use crucible_core::protocol::session_events::{SessionEventPayload, TurnPayload};
use crucible_core::turn::{TurnOrigin, TurnStatus};

/// The decoded fields of a `turn_finished` event.
fn finished_fields(
    event: &SessionEventMessage,
) -> (TurnStatus, Option<StopReason>, Option<String>) {
    match event.payload() {
        Ok(SessionEventPayload::Turn(TurnPayload::TurnFinished {
            status,
            stop_reason,
            error,
        })) => (status, stop_reason, error),
        other => panic!("expected turn_finished, got {other:?}"),
    }
}

/// Every event of the turn, up to and including `turn_finished`.
async fn events_until_turn_finished(
    rx: &mut broadcast::Receiver<SessionEventMessage>,
) -> Vec<SessionEventMessage> {
    let mut events = Vec::new();
    timeout(Duration::from_secs(10), async {
        loop {
            let event = rx.recv().await.expect("the event channel stays open");
            let last = event.event == "turn_finished";
            events.push(event);
            if last {
                return;
            }
        }
    })
    .await
    .expect("timed out waiting for turn_finished");
    events
}

/// No second `turn_finished` follows the first one. A probe turn is the end
/// signal: its `user_message` comes before the next `turn_finished`.
async fn assert_no_more_turn_finished(h: &mut ReactorTestHarness) {
    h.send("probe").await;
    let events = events_until_turn_finished(&mut h.event_rx).await;
    assert!(
        events.iter().any(|e| e.event == "user_message"),
        "a turn sent turn_finished twice"
    );
}

/// The origin of a `user_message` event.
fn origin_of(event: &SessionEventMessage) -> TurnOrigin {
    match event.payload() {
        Ok(SessionEventPayload::Turn(TurnPayload::UserMessage { origin, .. })) => origin,
        other => panic!("expected user_message, got {other:?}"),
    }
}

/// One turn sends exactly one `message_complete` and exactly one
/// `turn_finished`, and `turn_finished` is the last event of the turn.
#[tokio::test]
async fn a_turn_finishes_once_with_the_stop_reason_of_its_last_call() {
    let mut h = ReactorTestHarness::new().await;
    h.inject_streaming_agent(vec![
        script::text("part"),
        TurnEvent::Done {
            stop_reason: StopReason::MaxTokens,
        },
    ]);

    h.send("hello").await;
    let events = events_until_turn_finished(&mut h.event_rx).await;

    let completes = events
        .iter()
        .filter(|e| e.event == "message_complete")
        .count();
    assert_eq!(completes, 1, "a turn completes once");
    let finished = events.last().expect("turn_finished is the last event");
    assert_eq!(
        finished_fields(finished),
        (TurnStatus::Completed, Some(StopReason::MaxTokens), None)
    );
    assert_no_more_turn_finished(&mut h).await;
}

/// A handler cancel has its own status, apart from a user cancel.
#[tokio::test]
async fn a_pre_llm_call_cancel_finishes_the_turn_as_handler_cancelled() {
    let mut h = ReactorTestHarness::new().await;
    let _vm = h.load_daemon_lua(
        r#"
            cru.on("pre_llm_call", function(ctx, event)
                return { cancel = true, reason = "not now" }
            end)
        "#,
    );
    h.inject_streaming_agent(ReactorTestHarness::default_ok_events());

    h.send("hello").await;
    let events = events_until_turn_finished(&mut h.event_rx).await;

    let (status, stop_reason, error) = finished_fields(events.last().unwrap());
    assert_eq!(status, TurnStatus::HandlerCancelled);
    assert_eq!(stop_reason, None);
    assert!(
        error.as_deref().is_some_and(|e| e.contains("pre_llm_call")),
        "the reason names the handler stage: {error:?}"
    );
    assert_no_more_turn_finished(&mut h).await;
}

/// An agent error finishes the turn as failed, with the error text.
#[tokio::test]
async fn an_agent_error_finishes_the_turn_as_failed() {
    let mut h = ReactorTestHarness::new().await;
    h.inject_streaming_agent(vec![TurnEvent::Error(
        crucible_core::turn::TurnError::Communication("the provider is down".into()),
    )]);

    h.send("hello").await;
    let events = events_until_turn_finished(&mut h.event_rx).await;

    let (status, _, error) = finished_fields(events.last().unwrap());
    assert_eq!(status, TurnStatus::Failed);
    assert!(
        error
            .as_deref()
            .is_some_and(|e| e.contains("the provider is down")),
        "the error text reaches the client: {error:?}"
    );
}

/// A user cancel finishes the turn as cancelled.
#[tokio::test]
async fn a_user_cancel_finishes_the_turn_as_cancelled() {
    let mut h = ReactorTestHarness::new().await;
    h.inject_agent(Box::new(super::concurrency::PendingMockAgent));

    h.send("hello").await;
    h.wait_for("user_message").await;
    assert!(h.agent_manager.cancel(&h.session_id).await);
    let events = events_until_turn_finished(&mut h.event_rx).await;

    let (status, _, error) = finished_fields(events.last().unwrap());
    assert_eq!(status, TurnStatus::Cancelled);
    assert_eq!(error, None);
}

/// A `turn:complete` handler that returns an inject starts a NEW turn.
///
/// The new turn opens with its own `user_message`, after the first turn's
/// `turn_finished`, and that message is marked `origin: plugin`. The first
/// turn's `user_message` is a person's, so it carries no origin.
#[tokio::test]
async fn a_turn_complete_inject_starts_a_new_turn_marked_as_the_plugin_s() {
    let mut h = ReactorTestHarness::new().await;
    let _vm = h.load_daemon_lua(
        r#"
            asked = false
            cru.on("turn:complete", function(ctx, event)
                if asked then return end
                asked = true
                return { inject = { content = "keep going" } }
            end)
        "#,
    );
    h.inject_streaming_agent(ReactorTestHarness::default_ok_events());

    h.send("hello").await;
    let first = events_until_turn_finished(&mut h.event_rx).await;
    let opening = first
        .iter()
        .find(|e| e.event == "user_message")
        .expect("the turn opens with a user_message");
    assert_eq!(origin_of(opening), TurnOrigin::User);

    // The plugin's turn is a whole turn of its own, with its own end.
    let second = events_until_turn_finished(&mut h.event_rx).await;
    let opening = second
        .iter()
        .find(|e| e.event == "user_message")
        .expect("the plugin's turn opens with a user_message too");
    assert_eq!(origin_of(opening), TurnOrigin::Plugin);
    match opening.payload() {
        Ok(SessionEventPayload::Turn(TurnPayload::UserMessage { content, .. })) => {
            assert_eq!(content, "keep going");
        }
        other => panic!("expected user_message, got {other:?}"),
    }
    assert!(
        second.iter().any(|e| e.event == "message_complete"),
        "the plugin's turn runs the agent: {:?}",
        second.iter().map(|e| &e.event).collect::<Vec<_>>()
    );
    assert_eq!(
        finished_fields(second.last().unwrap()).0,
        TurnStatus::Completed
    );
}

/// A user cancel clears a follow-up a handler already stored, so the cancel
/// stops the work rather than starting more of it.
#[tokio::test]
async fn a_user_cancel_clears_the_turn_a_handler_asked_for() {
    let h = ReactorTestHarness::new().await;
    let slot = h.agent_manager.slot(&h.session_id);
    slot.set_follow_up("keep going".into());

    let mut rx = h.event_tx.subscribe();
    h.inject_agent(Box::new(super::concurrency::PendingMockAgent));
    h.agent_manager
        .send_message(&h.session_id, "hello".into(), &h.event_tx, true, None)
        .await
        .unwrap();
    next_event_or_skip(&mut rx, "user_message").await;
    assert!(h.agent_manager.cancel(&h.session_id).await);
    let events = events_until_turn_finished(&mut rx).await;
    assert_eq!(
        finished_fields(events.last().unwrap()).0,
        TurnStatus::Cancelled
    );

    assert!(
        slot.take_follow_up().is_none(),
        "the cancel must clear the stored follow-up"
    );
    // A probe turn is the end signal. A follow-up turn would take the slot
    // or open with its own `user_message` before the probe ends.
    h.inject_streaming_agent(ReactorTestHarness::default_ok_events());
    h.agent_manager
        .send_message(&h.session_id, "probe".into(), &h.event_tx, true, None)
        .await
        .expect("no turn holds the slot after the cancel");
    let opened: Vec<_> = events_until_turn_finished(&mut rx)
        .await
        .into_iter()
        .filter(|e| e.event == "user_message")
        .map(|e| e.data["content"].clone())
        .collect();
    assert_eq!(opened, ["probe"], "no turn may start after the cancel");
}

/// The permission state of a turn ends with the turn. An ACP permission
/// request outside any turn gets `Cancelled`, not the override of the last
/// turn.
#[tokio::test]
async fn a_permission_request_after_the_turn_is_cancelled() {
    use agent_client_protocol::schema::v1::{
        PermissionOption, PermissionOptionKind, RequestPermissionOutcome,
    };
    let h = ReactorTestHarness::new().await;
    h.inject_streaming_agent(ReactorTestHarness::default_ok_events());
    let allow = Some(crucible_core::config::components::permissions::PermissionMode::Allow);
    let (_, done) = h
        .agent_manager
        .send_message_notified(&h.session_id, "hi".into(), &h.event_tx, true, allow)
        .await
        .unwrap();
    assert_eq!(done.await.unwrap().status, TurnStatus::Completed);

    let handle = h.agent_manager.build_acp_permission_handler(
        &h.session_id,
        &h.event_tx,
        std::path::Path::new("/w"),
        None,
    );
    let call = crucible_core::types::CanonicalToolCall::crucible_tool(
        "bash",
        &serde_json::json!({ "command": "rm -rf x" }),
    );
    let options = vec![
        PermissionOption::new("allow", "Allow", PermissionOptionKind::AllowOnce),
        PermissionOption::new("reject", "Reject", PermissionOptionKind::RejectOnce),
    ];
    let outcome = handle(call, options).await;
    assert!(
        matches!(outcome, RequestPermissionOutcome::Cancelled),
        "{outcome:?}"
    );
}

/// A caller that awaits a turn owns the next turn of the session. A
/// `turn:complete` inject starts no follow-up for an awaited turn, so the
/// next send of the caller finds the slot free.
#[tokio::test]
async fn an_awaited_turn_starts_no_follow_up() {
    let mut h = ReactorTestHarness::new().await;
    let _vm = h.load_daemon_lua(
        r#"
            cru.on("turn:complete", function(ctx, event)
                return { inject = { content = "keep going" } }
            end)
        "#,
    );
    h.inject_streaming_agent(ReactorTestHarness::default_ok_events());

    for step in ["one", "two"] {
        let (_, done) = h
            .agent_manager
            .send_message_notified(&h.session_id, step.into(), &h.event_tx, false, None)
            .await
            .unwrap_or_else(|e| panic!("step {step} must start: {e}"));
        assert_eq!(done.await.unwrap().status, TurnStatus::Completed);
        let events = events_until_turn_finished(&mut h.event_rx).await;
        let opening = events.iter().find(|e| e.event == "user_message").unwrap();
        assert_eq!(
            origin_of(opening),
            TurnOrigin::User,
            "no plugin turn starts"
        );
    }
}
