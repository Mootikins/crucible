use super::super::agent::SessionAgent;
use super::super::config::{
    default_precognition_results, default_validation_retries, ContextStrategy, OutputValidation,
};
use super::super::enums::{SessionState, SessionType};
use super::super::session::Session;
use super::super::summary::SessionSummary;
use crate::config::{BackendType, KilnName};
use std::collections::HashMap;
use std::path::PathBuf;

/// A kiln name, for tests that only care that the session has one.
fn kiln_name(name: &str) -> KilnName {
    KilnName::parse(name).expect("test kiln name")
}

/// A session's kiln set, in one call, for tests that only care about scope.
fn session_with_kilns(kilns: &[&str]) -> Session {
    Session::new(
        SessionType::Chat,
        kilns.iter().copied().map(kiln_name).collect(),
    )
}

#[test]
fn test_session_new() {
    let kiln = kiln_name("notes");
    let session = Session::new(SessionType::Chat, vec![kiln.clone()]);

    assert!(session.id.starts_with("chat-"));
    assert_eq!(session.session_type, SessionType::Chat);
    assert_eq!(session.kilns, vec![kiln.clone()]);
    assert_eq!(session.state, SessionState::Active);
}

/// A session created without one HAS no workspace, and says so.
///
/// It used to be spelled `workspace == kilns[0]` — a sentinel that could not
/// tell "no project" from "the project happens to be the kiln", and which
/// every consumer had to re-derive. `None` states it once.
#[test]
fn a_session_created_without_a_workspace_has_none() {
    let kiln = kiln_name("notes");
    let session = Session::new(SessionType::Chat, vec![kiln]);

    assert_eq!(session.workspace, None);
}

#[test]
fn test_session_with_workspace() {
    let kiln = kiln_name("notes");
    let workspace = PathBuf::from("/home/user/project");
    let session = Session::new(SessionType::Agent, vec![kiln.clone()])
        .with_workspace(Some(workspace.clone()));

    assert_eq!(session.kilns, vec![kiln]);
    assert_eq!(session.workspace, Some(workspace));
}

/// An empty path is not a narrow workspace, it is no workspace: every root
/// builder downstream reads `""` as a root that encloses everything, and
/// `Path::starts_with("")` is true of every path. The setter is where that
/// becomes unrepresentable, so no consumer has to re-check it.
#[test]
fn an_empty_workspace_path_is_no_workspace() {
    let mut session = session_with_kilns(&["notes"]);

    session.set_workspace(Some(PathBuf::new()));
    assert_eq!(session.workspace, None);

    let session = session_with_kilns(&["notes"]).with_workspace(Some(PathBuf::new()));
    assert_eq!(session.workspace, None);
}

#[test]
fn test_session_reaches_every_kiln_in_its_set() {
    let kiln = kiln_name("notes");
    let reference = kiln_name("reference");
    let session = Session::new(SessionType::Chat, vec![kiln.clone()]).with_kiln(reference.clone());

    assert!(session.can_access_kiln(&kiln));
    assert!(session.can_access_kiln(&reference));
    assert!(!session.can_access_kiln(&kiln_name("other")));
}

#[test]
fn test_add_kiln_reports_whether_the_set_changed() {
    let mut session = session_with_kilns(&["notes"]);

    assert!(session.add_kiln(kiln_name("reference")));
    assert!(!session.add_kiln(kiln_name("reference")));
    assert_eq!(session.kilns.len(), 2);
}

#[test]
fn test_session_storage_paths() {
    let sessions_root = PathBuf::from("/home/user/.crucible/sessions");
    let session = session_with_kilns(&["notes"]);

    assert_eq!(
        session.storage_path(&sessions_root),
        sessions_root.join(session.id.as_str())
    );
    assert!(session.log_path(&sessions_root).ends_with("session.md"));
    assert!(session
        .jsonl_path(&sessions_root)
        .ends_with("session.jsonl"));
    assert!(session
        .artifacts_path(&sessions_root)
        .ends_with("artifacts"));
}

#[test]
fn test_legacy_meta_json_merges_kiln_and_connected_kilns() {
    // A meta.json written before the kiln set was flattened.
    let legacy = r#"{
        "id": "chat-2025-01-08T1530-abc123",
        "session_type": "chat",
        "kiln": "/home/user/notes",
        "workspace": "/home/user/project",
        "connected_kilns": ["/home/user/reference", "/home/user/notes"],
        "state": "active",
        "started_at": "2025-01-08T15:30:00Z"
    }"#;

    let mut session: Session = serde_json::from_str(legacy).unwrap();

    assert_eq!(
        session.workspace,
        Some(PathBuf::from("/home/user/project")),
        "a workspace distinct from the kilns is a real one and must survive"
    );
    // Deserialization alone never mints a kiln: turning a path into one needs
    // the registry, and this impl also runs on wire input.
    assert!(
        session.kilns.is_empty(),
        "a path in a meta.json must not become a kiln without the registry"
    );

    // The legacy spelling is not written back, and the flat one round-trips.
    let json = serde_json::to_string(&session).unwrap();
    assert!(!json.contains("connected_kilns"));
    assert!(!json.contains("\"kiln\":"));
    let reloaded: Session = serde_json::from_str(&json).unwrap();
    assert_eq!(reloaded, session);

    // Primary first, connected after, duplicates collapsed — that order is
    // what `default_kiln` reads position from once the storage layer has
    // resolved these into names.
    assert_eq!(
        session.take_persisted_kiln_paths(),
        vec![
            PathBuf::from("/home/user/notes"),
            PathBuf::from("/home/user/reference"),
        ]
    );
}

#[test]
fn test_legacy_meta_json_without_connected_kilns() {
    let legacy = r#"{
        "id": "chat-2025-01-08T1530-abc123",
        "session_type": "chat",
        "kiln": "/home/user/notes",
        "workspace": "/home/user/notes",
        "state": "active",
        "started_at": "2025-01-08T15:30:00Z"
    }"#;

    let mut session: Session = serde_json::from_str(legacy).unwrap();
    assert_eq!(
        session.take_persisted_kiln_paths(),
        vec![PathBuf::from("/home/user/notes")]
    );
}

/// A persisted `workspace` that equals the first kiln is a workspace.
///
/// It is also the shape the pre-`Option` `Session::new` stamped on sessions
/// that were never given one — and on disk the two are indistinguishable.
/// The tie goes to the user: collapsing it would discard a deliberate choice
/// (a repo that is also your kiln) on every restart, where NOT collapsing it
/// only means a rare legacy session reads as working in its own kiln — which
/// is what every consumer but the two sentinel readers already did.
#[test]
fn a_persisted_workspace_equal_to_the_first_kiln_is_a_workspace() {
    let legacy = r#"{
        "id": "chat-2025-01-08T1530-abc123",
        "session_type": "chat",
        "kilns": ["/home/user/notes", "/home/user/reference"],
        "workspace": "/home/user/notes",
        "state": "active",
        "started_at": "2025-01-08T15:30:00Z"
    }"#;

    let session: Session = serde_json::from_str(legacy).unwrap();

    assert_eq!(session.workspace, Some(PathBuf::from("/home/user/notes")));
}

/// The one pre-`Option` spelling that IS unambiguous: `Session::new` fell back
/// to `PathBuf::default()` when there was no first kiln to copy, and an empty
/// path names no directory under any reading. `Deserialize` writes the field
/// directly, so it is the one door the setters do not guard.
#[test]
fn a_persisted_empty_workspace_loads_as_no_workspace() {
    let legacy = r#"{
        "id": "chat-2025-01-08T1530-abc123",
        "session_type": "chat",
        "kilns": [],
        "workspace": "",
        "state": "active",
        "started_at": "2025-01-08T15:30:00Z"
    }"#;

    let session: Session = serde_json::from_str(legacy).unwrap();

    assert_eq!(session.workspace, None);
}

/// And a workspace matching a later kiln, which was never even the sentinel.
#[test]
fn a_persisted_workspace_matching_a_later_kiln_is_kept() {
    let legacy = r#"{
        "id": "chat-2025-01-08T1530-abc123",
        "session_type": "chat",
        "kilns": ["/home/user/notes", "/home/user/reference"],
        "workspace": "/home/user/reference",
        "state": "active",
        "started_at": "2025-01-08T15:30:00Z"
    }"#;

    let session: Session = serde_json::from_str(legacy).unwrap();

    assert_eq!(
        session.workspace,
        Some(PathBuf::from("/home/user/reference"))
    );
}

#[test]
fn test_session_without_kilns_round_trips() {
    let session = Session::new(SessionType::Chat, Vec::new());

    let json = serde_json::to_string(&session).unwrap();
    let reloaded: Session = serde_json::from_str(&json).unwrap();

    assert!(reloaded.kilns.is_empty());
    assert_eq!(reloaded, session);
}

#[test]
fn test_session_state_transitions() {
    let kiln = kiln_name("notes");
    let mut session = Session::new(SessionType::Chat, vec![kiln]);

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
    let kiln = kiln_name("notes");
    let session = Session::new(SessionType::Chat, vec![kiln]).with_title("Test session");

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
        tool_policy: None,
        mode: None,
    };

    let kiln = kiln_name("notes");
    let session = Session::new(SessionType::Chat, vec![kiln]).with_agent(agent.clone());

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
        tool_policy: None,
        mode: None,
    };

    let kiln = kiln_name("notes");
    let session = Session::new(SessionType::Chat, vec![kiln]).with_agent(agent);

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
    let kiln = kiln_name("notes");
    let session = Session::new(SessionType::Chat, vec![kiln]).with_title("Child session");

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
    let kiln = kiln_name("notes");
    let session = Session::new(SessionType::Chat, vec![kiln]);

    let json = serde_json::to_string(&session).unwrap();
    // parent_session_id should not appear in JSON when None
    assert!(!json.contains("parent_session_id"));
}

#[test]
fn test_session_default_no_recording_mode() {
    // Session::new() should have recording_mode: None
    let kiln = kiln_name("notes");
    let session = Session::new(SessionType::Chat, vec![kiln]);

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
    let kiln = kiln_name("notes");
    let mut session = Session::new(SessionType::Chat, vec![kiln]);
    session.last_activity = None;

    let json = serde_json::to_string(&session).unwrap();
    assert!(!json.contains("last_activity"));
}

#[test]
fn test_session_last_activity_set_on_creation() {
    // New sessions should have last_activity set
    let kiln = kiln_name("notes");
    let session = Session::new(SessionType::Chat, vec![kiln]);
    assert!(session.last_activity.is_some());
}

/// A DELIBERATE workspace that happens to equal the session's kiln must
/// survive a restart. Working in a repo that is also your kiln is an ordinary
/// setup (`cru chat --kiln .`), and the old encoding could not tell it from
/// "no project" — which is precisely why the sentinel had to go. Collapsing
/// it on load would re-import the ambiguity and silently discard the choice
/// every time the daemon restarts.
#[test]
fn a_workspace_equal_to_the_kiln_survives_a_save_and_load_round_trip() {
    let workspace = PathBuf::from("/repos/crucible");
    let session = Session::new(SessionType::Chat, vec![kiln_name("crucible")])
        .with_workspace(Some(workspace.clone()));

    let json = serde_json::to_string(&session).unwrap();
    let reloaded: Session = serde_json::from_str(&json).unwrap();

    assert_eq!(reloaded.workspace, Some(workspace));
}
