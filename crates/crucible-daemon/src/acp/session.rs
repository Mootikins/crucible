//! The agent session that the ACP handshake opens: its id, and what the
//! agent advertised for it.

use agent_client_protocol::schema::v1::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelectOptions, SessionModeState,
};

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
    /// The agent answered `session/resume` with `-32601` (no such method)
    /// or `-32002` (no such session), so the connect flow opened a fresh
    /// session. The agent-side history is gone.
    FellBackToNew,
}

/// The agent session that the handshake opened.
#[derive(Debug)]
pub struct AcpSession {
    session_id: String,
    /// The model selector that the agent advertised, if any.
    model: Option<ModelChoice>,
    /// The mode set that the agent declared, if any. The modes belong to the
    /// agent. Crucible's own set stands in for an agent that declares none;
    /// it is not a default to merge with.
    modes: Option<SessionModeState>,
    /// Every config option that the agent advertised, in wire order. The
    /// model selector is also in `model`. A client renders the others; the
    /// daemon does not interpret them.
    config_options: Vec<SessionConfigOption>,
    /// How the connect flow obtained this session.
    resume: ResumeDisposition,
}

impl AcpSession {
    /// A session from what the agent answered to `session/new` or
    /// `session/resume`.
    pub(crate) fn new(
        session_id: String,
        modes: Option<SessionModeState>,
        config_options: Option<Vec<SessionConfigOption>>,
        resume: ResumeDisposition,
    ) -> Self {
        let config_options = config_options.unwrap_or_default();
        Self {
            session_id,
            model: ModelChoice::from_config_options(&config_options),
            modes,
            config_options,
            resume,
        }
    }

    /// The config options the agent advertised, in wire order.
    pub fn config_options(&self) -> &[SessionConfigOption] {
        &self.config_options
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
}
