//! The session-settings events.
//!
//! Most of them are one-field acknowledgements routed through
//! `AgentManager::update_agent_config_and_emit`; the field name in each variant
//! is already the JSON key that helper wrote by hand, so these variants are a
//! rename with no shape change.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

event_payload! {
    /// Session-settings events, adjacently tagged so the enum's serialization *is*
    /// the `{event, data}` pair the envelope carries.
    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
    #[serde(tag = "event", content = "data")]
    pub enum SettingsPayload {
        "model_switched" => ModelSwitched {
            #[serde(default)]
            model_id: String,
            #[serde(default)]
            provider: String,
        },
        /// `data.mode` is the field the web SSE mapper and the TUI reducers read —
        /// keep the name stable.
        "mode_changed" => ModeChanged {
            #[serde(default)]
            mode: String,
        },
        /// The session's scope after a change: its kilns by registry name, and
        /// its workspace (`null` for a session with no workspace).
        "scope_changed" => ScopeChanged {
            #[serde(default)]
            #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
            workspace: Option<PathBuf>,
            /// Always serialized, as `[]` when empty.
            #[serde(default)]
            #[cfg_attr(feature = "openapi", schema(value_type = Vec<String>))]
            kilns: Vec<crate::config::KilnName>,
        },
        "title_changed" => TitleChanged {
            #[serde(default)]
            title: String,
        },
        "system_prompt_changed" => SystemPromptChanged {
            #[serde(default)]
            system_prompt: String,
        },
        "precognition_toggled" => PrecognitionToggled {
            #[serde(default)]
            enabled: bool,
        },
        "context_strategy_changed" => ContextStrategyChanged {
            #[serde(default)]
            context_strategy: String,
        },
        /// A plugin's approval knob changed. `approval` is the knob's own
        /// spelling (`inherit`, `ask`, `stop`).
        "plugin_approval_changed" => PluginApprovalChanged {
            #[serde(default)]
            plugin: String,
            #[serde(default)]
            approval: String,
        },
        "plugin_turn_limit_changed" => PluginTurnLimitChanged {
            #[serde(default)]
            limit: u32,
        },
        /// The session's ACP agent advertised a new command list. A client
        /// reads the catalog again with `session.commands`. A plugin reload
        /// sends no such event yet, so a client also reads the catalog again
        /// after a reload that it asked for.
        "commands_changed" => CommandsChanged {},
    }
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
