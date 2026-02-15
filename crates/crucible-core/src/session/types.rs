//! Core session types.

use chrono::{DateTime, Utc};
use crucible_config::{AgentProfile, DelegationConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Agent configuration bound to a session.
///
/// This captures everything needed to reconstruct an agent when resuming
/// a session. The configuration is inlined (not just a reference) so that
/// sessions are self-contained and reproducible.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionAgent {
    /// Agent type: "acp" (external) or "internal" (built-in)
    pub agent_type: String,

    /// ACP agent name (e.g., "opencode", "claude") - only for ACP agents
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,

    /// Provider key (e.g., "ollama", "openai", "anthropic") - only for internal agents
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_key: Option<String>,

    /// LLM provider identifier
    pub provider: String,

    /// Model identifier (e.g., "llama3.2", "gpt-4o", "claude-3-5-sonnet")
    pub model: String,

    /// System prompt (full text, inlined from agent card if applicable)
    pub system_prompt: String,

    /// Generation temperature (0.0 - 2.0)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    /// Maximum output tokens
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Maximum context window tokens
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_context_tokens: Option<usize>,

    /// Thinking/reasoning token budget for models that support extended thinking.
    /// -1 = unlimited, 0 = disabled, >0 = max tokens for thinking
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<i64>,

    /// Custom endpoint URL (for self-hosted models)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,

    /// Environment variable overrides for ACP agents
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env_overrides: HashMap<String, String>,

    /// MCP servers this agent can use
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<String>,

    /// Source agent card name (for reference, not used for reconstruction)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_card_name: Option<String>,

    /// List of capabilities this agent provides (from ACP agent profile)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,

    /// Human-readable description of this agent (from ACP agent profile)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_description: Option<String>,

    /// Delegation configuration for this agent (from ACP agent profile)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation_config: Option<DelegationConfig>,
}

impl SessionAgent {
    /// Construct a SessionAgent from an enriched AgentProfile.
    ///
    /// Creates an ACP-type SessionAgent with:
    /// - agent_type: "acp"
    /// - agent_name: the provided agent_name
    /// - provider: "acp"
    /// - model: the provided agent_name
    /// - capabilities, agent_description, delegation_config from profile
    /// - env_overrides: profile's env vars (isolated, parent env NOT inherited)
    pub fn from_profile(profile: &AgentProfile, agent_name: &str) -> Self {
        Self {
            agent_type: "acp".to_string(),
            agent_name: Some(agent_name.to_string()),
            provider_key: None,
            provider: "acp".to_string(),
            model: agent_name.to_string(),
            system_prompt: String::new(),
            temperature: None,
            max_tokens: None,
            max_context_tokens: None,
            thinking_budget: None,
            endpoint: None,
            env_overrides: profile.env.clone(),
            mcp_servers: Vec::new(),
            agent_card_name: None,
            capabilities: profile.capabilities.clone(),
            agent_description: profile.description.clone(),
            delegation_config: profile.delegation.clone(),
        }
    }
}

/// Generate a session ID with the given type prefix.
///
/// Format: `{type}-{YYYY-MM-DDTHHMM}-{random6}`
/// Example: `chat-2025-01-08T1530-a1b2c3`
fn generate_session_id(type_prefix: &str) -> String {
    use rand::Rng;
    let timestamp = Utc::now().format("%Y-%m-%dT%H%M");
    let mut rng = rand::rng();
    let random: String = (0..6)
        .map(|_| {
            let idx: u8 = rng.random_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + (idx - 10)) as char
            }
        })
        .collect();
    format!("{}-{}-{}", type_prefix, timestamp, random)
}

/// A session is a continuous sequence of agent actions in a workspace.
///
/// Sessions are the fundamental unit of agent interaction in Crucible.
/// They track conversation history, agent reasoning, tool calls, and
/// can be persisted, resumed, and searched.
///
/// # Storage
///
/// Sessions are stored in their owning kiln at:
/// `{kiln}/.crucible/sessions/{session_id}/`
///
/// Contents:
/// - `session.md` - Human-readable markdown log
/// - `session.jsonl` - Machine-readable event log
/// - `artifacts/` - Generated files, fetched content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Session {
    /// Unique identifier (e.g., "chat-2025-01-08T1530-abc123")
    pub id: String,

    /// Session type determines logging format and behavior
    pub session_type: SessionType,

    /// The kiln that owns/stores this session
    pub kiln: PathBuf,

    /// Working directory for file operations (may differ from kiln)
    pub workspace: PathBuf,

    /// Additional kilns this session can query (beyond the owning kiln)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connected_kilns: Vec<PathBuf>,

    /// Current state
    pub state: SessionState,

    /// When the session started
    pub started_at: DateTime<Utc>,

    /// Optional continuation from previous session
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continued_from: Option<String>,

    /// Optional title/description for the session
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Agent configuration for this session (persisted for resume)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<SessionAgent>,

    /// Notification queue for this session
    #[serde(
        default,
        skip_serializing_if = "crate::types::NotificationQueue::is_empty"
    )]
    pub notifications: crate::types::NotificationQueue,
}

impl Session {
    /// Create a new session with the given type and owning kiln.
    ///
    /// The workspace defaults to the kiln path.
    pub fn new(session_type: SessionType, kiln: PathBuf) -> Self {
        let type_prefix = session_type.as_prefix();
        let id = generate_session_id(type_prefix);

        Self {
            id,
            session_type,
            workspace: kiln.clone(),
            kiln,
            connected_kilns: Vec::new(),
            state: SessionState::Active,
            started_at: Utc::now(),
            continued_from: None,
            title: None,
            agent: None,
            notifications: crate::types::NotificationQueue::new(),
        }
    }

    /// Set the workspace (where agent operates).
    pub fn with_workspace(mut self, workspace: PathBuf) -> Self {
        self.workspace = workspace;
        self
    }

    /// Add a connected kiln for knowledge queries.
    pub fn with_connected_kiln(mut self, kiln: PathBuf) -> Self {
        self.connected_kilns.push(kiln);
        self
    }

    /// Set multiple connected kilns.
    pub fn with_connected_kilns(mut self, kilns: Vec<PathBuf>) -> Self {
        self.connected_kilns = kilns;
        self
    }

    /// Set the session as a continuation of another.
    pub fn continued_from(mut self, session_id: impl Into<String>) -> Self {
        self.continued_from = Some(session_id.into());
        self
    }

    /// Set the session title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the agent configuration.
    pub fn with_agent(mut self, agent: SessionAgent) -> Self {
        self.agent = Some(agent);
        self
    }

    /// Get the storage path for this session.
    ///
    /// When the kiln is the crucible home (`~/.crucible/`), returns
    /// `~/.crucible/sessions/{id}` to avoid double-nesting `.crucible/.crucible/`.
    /// Otherwise returns `{kiln}/.crucible/sessions/{session_id}/`.
    pub fn storage_path(&self) -> PathBuf {
        if crucible_config::is_crucible_home(&self.kiln) {
            self.kiln.join("sessions").join(&self.id)
        } else {
            self.kiln.join(".crucible").join("sessions").join(&self.id)
        }
    }

    /// Get the path to the markdown log file.
    pub fn log_path(&self) -> PathBuf {
        self.storage_path().join("session.md")
    }

    /// Get the path to the JSONL event log.
    pub fn jsonl_path(&self) -> PathBuf {
        self.storage_path().join("session.jsonl")
    }

    /// Get the artifacts directory path.
    pub fn artifacts_path(&self) -> PathBuf {
        self.storage_path().join("artifacts")
    }

    /// Check if this session can access a given kiln.
    pub fn can_access_kiln(&self, kiln: &PathBuf) -> bool {
        &self.kiln == kiln || self.connected_kilns.contains(kiln)
    }

    /// Pause the session.
    pub fn pause(&mut self) {
        if self.state == SessionState::Active {
            self.state = SessionState::Paused;
        }
    }

    /// Resume a paused session.
    pub fn resume(&mut self) {
        if self.state == SessionState::Paused {
            self.state = SessionState::Active;
        }
    }

    /// End the session.
    pub fn end(&mut self) {
        self.state = SessionState::Ended;
    }

    /// Check if the session is active.
    pub fn is_active(&self) -> bool {
        self.state == SessionState::Active
    }
}

/// Type of session, determines logging format and behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionType {
    /// User/assistant conversation (interactive chat)
    Chat,
    /// Autonomous agent actions (may run without user input)
    Agent,
    /// Programmatic workflow execution
    Workflow,
}

impl SessionType {
    /// Get the string prefix used in session IDs.
    pub fn as_prefix(&self) -> &'static str {
        match self {
            SessionType::Chat => "chat",
            SessionType::Agent => "agent",
            SessionType::Workflow => "workflow",
        }
    }
}

impl std::fmt::Display for SessionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_prefix())
    }
}

/// Current state of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Session is actively processing
    #[default]
    Active,
    /// Session is paused (not processing new events)
    Paused,
    /// Session is compacting old context
    Compacting,
    /// Session has ended
    Ended,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionState::Active => write!(f, "active"),
            SessionState::Paused => write!(f, "paused"),
            SessionState::Compacting => write!(f, "compacting"),
            SessionState::Ended => write!(f, "ended"),
        }
    }
}

/// Summary of a session for listing.
///
/// A lighter-weight version of Session without full event history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    /// Session ID
    pub id: String,
    /// Session type
    pub session_type: SessionType,
    /// Owning kiln
    pub kiln: PathBuf,
    /// Workspace
    pub workspace: PathBuf,
    /// Current state
    pub state: SessionState,
    /// When started
    pub started_at: DateTime<Utc>,
    /// Optional title
    pub title: Option<String>,
    /// Number of events in the session
    pub event_count: usize,
    /// Agent model name (for display)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_model: Option<String>,
}

impl From<&Session> for SessionSummary {
    fn from(session: &Session) -> Self {
        Self {
            id: session.id.clone(),
            session_type: session.session_type,
            kiln: session.kiln.clone(),
            workspace: session.workspace.clone(),
            state: session.state,
            started_at: session.started_at,
            title: session.title.clone(),
            event_count: 0, // Would be populated from storage
            agent_model: session.agent.as_ref().map(|a| a.model.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let session =
            Session::new(SessionType::Agent, kiln.clone()).with_workspace(workspace.clone());

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
    fn test_session_agent_serialization() {
        let agent = SessionAgent {
            agent_type: "internal".to_string(),
            agent_name: None,
            provider_key: Some("ollama".to_string()),
            provider: "ollama".to_string(),
            model: "llama3.2".to_string(),
            system_prompt: "You are a helpful assistant.".to_string(),
            temperature: Some(0.7),
            max_tokens: Some(4096),
            max_context_tokens: Some(8192),
            thinking_budget: None,
            endpoint: None,
            env_overrides: HashMap::new(),
            mcp_servers: vec!["filesystem".to_string()],
            agent_card_name: Some("default".to_string()),
            capabilities: None,
            agent_description: None,
            delegation_config: None,
        };

        let json = serde_json::to_string(&agent).unwrap();
        assert!(json.contains("\"agent_type\":\"internal\""));
        assert!(json.contains("\"model\":\"llama3.2\""));
        assert!(json.contains("\"temperature\":0.7"));

        let parsed: SessionAgent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model, "llama3.2");
        assert_eq!(parsed.temperature, Some(0.7));
        assert_eq!(parsed.mcp_servers, vec!["filesystem"]);
    }

    #[test]
    fn test_session_with_agent() {
        let agent = SessionAgent {
            agent_type: "internal".to_string(),
            agent_name: None,
            provider_key: Some("openai".to_string()),
            provider: "openai".to_string(),
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
            provider: "anthropic".to_string(),
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
        };

        let kiln = PathBuf::from("/home/user/notes");
        let session = Session::new(SessionType::Chat, kiln).with_agent(agent);

        let summary = SessionSummary::from(&session);
        assert_eq!(summary.agent_model, Some("claude-3-5-sonnet".to_string()));
    }

    // =============================================================================
    // SessionAgent Profile Construction Tests (TDD - RED phase)
    // =============================================================================

    #[test]
    fn test_session_agent_with_capabilities() {
        // SessionAgent should serialize/deserialize with capabilities field
        let agent = SessionAgent {
            agent_type: "acp".to_string(),
            agent_name: Some("opencode".to_string()),
            provider_key: None,
            provider: "acp".to_string(),
            model: "opencode".to_string(),
            system_prompt: "You are helpful.".to_string(),
            temperature: None,
            max_tokens: None,
            max_context_tokens: None,
            thinking_budget: None,
            endpoint: None,
            env_overrides: HashMap::new(),
            mcp_servers: Vec::new(),
            agent_card_name: None,
            capabilities: Some(vec!["search".to_string(), "write".to_string()]),
            agent_description: None,
            delegation_config: None,
        };

        let json = serde_json::to_string(&agent).unwrap();
        assert!(json.contains("\"capabilities\""));
        assert!(json.contains("\"search\""));

        let parsed: SessionAgent = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed.capabilities,
            Some(vec!["search".to_string(), "write".to_string()])
        );
    }

    #[test]
    fn test_session_agent_with_agent_description() {
        // SessionAgent should serialize/deserialize with agent_description field
        let agent = SessionAgent {
            agent_type: "acp".to_string(),
            agent_name: Some("claude".to_string()),
            provider_key: None,
            provider: "acp".to_string(),
            model: "claude".to_string(),
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
            agent_description: Some("Claude AI assistant".to_string()),
            delegation_config: None,
        };

        let json = serde_json::to_string(&agent).unwrap();
        assert!(json.contains("\"agent_description\""));
        assert!(json.contains("Claude AI assistant"));

        let parsed: SessionAgent = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed.agent_description,
            Some("Claude AI assistant".to_string())
        );
    }

    #[test]
    fn test_session_agent_with_delegation_config() {
        // SessionAgent should serialize/deserialize with delegation_config field
        use crucible_config::DelegationConfig;

        let delegation = DelegationConfig {
            enabled: true,
            max_depth: 2,
            allowed_targets: Some(vec!["tool-agent".to_string()]),
            result_max_bytes: 102400,
        };

        let agent = SessionAgent {
            agent_type: "acp".to_string(),
            agent_name: Some("delegating-agent".to_string()),
            provider_key: None,
            provider: "acp".to_string(),
            model: "delegating-agent".to_string(),
            system_prompt: "You can delegate.".to_string(),
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
            delegation_config: Some(delegation),
        };

        let json = serde_json::to_string(&agent).unwrap();
        assert!(json.contains("\"delegation_config\""));
        assert!(json.contains("\"enabled\":true"));
        assert!(json.contains("\"max_depth\":2"));

        let parsed: SessionAgent = serde_json::from_str(&json).unwrap();
        assert!(parsed.delegation_config.is_some());
        let parsed_delegation = parsed.delegation_config.unwrap();
        assert!(parsed_delegation.enabled);
        assert_eq!(parsed_delegation.max_depth, 2);
    }

    #[test]
    fn test_session_agent_backward_compat_without_new_fields() {
        // Old JSON without new fields should deserialize correctly
        let old_json = r#"{
            "agent_type": "internal",
            "provider_key": "ollama",
            "provider": "ollama",
            "model": "llama3.2",
            "system_prompt": "You are helpful.",
            "temperature": 0.7,
            "max_tokens": 4096,
            "env_overrides": {},
            "mcp_servers": []
        }"#;

        let agent: SessionAgent = serde_json::from_str(old_json).unwrap();
        assert_eq!(agent.model, "llama3.2");
        assert_eq!(agent.temperature, Some(0.7));
        assert!(agent.capabilities.is_none());
        assert!(agent.agent_description.is_none());
        assert!(agent.delegation_config.is_none());
    }

    #[test]
    fn test_session_agent_round_trip_with_all_fields() {
        // SessionAgent → JSON → SessionAgent should preserve all fields
        use crucible_config::DelegationConfig;

        let delegation = DelegationConfig {
            enabled: true,
            max_depth: 3,
            allowed_targets: Some(vec!["agent1".to_string(), "agent2".to_string()]),
            result_max_bytes: 204800,
        };

        let original = SessionAgent {
            agent_type: "acp".to_string(),
            agent_name: Some("full-agent".to_string()),
            provider_key: None,
            provider: "acp".to_string(),
            model: "full-agent".to_string(),
            system_prompt: "Full agent.".to_string(),
            temperature: Some(0.8),
            max_tokens: Some(8192),
            max_context_tokens: Some(16384),
            thinking_budget: Some(10000),
            endpoint: Some("http://localhost:8000".to_string()),
            env_overrides: {
                let mut map = HashMap::new();
                map.insert("API_KEY".to_string(), "secret".to_string());
                map
            },
            mcp_servers: vec!["filesystem".to_string(), "web".to_string()],
            agent_card_name: Some("full-card".to_string()),
            capabilities: Some(vec![
                "search".to_string(),
                "write".to_string(),
                "execute".to_string(),
            ]),
            agent_description: Some("A full-featured agent".to_string()),
            delegation_config: Some(delegation),
        };

        let json = serde_json::to_string(&original).unwrap();
        let parsed: SessionAgent = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.agent_type, original.agent_type);
        assert_eq!(parsed.agent_name, original.agent_name);
        assert_eq!(parsed.model, original.model);
        assert_eq!(parsed.capabilities, original.capabilities);
        assert_eq!(parsed.agent_description, original.agent_description);
        assert!(parsed.delegation_config.is_some());
        let parsed_delegation = parsed.delegation_config.unwrap();
        assert_eq!(parsed_delegation.enabled, true);
        assert_eq!(parsed_delegation.max_depth, 3);
    }

    #[test]
    fn test_session_agent_from_profile_basic() {
        // SessionAgent::from_profile() should construct ACP agent from AgentProfile
        use crucible_config::AgentProfile;

        let profile = AgentProfile {
            extends: Some("claude".to_string()),
            command: None,
            args: None,
            env: {
                let mut map = HashMap::new();
                map.insert("ANTHROPIC_API_KEY".to_string(), "key123".to_string());
                map
            },
            description: Some("Claude via profile".to_string()),
            capabilities: Some(vec!["chat".to_string(), "reasoning".to_string()]),
            delegation: None,
        };

        let agent = SessionAgent::from_profile(&profile, "claude-custom");

        assert_eq!(agent.agent_type, "acp");
        assert_eq!(agent.agent_name, Some("claude-custom".to_string()));
        assert_eq!(agent.provider, "acp");
        assert_eq!(agent.model, "claude-custom");
        assert_eq!(
            agent.agent_description,
            Some("Claude via profile".to_string())
        );
        assert_eq!(
            agent.capabilities,
            Some(vec!["chat".to_string(), "reasoning".to_string()])
        );
        // env vars should be in env_overrides, not inherited from parent
        assert_eq!(
            agent.env_overrides.get("ANTHROPIC_API_KEY"),
            Some(&"key123".to_string())
        );
    }

    #[test]
    fn test_session_agent_from_profile_env_isolation() {
        // Profile env vars should go into SessionAgent.env_overrides, parent env NOT inherited
        use crucible_config::AgentProfile;

        let profile = AgentProfile {
            extends: None,
            command: None,
            args: None,
            env: {
                let mut map = HashMap::new();
                map.insert("CUSTOM_VAR".to_string(), "custom_value".to_string());
                map.insert("ANOTHER_VAR".to_string(), "another_value".to_string());
                map
            },
            description: None,
            capabilities: None,
            delegation: None,
        };

        let agent = SessionAgent::from_profile(&profile, "test-agent");

        // Only profile env vars should be in env_overrides
        assert_eq!(agent.env_overrides.len(), 2);
        assert_eq!(
            agent.env_overrides.get("CUSTOM_VAR"),
            Some(&"custom_value".to_string())
        );
        assert_eq!(
            agent.env_overrides.get("ANOTHER_VAR"),
            Some(&"another_value".to_string())
        );
    }

    #[test]
    fn test_session_agent_from_profile_with_delegation() {
        // SessionAgent::from_profile() should include delegation config from profile
        use crucible_config::{AgentProfile, DelegationConfig};

        let delegation = DelegationConfig {
            enabled: true,
            max_depth: 2,
            allowed_targets: Some(vec!["worker1".to_string(), "worker2".to_string()]),
            result_max_bytes: 102400,
        };

        let profile = AgentProfile {
            extends: Some("opencode".to_string()),
            command: None,
            args: None,
            env: HashMap::new(),
            description: Some("Delegating agent".to_string()),
            capabilities: Some(vec!["delegate".to_string()]),
            delegation: Some(delegation),
        };

        let agent = SessionAgent::from_profile(&profile, "delegator");

        assert_eq!(agent.agent_name, Some("delegator".to_string()));
        assert_eq!(
            agent.agent_description,
            Some("Delegating agent".to_string())
        );
        assert!(agent.delegation_config.is_some());
        let parsed_delegation = agent.delegation_config.unwrap();
        assert!(parsed_delegation.enabled);
        assert_eq!(parsed_delegation.max_depth, 2);
        assert_eq!(
            parsed_delegation.allowed_targets,
            Some(vec!["worker1".to_string(), "worker2".to_string()])
        );
    }
}
