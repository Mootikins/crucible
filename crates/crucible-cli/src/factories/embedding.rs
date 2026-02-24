//! Embedding configuration utilities for CLI commands
//!
//! Provides config-to-provider-config conversion for commands that need
//! to create embedding providers (MCP server, semantic search).

use crate::config::CliConfig;
use crucible_config::{BackendType, EmbeddingProviderConfig, OllamaConfig, OpenAIConfig};
use tracing::warn;

/// Derive embedding provider config from CLI config
///
/// Checks enrichment config first, then falls back to LLM provider config.
pub fn embedding_provider_config_from_cli(config: &CliConfig) -> EmbeddingProviderConfig {
    // Check if enrichment provider is explicitly configured — use it directly
    if let Some(enrichment) = &config.enrichment {
        return enrichment.provider.clone();
    }

    // Fall back to deriving from the LLM provider config
    let effective = match config.effective_llm_provider() {
        Ok(cfg) => cfg,
        Err(err) => {
            warn!(error = %err, "No effective llm provider; using default ollama embedding config");
            return EmbeddingProviderConfig::ollama(None, None);
        }
    };

    match effective.provider_type {
        BackendType::OpenAI => {
            let cfg = OpenAIConfig {
                base_url: effective.endpoint,
                api_key: effective.api_key.unwrap_or_default(),
                model: effective.model,
                ..Default::default()
            };
            EmbeddingProviderConfig::OpenAI(cfg)
        }
        BackendType::Ollama => {
            let cfg = OllamaConfig {
                base_url: effective.endpoint,
                model: effective.model,
                ..Default::default()
            };
            EmbeddingProviderConfig::Ollama(cfg)
        }
        unsupported => {
            warn!(provider = ?unsupported, "Provider does not support embeddings; using default ollama config");
            EmbeddingProviderConfig::ollama(None, None)
        }
    }
}
