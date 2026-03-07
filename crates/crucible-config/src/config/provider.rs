//! Resolved LLM provider configuration.

use crate::components::BackendType;

/// Resolved LLM provider configuration
#[derive(Clone)]
pub struct EffectiveLlmConfig {
    /// Provider key (e.g., "local", "cloud", or "default" for fallback)
    pub key: String,
    /// Provider type
    pub provider_type: BackendType,
    /// API endpoint
    pub endpoint: String,
    /// Model name
    pub model: String,
    /// Temperature
    pub temperature: f32,
    /// Maximum tokens
    pub max_tokens: u32,
    /// Timeout in seconds
    pub timeout_secs: u64,
    /// API key (if applicable)
    pub api_key: Option<String>,
}

impl std::fmt::Debug for EffectiveLlmConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EffectiveLlmConfig")
            .field("key", &self.key)
            .field("provider_type", &self.provider_type)
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("temperature", &self.temperature)
            .field("max_tokens", &self.max_tokens)
            .field("timeout_secs", &self.timeout_secs)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}
