//! `session_json` is the record a Lua caller sees for one session. The
//! function is pure, so these tests need no daemon.

use super::super::{session_json, session_record_json};
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

/// `cru.session.get` names the workspace and the isolation value, so a plugin
/// can start a session like the one it read. The reflection pass does this.
#[test]
fn session_record_json_names_the_workspace_and_the_isolation() {
    let session = Session::new(SessionType::Chat, Vec::new())
        .with_workspace(Some("/work/project".into()))
        .with_isolation(Some(serde_json::json!("rust")));

    let json = session_record_json(&session);

    assert_eq!(json["workspace"], "/work/project");
    assert_eq!(json["isolation"], "rust");
}

/// An absent workspace or isolation is an absent key, not `null`: mlua maps
/// `null` to a truthy value, and a plugin that copies it would ask for
/// something.
#[test]
fn session_record_json_leaves_out_an_absent_workspace_and_isolation() {
    let session = Session::new(SessionType::Chat, Vec::new());

    let json = session_record_json(&session);

    assert!(json.get("workspace").is_none(), "{json}");
    assert!(json.get("isolation").is_none(), "{json}");
}
