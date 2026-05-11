use super::super::agent::SessionAgent;
use super::super::config::{
    default_precognition_results, default_validation_retries, ContextStrategy, OutputValidation,
};
use super::super::enums::{SessionState, SessionType};
use super::super::session::Session;
use super::super::summary::SessionSummary;
use crate::config::BackendType;
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn test_session_new() {
    let kiln = PathBuf::from("/home/user/notes");
    let session = Session::new(SessionType::Chat, kiln.clone());

    assert!(session.id.starts_with("chat-"));
    assert_eq!(session.session_type, SessionType::Chat);
    assert_eq!(session.kiln, kiln);
    assert_eq!(session.workspace, kiln); // defaults to kiln
    assert!(session.connected_kilns.is_empty());
    assert_eq!(session.state, SessionState::Active);
}

#[test]
fn test_session_with_workspace() {
    let kiln = PathBuf::from("/home/user/notes");
    let workspace = PathBuf::from("/home/user/project");
    let session = Session::new(SessionType::Agent, kiln.clone()).with_workspace(workspace.clone());

    assert_eq!(session.kiln, kiln);
    assert_eq!(session.workspace, workspace);
}

#[test]
fn test_session_with_connected_kilns() {
    let kiln = PathBuf::from("/home/user/notes");
    let reference = PathBuf::from("/home/user/reference");
    let session =
        Session::new(SessionType::Chat, kiln.clone()).with_connected_kiln(reference.clone());

    assert!(session.can_access_kiln(&kiln));
    assert!(session.can_access_kiln(&reference));
    assert!(!session.can_access_kiln(&PathBuf::from("/other")));
}

#[test]
fn test_session_storage_paths() {
    let kiln = PathBuf::from("/home/user/notes");
    let session = Session::new(SessionType::Chat, kiln);

    let storage = session.storage_path();
    assert!(storage
        .to_string_lossy()
        .contains(".crucible/sessions/chat-"));
    assert!(session.log_path().ends_with("session.md"));
    assert!(session.jsonl_path().ends_with("session.jsonl"));
    assert!(session.artifacts_path().ends_with("artifacts"));
}

#[test]
fn test_session_state_transitions() {
    let kiln = PathBuf::from("/home/user/notes");
    let mut session = Session::new(SessionType::Chat, kiln);

    assert!(session.is_active());

    session.pause();
    assert_eq!(session.state, SessionState::Paused);
    assert!(!session.is_active());

    session.resume();
    assert_eq!(session.state, SessionState::Active);
    assert!(session.is_active());

    session.end();
    assert_eq!(session.state, SessionState::Ended);
    assert!(!session.is_active());
}

#[test]
fn test_session_serialization() {
    let kiln = PathBuf::from("/home/user/notes");
    let session = Session::new(SessionType::Chat, kiln).with_title("Test session");

    let json = serde_json::to_string(&session).unwrap();
    assert!(json.contains("\"session_type\":\"chat\""));
    assert!(json.contains("\"state\":\"active\""));
    assert!(json.contains("\"title\":\"Test session\""));

    let parsed: Session = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.session_type, session.session_type);
    assert_eq!(parsed.title, session.title);
}

#[test]
fn test_session_with_agent() {
    let agent = SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("openai".to_string()),
        provider: BackendType::OpenAI,
        model: "gpt-4o".to_string(),
        system_prompt: "You are helpful.".to_string(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: Vec::new(),
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: true,
        precognition_results: default_precognition_results(),
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: ContextStrategy::default(),
        context_window: None,
        output_validation: OutputValidation::default(),
        validation_retries: default_validation_retries(),
        autocompact_threshold: None,
        grammar: None,
    };

    let kiln = PathBuf::from("/home/user/notes");
    let session = Session::new(SessionType::Chat, kiln).with_agent(agent.clone());

    assert!(session.agent.is_some());
    assert_eq!(session.agent.as_ref().unwrap().model, "gpt-4o");

    let json = serde_json::to_string(&session).unwrap();
    assert!(json.contains("\"model\":\"gpt-4o\""));

    let parsed: Session = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.agent.as_ref().unwrap().model, "gpt-4o");
}

#[test]
fn test_session_backward_compatibility() {
    // Simulate loading a session.json from before agent field existed
    let old_json = r#"{
        "id": "chat-2025-01-08T1530-abc123",
        "session_type": "chat",
        "kiln": "/home/user/notes",
        "workspace": "/home/user/notes",
        "state": "active",
        "started_at": "2025-01-08T15:30:00Z"
    }"#;

    let session: Session = serde_json::from_str(old_json).unwrap();
    assert!(session.agent.is_none());
    assert_eq!(session.id, "chat-2025-01-08T1530-abc123");
}

#[test]
fn test_session_summary_includes_agent_model() {
    let agent = SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("anthropic".to_string()),
        provider: BackendType::Anthropic,
        model: "claude-3-5-sonnet".to_string(),
        system_prompt: "".to_string(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: Vec::new(),
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: true,
        precognition_results: default_precognition_results(),
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: ContextStrategy::default(),
        context_window: None,
        output_validation: OutputValidation::default(),
        validation_retries: default_validation_retries(),
        autocompact_threshold: None,
        grammar: None,
    };

    let kiln = PathBuf::from("/home/user/notes");
    let session = Session::new(SessionType::Chat, kiln).with_agent(agent);

    let summary = SessionSummary::from(&session);
    assert_eq!(summary.agent_model, Some("claude-3-5-sonnet".to_string()));
}

#[test]
fn test_session_parent_session_id_backward_compat_old_json_without_field() {
    // Old JSON without parent_session_id should deserialize to None
    let old_json = r#"{
        "id": "chat-2025-01-08T1530-abc123",
        "session_type": "chat",
        "kiln": "/home/user/notes",
        "workspace": "/home/user/notes",
        "state": "active",
        "started_at": "2025-01-08T15:30:00Z"
    }"#;

    let session: Session = serde_json::from_str(old_json).unwrap();
    assert_eq!(session.parent_session_id, None);
    assert_eq!(session.id, "chat-2025-01-08T1530-abc123");
}

#[test]
fn test_session_parent_session_id_round_trip() {
    // parent_session_id: Some("parent-123") should round-trip correctly
    let kiln = PathBuf::from("/home/user/notes");
    let session = Session::new(SessionType::Chat, kiln).with_title("Child session");

    // Manually set parent_session_id (no builder method yet, just for test)
    let mut session_with_parent = session;
    session_with_parent.parent_session_id = Some("parent-123".to_string());

    let json = serde_json::to_string(&session_with_parent).unwrap();
    assert!(json.contains("\"parent_session_id\":\"parent-123\""));

    let parsed: Session = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.parent_session_id, Some("parent-123".to_string()));
}

#[test]
fn test_session_parent_session_id_omitted_when_none() {
    // When parent_session_id is None, it should be omitted from JSON
    let kiln = PathBuf::from("/home/user/notes");
    let session = Session::new(SessionType::Chat, kiln);

    let json = serde_json::to_string(&session).unwrap();
    // parent_session_id should not appear in JSON when None
    assert!(!json.contains("parent_session_id"));
}

#[test]
fn test_session_default_no_recording_mode() {
    // Session::new() should have recording_mode: None
    let kiln = PathBuf::from("/home/user/notes");
    let session = Session::new(SessionType::Chat, kiln);

    assert_eq!(session.recording_mode, None);
    assert!(!session.is_granular());
}

#[test]
fn test_session_last_activity_serde_compat() {
    // Old JSON without last_activity should deserialize with None
    let old_json = r#"{
        "id": "chat-2025-01-08T1530-abc123",
        "session_type": "chat",
        "kiln": "/home/user/notes",
        "workspace": "/home/user/notes",
        "state": "active",
        "started_at": "2025-01-08T15:30:00Z",
        "archived": false
    }"#;

    let session: Session = serde_json::from_str(old_json).unwrap();
    assert!(session.last_activity.is_none());
}

#[test]
fn test_session_last_activity_omitted_when_none() {
    // When last_activity is None, it should be omitted from JSON
    let kiln = PathBuf::from("/home/user/notes");
    let mut session = Session::new(SessionType::Chat, kiln);
    session.last_activity = None;

    let json = serde_json::to_string(&session).unwrap();
    assert!(!json.contains("last_activity"));
}

#[test]
fn test_session_last_activity_set_on_creation() {
    // New sessions should have last_activity set
    let kiln = PathBuf::from("/home/user/notes");
    let session = Session::new(SessionType::Chat, kiln);
    assert!(session.last_activity.is_some());
}
