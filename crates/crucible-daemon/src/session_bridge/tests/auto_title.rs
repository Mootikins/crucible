//! A user runs the shipped `auto-title` command from a session, through the
//! `plugin.run_command` handler, as the TUI does for `/generate`.
//!
//! Nothing here is scripted except the provider: the real loader activates the
//! real plugin, the real bridge answers `cru.session.messages` and
//! `cru.session.set_title`, and `cru.session.complete` reaches a wiremock
//! provider through the daemon's one-shot completion.

use super::rig::Rig;
use super::*;
use crate::observe::LogEvent;
use crate::protocol::{Request, RequestId};
use serde_json::json;

impl Rig {
    /// A chat session on the fixture provider, with two exchanges.
    async fn session_with_two_exchanges(&self) -> String {
        let session = self
            .sessions
            .create_session(SessionType::Chat, vec![], None, None)
            .await
            .unwrap();
        let mut agent = make_test_agent(None);
        agent.endpoint = Some(self.provider.uri());
        self.agents
            .configure_agent(&session.id, agent)
            .await
            .unwrap();
        for event in [
            LogEvent::user("what is the weather"),
            LogEvent::assistant("sunny"),
            LogEvent::user("now help me fix the auth flow"),
            LogEvent::assistant("it is fixed"),
        ] {
            self.sessions
                .storage()
                .append_event(&session, &event.to_jsonl().unwrap())
                .await
                .unwrap();
        }
        session.id.to_string()
    }

    /// `plugin.run_command` as the TUI sends it for a bare `/generate`.
    async fn run(&self, session_id: Option<&str>) -> crate::protocol::Response {
        let mut params = json!({ "name": "generate" });
        if let Some(id) = session_id {
            params["session_id"] = json!(id);
        }
        crate::server::plugins::handle_plugin_run_command(
            Request {
                jsonrpc: "2.0".to_string(),
                id: Some(RequestId::Number(1)),
                method: "plugin.run_command".to_string(),
                params,
            },
            &self.loader,
        )
        .await
    }
}

/// The regression: `/generate` in the TUI raised "attempt to index userdata
/// with 'user'". It now titles the session from its latest exchange.
#[tokio::test]
async fn generate_run_from_a_session_titles_it_after_the_latest_exchange() {
    let rig = Rig::auto_title().await;
    let id = rig.session_with_two_exchanges().await;

    let response = rig.run(Some(&id)).await;

    assert!(response.error.is_none(), "{:?}", response.error);
    assert_eq!(
        response.result.expect("a result")["result"],
        json!("Session titled: Fixing the auth flow")
    );
    assert_eq!(
        rig.sessions.get_session(&id).unwrap().title.as_deref(),
        Some("Fixing the auth flow")
    );
    let asked = rig.provider.received_requests().await.unwrap();
    let prompt = String::from_utf8_lossy(&asked.last().expect("a completion").body).to_string();
    assert!(
        prompt.contains("now help me fix the auth flow") && prompt.contains("it is fixed"),
        "the prompt must hold the latest exchange: {prompt}"
    );
    assert!(
        prompt.contains("what is the weather"),
        "explicit regeneration uses recent conversation context: {prompt}"
    );
}

/// Without a session the command cannot know what to title, and says so.
#[tokio::test]
async fn generate_run_without_a_session_names_the_reason() {
    let rig = Rig::auto_title().await;

    let response = rig.run(None).await;

    let error = response.error.expect("no session to title").message;
    assert!(
        error.contains("run /generate in a chat session"),
        "the error must name the reason: {error}"
    );
}

impl Rig {
    async fn finish_title_turn(
        &self,
        id: &str,
        label: &str,
    ) -> crate::protocol::SessionEventMessage {
        let session = self.sessions.get_session(id).unwrap();
        let offset = self
            .sessions
            .storage()
            .load_events(&session.id, None, None)
            .await
            .unwrap()
            .len() as u64;
        let mut events = vec![
            crate::protocol::SessionEventMessage::new(
                id,
                "user_message",
                json!({ "message_id": label, "content": label }),
            ),
            crate::protocol::SessionEventMessage::new(
                id,
                "message_complete",
                json!({ "message_id": label, "full_response": format!("answer {label}") }),
            ),
            crate::protocol::SessionEventMessage::new(
                id,
                "turn_finished",
                json!({ "status":"completed", "stop_reason":"end_turn" }),
            ),
        ];
        for (index, event) in events.iter_mut().enumerate() {
            event.seq = Some(offset + index as u64 + 1);
            self.sessions
                .storage()
                .append_event(&session, &serde_json::to_string(event).unwrap())
                .await
                .unwrap();
        }
        events.pop().unwrap()
    }

    async fn title_session(&self) -> String {
        let session = self
            .sessions
            .create_session(SessionType::Chat, vec![], None, None)
            .await
            .unwrap();
        let mut agent = make_test_agent(None);
        agent.endpoint = Some(self.provider.uri());
        self.agents
            .configure_agent(&session.id, agent)
            .await
            .unwrap();
        session.id.to_string()
    }
}

#[tokio::test]
async fn core_titling_waits_for_three_turns_without_the_compatibility_plugin() {
    let mut rig = Rig::title_runtime(false).await;
    let id = rig.title_session().await;
    for label in ["first topic", "second topic"] {
        let event = rig.finish_title_turn(&id, label).await;
        assert!(rig
            .agents
            .auto_title_after_turn(&event, &rig.ctx.event_tx)
            .await
            .unwrap()
            .is_none());
    }
    assert!(rig.provider.received_requests().await.unwrap().is_empty());
    let event = rig.finish_title_turn(&id, "third topic").await;
    assert_eq!(
        rig.agents
            .auto_title_after_turn(&event, &rig.ctx.event_tx)
            .await
            .unwrap()
            .as_deref(),
        Some("Fixing the auth flow")
    );
    assert_eq!(
        rig.sessions.get_session(&id).unwrap().title.as_deref(),
        Some("Fixing the auth flow")
    );
    assert!(rig
        .agents
        .auto_title_after_turn(&event, &rig.ctx.event_tx)
        .await
        .unwrap()
        .is_none());
    let requests = rig.provider.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body = String::from_utf8_lossy(&requests[0].body);
    for label in ["first topic", "second topic", "third topic"] {
        assert!(body.contains(label), "{body}");
    }
    let titles: Vec<_> = std::iter::from_fn(|| rig.observed.try_recv().ok())
        .filter(|e| e.event == "title_changed")
        .collect();
    assert_eq!(titles.len(), 1);
}

#[tokio::test]
async fn core_titling_retries_a_failed_model_on_the_next_successful_turn() {
    use wiremock::{Mock, ResponseTemplate};
    let rig = Rig::title_runtime(false).await;
    let id = rig.title_session().await;
    rig.provider.reset().await;
    Mock::given(super::rig::is_chat_request)
        .respond_with(ResponseTemplate::new(503))
        .mount(&rig.provider)
        .await;
    let mut event = rig.finish_title_turn(&id, "first").await;
    for label in ["second", "third"] {
        event = rig.finish_title_turn(&id, label).await;
    }
    assert!(rig
        .agents
        .auto_title_after_turn(&event, &rig.ctx.event_tx)
        .await
        .is_err());
    assert!(rig.sessions.get_session(&id).unwrap().title.is_none());
    let requests_before = rig.provider.received_requests().await.unwrap().len();
    assert!(rig
        .agents
        .auto_title_after_turn(&event, &rig.ctx.event_tx)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        rig.provider.received_requests().await.unwrap().len(),
        requests_before
    );
    rig.provider.reset().await;
    Mock::given(super::rig::is_chat_request).respond_with(ResponseTemplate::new(200).set_body_json(json!({
        "model":"llama3.2", "message":{"role":"assistant","content":"Recovered title"}, "done":true,
    }))).mount(&rig.provider).await;
    event = rig.finish_title_turn(&id, "fourth").await;
    assert_eq!(
        rig.agents
            .auto_title_after_turn(&event, &rig.ctx.event_tx)
            .await
            .unwrap()
            .as_deref(),
        Some("Recovered title")
    );
}

#[tokio::test]
async fn acp_background_titling_uses_the_default_llm_without_sending_an_acp_turn() {
    let rig = Rig::title_runtime(false).await;
    let id = rig.title_session().await;
    let mut agent = rig.sessions.get_session(&id).unwrap().agent.unwrap();
    agent.agent_type = "acp".into();
    agent.agent_name = Some("codex".into());
    agent.model = "codex-display-name".into();
    rig.agents.configure_agent(&id, agent).await.unwrap();
    let mut event = rig.finish_title_turn(&id, "first").await;
    for label in ["second", "third"] {
        event = rig.finish_title_turn(&id, label).await;
    }
    assert_eq!(
        rig.agents
            .auto_title_after_turn(&event, &rig.ctx.event_tx)
            .await
            .unwrap()
            .as_deref(),
        Some("Fixing the auth flow")
    );
    let requests = rig.provider.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["model"], "llama3.2");
}

#[tokio::test]
async fn core_titling_recovers_a_new_boundary_arriving_during_a_failed_attempt() {
    use std::time::Duration;
    use wiremock::{Mock, ResponseTemplate};
    let rig = Rig::title_runtime(false).await;
    let id = rig.title_session().await;
    rig.provider.reset().await;
    Mock::given(super::rig::is_chat_request)
        .respond_with(ResponseTemplate::new(503).set_delay(Duration::from_millis(300)))
        .mount(&rig.provider)
        .await;
    rig.finish_title_turn(&id, "first").await;
    rig.finish_title_turn(&id, "second").await;
    let third = rig.finish_title_turn(&id, "third").await;
    let manager = rig.agents.clone();
    let events = rig.ctx.event_tx.clone();
    let attempt = tokio::spawn(async move { manager.auto_title_after_turn(&third, &events).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while rig.provider.received_requests().await.unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    rig.provider.reset().await;
    Mock::given(super::rig::is_chat_request)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"llama3.2", "message":{"role":"assistant","content":"Recovered later boundary"}, "done":true,
        }))).mount(&rig.provider).await;
    let fourth = rig.finish_title_turn(&id, "fourth").await;
    assert!(rig
        .agents
        .auto_title_after_turn(&fourth, &rig.ctx.event_tx)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        attempt.await.unwrap().unwrap().as_deref(),
        Some("Recovered later boundary")
    );
    assert_eq!(rig.provider.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn explicit_title_failure_recovers_an_automatic_boundary_arriving_during_the_request() {
    use std::time::Duration;
    use wiremock::{Mock, ResponseTemplate};
    let rig = Rig::title_runtime(false).await;
    let id = rig.title_session().await;
    rig.provider.reset().await;
    Mock::given(super::rig::is_chat_request)
        .respond_with(ResponseTemplate::new(503).set_delay(Duration::from_millis(300)))
        .mount(&rig.provider)
        .await;
    rig.finish_title_turn(&id, "first").await;
    rig.finish_title_turn(&id, "second").await;
    let manager = rig.agents.clone();
    let events = rig.ctx.event_tx.clone();
    let target = id.clone();
    let attempt =
        tokio::spawn(async move { manager.generate_session_title(&target, &events).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while rig.provider.received_requests().await.unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    rig.provider.reset().await;
    Mock::given(super::rig::is_chat_request)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "model":"llama3.2", "message":{"role":"assistant","content":"Recovered automatic title"}, "done":true,
        }))).mount(&rig.provider).await;
    let third = rig.finish_title_turn(&id, "third").await;
    assert!(rig
        .agents
        .auto_title_after_turn(&third, &rig.ctx.event_tx)
        .await
        .unwrap()
        .is_none());
    assert_eq!(attempt.await.unwrap().unwrap(), "Recovered automatic title");
    assert_eq!(
        rig.sessions.get_session(&id).unwrap().title.as_deref(),
        Some("Recovered automatic title")
    );
}
