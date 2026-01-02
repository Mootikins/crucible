//! Simple chat configuration

use serde::{Deserialize, Serialize};

/// LLM provider type for chat
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    /// Ollama provider
    #[default]
    Ollama,
    /// OpenAI provider
    OpenAI,
    /// Anthropic provider
    Anthropic,
}

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
    /// LLM provider to use
    #[serde(default)]
    pub provider: LlmProvider,
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
    /// Enable size-aware prompts and tool filtering for small models
    ///
    /// When enabled (default), small models (<4B params) get:
    /// - Explicit tool usage guidance to prevent loops
    /// - Read-only tools only (read_file, glob, grep)
    ///
    /// When disabled, all models get standard prompts and all tools.
    #[serde(default = "default_true")]
    pub size_aware_prompts: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            model: None, // Use agent default
            enable_markdown: true,
            provider: LlmProvider::default(),
            agent_preference: AgentPreference::default(),
            endpoint: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            size_aware_prompts: true,
        }
    }
}

impl ChatConfig {
    /// Get the LLM endpoint, using provider-specific default if not specified
    pub fn llm_endpoint(&self) -> String {
        self.endpoint
            .clone()
            .unwrap_or_else(|| match self.provider {
                LlmProvider::Ollama => "http://localhost:11434".to_string(),
                LlmProvider::OpenAI => "https://api.openai.com/v1".to_string(),
                LlmProvider::Anthropic => "https://api.anthropic.com/v1".to_string(),
            })
    }

    /// Get the chat model, using default if not specified
    pub fn chat_model(&self) -> String {
        self.model.clone().unwrap_or_else(|| "llama3.2".to_string())
    }

    /// Get the temperature, using default if not specified
    pub fn temperature(&self) -> f32 {
        self.temperature.unwrap_or(0.7)
    }

    /// Get max tokens, using default if not specified
    pub fn max_tokens(&self) -> u32 {
        self.max_tokens.unwrap_or(2048)
    }

    /// Get timeout in seconds, using default if not specified
    pub fn timeout_secs(&self) -> u64 {
        self.timeout_secs.unwrap_or(120)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_size_aware_prompts_default_enabled() {
        let config = ChatConfig::default();
        assert!(config.size_aware_prompts);
    }

    #[test]
    fn test_size_aware_prompts_deserialize_disabled() {
        let toml = r#"
            size_aware_prompts = false
        "#;
        let config: ChatConfig = toml::from_str(toml).unwrap();
        assert!(!config.size_aware_prompts);
    }

    #[test]
    fn test_size_aware_prompts_deserialize_enabled() {
        let toml = r#"
            size_aware_prompts = true
        "#;
        let config: ChatConfig = toml::from_str(toml).unwrap();
        assert!(config.size_aware_prompts);
    }

    #[test]
    fn test_size_aware_prompts_missing_defaults_to_true() {
        let toml = r#"
            model = "test-model"
        "#;
        let config: ChatConfig = toml::from_str(toml).unwrap();
        assert!(config.size_aware_prompts);
    }
}
