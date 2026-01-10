//! Runtime configuration state for provider/model switching
//!
//! Tracks session-scoped overrides to the configured provider/model.
//! Changes made via :model command are stored here and do not persist to config.toml.
//!
//! ## Backend Format
//!
//! Uses unified `provider/model` format matching opencode conventions:
//! - `ollama/llama3.2` - Ollama with specific model
//! - `openai/gpt-4o` - OpenAI with specific model
//! - `anthropic/claude-sonnet-4` - Anthropic with specific model
//! - `acp/opencode` - ACP agent (model determined by agent)

use std::fmt;

/// Known LLM providers (direct backends)
const KNOWN_PROVIDERS: &[&str] = &["ollama", "openai", "anthropic"];

/// Unified backend specification
///
/// Represents either a direct LLM provider or an ACP agent proxy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendSpec {
    /// Direct LLM provider - "ollama/llama3.2", "openai/gpt-4o"
    Direct { provider: String, model: String },
    /// ACP agent proxy - "acp/opencode"
    Acp { agent: String },
}

impl BackendSpec {
    /// Parse a backend specification string
    ///
    /// Accepts formats:
    /// - `provider/model` - e.g., "ollama/llama3.2", "openai/gpt-4o"
    /// - `acp/agent` - e.g., "acp/opencode", "acp/claude"
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();

        match s.split_once('/') {
            Some(("acp", agent)) if !agent.is_empty() => Ok(Self::Acp {
                agent: agent.to_string(),
            }),
            Some((provider, model)) if !model.is_empty() => {
                let provider_lower = provider.to_lowercase();
                if KNOWN_PROVIDERS.contains(&provider_lower.as_str()) {
                    Ok(Self::Direct {
                        provider: provider_lower,
                        model: model.to_string(),
                    })
                } else {
                    Err(format!(
                        "Unknown provider '{}'. Use: ollama, openai, anthropic, or acp/<agent>",
                        provider
                    ))
                }
            }
            _ => Err(format!(
                "Invalid format '{}'. Use: provider/model (e.g., ollama/llama3.2)",
                s
            )),
        }
    }

    /// Create a direct LLM backend spec
    pub fn direct(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self::Direct {
            provider: provider.into(),
            model: model.into(),
        }
    }

    /// Create an ACP agent backend spec
    pub fn acp(agent: impl Into<String>) -> Self {
        Self::Acp {
            agent: agent.into(),
        }
    }

    /// Get the provider name (or "acp" for agents)
    pub fn provider(&self) -> &str {
        match self {
            Self::Direct { provider, .. } => provider,
            Self::Acp { .. } => "acp",
        }
    }

    /// Get the model name (or agent name for ACP)
    pub fn model(&self) -> &str {
        match self {
            Self::Direct { model, .. } => model,
            Self::Acp { agent } => agent,
        }
    }

    /// Check if this is a direct LLM backend
    pub fn is_direct(&self) -> bool {
        matches!(self, Self::Direct { .. })
    }

    /// Check if this is an ACP agent
    pub fn is_acp(&self) -> bool {
        matches!(self, Self::Acp { .. })
    }
}

impl fmt::Display for BackendSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Direct { provider, model } => write!(f, "{}/{}", provider, model),
            Self::Acp { agent } => write!(f, "acp/{}", agent),
        }
    }
}

/// Runtime provider/model configuration
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Current backend specification
    pub backend: BackendSpec,
    /// True if user has overridden config defaults
    pub is_override: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            backend: BackendSpec::direct("ollama", "llama3.2"),
            is_override: false,
        }
    }
}

impl RuntimeConfig {
    /// Create with specific provider and model (direct LLM)
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            backend: BackendSpec::direct(provider, model),
            is_override: false,
        }
    }

    /// Create from config values
    pub fn from_config(provider: &str, model: &str) -> Self {
        Self {
            backend: BackendSpec::direct(provider, model),
            is_override: false,
        }
    }

    /// Create with a backend spec
    pub fn with_backend(backend: BackendSpec) -> Self {
        Self {
            backend,
            is_override: false,
        }
    }

    /// Set backend from parsed spec (marks as override)
    pub fn set_backend(&mut self, backend: BackendSpec) {
        self.backend = backend;
        self.is_override = true;
    }

    /// Get provider name (for compatibility)
    pub fn provider(&self) -> &str {
        self.backend.provider()
    }

    /// Get model name (for compatibility)
    pub fn model(&self) -> &str {
        self.backend.model()
    }

    /// Set provider (marks as override) - legacy compat
    pub fn set_provider(&mut self, provider: impl Into<String>) {
        if let BackendSpec::Direct { model, .. } = &self.backend {
            self.backend = BackendSpec::direct(provider, model.clone());
            self.is_override = true;
        }
    }

    /// Set model (marks as override) - legacy compat
    pub fn set_model(&mut self, model: impl Into<String>) {
        if let BackendSpec::Direct { provider, .. } = &self.backend {
            self.backend = BackendSpec::direct(provider.clone(), model);
            self.is_override = true;
        }
    }

    /// Get display string for status bar (provider/model)
    pub fn display_string(&self) -> String {
        self.backend.to_string()
    }

    /// Get short display (just model name if ollama)
    pub fn short_display(&self) -> String {
        match &self.backend {
            BackendSpec::Direct { provider, model } if provider == "ollama" => model.clone(),
            _ => self.display_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // BackendSpec parsing tests
    #[test]
    fn test_parse_ollama() {
        let spec = BackendSpec::parse("ollama/llama3.2").unwrap();
        assert_eq!(spec, BackendSpec::direct("ollama", "llama3.2"));
    }

    #[test]
    fn test_parse_openai() {
        let spec = BackendSpec::parse("openai/gpt-4o").unwrap();
        assert_eq!(spec, BackendSpec::direct("openai", "gpt-4o"));
    }

    #[test]
    fn test_parse_anthropic() {
        let spec = BackendSpec::parse("anthropic/claude-sonnet-4").unwrap();
        assert_eq!(spec, BackendSpec::direct("anthropic", "claude-sonnet-4"));
    }

    #[test]
    fn test_parse_acp() {
        let spec = BackendSpec::parse("acp/opencode").unwrap();
        assert_eq!(spec, BackendSpec::acp("opencode"));
    }

    #[test]
    fn test_parse_case_insensitive_provider() {
        let spec = BackendSpec::parse("OLLAMA/llama3.2").unwrap();
        assert_eq!(spec.provider(), "ollama");
    }

    #[test]
    fn test_parse_unknown_provider() {
        let result = BackendSpec::parse("unknown/model");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_format() {
        let result = BackendSpec::parse("justmodel");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_model() {
        let result = BackendSpec::parse("ollama/");
        assert!(result.is_err());
    }

    // BackendSpec display tests
    #[test]
    fn test_display_direct() {
        let spec = BackendSpec::direct("openai", "gpt-4o");
        assert_eq!(spec.to_string(), "openai/gpt-4o");
    }

    #[test]
    fn test_display_acp() {
        let spec = BackendSpec::acp("opencode");
        assert_eq!(spec.to_string(), "acp/opencode");
    }

    // RuntimeConfig tests
    #[test]
    fn test_runtime_config_default() {
        let config = RuntimeConfig::default();
        assert_eq!(config.provider(), "ollama");
        assert_eq!(config.model(), "llama3.2");
        assert!(!config.is_override);
    }

    #[test]
    fn test_set_backend() {
        let mut config = RuntimeConfig::default();
        config.set_backend(BackendSpec::direct("openai", "gpt-4o"));
        assert_eq!(config.provider(), "openai");
        assert_eq!(config.model(), "gpt-4o");
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
    fn test_from_config_not_override() {
        let config = RuntimeConfig::from_config("anthropic", "claude-3");
        assert_eq!(config.provider(), "anthropic");
        assert_eq!(config.model(), "claude-3");
        assert!(!config.is_override);
    }

    #[test]
    fn test_backend_is_acp() {
        let spec = BackendSpec::acp("opencode");
        assert!(spec.is_acp());
        assert!(!spec.is_direct());
    }

    #[test]
    fn test_backend_is_direct() {
        let spec = BackendSpec::direct("ollama", "llama3.2");
        assert!(spec.is_direct());
        assert!(!spec.is_acp());
    }
}
