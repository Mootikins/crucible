//! LLM provider configuration with support for named instances

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

/// LLM provider type
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LlmProviderType {
    /// Ollama provider
    #[default]
    Ollama,
    /// OpenAI provider
    OpenAI,
    /// Anthropic provider
    Anthropic,
    /// GitHub Copilot provider (via VS Code OAuth flow)
    /// Uses the same OAuth client ID as VS Code for authentication.
    /// Requires initial device flow authentication, then stores OAuth token.
    #[serde(alias = "github-copilot", alias = "github_copilot", alias = "copilot")]
    GitHubCopilot,
}

impl LlmProviderType {
    /// Get the environment variable name for this provider's API key
    pub fn api_key_env_var(&self) -> Option<&'static str> {
        match self {
            LlmProviderType::Ollama => None,
            LlmProviderType::OpenAI => Some("OPENAI_API_KEY"),
            LlmProviderType::Anthropic => Some("ANTHROPIC_API_KEY"),
            LlmProviderType::GitHubCopilot => None,
        }
    }
}

impl FromStr for LlmProviderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ollama" => Ok(LlmProviderType::Ollama),
            "openai" => Ok(LlmProviderType::OpenAI),
            "anthropic" => Ok(LlmProviderType::Anthropic),
            "github-copilot" | "github_copilot" | "copilot" => Ok(LlmProviderType::GitHubCopilot),
            other => Err(format!("Unknown provider: {}", other)),
        }
    }
}

/// Named LLM provider instance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProviderConfig {
    /// Provider type
    #[serde(rename = "type")]
    pub provider_type: LlmProviderType,

    /// API endpoint (uses provider default if not set)
    pub endpoint: Option<String>,

    /// Default model for this provider
    pub default_model: Option<String>,

    /// Temperature for generation (0.0-2.0)
    pub temperature: Option<f32>,

    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,

    /// API timeout in seconds
    pub timeout_secs: Option<u64>,

    /// API key for this provider (use `{env:VAR}` syntax for env vars)
    pub api_key: Option<String>,
}

impl LlmProviderConfig {
    /// Get the API endpoint, using provider-specific default if not set
    pub fn endpoint(&self) -> String {
        self.endpoint
            .clone()
            .unwrap_or_else(|| match self.provider_type {
                LlmProviderType::Ollama => "http://localhost:11434".to_string(),
                LlmProviderType::OpenAI => "https://api.openai.com/v1".to_string(),
                LlmProviderType::Anthropic => "https://api.anthropic.com/v1".to_string(),
                LlmProviderType::GitHubCopilot => "https://api.githubcopilot.com".to_string(),
            })
    }

    /// Get the default model, using provider-specific default if not set
    pub fn model(&self) -> String {
        self.default_model
            .clone()
            .unwrap_or_else(|| match self.provider_type {
                LlmProviderType::Ollama => "llama3.2".to_string(),
                LlmProviderType::OpenAI => "gpt-4o".to_string(),
                LlmProviderType::Anthropic => "claude-3-5-sonnet-20241022".to_string(),
                LlmProviderType::GitHubCopilot => "gpt-4o".to_string(),
            })
    }

    /// Get temperature (default 0.7)
    pub fn temperature(&self) -> f32 {
        self.temperature
            .unwrap_or(super::defaults::DEFAULT_TEMPERATURE)
    }

    /// Get max tokens (default 4096)
    pub fn max_tokens(&self) -> u32 {
        self.max_tokens
            .unwrap_or(super::defaults::DEFAULT_PROVIDER_MAX_TOKENS)
    }

    /// Get timeout in seconds (default 120)
    pub fn timeout_secs(&self) -> u64 {
        self.timeout_secs
            .unwrap_or(super::defaults::DEFAULT_TIMEOUT_SECS)
    }

    /// Get the API key (already resolved if `{env:VAR}` was used)
    pub fn api_key(&self) -> Option<String> {
        self.api_key.clone()
    }

    /// Create a new builder for this config type
    pub fn builder(provider_type: LlmProviderType) -> LlmProviderConfigBuilder {
        LlmProviderConfigBuilder::new(provider_type)
    }
}

/// Builder for LlmProviderConfig
#[derive(Debug, Clone)]
pub struct LlmProviderConfigBuilder {
    provider_type: LlmProviderType,
    endpoint: Option<String>,
    default_model: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    timeout_secs: Option<u64>,
    api_key: Option<String>,
}

impl LlmProviderConfigBuilder {
    /// Create a new builder with the specified provider type
    pub fn new(provider_type: LlmProviderType) -> Self {
        Self {
            provider_type,
            endpoint: None,
            default_model: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
        }
    }

    /// Set the API endpoint
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Set the API endpoint if Some
    pub fn maybe_endpoint(mut self, endpoint: Option<String>) -> Self {
        self.endpoint = endpoint;
        self
    }

    /// Set the default model
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.default_model = Some(model.into());
        self
    }

    /// Set the temperature
    pub fn temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Set the temperature if Some
    pub fn maybe_temperature(mut self, temp: Option<f32>) -> Self {
        self.temperature = temp;
        self
    }

    /// Set max tokens
    pub fn max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    /// Set max tokens if Some
    pub fn maybe_max_tokens(mut self, tokens: Option<u32>) -> Self {
        self.max_tokens = tokens;
        self
    }

    /// Set timeout in seconds
    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    /// Set timeout if Some
    pub fn maybe_timeout_secs(mut self, secs: Option<u64>) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Set API key
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Set API key from provider's default env var name
    pub fn api_key_from_env(mut self) -> Self {
        self.api_key = self.provider_type.api_key_env_var().map(String::from);
        self
    }

    /// Build the config
    pub fn build(self) -> LlmProviderConfig {
        LlmProviderConfig {
            provider_type: self.provider_type,
            endpoint: self.endpoint,
            default_model: self.default_model,
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            timeout_secs: self.timeout_secs,
            api_key: self.api_key,
        }
    }
}

/// Main LLM configuration with named provider instances
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmConfig {
    /// Default provider key for chat
    pub default: Option<String>,

    /// Named provider instances
    #[serde(default)]
    pub providers: HashMap<String, LlmProviderConfig>,
}

impl LlmConfig {
    /// Get the default provider configuration
    pub fn default_provider(&self) -> Option<(&String, &LlmProviderConfig)> {
        let default_key = self.default.as_ref()?;
        let config = self.providers.get(default_key)?;
        Some((default_key, config))
    }

    /// Get a provider by key
    pub fn get_provider(&self, key: &str) -> Option<&LlmProviderConfig> {
        self.providers.get(key)
    }

    /// List all provider keys
    pub fn provider_keys(&self) -> Vec<&String> {
        self.providers.keys().collect()
    }

    /// Check if any providers are configured
    pub fn has_providers(&self) -> bool {
        !self.providers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_defaults() {
        let ollama = LlmProviderConfig {
            provider_type: LlmProviderType::Ollama,
            endpoint: None,
            default_model: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
        };

        assert_eq!(ollama.endpoint(), "http://localhost:11434");
        assert_eq!(ollama.model(), "llama3.2");
        assert_eq!(ollama.temperature(), 0.7);
        assert_eq!(ollama.max_tokens(), 4096);
        assert_eq!(ollama.timeout_secs(), 120);

        let openai = LlmProviderConfig {
            provider_type: LlmProviderType::OpenAI,
            endpoint: None,
            default_model: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
        };

        assert_eq!(openai.endpoint(), "https://api.openai.com/v1");
        assert_eq!(openai.model(), "gpt-4o");

        let anthropic = LlmProviderConfig {
            provider_type: LlmProviderType::Anthropic,
            endpoint: None,
            default_model: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
        };

        assert_eq!(anthropic.endpoint(), "https://api.anthropic.com/v1");
        assert_eq!(anthropic.model(), "claude-3-5-sonnet-20241022");

        let copilot = LlmProviderConfig {
            provider_type: LlmProviderType::GitHubCopilot,
            endpoint: None,
            default_model: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
        };

        assert_eq!(copilot.endpoint(), "https://api.githubcopilot.com");
        assert_eq!(copilot.model(), "gpt-4o");
    }

    #[test]
    fn test_github_copilot_serde_aliases() {
        // Test that various TOML formats deserialize correctly
        // Note: rename_all = "lowercase" makes canonical form "githubcopilot"
        // Aliases allow alternative spellings
        let variants = [
            r#"{"type": "githubcopilot"}"#,  // canonical lowercase form
            r#"{"type": "github-copilot"}"#, // kebab-case alias
            r#"{"type": "github_copilot"}"#, // snake_case alias
            r#"{"type": "copilot"}"#,        // short alias
        ];

        for json in variants {
            let config: LlmProviderConfig = serde_json::from_str(json).unwrap();
            assert_eq!(
                config.provider_type,
                LlmProviderType::GitHubCopilot,
                "Failed to parse: {}",
                json
            );
        }
    }

    #[test]
    fn test_provider_custom_values() {
        let config = LlmProviderConfig {
            provider_type: LlmProviderType::Ollama,
            endpoint: Some("http://192.168.1.100:11434".to_string()),
            default_model: Some("llama3.1:70b".to_string()),
            temperature: Some(0.9),
            max_tokens: Some(8192),
            timeout_secs: Some(300),
            api_key: None,
        };

        assert_eq!(config.endpoint(), "http://192.168.1.100:11434");
        assert_eq!(config.model(), "llama3.1:70b");
        assert_eq!(config.temperature(), 0.9);
        assert_eq!(config.max_tokens(), 8192);
        assert_eq!(config.timeout_secs(), 300);
    }

    #[test]
    fn test_api_key_direct_value() {
        // With new model, api_key is the direct value (resolved at config load)
        let config = LlmProviderConfig {
            provider_type: LlmProviderType::OpenAI,
            endpoint: None,
            default_model: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("sk-test-key-123".to_string()),
        };

        assert_eq!(config.api_key(), Some("sk-test-key-123".to_string()));
    }

    #[test]
    fn test_llm_config_default_provider() {
        let mut providers = HashMap::new();
        providers.insert(
            "local".to_string(),
            LlmProviderConfig {
                provider_type: LlmProviderType::Ollama,
                endpoint: Some("http://localhost:11434".to_string()),
                default_model: Some("llama3.2".to_string()),
                temperature: None,
                max_tokens: None,
                timeout_secs: None,
                api_key: None,
            },
        );
        providers.insert(
            "cloud".to_string(),
            LlmProviderConfig {
                provider_type: LlmProviderType::OpenAI,
                endpoint: None,
                default_model: Some("gpt-4o".to_string()),
                temperature: None,
                max_tokens: None,
                timeout_secs: None,
                api_key: Some("OPENAI_API_KEY".to_string()),
            },
        );

        let config = LlmConfig {
            default: Some("local".to_string()),
            providers,
        };

        let (key, provider) = config.default_provider().unwrap();
        assert_eq!(key, "local");
        assert_eq!(provider.provider_type, LlmProviderType::Ollama);
        assert_eq!(provider.model(), "llama3.2");
    }

    #[test]
    fn test_llm_config_get_provider() {
        let mut providers = HashMap::new();
        providers.insert(
            "local".to_string(),
            LlmProviderConfig {
                provider_type: LlmProviderType::Ollama,
                endpoint: Some("http://localhost:11434".to_string()),
                default_model: Some("llama3.2".to_string()),
                temperature: None,
                max_tokens: None,
                timeout_secs: None,
                api_key: None,
            },
        );

        let config = LlmConfig {
            default: Some("local".to_string()),
            providers,
        };

        let provider = config.get_provider("local").unwrap();
        assert_eq!(provider.provider_type, LlmProviderType::Ollama);

        assert!(config.get_provider("nonexistent").is_none());
    }

    #[test]
    fn test_llm_config_provider_keys() {
        let mut providers = HashMap::new();
        providers.insert(
            "local".to_string(),
            LlmProviderConfig {
                provider_type: LlmProviderType::Ollama,
                endpoint: None,
                default_model: None,
                temperature: None,
                max_tokens: None,
                timeout_secs: None,
                api_key: None,
            },
        );
        providers.insert(
            "cloud".to_string(),
            LlmProviderConfig {
                provider_type: LlmProviderType::OpenAI,
                endpoint: None,
                default_model: None,
                temperature: None,
                max_tokens: None,
                timeout_secs: None,
                api_key: None,
            },
        );

        let config = LlmConfig {
            default: None,
            providers,
        };

        let keys = config.provider_keys();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&&"local".to_string()));
        assert!(keys.contains(&&"cloud".to_string()));
    }

    #[test]
    fn test_llm_config_has_providers() {
        let config = LlmConfig {
            default: None,
            providers: HashMap::new(),
        };
        assert!(!config.has_providers());

        let mut providers = HashMap::new();
        providers.insert(
            "local".to_string(),
            LlmProviderConfig {
                provider_type: LlmProviderType::Ollama,
                endpoint: None,
                default_model: None,
                temperature: None,
                max_tokens: None,
                timeout_secs: None,
                api_key: None,
            },
        );

        let config = LlmConfig {
            default: Some("local".to_string()),
            providers,
        };
        assert!(config.has_providers());
    }

    #[test]
    fn test_llm_config_no_default_provider() {
        let config = LlmConfig {
            default: None,
            providers: HashMap::new(),
        };

        assert!(config.default_provider().is_none());
    }

    #[test]
    fn test_llm_config_invalid_default_provider() {
        let mut providers = HashMap::new();
        providers.insert(
            "local".to_string(),
            LlmProviderConfig {
                provider_type: LlmProviderType::Ollama,
                endpoint: None,
                default_model: None,
                temperature: None,
                max_tokens: None,
                timeout_secs: None,
                api_key: None,
            },
        );

        let config = LlmConfig {
            default: Some("nonexistent".to_string()),
            providers,
        };

        assert!(config.default_provider().is_none());
    }
}
