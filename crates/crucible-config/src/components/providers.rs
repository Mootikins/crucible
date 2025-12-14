//! Unified providers configuration
//!
//! This module defines the `ProvidersConfig` struct that manages multiple
//! provider instances and default provider selections for embedding and chat.

use super::backend::BackendType;
use super::provider::ProviderConfig;
use serde::{de::Deserializer, Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Configuration for multiple provider instances
///
/// This is the top-level configuration for all providers in the system.
/// It manages named provider instances and default selections for different
/// capabilities (embedding, chat).
///
/// # Example TOML (New Flat Format)
///
/// ```toml
/// [providers]
/// default_embedding = "local-ollama"
/// default_chat = "local-ollama"
///
/// [providers.local-ollama]
/// backend = "ollama"
/// endpoint = "http://localhost:11434"
/// models.embedding = "nomic-embed-text"
/// models.chat = "llama3.2"
///
/// [providers.openai-prod]
/// backend = "openai"
/// api_key = { env = "OPENAI_API_KEY" }
/// models.embedding = "text-embedding-3-small"
/// models.chat = "gpt-4o"
/// ```
///
/// # Legacy Format (Still Supported)
///
/// ```toml
/// [providers]
/// default_embedding = "local-ollama"
///
/// [providers.instances.local-ollama]
/// backend = "ollama"
/// ```
#[derive(Debug, Clone, Serialize, Default)]
pub struct ProvidersConfig {
    /// Default provider name for embedding operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_embedding: Option<String>,

    /// Default provider name for chat operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_chat: Option<String>,

    /// Named provider instances (flattened in serialization)
    #[serde(flatten)]
    providers: HashMap<String, ProviderConfig>,
}

// Custom deserialization to support both flat and legacy formats
impl<'de> Deserialize<'de> for ProvidersConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;

        // Parse as a generic map
        let mut map: HashMap<String, Value> = HashMap::deserialize(deserializer)?;

        // Extract special keys
        let default_embedding = map
            .remove("default_embedding")
            .map(|v| serde_json::from_value(v).map_err(D::Error::custom))
            .transpose()?;

        let default_chat = map
            .remove("default_chat")
            .map(|v| serde_json::from_value(v).map_err(D::Error::custom))
            .transpose()?;

        // Handle legacy "instances" key
        let mut providers = HashMap::new();
        if let Some(instances_value) = map.remove("instances") {
            // Legacy format: [providers.instances.X]
            let instances: HashMap<String, ProviderConfig> =
                serde_json::from_value(instances_value).map_err(D::Error::custom)?;
            providers.extend(instances);
        }

        // All remaining keys are provider names in flat format
        for (key, value) in map {
            let config: ProviderConfig = serde_json::from_value(value).map_err(D::Error::custom)?;
            providers.insert(key, config);
        }

        Ok(ProvidersConfig {
            default_embedding,
            default_chat,
            providers,
        })
    }
}

impl ProvidersConfig {
    /// Create a new empty providers configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a provider instance
    pub fn add(&mut self, name: impl Into<String>, config: ProviderConfig) {
        self.providers.insert(name.into(), config);
    }

    /// Get a provider by name
    pub fn get(&self, name: &str) -> Option<&ProviderConfig> {
        self.providers.get(name)
    }

    /// Get the default embedding provider
    pub fn default_embedding_provider(&self) -> Option<(&String, &ProviderConfig)> {
        let name = self.default_embedding.as_ref()?;
        let config = self.providers.get(name)?;
        if config.supports_embeddings() {
            Some((name, config))
        } else {
            None
        }
    }

    /// Get the default chat provider
    pub fn default_chat_provider(&self) -> Option<(&String, &ProviderConfig)> {
        let name = self.default_chat.as_ref()?;
        let config = self.providers.get(name)?;
        if config.supports_chat() {
            Some((name, config))
        } else {
            None
        }
    }

    /// Find the first provider that supports embeddings
    pub fn first_embedding_provider(&self) -> Option<(&String, &ProviderConfig)> {
        self.default_embedding_provider().or_else(|| {
            self.providers
                .iter()
                .find(|(_, c)| c.supports_embeddings())
        })
    }

    /// Find the first provider that supports chat
    pub fn first_chat_provider(&self) -> Option<(&String, &ProviderConfig)> {
        self.default_chat_provider()
            .or_else(|| self.providers.iter().find(|(_, c)| c.supports_chat()))
    }

    /// List all provider names
    pub fn names(&self) -> Vec<&String> {
        self.providers.keys().collect()
    }

    /// List providers that support embeddings
    pub fn embedding_providers(&self) -> Vec<(&String, &ProviderConfig)> {
        self.providers
            .iter()
            .filter(|(_, c)| c.supports_embeddings())
            .collect()
    }

    /// List providers that support chat
    pub fn chat_providers(&self) -> Vec<(&String, &ProviderConfig)> {
        self.providers
            .iter()
            .filter(|(_, c)| c.supports_chat())
            .collect()
    }

    /// Check if any providers are configured
    pub fn has_providers(&self) -> bool {
        !self.providers.is_empty()
    }

    /// Validate all provider configurations
    pub fn validate(&self) -> Result<(), Vec<(String, String)>> {
        let errors: Vec<(String, String)> = self
            .providers
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
        self.providers.insert(name.into(), config);
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

#[cfg(test)]
mod flat_config_tests {
    use super::*;

    // RED: Test flat config parsing
    #[test]
    fn test_flat_providers_config_parsing() {
        let toml = r#"
default_embedding = "ollama"
default_chat = "ollama"

[ollama]
backend = "ollama"
endpoint = "http://localhost:11434"

[fastembed]
backend = "fastembed"
"#;
        let config: ProvidersConfig = toml::from_str(toml).unwrap();

        assert_eq!(config.default_embedding, Some("ollama".to_string()));
        assert_eq!(config.default_chat, Some("ollama".to_string()));
        assert!(config.get("ollama").is_some());
        assert!(config.get("fastembed").is_some());
        // "instances" should not be in the config
        assert!(config.get("instances").is_none());
    }

    // RED: Test backwards compatibility with old format
    #[test]
    fn test_legacy_instances_format_still_works() {
        let toml = r#"
default_embedding = "ollama"

[instances.ollama]
backend = "ollama"
"#;
        let config: ProvidersConfig = toml::from_str(toml).unwrap();
        assert!(config.get("ollama").is_some());
        assert_eq!(config.default_embedding, Some("ollama".to_string()));
    }

    // RED: Test serialization produces flat format
    #[test]
    fn test_serialization_produces_flat_format() {
        let config = ProvidersConfig::new()
            .with_provider("ollama", ProviderConfig::new(BackendType::Ollama))
            .with_default_embedding("ollama");

        let toml = toml::to_string_pretty(&config).unwrap();

        // Should NOT contain "instances"
        assert!(!toml.contains("[instances"));
        // Should have flat provider section
        assert!(toml.contains("[ollama]"));
    }

    // RED: Reserved keys should not be treated as providers
    #[test]
    fn test_reserved_keys_not_providers() {
        let config = ProvidersConfig::new()
            .with_default_embedding("test")
            .with_default_chat("chat-test");

        // These should not appear as provider names
        let names = config.names();
        assert!(!names.contains(&&"default_embedding".to_string()));
        assert!(!names.contains(&&"default_chat".to_string()));
    }

    // Verify the actual serialized output looks correct
    #[test]
    fn test_flat_format_visual_verification() {
        let config = ProvidersConfig::new()
            .with_provider(
                "ollama",
                ProviderConfig::new(BackendType::Ollama)
                    .with_endpoint("http://localhost:11434"),
            )
            .with_provider("fastembed", ProviderConfig::new(BackendType::FastEmbed))
            .with_default_embedding("ollama")
            .with_default_chat("ollama");

        let toml = toml::to_string_pretty(&config).unwrap();

        // Verify structure
        assert!(toml.contains("default_embedding = \"ollama\""));
        assert!(toml.contains("default_chat = \"ollama\""));
        assert!(toml.contains("[ollama]"));
        assert!(toml.contains("[fastembed]"));
        assert!(!toml.contains("[instances"));

        // Print for visual verification during development
        eprintln!("Serialized TOML:\n{}", toml);
    }
}
