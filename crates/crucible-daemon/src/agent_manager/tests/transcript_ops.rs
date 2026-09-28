//! The ops that the event bus sends with each live event rebuild the
//! transcript that the session history serves.

use super::*;

/// Run one scripted turn, store the events that the daemon writer stores,
/// and answer every event that the session broadcast.
async fn run_turn(
    am: &Arc<AgentManager>,
    sm: &SessionManager,
    session: &crucible_core::session::Session,
    tx: &crate::EventBus,
    rx: &mut broadcast::Receiver<SessionEventMessage>,
    prompt: &str,
    events: Vec<TurnEvent>,
) -> Vec<SessionEventMessage> {
    am.install_agent_for_test(
        session.id.to_string(),
        Arc::new(Mutex::new(Box::new(PromptCapturingAgent {
            received_prompt: Arc::new(StdMutex::new(None)),
            received_messages: Arc::new(StdMutex::new(None)),
            events,
        }))),
    );
    am.send_message(&session.id, prompt.to_string(), tx, true, None)
        .await
        .unwrap();
    let mut seen = Vec::new();
    loop {
        let event = timeout(Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap();
        if event.payload().is_ok_and(|payload| payload.is_persisted()) {
            // The journal stores the event without its ops.
            let mut stored = event.clone();
            stored.transcript.clear();
            sm.storage()
                .append_event(session, &serde_json::to_string(&stored).unwrap())
                .await
                .unwrap();
        }
        let finished = event.event == "turn_finished";
        seen.push(event);
        if finished {
            break;
        }
    }
    while am.request_state.contains_key(session.id.as_str()) {
        tokio::task::yield_now().await;
    }
    seen
}

#[tokio::test]
async fn the_live_ops_rebuild_the_history_transcript() {
    let sm = temp_session_manager();
    let session = sm
        .create_session(SessionType::Chat, vec![], None, None)
        .await
        .unwrap();
    let am = create_test_agent_manager(sm.clone());
    am.configure_agent(&session.id, test_agent()).await.unwrap();
    let (tx, mut rx) = crate::EventBus::channel(256);

    let mut followed = sm.load_transcript(&session.id).await.unwrap();
    for (prompt, events) in [
        (
            "first",
            vec![
                script::thinking("think "),
                script::thinking("more"),
                script::text("one "),
                script::text("answer"),
                script::done(),
            ],
        ),
        ("second", vec![script::text("two"), script::done()]),
    ] {
        for event in run_turn(&am, &sm, &session, &tx, &mut rx, prompt, events).await {
            for op in &event.transcript {
                assert!(followed.apply(op), "an op did not fit: {op:?}");
            }
        }
    }

    let history = sm.load_transcript(&session.id).await.unwrap();
    assert_eq!(followed.items, history.items);
    let answers: Vec<&str> = history
        .items
        .iter()
        .filter_map(|item| match &item.body {
            crucible_core::transcript::ItemBody::AssistantSegment { text, .. } => {
                Some(text.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(answers, ["one answer", "two"]);
}
