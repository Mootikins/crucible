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
        !prompt.contains("what is the weather"),
        "the prompt must not hold an earlier exchange: {prompt}"
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
