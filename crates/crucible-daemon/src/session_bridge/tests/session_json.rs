//! `session_json` is the record a Lua caller sees for one session. The
//! function is pure, so these tests need no daemon.

use super::super::session_json;
use super::make_test_agent;
use crucible_core::session::{Session, SessionSummary, SessionType};

#[test]
fn session_json_names_the_agent_model() {
    let mut session = Session::new(SessionType::Chat, Vec::new());
    let mut agent = make_test_agent(None);
    agent.model = "claude-haiku-4-5-20251001".to_string();
    session.agent = Some(agent);

    let json = session_json(&SessionSummary::from(&session));

    assert_eq!(json["model"], "claude-haiku-4-5-20251001");
    assert!(json["started_at"].is_string());
    assert_eq!(
        json["event_count"],
        SessionSummary::from(&session).event_count
    );
}

#[test]
fn session_json_leaves_model_null_without_an_agent() {
    let session = Session::new(SessionType::Chat, Vec::new());

    let json = session_json(&SessionSummary::from(&session));

    assert!(json["model"].is_null());
    assert_eq!(json["session_type"], "chat");
}
