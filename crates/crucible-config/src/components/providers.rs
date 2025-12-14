//! Unified providers configuration
//!
//! This module defines the `ProvidersConfig` struct that manages multiple
//! provider instances and default provider selections for embedding and chat.

use super::backend::BackendType;
use super::provider::ProviderConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for multiple provider instances
///
/// This is the top-level configuration for all providers in the system.
/// It manages named provider instances and default selections for different
/// capabilities (embedding, chat).
///
/// # Example TOML
///
/// ```toml
/// [providers]
/// default_embedding = "local-ollama"
/// default_chat = "local-ollama"
///
/// [providers.instances.local-ollama]
/// backend = "ollama"
/// endpoint = "http://localhost:11434"
/// models.embedding = "nomic-embed-text"
/// models.chat = "llama3.2"
///
/// [providers.instances.openai-prod]
/// backend = "openai"
/// api_key = { env = "OPENAI_API_KEY" }
/// models.embedding = "text-embedding-3-small"
/// models.chat = "gpt-4o"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvidersConfig {
    /// Default provider name for embedding operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_embedding: Option<String>,

    /// Default provider name for chat operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_chat: Option<String>,

    /// Named provider instances
    #[serde(default)]
    pub instances: HashMap<String, ProviderConfig>,
}

impl ProvidersConfig {
    /// Create a new empty providers configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a provider instance
    pub fn add(&mut self, name: impl Into<String>, config: ProviderConfig) {
        self.instances.insert(name.into(), config);
    }

    /// Get a provider by name
    pub fn get(&self, name: &str) -> Option<&ProviderConfig> {
        self.instances.get(name)
    }

    /// Get the default embedding provider
    pub fn default_embedding_provider(&self) -> Option<(&String, &ProviderConfig)> {
        let name = self.default_embedding.as_ref()?;
        let config = self.instances.get(name)?;
        if config.supports_embeddings() {
            Some((name, config))
        } else {
            None
        }
    }

    /// Get the default chat provider
    pub fn default_chat_provider(&self) -> Option<(&String, &ProviderConfig)> {
        let name = self.default_chat.as_ref()?;
        let config = self.instances.get(name)?;
        if config.supports_chat() {
            Some((name, config))
        } else {
            None
        }
    }

    /// Find the first provider that supports embeddings
    pub fn first_embedding_provider(&self) -> Option<(&String, &ProviderConfig)> {
        self.default_embedding_provider().or_else(|| {
            self.instances
                .iter()
                .find(|(_, c)| c.supports_embeddings())
        })
    }

    /// Find the first provider that supports chat
    pub fn first_chat_provider(&self) -> Option<(&String, &ProviderConfig)> {
        self.default_chat_provider()
            .or_else(|| self.instances.iter().find(|(_, c)| c.supports_chat()))
    }

    /// List all provider names
    pub fn names(&self) -> Vec<&String> {
        self.instances.keys().collect()
    }

    /// List providers that support embeddings
    pub fn embedding_providers(&self) -> Vec<(&String, &ProviderConfig)> {
        self.instances
            .iter()
            .filter(|(_, c)| c.supports_embeddings())
            .collect()
    }

    /// List providers that support chat
    pub fn chat_providers(&self) -> Vec<(&String, &ProviderConfig)> {
        self.instances
            .iter()
            .filter(|(_, c)| c.supports_chat())
            .collect()
    }

    /// Check if any providers are configured
    pub fn has_providers(&self) -> bool {
        !self.instances.is_empty()
    }

    /// Validate all provider configurations
    pub fn validate(&self) -> Result<(), Vec<(String, String)>> {
        let errors: Vec<(String, String)> = self
            .instances
            .iter()
            .filter_map(|(name, config)| config.validate().err().map(|e| (name.clone(), e)))
            .collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    // === Builder pattern ===

    /// Set the default embedding provider
    pub fn with_default_embedding(mut self, name: impl Into<String>) -> Self {
        self.default_embedding = Some(name.into());
        self
    }

    /// Set the default chat provider
    pub fn with_default_chat(mut self, name: impl Into<String>) -> Self {
        self.default_chat = Some(name.into());
        self
    }

    /// Add a provider instance (builder pattern)
    pub fn with_provider(mut self, name: impl Into<String>, config: ProviderConfig) -> Self {
        self.instances.insert(name.into(), config);
        self
    }

    // === Convenience constructors ===

    /// Create a configuration with a single Ollama provider
    pub fn ollama_only(endpoint: Option<&str>) -> Self {
        let mut config = ProviderConfig::new(BackendType::Ollama);
        if let Some(ep) = endpoint {
            config = config.with_endpoint(ep);
        }

        Self::new()
            .with_provider("ollama", config)
            .with_default_embedding("ollama")
            .with_default_chat("ollama")
    }

    /// Create a configuration with FastEmbed for embeddings
    pub fn fastembed_only() -> Self {
        let config = ProviderConfig::new(BackendType::FastEmbed);

        Self::new()
            .with_provider("fastembed", config)
            .with_default_embedding("fastembed")
    }

    /// Create a configuration with mock providers for testing
    pub fn mock_for_testing() -> Self {
        let config = ProviderConfig::new(BackendType::Mock);

        Self::new()
            .with_provider("mock", config)
            .with_default_embedding("mock")
            .with_default_chat("mock")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_config() {
        let config = ProvidersConfig::new();
        assert!(!config.has_providers());
        assert!(config.names().is_empty());
        assert!(config.default_embedding_provider().is_none());
        assert!(config.default_chat_provider().is_none());
    }

    #[test]
    fn test_add_and_get_provider() {
        let mut config = ProvidersConfig::new();
        config.add("local", ProviderConfig::new(BackendType::Ollama));

        assert!(config.has_providers());
        assert!(config.get("local").is_some());
        assert!(config.get("nonexistent").is_none());
    }

    #[test]
    fn test_default_providers() {
        let config = ProvidersConfig::new()
            .with_provider("ollama", ProviderConfig::new(BackendType::Ollama))
            .with_provider("fastembed", ProviderConfig::new(BackendType::FastEmbed))
            .with_default_embedding("fastembed")
            .with_default_chat("ollama");

        let (name, provider) = config.default_embedding_provider().unwrap();
        assert_eq!(name, "fastembed");
        assert_eq!(provider.backend, BackendType::FastEmbed);

        let (name, provider) = config.default_chat_provider().unwrap();
        assert_eq!(name, "ollama");
        assert_eq!(provider.backend, BackendType::Ollama);
    }

    #[test]
    fn test_capability_filtering() {
        let config = ProvidersConfig::new()
            .with_provider("ollama", ProviderConfig::new(BackendType::Ollama))
            .with_provider("fastembed", ProviderConfig::new(BackendType::FastEmbed))
            .with_provider("anthropic", ProviderConfig::new(BackendType::Anthropic));

        // Ollama and FastEmbed support embeddings
        let embedding_providers = config.embedding_providers();
        assert_eq!(embedding_providers.len(), 2);

        // Ollama and Anthropic support chat
        let chat_providers = config.chat_providers();
        assert_eq!(chat_providers.len(), 2);
    }

    #[test]
    fn test_first_provider_fallback() {
        let config = ProvidersConfig::new()
            .with_provider("ollama", ProviderConfig::new(BackendType::Ollama));

        // No default set, but should find ollama
        let (name, _) = config.first_embedding_provider().unwrap();
        assert_eq!(name, "ollama");

        let (name, _) = config.first_chat_provider().unwrap();
        assert_eq!(name, "ollama");
    }

    #[test]
    fn test_ollama_only_convenience() {
        let config = ProvidersConfig::ollama_only(Some("http://192.168.1.100:11434"));

        assert!(config.has_providers());
        let (name, provider) = config.default_embedding_provider().unwrap();
        assert_eq!(name, "ollama");
        assert_eq!(
            provider.endpoint(),
            Some("http://192.168.1.100:11434".to_string())
        );
    }

    #[test]
    fn test_validation() {
        // Valid config
        let config = ProvidersConfig::fastembed_only();
        assert!(config.validate().is_ok());

        // Invalid config (OpenAI without API key)
        let config =
            ProvidersConfig::new().with_provider("openai", ProviderConfig::new(BackendType::OpenAI));
        let result = config.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, "openai");
    }

    #[test]
    fn test_serde_toml() {
        let config = ProvidersConfig::new()
            .with_provider(
                "local-ollama",
                ProviderConfig::new(BackendType::Ollama)
                    .with_endpoint("http://localhost:11434")
                    .with_embedding_model("nomic-embed-text")
                    .with_chat_model("llama3.2"),
            )
            .with_default_embedding("local-ollama")
            .with_default_chat("local-ollama");

        let toml = toml::to_string_pretty(&config).unwrap();
        assert!(toml.contains("default_embedding"));
        assert!(toml.contains("local-ollama"));

        let parsed: ProvidersConfig = toml::from_str(&toml).unwrap();
        assert_eq!(parsed.default_embedding, Some("local-ollama".to_string()));
        assert!(parsed.get("local-ollama").is_some());
    }
}
