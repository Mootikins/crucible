//! Configuration conversion utilities
//!
//! This module provides conversion functions between the old crucible-config
//! EmbeddingProviderConfig format and the new crucible-core EnrichmentConfig format.
//!
//! ## Migration Path
//!
//! The old configuration structure used a single `EmbeddingProviderConfig` with
//! generic `api` and `model` fields plus an `options` HashMap for provider-specific
//! settings. The new structure uses type-safe provider-specific enums.
//!
//! Old structure (crucible-config):
//! ```ignore
//! EmbeddingProviderConfig {
//!     provider_type: OpenAI,
//!     api: { key, base_url, ... },
//!     model: { name, dimensions, ... },
//!     options: { ... }
//! }
//! ```
//!
//! New structure (crucible-core):
//! ```ignore
//! EnrichmentConfig {
//!     provider: EmbeddingProviderConfig::OpenAI(OpenAIConfig { api_key, model, ... }),
//!     pipeline: PipelineConfig { ... }
//! }
//! ```

use crucible_config::{EmbeddingProviderConfig as OldConfig, EmbeddingProviderType};
use crucible_core::enrichment::{
    CohereConfig, CustomConfig, EmbeddingProviderConfig, EnrichmentConfig, FastEmbedConfig,
    MockConfig, OllamaConfig, OpenAIConfig, PipelineConfig, VertexAIConfig,
};

/// Convert old crucible-config format to new crucible-core format
pub fn convert_to_enrichment_config(old: &OldConfig) -> EnrichmentConfig {
    let provider = convert_provider_config(old);
    EnrichmentConfig {
        provider,
        pipeline: PipelineConfig::default(),
    }
}

/// Convert old provider config to new provider enum
fn convert_provider_config(old: &OldConfig) -> EmbeddingProviderConfig {
    match &old.provider_type {
        EmbeddingProviderType::OpenAI => {
            EmbeddingProviderConfig::OpenAI(OpenAIConfig {
                api_key: old.api.key.clone().unwrap_or_default(),
                model: old.model.name.clone(),
                base_url: old
                    .api
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
                timeout_seconds: old.api.timeout_seconds.unwrap_or(30),
                retry_attempts: old.api.retry_attempts.unwrap_or(3),
                dimensions: old.model.dimensions.unwrap_or(1536),
                headers: old.api.headers.clone(),
            })
        }
        EmbeddingProviderType::Ollama => {
            EmbeddingProviderConfig::Ollama(OllamaConfig {
                model: old.model.name.clone(),
                base_url: old
                    .api
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "http://localhost:11434".to_string()),
                timeout_seconds: old.api.timeout_seconds.unwrap_or(30),
                retry_attempts: old.api.retry_attempts.unwrap_or(3),
                dimensions: old.model.dimensions.unwrap_or(768),
            })
        }
        EmbeddingProviderType::FastEmbed => {
            // Extract FastEmbed-specific options
            let cache_dir = old
                .options
                .get("cache_dir")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let batch_size = old
                .options
                .get("batch_size")
                .and_then(|v| v.as_u64())
                .map(|n| n as u32)
                .unwrap_or(32);

            let num_threads = old
                .options
                .get("num_threads")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);

            EmbeddingProviderConfig::FastEmbed(FastEmbedConfig {
                model: old.model.name.clone(),
                cache_dir,
                batch_size,
                dimensions: old.model.dimensions.unwrap_or(384),
                num_threads,
            })
        }
        EmbeddingProviderType::Cohere => {
            let input_type = old
                .options
                .get("input_type")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "search_document".to_string());

            EmbeddingProviderConfig::Cohere(CohereConfig {
                api_key: old.api.key.clone().unwrap_or_default(),
                model: old.model.name.clone(),
                base_url: old
                    .api
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.cohere.ai/v1".to_string()),
                timeout_seconds: old.api.timeout_seconds.unwrap_or(30),
                retry_attempts: old.api.retry_attempts.unwrap_or(3),
                input_type,
                headers: old.api.headers.clone(),
            })
        }
        EmbeddingProviderType::VertexAI => {
            let project_id = old
                .options
                .get("project_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default();

            let credentials_path = old
                .options
                .get("credentials_path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            EmbeddingProviderConfig::VertexAI(VertexAIConfig {
                project_id,
                model: old.model.name.clone(),
                base_url: old
                    .api
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://aiplatform.googleapis.com/v1".to_string()),
                timeout_seconds: old.api.timeout_seconds.unwrap_or(30),
                retry_attempts: old.api.retry_attempts.unwrap_or(3),
                credentials_path,
                headers: old.api.headers.clone(),
            })
        }
        EmbeddingProviderType::Custom(name) => {
            let request_template = old
                .options
                .get("request_template")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let response_path = old
                .options
                .get("response_path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            EmbeddingProviderConfig::Custom(CustomConfig {
                base_url: old.api.base_url.clone().unwrap_or_default(),
                api_key: old.api.key.clone(),
                model: old.model.name.clone(),
                timeout_seconds: old.api.timeout_seconds.unwrap_or(30),
                retry_attempts: old.api.retry_attempts.unwrap_or(3),
                dimensions: old.model.dimensions.unwrap_or(768),
                headers: old.api.headers.clone(),
                request_template,
                response_path,
            })
        }
        EmbeddingProviderType::Mock => {
            let simulated_latency_ms = old
                .options
                .get("simulated_latency_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            EmbeddingProviderConfig::Mock(MockConfig {
                model: old.model.name.clone(),
                dimensions: old.model.dimensions.unwrap_or(768),
                simulated_latency_ms,
            })
        }
    }
}

/// Convert new crucible-core format back to old crucible-config format
///
/// This is useful for backwards compatibility when saving configurations
/// or interacting with legacy systems.
pub fn convert_from_enrichment_config(new: &EnrichmentConfig) -> OldConfig {
    use crucible_config::{ApiConfig, ModelConfig};
    use std::collections::HashMap;

    match &new.provider {
        EmbeddingProviderConfig::OpenAI(cfg) => OldConfig {
            provider_type: EmbeddingProviderType::OpenAI,
            api: ApiConfig {
                key: Some(cfg.api_key.clone()),
                base_url: Some(cfg.base_url.clone()),
                timeout_seconds: Some(cfg.timeout_seconds),
                retry_attempts: Some(cfg.retry_attempts),
                headers: cfg.headers.clone(),
            },
            model: ModelConfig {
                name: cfg.model.clone(),
                dimensions: Some(cfg.dimensions),
                max_tokens: None,
            },
            options: HashMap::new(),
        },
        EmbeddingProviderConfig::Ollama(cfg) => OldConfig {
            provider_type: EmbeddingProviderType::Ollama,
            api: ApiConfig {
                key: None,
                base_url: Some(cfg.base_url.clone()),
                timeout_seconds: Some(cfg.timeout_seconds),
                retry_attempts: Some(cfg.retry_attempts),
                headers: HashMap::new(),
            },
            model: ModelConfig {
                name: cfg.model.clone(),
                dimensions: Some(cfg.dimensions),
                max_tokens: None,
            },
            options: HashMap::new(),
        },
        EmbeddingProviderConfig::FastEmbed(cfg) => {
            let mut options = HashMap::new();
            if let Some(cache_dir) = &cfg.cache_dir {
                options.insert(
                    "cache_dir".to_string(),
                    serde_json::Value::String(cache_dir.clone()),
                );
            }
            options.insert(
                "batch_size".to_string(),
                serde_json::Value::Number(cfg.batch_size.into()),
            );
            if let Some(num_threads) = cfg.num_threads {
                options.insert(
                    "num_threads".to_string(),
                    serde_json::Value::Number(num_threads.into()),
                );
            }

            OldConfig {
                provider_type: EmbeddingProviderType::FastEmbed,
                api: ApiConfig {
                    key: None,
                    base_url: Some("local".to_string()),
                    timeout_seconds: Some(60),
                    retry_attempts: Some(1),
                    headers: HashMap::new(),
                },
                model: ModelConfig {
                    name: cfg.model.clone(),
                    dimensions: Some(cfg.dimensions),
                    max_tokens: None,
                },
                options,
            }
        }
        EmbeddingProviderConfig::Cohere(cfg) => {
            let mut options = HashMap::new();
            options.insert(
                "input_type".to_string(),
                serde_json::Value::String(cfg.input_type.clone()),
            );

            OldConfig {
                provider_type: EmbeddingProviderType::Cohere,
                api: ApiConfig {
                    key: Some(cfg.api_key.clone()),
                    base_url: Some(cfg.base_url.clone()),
                    timeout_seconds: Some(cfg.timeout_seconds),
                    retry_attempts: Some(cfg.retry_attempts),
                    headers: cfg.headers.clone(),
                },
                model: ModelConfig {
                    name: cfg.model.clone(),
                    dimensions: None,
                    max_tokens: None,
                },
                options,
            }
        }
        EmbeddingProviderConfig::VertexAI(cfg) => {
            let mut options = HashMap::new();
            options.insert(
                "project_id".to_string(),
                serde_json::Value::String(cfg.project_id.clone()),
            );
            if let Some(credentials_path) = &cfg.credentials_path {
                options.insert(
                    "credentials_path".to_string(),
                    serde_json::Value::String(credentials_path.clone()),
                );
            }

            OldConfig {
                provider_type: EmbeddingProviderType::VertexAI,
                api: ApiConfig {
                    key: None,
                    base_url: Some(cfg.base_url.clone()),
                    timeout_seconds: Some(cfg.timeout_seconds),
                    retry_attempts: Some(cfg.retry_attempts),
                    headers: cfg.headers.clone(),
                },
                model: ModelConfig {
                    name: cfg.model.clone(),
                    dimensions: None,
                    max_tokens: None,
                },
                options,
            }
        }
        EmbeddingProviderConfig::Custom(cfg) => {
            let mut options = HashMap::new();
            if let Some(request_template) = &cfg.request_template {
                options.insert(
                    "request_template".to_string(),
                    serde_json::Value::String(request_template.clone()),
                );
            }
            if let Some(response_path) = &cfg.response_path {
                options.insert(
                    "response_path".to_string(),
                    serde_json::Value::String(response_path.clone()),
                );
            }

            OldConfig {
                provider_type: EmbeddingProviderType::Custom(cfg.model.clone()),
                api: ApiConfig {
                    key: cfg.api_key.clone(),
                    base_url: Some(cfg.base_url.clone()),
                    timeout_seconds: Some(cfg.timeout_seconds),
                    retry_attempts: Some(cfg.retry_attempts),
                    headers: cfg.headers.clone(),
                },
                model: ModelConfig {
                    name: cfg.model.clone(),
                    dimensions: Some(cfg.dimensions),
                    max_tokens: None,
                },
                options,
            }
        }
        EmbeddingProviderConfig::Mock(cfg) => {
            let mut options = HashMap::new();
            options.insert(
                "simulated_latency_ms".to_string(),
                serde_json::Value::Number(cfg.simulated_latency_ms.into()),
            );

            OldConfig {
                provider_type: EmbeddingProviderType::Mock,
                api: ApiConfig {
                    key: None,
                    base_url: Some("mock".to_string()),
                    timeout_seconds: Some(5),
                    retry_attempts: Some(1),
                    headers: HashMap::new(),
                },
                model: ModelConfig {
                    name: cfg.model.clone(),
                    dimensions: Some(cfg.dimensions),
                    max_tokens: None,
                },
                options,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_openai_to_new() {
        use crucible_config::{ApiConfig, ModelConfig};
        use std::collections::HashMap;

        let old = OldConfig {
            provider_type: EmbeddingProviderType::OpenAI,
            api: ApiConfig {
                key: Some("test-key".to_string()),
                base_url: Some("https://api.openai.com/v1".to_string()),
                timeout_seconds: Some(30),
                retry_attempts: Some(3),
                headers: HashMap::new(),
            },
            model: ModelConfig {
                name: "text-embedding-3-small".to_string(),
                dimensions: Some(1536),
                max_tokens: None,
            },
            options: HashMap::new(),
        };

        let new = convert_to_enrichment_config(&old);

        match &new.provider {
            EmbeddingProviderConfig::OpenAI(cfg) => {
                assert_eq!(cfg.api_key, "test-key");
                assert_eq!(cfg.model, "text-embedding-3-small");
                assert_eq!(cfg.dimensions, 1536);
            }
            _ => panic!("Expected OpenAI config"),
        }
    }

    #[test]
    fn test_roundtrip_conversion() {
        use crucible_config::{ApiConfig, ModelConfig};
        use std::collections::HashMap;

        let original = OldConfig {
            provider_type: EmbeddingProviderType::Ollama,
            api: ApiConfig {
                key: None,
                base_url: Some("http://localhost:11434".to_string()),
                timeout_seconds: Some(30),
                retry_attempts: Some(3),
                headers: HashMap::new(),
            },
            model: ModelConfig {
                name: "nomic-embed-text".to_string(),
                dimensions: Some(768),
                max_tokens: None,
            },
            options: HashMap::new(),
        };

        let new = convert_to_enrichment_config(&original);
        let back = convert_from_enrichment_config(&new);

        assert_eq!(original.provider_type, back.provider_type);
        assert_eq!(original.model.name, back.model.name);
    }

    #[test]
    fn test_convert_fastembed_with_options() {
        use crucible_config::{ApiConfig, ModelConfig};
        use std::collections::HashMap;

        let mut options = HashMap::new();
        options.insert(
            "cache_dir".to_string(),
            serde_json::Value::String("/tmp/cache".to_string()),
        );
        options.insert("batch_size".to_string(), serde_json::Value::Number(64.into()));

        let old = OldConfig {
            provider_type: EmbeddingProviderType::FastEmbed,
            api: ApiConfig {
                key: None,
                base_url: Some("local".to_string()),
                timeout_seconds: Some(60),
                retry_attempts: Some(1),
                headers: HashMap::new(),
            },
            model: ModelConfig {
                name: "BAAI/bge-small-en-v1.5".to_string(),
                dimensions: Some(384),
                max_tokens: None,
            },
            options,
        };

        let new = convert_to_enrichment_config(&old);

        match &new.provider {
            EmbeddingProviderConfig::FastEmbed(cfg) => {
                assert_eq!(cfg.cache_dir, Some("/tmp/cache".to_string()));
                assert_eq!(cfg.batch_size, 64);
                assert_eq!(cfg.model, "BAAI/bge-small-en-v1.5");
            }
            _ => panic!("Expected FastEmbed config"),
        }
    }
}
