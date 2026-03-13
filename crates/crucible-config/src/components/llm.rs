//! LLM provider configuration with support for named instances

use super::backend::BackendType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Named LLM provider instance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProviderConfig {
    /// Provider type
    #[serde(rename = "type")]
    pub provider_type: BackendType,

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

    /// Available models for this provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_models: Option<Vec<String>>,

    /// Trust level for this provider (uses backend default if not set)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_level: Option<super::trust::TrustLevel>,

    /// Optional custom display name for this provider (shown in model lists/UI)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl LlmProviderConfig {
    /// Get the API endpoint, using provider-specific default if not set
    pub fn endpoint(&self) -> String {
        self.endpoint.clone().unwrap_or_else(|| {
            self.provider_type
                .default_endpoint()
                .unwrap_or(super::defaults::DEFAULT_OLLAMA_ENDPOINT)
                .to_string()
        })
    }

    /// Get the default model, using provider-specific default if not set
    pub fn model(&self) -> String {
        self.default_model.clone().unwrap_or_else(|| {
            self.provider_type
                .default_chat_model()
                .unwrap_or(super::defaults::DEFAULT_CHAT_MODEL)
                .to_string()
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

    /// Get the effective trust level, using explicit override or backend default
    pub fn effective_trust_level(&self) -> super::trust::TrustLevel {
        self.trust_level
            .unwrap_or_else(|| self.provider_type.default_trust_level())
    }

    /// Get effective models for this provider.
    ///
    /// Returns `available_models` from config if set, otherwise empty vec.
    /// Dynamic discovery (API calls) is handled at the daemon layer.
    pub fn effective_models(&self) -> Vec<String> {
        self.available_models.clone().unwrap_or_default()
    }

    /// Create a new builder for this config type
    pub fn builder(provider_type: BackendType) -> LlmProviderConfigBuilder {
        LlmProviderConfigBuilder::new(provider_type)
    }
}

/// Builder for LlmProviderConfig
#[derive(Debug, Clone)]
pub struct LlmProviderConfigBuilder {
    provider_type: BackendType,
    endpoint: Option<String>,
    default_model: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    timeout_secs: Option<u64>,
    api_key: Option<String>,
    available_models: Option<Vec<String>>,
    trust_level: Option<super::trust::TrustLevel>,
    name: Option<String>,
}

impl LlmProviderConfigBuilder {
    /// Create a new builder with the specified provider type
    pub fn new(provider_type: BackendType) -> Self {
        Self {
            provider_type,
            endpoint: None,
            default_model: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
            available_models: None,
            trust_level: None,
            name: None,
        }
    }

    /// Set the API endpoint
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
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

    /// Set max tokens
    pub fn max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
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

    /// Set API key to the provider's default environment variable name.
    ///
    /// This method stores the **environment variable name** (e.g., `"OPENAI_API_KEY"`),
    /// not the actual value from the environment. The actual value is resolved later
    /// when the configuration is used, allowing for dynamic environment variable lookup.
    ///
    /// # Behavior
    ///
    /// - For providers with a standard env var (OpenAI, Anthropic, OpenRouter, ZAI),
    ///   this sets `api_key` to that variable name.
    /// - For providers without a standard env var (Ollama, GitHub Copilot),
    ///   this has no effect (api_key remains `None`).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let config = LlmProviderConfig::builder(BackendType::OpenAI)
    ///     .with_api_key_env_var_name()  // Sets api_key to "OPENAI_API_KEY"
    ///     .build();
    /// ```
    pub fn with_api_key_env_var_name(mut self) -> Self {
        self.api_key = self.provider_type.api_key_env_var().map(String::from);
        self
    }

    /// Set available models
    pub fn available_models(mut self, models: Vec<String>) -> Self {
        self.available_models = Some(models);
        self
    }

    /// Set custom display name for this provider
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
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
            available_models: self.available_models,
            trust_level: self.trust_level,
            name: self.name,
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

    /// Get all provider models aggregated across all configured providers
    pub fn all_provider_models(&self) -> Vec<(String, Vec<String>)> {
        self.providers
            .iter()
            .filter_map(|(key, config)| {
                let models = config.effective_models();
                if models.is_empty() {
                    None
                } else {
                    Some((key.clone(), models))
                }
            })
            .collect()
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
    use std::str::FromStr;

    #[test]
    fn test_provider_defaults() {
        let ollama = LlmProviderConfig {
                    provider_type: BackendType::Ollama,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                    name: None,
                };

        assert_eq!(ollama.endpoint(), "http://localhost:11434");
        assert_eq!(ollama.model(), "llama3.2");
        assert_eq!(ollama.temperature(), 0.7);
        assert_eq!(ollama.max_tokens(), 4096);
        assert_eq!(ollama.timeout_secs(), 120);

        let openai = LlmProviderConfig {
                    provider_type: BackendType::OpenAI,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                    name: None,
                };

        assert_eq!(openai.endpoint(), "https://api.openai.com/v1");
        assert_eq!(openai.model(), "gpt-4o");

        let anthropic = LlmProviderConfig {
                    provider_type: BackendType::Anthropic,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                    name: None,
                };

        assert_eq!(anthropic.endpoint(), "https://api.anthropic.com/v1");
        assert_eq!(anthropic.model(), "claude-3-5-sonnet-20241022");

        let copilot = LlmProviderConfig {
                    provider_type: BackendType::GitHubCopilot,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                    name: None,
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
                BackendType::GitHubCopilot,
                "Failed to parse: {}",
                json
            );
        }
    }

    #[test]
    fn test_provider_custom_values() {
        let config = LlmProviderConfig {
                    provider_type: BackendType::Ollama,
                    endpoint: Some("http://192.168.1.100:11434".to_string()),
                    default_model: Some("llama3.1:70b".to_string()),
                    temperature: Some(0.9),
                    max_tokens: Some(8192),
                    timeout_secs: Some(300),
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                    name: None,
                };

        assert_eq!(config.endpoint(), "http://192.168.1.100:11434");
        assert_eq!(config.model(), "llama3.1:70b");
        assert_eq!(config.temperature(), 0.9);
        assert_eq!(config.max_tokens(), 8192);
        assert_eq!(config.timeout_secs(), 300);
    }

    #[test]
    fn test_api_key_direct_value() {
        let config = LlmProviderConfig {
                    provider_type: BackendType::OpenAI,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: Some("sk-test-key-123".to_string()),
                    available_models: None,
                    trust_level: None,
                    name: None,
                };

        assert_eq!(config.api_key(), Some("sk-test-key-123".to_string()));
    }

    #[test]
    fn test_llm_config_default_provider() {
        let mut providers = HashMap::new();
        providers.insert(
            "local".to_string(),
            LlmProviderConfig {
                        provider_type: BackendType::Ollama,
                        endpoint: Some("http://localhost:11434".to_string()),
                        default_model: Some("llama3.2".to_string()),
                        temperature: None,
                        max_tokens: None,
                        timeout_secs: None,
                        api_key: None,
                        available_models: None,
                        trust_level: None,
                        name: None,
                    },
        );
        providers.insert(
            "cloud".to_string(),
            LlmProviderConfig {
                        provider_type: BackendType::OpenAI,
                        endpoint: None,
                        default_model: Some("gpt-4o".to_string()),
                        temperature: None,
                        max_tokens: None,
                        timeout_secs: None,
                        api_key: Some("OPENAI_API_KEY".to_string()),
                        available_models: None,
                        trust_level: None,
                        name: None,
                    },
        );

        let config = LlmConfig {
            default: Some("local".to_string()),
            providers,
        };

        let (key, provider) = config.default_provider().unwrap();
        assert_eq!(key, "local");
        assert_eq!(provider.provider_type, BackendType::Ollama);
        assert_eq!(provider.model(), "llama3.2");
    }

    #[test]
    fn test_llm_config_get_provider() {
        let mut providers = HashMap::new();
        providers.insert(
            "local".to_string(),
            LlmProviderConfig {
                        provider_type: BackendType::Ollama,
                        endpoint: Some("http://localhost:11434".to_string()),
                        default_model: Some("llama3.2".to_string()),
                        temperature: None,
                        max_tokens: None,
                        timeout_secs: None,
                        api_key: None,
                        available_models: None,
                        trust_level: None,
                        name: None,
                    },
        );

        let config = LlmConfig {
            default: Some("local".to_string()),
            providers,
        };

        let provider = config.get_provider("local").unwrap();
        assert_eq!(provider.provider_type, BackendType::Ollama);

        assert!(config.get_provider("nonexistent").is_none());
    }

    #[test]
    fn test_llm_config_provider_keys() {
        let mut providers = HashMap::new();
        providers.insert(
            "local".to_string(),
            LlmProviderConfig {
                        provider_type: BackendType::Ollama,
                        endpoint: None,
                        default_model: None,
                        temperature: None,
                        max_tokens: None,
                        timeout_secs: None,
                        api_key: None,
                        available_models: None,
                        trust_level: None,
                        name: None,
                    },
        );
        providers.insert(
            "cloud".to_string(),
            LlmProviderConfig {
                        provider_type: BackendType::OpenAI,
                        endpoint: None,
                        default_model: None,
                        temperature: None,
                        max_tokens: None,
                        timeout_secs: None,
                        api_key: None,
                        available_models: None,
                        trust_level: None,
                        name: None,
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
                        provider_type: BackendType::Ollama,
                        endpoint: None,
                        default_model: None,
                        temperature: None,
                        max_tokens: None,
                        timeout_secs: None,
                        api_key: None,
                        available_models: None,
                        trust_level: None,
                        name: None,
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
                        provider_type: BackendType::Ollama,
                        endpoint: None,
                        default_model: None,
                        temperature: None,
                        max_tokens: None,
                        timeout_secs: None,
                        api_key: None,
                        available_models: None,
                        trust_level: None,
                        name: None,
                    },
        );

        let config = LlmConfig {
            default: Some("nonexistent".to_string()),
            providers,
        };

        assert!(config.default_provider().is_none());
    }

    #[test]
    fn test_available_models_deserialization() {
        let config = LlmProviderConfig {
                    provider_type: BackendType::Ollama,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: Some(vec!["model-a".to_string(), "model-b".to_string()]),
                    trust_level: None,
                    name: None,
                };

        assert_eq!(
            config.available_models,
            Some(vec!["model-a".to_string(), "model-b".to_string()])
        );
    }

    #[test]
    fn test_available_models_none_by_default() {
        let config = LlmProviderConfig {
                    provider_type: BackendType::Ollama,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                    name: None,
                };

        assert_eq!(config.available_models, None);
    }

    #[test]
    fn test_all_provider_models_aggregates() {
        let mut providers = HashMap::new();
        providers.insert(
            "local".to_string(),
            LlmProviderConfig {
                        provider_type: BackendType::Ollama,
                        endpoint: None,
                        default_model: None,
                        temperature: None,
                        max_tokens: None,
                        timeout_secs: None,
                        api_key: None,
                        available_models: Some(vec!["llama3.2".to_string()]),
                        trust_level: None,
                        name: None,
                    },
        );
        providers.insert(
            "cloud".to_string(),
            LlmProviderConfig {
                        provider_type: BackendType::OpenAI,
                        endpoint: None,
                        default_model: None,
                        temperature: None,
                        max_tokens: None,
                        timeout_secs: None,
                        api_key: None,
                        available_models: Some(vec!["gpt-4o".to_string(), "gpt-4o-mini".to_string()]),
                        trust_level: None,
                        name: None,
                    },
        );

        let config = LlmConfig {
            default: None,
            providers,
        };

        let result = config.all_provider_models();
        assert_eq!(result.len(), 2);

        // Check that both providers are in the result
        let keys: Vec<_> = result.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"local"));
        assert!(keys.contains(&"cloud"));

        // Check models for each provider
        for (key, models) in result {
            if key == "local" {
                assert_eq!(models, vec!["llama3.2".to_string()]);
            } else if key == "cloud" {
                assert_eq!(models.len(), 2);
                assert!(models.contains(&"gpt-4o".to_string()));
                assert!(models.contains(&"gpt-4o-mini".to_string()));
            }
        }
    }

    #[test]
    fn test_all_provider_models_without_available_models_returns_empty() {
        let mut providers = HashMap::new();
        // Without available_models, effective_models returns empty (dynamic discovery at daemon layer)
        providers.insert(
            "anthropic".to_string(),
            LlmProviderConfig {
                        provider_type: BackendType::Anthropic,
                        endpoint: None,
                        default_model: None,
                        temperature: None,
                        max_tokens: None,
                        timeout_secs: None,
                        api_key: None,
                        available_models: None,
                        trust_level: None,
                        name: None,
                    },
        );
        providers.insert(
            "openai".to_string(),
            LlmProviderConfig {
                        provider_type: BackendType::OpenAI,
                        endpoint: None,
                        default_model: None,
                        temperature: None,
                        max_tokens: None,
                        timeout_secs: None,
                        api_key: None,
                        available_models: None,
                        trust_level: None,
                        name: None,
                    },
        );

        let config = LlmConfig {
            default: None,
            providers,
        };

        let result = config.all_provider_models();
        // Both providers return empty without available_models (no hardcoded fallback)
        for (_key, models) in &result {
            assert!(
                models.is_empty(),
                "Without available_models, effective_models should return empty"
            );
        }
    }

    #[test]
    fn test_zai_from_str_variants() {
        assert_eq!(BackendType::from_str("zai"), Ok(BackendType::ZAI));
        assert_eq!(BackendType::from_str("z.ai"), Ok(BackendType::ZAI));
        assert_eq!(BackendType::from_str("z_ai"), Ok(BackendType::ZAI));
    }

    #[test]
    fn test_zai_serde_aliases() {
        // Test that various TOML formats deserialize correctly
        let variants = [
            r#"{"type": "zai"}"#,  // canonical lowercase form
            r#"{"type": "z.ai"}"#, // dot alias
            r#"{"type": "z_ai"}"#, // snake_case alias
        ];

        for json in variants {
            let config: LlmProviderConfig = serde_json::from_str(json).unwrap();
            assert_eq!(
                config.provider_type,
                BackendType::ZAI,
                "Failed to parse: {}",
                json
            );
        }
    }

    #[test]
    fn test_zai_api_key_env_var() {
        let zai = BackendType::ZAI;
        assert_eq!(zai.api_key_env_var(), Some("GLM_AUTH_TOKEN"));
    }

    #[test]
    fn test_zai_endpoint_default() {
        let config = LlmProviderConfig {
                    provider_type: BackendType::ZAI,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                    name: None,
                };

        assert_eq!(config.endpoint(), "https://api.z.ai/api/coding/paas/v4");
    }

    #[test]
    fn test_zai_model_default() {
        let config = LlmProviderConfig {
                    provider_type: BackendType::ZAI,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                    name: None,
                };

        assert_eq!(config.model(), "GLM-4.7");
    }

    #[test]
    fn test_zai_effective_models_empty_without_config() {
        let config = LlmProviderConfig {
                    provider_type: BackendType::ZAI,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                    name: None,
                };

        let models = config.effective_models();
        assert!(
            models.is_empty(),
            "Without available_models, ZAI should return empty"
        );
    }

    #[test]
    fn test_zai_effective_models_custom() {
        let custom_models = vec!["custom-model".to_string()];
        let config = LlmProviderConfig {
                    provider_type: BackendType::ZAI,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: Some(custom_models.clone()),
                    trust_level: None,
                    name: None,
                };

        assert_eq!(config.effective_models(), custom_models);
    }

    #[test]
    fn test_provider_as_str() {
        assert_eq!(BackendType::Ollama.as_str(), "ollama");
        assert_eq!(BackendType::OpenAI.as_str(), "openai");
        assert_eq!(BackendType::Anthropic.as_str(), "anthropic");
        assert_eq!(BackendType::GitHubCopilot.as_str(), "github-copilot");
        assert_eq!(BackendType::OpenRouter.as_str(), "openrouter");
        assert_eq!(BackendType::ZAI.as_str(), "zai");
    }

    #[test]
    fn test_effective_trust_level_uses_backend_default() {
        // When trust_level is None, should use backend's default
        let config = LlmProviderConfig {
                    provider_type: BackendType::FastEmbed,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                    name: None,
                };
        assert_eq!(
            config.effective_trust_level(),
            super::super::trust::TrustLevel::Local
        );

        let config = LlmProviderConfig {
                    provider_type: BackendType::OpenAI,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                    name: None,
                };
        assert_eq!(
            config.effective_trust_level(),
            super::super::trust::TrustLevel::Cloud
        );
    }

    #[test]
    fn test_effective_trust_level_explicit_override() {
        // When trust_level is Some, should use explicit value
        let config = LlmProviderConfig {
                    provider_type: BackendType::OpenAI,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: Some(super::super::trust::TrustLevel::Local),
                    name: None,
                };
        assert_eq!(
            config.effective_trust_level(),
            super::super::trust::TrustLevel::Local
        );
    }

    #[test]
    fn test_trust_level_serde_skip_none() {
        // When trust_level is None, it should not be serialized
        let config = LlmProviderConfig {
                    provider_type: BackendType::OpenAI,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: None,
                    name: None,
                };
        let json = serde_json::to_string(&config).expect("Failed to serialize");
        assert!(!json.contains("trust_level"));
    }

    #[test]
    fn test_trust_level_serde_include_some() {
        // When trust_level is Some, it should be serialized
        let config = LlmProviderConfig {
                    provider_type: BackendType::OpenAI,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: None,
                    trust_level: Some(super::super::trust::TrustLevel::Local),
                    name: None,
                };
        let json = serde_json::to_string(&config).expect("Failed to serialize");
        assert!(json.contains("trust_level"));
        assert!(json.contains("local"));
    }

    #[test]
    fn test_trust_level_toml_backward_compat() {
        // TOML without trust_level field should deserialize with None
        let toml_str = r#"
type = "openai"
endpoint = "https://api.openai.com/v1"
default_model = "gpt-4"
"#;
        let config: LlmProviderConfig =
            toml::from_str(toml_str).expect("Failed to deserialize TOML");
        assert_eq!(config.trust_level, None);
        assert_eq!(
            config.effective_trust_level(),
            super::super::trust::TrustLevel::Cloud
        );
    }

    #[test]
    fn effective_models_empty_without_available_models() {
        // All providers return empty when no available_models configured
        for backend in [
            BackendType::Anthropic,
            BackendType::OpenAI,
            BackendType::ZAI,
            BackendType::Ollama,
        ] {
            let config = LlmProviderConfig::builder(backend).build();
            assert!(
                config.effective_models().is_empty(),
                "{:?} should return empty without available_models",
                backend
            );
        }
    }

    #[test]
    fn effective_models_ollama_returns_empty() {
        let config = LlmProviderConfig::builder(BackendType::Ollama).build();
        let models = config.effective_models();
        assert_eq!(
            models,
            Vec::<String>::new(),
            "Ollama should return empty when no available_models set"
        );
    }

    #[test]
    fn effective_models_custom_returns_empty() {
        let config = LlmProviderConfig::builder(BackendType::Custom).build();
        let models = config.effective_models();
        assert_eq!(
            models,
            Vec::<String>::new(),
            "Custom should return empty when no available_models set"
        );
    }

    #[test]
    fn effective_models_openrouter_returns_empty() {
        let config = LlmProviderConfig::builder(BackendType::OpenRouter).build();
        let models = config.effective_models();
        assert_eq!(
            models,
            Vec::<String>::new(),
            "OpenRouter should return empty when no available_models set"
        );
    }

    #[test]
    fn effective_models_explicit_override_wins() {
        // Test with Anthropic (has hardcoded fallback)
        let custom_models = vec!["my-custom-model".to_string()];
        let config = LlmProviderConfig::builder(BackendType::Anthropic)
            .available_models(custom_models.clone())
            .build();
        assert_eq!(
            config.effective_models(),
            custom_models,
            "available_models should be returned for Anthropic"
        );

        // Test with OpenAI
        let custom_models = vec!["gpt-5-turbo".to_string()];
        let config = LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(custom_models.clone())
            .build();
        assert_eq!(
            config.effective_models(),
            custom_models,
            "available_models should be returned for OpenAI"
        );

        // Test with ZAI
        let custom_models = vec!["GLM-6".to_string()];
        let config = LlmProviderConfig::builder(BackendType::ZAI)
            .available_models(custom_models.clone())
            .build();
        assert_eq!(
            config.effective_models(),
            custom_models,
            "available_models should be returned for ZAI"
        );

        // Test with Ollama
        let custom_models = vec!["llama3.1:70b".to_string()];
        let config = LlmProviderConfig::builder(BackendType::Ollama)
            .available_models(custom_models.clone())
            .build();
        assert_eq!(
            config.effective_models(),
            custom_models,
            "available_models should be returned for Ollama"
        );
    }
}
