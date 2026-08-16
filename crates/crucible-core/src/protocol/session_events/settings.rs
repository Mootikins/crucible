//! The eighteen session-settings events.
//!
//! Fourteen of them are one-field acknowledgements routed through
//! `AgentManager::update_agent_config_and_emit`; the field name in each variant
//! is already the JSON key that helper wrote by hand, so these variants are a
//! rename with no shape change.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Session-settings events, adjacently tagged so the enum's serialization *is*
/// the `{event, data}` pair the envelope carries.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum SettingsPayload {
    ModelSwitched {
        #[serde(default)]
        model_id: String,
        #[serde(default)]
        provider: String,
    },
    /// `data.mode` is the field the web SSE mapper and the TUI reducers read —
    /// keep the name stable.
    ModeChanged {
        #[serde(default)]
        mode: String,
    },
    ScopeChanged {
        #[serde(default)]
        workspace: PathBuf,
        /// Always serialized, as `[]` when empty: the producer builds this with
        /// `json!` from `Session::kilns`, which bypasses that field's own
        /// `skip_serializing_if`.
        #[serde(default)]
        kilns: Vec<PathBuf>,
    },
    TitleChanged {
        #[serde(default)]
        title: String,
    },
    ThinkingBudgetChanged {
        #[serde(default)]
        budget: i64,
    },
    SystemPromptChanged {
        #[serde(default)]
        system_prompt: String,
    },
    PrecognitionToggled {
        #[serde(default)]
        enabled: bool,
    },
    PrecognitionResultsChanged {
        #[serde(default)]
        precognition_results: usize,
    },
    TemperatureChanged {
        #[serde(default)]
        temperature: f64,
    },
    MaxTokensChanged {
        #[serde(default)]
        max_tokens: Option<u32>,
    },
    MaxIterationsChanged {
        #[serde(default)]
        max_iterations: Option<u32>,
    },
    ExecutionTimeoutChanged {
        #[serde(default)]
        timeout_secs: Option<u64>,
    },
    ContextBudgetChanged {
        #[serde(default)]
        context_budget: Option<usize>,
    },
    AutocompactThresholdChanged {
        #[serde(default)]
        autocompact_threshold: Option<f32>,
    },
    ContextStrategyChanged {
        #[serde(default)]
        context_strategy: String,
    },
    ContextWindowChanged {
        #[serde(default)]
        context_window: Option<usize>,
    },
    OutputValidationChanged {
        #[serde(default)]
        output_validation: String,
    },
    ValidationRetriesChanged {
        #[serde(default)]
        validation_retries: u32,
    },
}

impl SettingsPayload {
    /// Does this event belong in `session.jsonl`?
    ///
    /// Only `model_switched`. A resumed transcript has to attribute each turn to
    /// the model that produced it, and a mid-session switch is the only thing
    /// that moves the answer. Every other setting is recovered from the session
    /// record itself, so persisting it would duplicate state that is already
    /// authoritative elsewhere.
    pub fn is_persisted(&self) -> bool {
        matches!(self, Self::ModelSwitched { .. })
    }
}
