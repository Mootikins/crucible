//! Simple chat configuration

use serde::{Deserialize, Serialize};

use crate::config::serde_helpers::default_true;

/// Agent type preference for chat
///
/// Controls whether to prefer external ACP agents or Crucible's built-in agents.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentPreference {
    /// Prefer external ACP agents (claude-code, opencode, etc.)
    Acp,
    /// Prefer Crucible's built-in agents (using Rig or native backend)
    #[default]
    Crucible,
}

/// The system prompt a session starts with when nothing else sets one.
///
/// This lives here, rather than in `runtime/defaults/init.luau`, because a
/// shipped value is [`SourceTag::Default`] — the lowest layer. Written from
/// Lua it would have outranked `settings.json`, so the settings UI could not
/// have changed it.
///
/// [`SourceTag::Default`]: crate::config::SourceTag::Default
pub const DEFAULT_SYSTEM_PROMPT: &str = "\
You are Crucible, a knowledge-grounded agent working alongside the user.

Ground your answers in the notes and context you are given. When context
is missing, say so and offer to look \u{2014} never invent a note, a path, or a
quotation. Reference notes by title, and link them with [[wikilinks]] when
you write to the kiln.

Use your tools rather than guessing: read a file before describing it, and
verify a change before reporting it done. Prefer one decisive action over a
list of options.

Be concise. Match the depth of the question \u{2014} a short question gets a short
answer, and code or structure only when it earns its place.";

/// The context budget used when nothing else supplies one.
///
/// A conservative modern window. Before this was a fallback, `context_budget`
/// defaulted to `None`, and both auto-compaction and budget enforcement early
/// -returned on `None` — so neither ran on a default session.
pub const DEFAULT_CONTEXT_BUDGET: usize = 128_000;

/// Matches `crucible_daemon::agent_manager::autocompact::DEFAULT_AUTOCOMPACT_THRESHOLD`,
/// which is the constant the trigger used before this became a config key.
fn default_autocompact_threshold() -> f32 {
    0.95
}

fn default_precognition_results() -> usize {
    5
}

fn default_system_prompt() -> String {
    DEFAULT_SYSTEM_PROMPT.to_string()
}

/// Simple chat configuration - only essential user settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConfig {
    /// Default chat model (can be overridden by agents)
    pub model: Option<String>,
    /// Default agent type preference (acp or internal)
    #[serde(default)]
    pub agent_preference: AgentPreference,
    /// LLM endpoint URL (for Ollama/compatible providers)
    pub endpoint: Option<String>,
    /// Show thinking/reasoning tokens from models that support it
    ///
    /// When enabled, thinking tokens are streamed in a quote block below the
    /// spinner instead of just showing "Thinking...". Useful for debugging
    /// or understanding model reasoning.
    #[serde(default)]
    pub show_thinking: bool,
    /// Show inline diff bodies for edit/write tool calls
    ///
    /// When enabled, edit and write tool calls render a unified diff body
    /// beneath the tool header. When disabled, only the tool header is shown.
    /// Useful for trimming visual noise in long sessions.
    #[serde(default = "default_true")]
    pub show_diffs: bool,
    /// Token budget for the assembled context.
    ///
    /// `None` derives it: the daemon asks the provider for the model's real
    /// window at session start, and falls back to [`DEFAULT_CONTEXT_BUDGET`].
    /// Set it to pin a budget smaller than the window.
    #[serde(default)]
    pub context_budget: Option<usize>,
    /// How many notes a Precognition search injects.
    ///
    /// Read once per session, on the first user message: `should_run_precognition`
    /// gates the search to that turn.
    #[serde(default = "default_precognition_results")]
    pub precognition_results: usize,
    /// The fraction of `context_budget` that triggers an auto-compaction.
    ///
    /// `0.0` disables it. A tuning constant, not a per-session decision, which
    /// is why it is here and not a session knob.
    #[serde(default = "default_autocompact_threshold")]
    pub autocompact_threshold: f32,
    /// The system prompt a new session starts from.
    ///
    /// An agent card's own prompt wins; this fills a card that names none.
    /// A start hook reads it through `session.system_prompt` and may extend
    /// it, which is the per-session tier.
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            model: None,
            agent_preference: AgentPreference::default(),
            endpoint: None,
            show_thinking: false,
            show_diffs: true,
            context_budget: None,
            precognition_results: default_precognition_results(),
            autocompact_threshold: default_autocompact_threshold(),
            system_prompt: default_system_prompt(),
        }
    }
}

impl ChatConfig {
    /// Get the chat model, using default if not specified
    pub fn chat_model(&self) -> String {
        self.model
            .clone()
            .unwrap_or_else(|| super::defaults::DEFAULT_CHAT_MODEL.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backward_compat_size_aware_prompts_in_config() {
        // Verify that existing configs with size_aware_prompts = true still parse
        // after the field is removed (serde silently ignores unknown fields)
        let toml = r#"
            model = "test-model"
            size_aware_prompts = true
        "#;
        let config: ChatConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.model, Some("test-model".to_string()));
    }
}
