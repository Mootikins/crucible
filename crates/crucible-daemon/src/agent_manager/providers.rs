use super::*;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct ProviderInfo {
    pub name: String,
    pub provider_type: String,
    pub available: bool,
    pub default_model: Option<String>,
    pub models: Vec<String>,
    pub endpoint: Option<String>,
    pub reason: Option<String>,
    pub is_local: bool,
}

impl AgentManager {
    pub async fn list_providers(&self) -> Vec<ProviderInfo> {
        let mut providers = Vec::new();
        let mut seen_types = HashSet::new();

        if let Some(llm_config) = &self.llm_config {
            for (key, provider_config) in &llm_config.providers {
                let backend = provider_config.provider_type;
                if !backend.supports_chat() {
                    continue;
                }

                seen_types.insert(backend.as_str().to_string());

                let models = self.discover_models(key, provider_config).await;
                providers.push(ProviderInfo {
                    name: format_provider_name(key, backend),
                    provider_type: backend.as_str().to_string(),
                    available: !models.is_empty() || backend != BackendType::Ollama,
                    default_model: Some(provider_config.model()),
                    models,
                    endpoint: Some(provider_config.endpoint()),
                    reason: Some("config".to_string()),
                    is_local: backend.is_local(),
                });
            }
        }

        for &backend in all_backend_types() {
            if !backend.supports_chat() {
                continue;
            }

            if seen_types.contains(backend.as_str()) {
                continue;
            }

            let reason = if backend == BackendType::Ollama {
                std::env::var("OLLAMA_HOST")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
                    .map(|_| "OLLAMA_HOST env var".to_string())
            } else {
                backend.api_key_env_var().and_then(|env_var| {
                    std::env::var(env_var)
                        .ok()
                        .filter(|value| !value.trim().is_empty())
                        .map(|_| format!("{env_var} env var"))
                })
            };

            let Some(reason) = reason else {
                continue;
            };

            let endpoint = if backend == BackendType::Ollama {
                ollama_endpoint_from_env()
            } else {
                backend.default_endpoint().map(str::to_string)
            };

            let provider_config = LlmProviderConfig {
                provider_type: backend,
                endpoint: endpoint.clone(),
                default_model: backend.default_chat_model().map(str::to_string),
                temperature: None,
                max_tokens: None,
                timeout_secs: None,
                api_key: backend
                    .api_key_env_var()
                    .and_then(|env_var| std::env::var(env_var).ok()),
                available_models: None,
                trust_level: None,
            };

            let provider_key = backend.as_str().to_string();
            let models = self.discover_models(&provider_key, &provider_config).await;

            providers.push(ProviderInfo {
                name: format_provider_name(&provider_key, backend),
                provider_type: backend.as_str().to_string(),
                available: !models.is_empty() || backend != BackendType::Ollama,
                default_model: backend.default_chat_model().map(str::to_string),
                models,
                endpoint,
                reason: Some(reason),
                is_local: backend.is_local(),
            });
        }

        providers
    }
}

fn all_backend_types() -> &'static [BackendType] {
    &[
        BackendType::Ollama,
        BackendType::OpenAI,
        BackendType::Anthropic,
        BackendType::Cohere,
        BackendType::VertexAI,
        BackendType::FastEmbed,
        BackendType::Burn,
        BackendType::GitHubCopilot,
        BackendType::OpenRouter,
        BackendType::ZAI,
        BackendType::Custom,
        BackendType::Mock,
    ]
}

fn format_provider_name(key: &str, provider_type: BackendType) -> String {
    let type_label = provider_type_label(provider_type);
    if key.eq_ignore_ascii_case(provider_type.as_str()) {
        type_label.to_string()
    } else {
        format!("{type_label} ({key})")
    }
}

fn provider_type_label(provider_type: BackendType) -> &'static str {
    match provider_type {
        BackendType::Ollama => "Ollama",
        BackendType::OpenAI => "OpenAI",
        BackendType::Anthropic => "Anthropic",
        BackendType::Cohere => "Cohere",
        BackendType::VertexAI => "VertexAI",
        BackendType::GitHubCopilot => "GitHub Copilot",
        BackendType::OpenRouter => "OpenRouter",
        BackendType::ZAI => "Z.AI",
        BackendType::Custom => "Custom",
        BackendType::FastEmbed => "FastEmbed",
        BackendType::Burn => "Burn",
        BackendType::Mock => "Mock",
    }
}

fn ollama_endpoint_from_env() -> Option<String> {
    std::env::var("OLLAMA_HOST").ok().map(|host| {
        if host.starts_with("http://") || host.starts_with("https://") {
            host
        } else {
            format!("http://{host}")
        }
    })
}
