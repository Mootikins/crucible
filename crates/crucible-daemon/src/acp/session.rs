//! Session management for ACP connections
//!
//! This module handles the lifecycle and state of individual agent sessions.
//!
//! ## Responsibilities
//!
//! - Session state management (active, idle, closed)
//! - Message sending and receiving
//! - Session-level error handling and recovery
//! - Resource cleanup on session termination
//!
//! ## Design Principles
//!
//! - **Single Responsibility**: Focused on session lifecycle and message exchange
//! - **Open/Closed**: Extensible through configuration without modification

use agent_client_protocol::schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelectOptions, SessionModeState,
};
use serde::{Deserialize, Serialize};

/// The model selector that an agent advertises in `configOptions`.
///
/// The agent lists its options in the `session/new` reply; the option with
/// category `model` is the model selector. `config_id` names that option,
/// and `session/set_config_option` switches it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelChoice {
    /// The id of the `configOptions` entry that selects the model.
    pub config_id: String,
    /// The value id the agent reports as current.
    pub current: String,
    /// The value ids the agent accepts, in the order it listed them.
    pub available: Vec<String>,
}

impl ModelChoice {
    /// Find the model selector in an agent's config options.
    ///
    /// A selector is a `select` option with category `model`. When no
    /// option has that category, a `select` option with category
    /// `model_config` serves instead. Returns `None` when neither exists.
    pub fn from_config_options(options: &[SessionConfigOption]) -> Option<Self> {
        let pick = |category: SessionConfigOptionCategory| {
            options.iter().find_map(|option| {
                let SessionConfigKind::Select(select) = &option.kind else {
                    return None;
                };
                (option.category.as_ref() == Some(&category)).then(|| Self {
                    config_id: option.id.to_string(),
                    current: select.current_value.to_string(),
                    available: select_values(&select.options),
                })
            })
        };
        pick(SessionConfigOptionCategory::Model)
            .or_else(|| pick(SessionConfigOptionCategory::ModelConfig))
    }
}

/// The value ids of a select option, flat or grouped, in wire order.
fn select_values(options: &SessionConfigSelectOptions) -> Vec<String> {
    match options {
        SessionConfigSelectOptions::Ungrouped(choices) => {
            choices.iter().map(|c| c.value.to_string()).collect()
        }
        SessionConfigSelectOptions::Grouped(groups) => groups
            .iter()
            .flat_map(|g| g.options.iter().map(|c| c.value.to_string()))
            .collect(),
        // The enum is `#[non_exhaustive]`; an unknown shape lists nothing.
        _ => Vec::new(),
    }
}

/// ACP transport layer configuration.
///
/// Settings for the underlying ACP client transport (timeouts, message limits).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// Session timeout in milliseconds
    pub timeout_ms: u64,

    /// Maximum message size in bytes
    pub max_message_size: usize,

    /// Enable debug logging for this session
    pub debug: bool,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 30000,                  // 30 seconds
            max_message_size: 10 * 1024 * 1024, // 10 MB
            debug: false,
        }
    }
}

/// How the connect flow obtained this session.
///
/// The handle reads `FellBackToNew` to tell the event stream that the
/// agent-side history did not survive a daemon restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeDisposition {
    /// The connect flow opened a fresh session; no resume was requested.
    NotAttempted,
    /// The agent answered `session/resume` and kept its history.
    Resumed,
    /// The agent answered `session/resume` with `-32601`, so the connect
    /// flow opened a fresh session. The agent-side history is gone.
    FellBackToNew,
}

/// Represents an active session with an agent
///
/// The session handles communication with a connected agent,
/// including sending requests and receiving responses.
#[derive(Debug)]
pub struct AcpSession {
    session_id: String,
    /// The model selector from the agent's `session/new` reply, when it
    /// advertised one.
    model: Option<ModelChoice>,
    /// The mode set from the agent's `session/new` reply, when it declared
    /// one. The modes belong to the agent — Crucible's own set is a stand-in
    /// for agents that declare none, not a default to merge with.
    modes: Option<SessionModeState>,
    /// Every config option the agent advertised, in wire order.
    ///
    /// `model` is extracted above because Crucible has a typed model
    /// selector to project it onto. The rest are kept as the agent sent
    /// them: `thought_level` is the one Crucible has a knob for, and
    /// `Other(_)` is whatever this particular agent invented. A client
    /// renders them; the daemon does not interpret them.
    config_options: Vec<SessionConfigOption>,
    /// How the connect flow obtained this session.
    resume: ResumeDisposition,
}

impl AcpSession {
    /// Create a new session with the given configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Session configuration
    /// * `session_id` - Unique identifier for this session
    pub fn new(_config: TransportConfig, session_id: String) -> Self {
        Self {
            session_id,
            model: None,
            modes: None,
            config_options: Vec::new(),
            resume: ResumeDisposition::NotAttempted,
        }
    }

    /// Attach the model selector the agent advertised.
    pub fn with_model(mut self, model: Option<ModelChoice>) -> Self {
        self.model = model;
        self
    }

    /// Attach the mode set the agent declared.
    pub fn with_modes(mut self, modes: Option<SessionModeState>) -> Self {
        self.modes = modes;
        self
    }

    /// Attach every config option the agent advertised.
    pub fn with_config_options(mut self, options: Option<Vec<SessionConfigOption>>) -> Self {
        self.config_options = options.unwrap_or_default();
        self
    }

    /// The config options the agent advertised, in wire order.
    pub fn config_options(&self) -> &[SessionConfigOption] {
        &self.config_options
    }

    /// Record how the connect flow obtained this session.
    pub fn with_resume(mut self, resume: ResumeDisposition) -> Self {
        self.resume = resume;
        self
    }

    /// Get the session ID
    pub fn id(&self) -> &str {
        &self.session_id
    }

    /// The model selector the agent advertised, if any.
    pub fn model(&self) -> Option<&ModelChoice> {
        self.model.as_ref()
    }

    /// The mode set the agent declared, if any.
    pub fn modes(&self) -> Option<&SessionModeState> {
        self.modes.as_ref()
    }

    /// How the connect flow obtained this session.
    pub fn resume(&self) -> ResumeDisposition {
        self.resume
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let config = TransportConfig::default();
        let session = AcpSession::new(config, "test-session-id".to_string());
        assert_eq!(session.id(), "test-session-id");
    }

    /// The `session/new` reply that claude-agent-acp writes, reduced to the
    /// fields this parser reads. The model selector carries category
    /// `model`; a second option with no category is a thought-level toggle.
    const SESSION_NEW_WITH_MODEL_SELECTOR: &str = r#"{
        "sessionId": "sess-1",
        "configOptions": [
            {
                "id": "thinking",
                "name": "Thinking",
                "type": "boolean",
                "currentValue": true
            },
            {
                "id": "model",
                "name": "Model",
                "category": "model",
                "type": "select",
                "currentValue": "mock-sonnet",
                "options": [
                    {"value": "mock-sonnet", "name": "Mock Sonnet"},
                    {"value": "mock-opus", "name": "Mock Opus"}
                ]
            }
        ]
    }"#;

    fn config_options(reply: &str) -> Vec<SessionConfigOption> {
        let reply: agent_client_protocol::schema::v1::NewSessionResponse =
            serde_json::from_str(reply).expect("reply parses");
        reply.config_options.unwrap_or_default()
    }

    #[test]
    fn model_choice_reads_the_select_option_with_category_model() {
        let choice =
            ModelChoice::from_config_options(&config_options(SESSION_NEW_WITH_MODEL_SELECTOR))
                .expect("the reply advertises a model selector");
        assert_eq!(
            choice,
            ModelChoice {
                config_id: "model".into(),
                current: "mock-sonnet".into(),
                available: vec!["mock-sonnet".into(), "mock-opus".into()],
            }
        );
    }

    #[test]
    fn model_choice_falls_back_to_category_model_config() {
        let reply = SESSION_NEW_WITH_MODEL_SELECTOR
            .replace(r#""category": "model""#, r#""category": "model_config""#);
        let choice = ModelChoice::from_config_options(&config_options(&reply))
            .expect("a model_config select serves as the selector");
        assert_eq!(choice.config_id, "model");
    }

    #[test]
    fn model_choice_is_none_without_a_model_category() {
        let reply = r#"{"sessionId": "sess-1", "configOptions": [
            {"id": "mode", "name": "Mode", "category": "mode", "type": "select",
             "currentValue": "ask", "options": [{"value": "ask", "name": "Ask"}]}
        ]}"#;
        assert_eq!(
            ModelChoice::from_config_options(&config_options(reply)),
            None
        );
        assert_eq!(
            ModelChoice::from_config_options(&config_options(r#"{"sessionId": "sess-1"}"#)),
            None
        );
    }

    #[test]
    fn test_default_config() {
        let config = TransportConfig::default();
        assert_eq!(config.timeout_ms, 30000);
        assert_eq!(config.max_message_size, 10 * 1024 * 1024);
        assert!(!config.debug);
    }
}
