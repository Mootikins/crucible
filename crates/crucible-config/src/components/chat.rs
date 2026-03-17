//! Simple chat configuration

use serde::{Deserialize, Serialize};

use crate::serde_helpers::default_true;

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

/// Simple chat configuration - only essential user settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConfig {
    /// Default chat model (can be overridden by agents)
    pub model: Option<String>,
    /// Enable markdown rendering
    #[serde(default = "default_true")]
    pub enable_markdown: bool,
    /// Default agent type preference (acp or internal)
    #[serde(default)]
    pub agent_preference: AgentPreference,
    /// LLM endpoint URL (for Ollama/compatible providers)
    pub endpoint: Option<String>,
    /// Temperature for generation (0.0-2.0)
    pub temperature: Option<f32>,
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// API timeout in seconds
    pub timeout_secs: Option<u64>,
    /// Show thinking/reasoning tokens from models that support it
    ///
    /// When enabled, thinking tokens are streamed in a quote block below the
    /// spinner instead of just showing "Thinking...". Useful for debugging
    /// or understanding model reasoning.
    #[serde(default)]
    pub show_thinking: bool,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            model: None,
            enable_markdown: true,
            agent_preference: AgentPreference::default(),
            endpoint: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            show_thinking: false,
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

    /// Get the temperature, using default if not specified
    pub fn temperature(&self) -> f32 {
        self.temperature
            .unwrap_or(super::defaults::DEFAULT_TEMPERATURE)
    }

    /// Get max tokens, using default if not specified
    pub fn max_tokens(&self) -> u32 {
        self.max_tokens
            .unwrap_or(super::defaults::DEFAULT_CHAT_MAX_TOKENS)
    }

    /// Get timeout in seconds, using default if not specified
    pub fn timeout_secs(&self) -> u64 {
        self.timeout_secs
            .unwrap_or(super::defaults::DEFAULT_TIMEOUT_SECS)
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
