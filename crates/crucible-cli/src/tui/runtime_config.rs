//! Runtime configuration state for provider/model switching
//!
//! Tracks session-scoped overrides to the configured provider/model.
//! Changes made via :provider and :model commands are stored here and
//! do not persist to config.toml.

/// Runtime provider/model configuration
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Current provider (ollama, openai, anthropic)
    pub provider: String,
    /// Current model name
    pub model: String,
    /// True if user has overridden config defaults
    pub is_override: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            model: "llama3.2".to_string(),
            is_override: false,
        }
    }
}

impl RuntimeConfig {
    /// Create with specific provider and model
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            is_override: false,
        }
    }

    /// Create from config values
    pub fn from_config(provider: &str, model: &str) -> Self {
        Self {
            provider: provider.to_string(),
            model: model.to_string(),
            is_override: false,
        }
    }

    /// Set provider (marks as override)
    pub fn set_provider(&mut self, provider: impl Into<String>) {
        self.provider = provider.into();
        self.is_override = true;
    }

    /// Set model (marks as override)
    pub fn set_model(&mut self, model: impl Into<String>) {
        self.model = model.into();
        self.is_override = true;
    }

    /// Get display string for status bar (provider/model)
    pub fn display_string(&self) -> String {
        format!("{}/{}", self.provider, self.model)
    }

    /// Get short display (just model name if common provider)
    pub fn short_display(&self) -> String {
        if self.provider == "ollama" {
            self.model.clone()
        } else {
            self.display_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_config_default() {
        let config = RuntimeConfig::default();
        assert_eq!(config.provider, "ollama");
        assert_eq!(config.model, "llama3.2");
        assert!(!config.is_override);
    }

    #[test]
    fn test_set_provider() {
        let mut config = RuntimeConfig::default();
        config.set_provider("openai");
        assert_eq!(config.provider, "openai");
        assert!(config.is_override);
    }

    #[test]
    fn test_set_model() {
        let mut config = RuntimeConfig::default();
        config.set_model("gpt-4o");
        assert_eq!(config.model, "gpt-4o");
        assert!(config.is_override);
    }

    #[test]
    fn test_display_string() {
        let config = RuntimeConfig::new("openai", "gpt-4o");
        assert_eq!(config.display_string(), "openai/gpt-4o");
    }

    #[test]
    fn test_short_display_ollama() {
        let config = RuntimeConfig::new("ollama", "llama3.2");
        assert_eq!(config.short_display(), "llama3.2");
    }

    #[test]
    fn test_short_display_other_provider() {
        let config = RuntimeConfig::new("openai", "gpt-4o");
        assert_eq!(config.short_display(), "openai/gpt-4o");
    }

    #[test]
    fn test_new_not_override() {
        let config = RuntimeConfig::new("openai", "gpt-4o");
        assert!(!config.is_override);
    }

    #[test]
    fn test_from_config_not_override() {
        let config = RuntimeConfig::from_config("anthropic", "claude-3");
        assert_eq!(config.provider, "anthropic");
        assert_eq!(config.model, "claude-3");
        assert!(!config.is_override);
    }
}
