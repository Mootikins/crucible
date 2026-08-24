//! Resolved LLM provider configuration.

use crate::config::components::BackendType;

/// Resolved LLM provider configuration
#[derive(Clone)]
pub struct EffectiveLlmConfig {
    /// Provider type
    pub provider_type: BackendType,
    /// API endpoint
    pub endpoint: String,
    /// Model name
    pub model: String,
    /// API key (if applicable)
    pub api_key: Option<String>,
}

impl std::fmt::Debug for EffectiveLlmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EffectiveLlmConfig")
            .field("provider_type", &self.provider_type)
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}
