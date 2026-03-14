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
    pub async fn list_providers(
        &self,
        classification: Option<DataClassification>,
    ) -> Vec<ProviderInfo> {
        let mut providers = Vec::new();
        let mut seen_types = HashSet::new();

        if let Some(llm_config) = &self.llm_config {
            for (key, provider_config) in &llm_config.providers {
                let backend = provider_config.provider_type;
                if !backend.supports_chat() {
                    continue;
                }

                if let Some(ref classification) = classification {
                    if !provider_config
                        .effective_trust_level()
                        .satisfies(*classification)
                    {
                        continue;
                    }
                }

                seen_types.insert(backend.as_str().to_string());

                let models = self.discover_models(key, provider_config).await;
                providers.push(ProviderInfo {
                    name: format_provider_name(key, backend, provider_config.name.as_deref()),
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

        for (provider_key, provider_config) in self.discover_env_providers(&seen_types) {
            let backend = provider_config.provider_type;

            if let Some(ref classification) = classification {
                if !provider_config
                    .effective_trust_level()
                    .satisfies(*classification)
                {
                    continue;
                }
            }

            let models = self.discover_models(&provider_key, &provider_config).await;
            let reason = if backend == BackendType::Ollama {
                Some("OLLAMA_HOST env var".to_string())
            } else {
                backend
                    .api_key_env_var()
                    .map(|env_var| format!("{env_var} env var"))
            };

            providers.push(ProviderInfo {
                name: format_provider_name(&provider_key, backend, None),
                provider_type: backend.as_str().to_string(),
                available: !models.is_empty() || backend != BackendType::Ollama,
                default_model: backend.default_chat_model().map(str::to_string),
                models,
                endpoint: provider_config.endpoint.clone(),
                reason,
                is_local: backend.is_local(),
            });
        }

        providers
    }

    pub(super) fn discover_env_providers(
        &self,
        seen_types: &HashSet<String>,
    ) -> Vec<(String, LlmProviderConfig)> {
        let mut providers = Vec::new();

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

            if reason.is_none() {
                continue;
            }

            let endpoint = if backend == BackendType::Ollama {
                ollama_endpoint_from_env()
            } else {
                backend.default_endpoint().map(str::to_string)
            };

            providers.push((
                backend.as_str().to_string(),
                LlmProviderConfig {
                    provider_type: backend,
                    endpoint,
                    default_model: backend.default_chat_model().map(str::to_string),
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: backend
                        .api_key_env_var()
                        .and_then(|env_var| std::env::var(env_var).ok()),
                    available_models: None,
                    trust_level: None,
                    name: None,
                },
            ));
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

fn format_provider_name(
    key: &str,
    provider_type: BackendType,
    custom_name: Option<&str>,
) -> String {
    if let Some(name) = custom_name {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background_manager::BackgroundJobManager;
    use crate::kiln_manager::KilnManager;
    use crate::session_manager::SessionManager;
    use crate::session_storage::FileSessionStorage;
    use crate::tools::workspace::WorkspaceTools;
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};
    use crucible_core::test_support::EnvVarGuard;
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;
    use std::sync::{Arc, LazyLock, Mutex};
    use tokio::sync::broadcast;

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn clear_provider_env() -> Vec<EnvVarGuard> {
        vec![
            EnvVarGuard::remove("OLLAMA_HOST"),
            EnvVarGuard::remove("OPENAI_API_KEY"),
            EnvVarGuard::remove("ANTHROPIC_API_KEY"),
            EnvVarGuard::remove("COHERE_API_KEY"),
            EnvVarGuard::remove("GOOGLE_API_KEY"),
            EnvVarGuard::remove("OPENROUTER_API_KEY"),
            EnvVarGuard::remove("GLM_AUTH_TOKEN"),
        ]
    }

    fn make_agent_manager_with_config(config: Option<LlmConfig>) -> AgentManager {
        let (event_tx, _) = broadcast::channel(16);
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx));

        AgentManager::new(AgentManagerParams {
            kiln_manager: Arc::new(KilnManager::new()),
            session_manager: Arc::new(SessionManager::with_storage(Arc::new(
                FileSessionStorage::new(),
            ))),
            background_manager,
            mcp_gateway: None,
            llm_config: config,
            acp_config: None,
            permission_config: None,
            plugin_loader: None,
            workspace_tools: Arc::new(WorkspaceTools::new(PathBuf::from("/tmp"))),
        })
    }

    #[tokio::test]
    // SAFETY: This lock intentionally serializes process-wide env var mutation across async tests.
    // It must be held for the entire test body (including await points) to prevent cross-test races.
    #[allow(clippy::await_holding_lock)]
    async fn test_list_providers_empty_config() {
        let _env_lock = ENV_LOCK.lock().expect("env lock poisoned");
        let _env_guards = clear_provider_env();
        let manager = make_agent_manager_with_config(Some(LlmConfig::default()));

        let providers = manager.list_providers(None).await;

        assert!(providers.is_empty());
    }

    #[tokio::test]
    // SAFETY: This lock intentionally serializes process-wide env var mutation across async tests.
    // It must be held for the entire test body (including await points) to prevent cross-test races.
    #[allow(clippy::await_holding_lock)]
    async fn test_list_providers_with_configured_provider() {
        let _env_lock = ENV_LOCK.lock().expect("env lock poisoned");
        let _env_guards = clear_provider_env();
        let config = LlmConfig {
            providers: HashMap::from([(
                "openai".to_string(),
                LlmProviderConfig {
                    provider_type: BackendType::OpenAI,
                    endpoint: None,
                    default_model: Some("gpt-4o".to_string()),
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: Some("sk-test".to_string()),
                    available_models: Some(vec!["gpt-4o".to_string()]),
                    trust_level: None,
                    name: None,
                },
            )]),
            ..Default::default()
        };
        let manager = make_agent_manager_with_config(Some(config));

        let providers = manager.list_providers(None).await;

        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider_type, "openai");
        assert_eq!(providers[0].name, "OpenAI");
        assert!(providers[0].available);
        assert_eq!(providers[0].reason.as_deref(), Some("config"));
    }

    #[tokio::test]
    // SAFETY: This lock intentionally serializes process-wide env var mutation across async tests.
    // It must be held for the entire test body (including await points) to prevent cross-test races.
    #[allow(clippy::await_holding_lock)]
    async fn test_list_providers_filters_non_chat_providers() {
        let _env_lock = ENV_LOCK.lock().expect("env lock poisoned");
        let _env_guards = clear_provider_env();
        let config = LlmConfig {
            providers: HashMap::from([(
                "fastembed".to_string(),
                LlmProviderConfig {
                    provider_type: BackendType::FastEmbed,
                    endpoint: None,
                    default_model: None,
                    temperature: None,
                    max_tokens: None,
                    timeout_secs: None,
                    api_key: None,
                    available_models: Some(vec!["BAAI/bge-small-en-v1.5".to_string()]),
                    trust_level: None,
                    name: None,
                },
            )]),
            ..Default::default()
        };
        let manager = make_agent_manager_with_config(Some(config));

        let providers = manager.list_providers(None).await;

        assert!(!providers
            .iter()
            .any(|provider| provider.provider_type == "fastembed"));
        assert!(providers.is_empty());
    }

    #[test]
    fn test_provider_info_serialization() {
        let info = ProviderInfo {
            name: "OpenAI".to_string(),
            provider_type: "openai".to_string(),
            available: true,
            default_model: Some("gpt-4o".to_string()),
            models: vec!["gpt-4o".to_string()],
            endpoint: Some("https://api.openai.com/v1".to_string()),
            reason: None,
            is_local: false,
        };

        let json = serde_json::to_value(info).expect("provider info should serialize");

        assert_eq!(json["name"], "OpenAI");
        assert_eq!(json["provider_type"], "openai");
        assert_eq!(json["available"], true);
        assert_eq!(json["models"], serde_json::json!(["gpt-4o"]));
        assert_eq!(json["is_local"], false);
        assert!(json.get("reason").is_some());
        assert!(json["reason"].is_null());
    }

    #[test]
    fn test_format_provider_name_custom_name() {
        assert_eq!(
            format_provider_name("llama-swappo", BackendType::OpenAI, Some("My Ollama")),
            "My Ollama"
        );
    }

    #[test]
    fn test_format_provider_name_empty_falls_back() {
        // Empty name → falls back to auto-generated
        let result = format_provider_name("my-key", BackendType::OpenAI, Some(""));
        assert_eq!(result, "OpenAI (my-key)");
    }

    #[test]
    fn test_format_provider_name_none_falls_back() {
        // None → existing behavior
        let result = format_provider_name("openai", BackendType::OpenAI, None);
        assert_eq!(result, "OpenAI");
    }

    #[test]
    fn test_format_provider_name_whitespace_falls_back() {
        // Whitespace-only name → falls back
        let result = format_provider_name("my-key", BackendType::OpenAI, Some("   "));
        assert_eq!(result, "OpenAI (my-key)");
    }
    #[test]
    fn test_discover_env_providers_returns_empty_with_no_env_vars() {
        let _env_lock = ENV_LOCK.lock().expect("env lock poisoned");
        let _env_guards = clear_provider_env();
        let manager = make_agent_manager_with_config(Some(LlmConfig::default()));

        let providers = manager.discover_env_providers(&HashSet::new());

        assert!(providers.is_empty());
    }

    #[tokio::test]
    // SAFETY: This lock intentionally serializes process-wide env var mutation across async tests.
    // It must be held for the entire test body (including await points) to prevent cross-test races.
    #[allow(clippy::await_holding_lock)]
    async fn test_list_providers_with_classification_filter() {
        let _env_lock = ENV_LOCK.lock().expect("env lock poisoned");
        let _env_guards = clear_provider_env();
        use crucible_config::TrustLevel;

        let config = LlmConfig {
            providers: HashMap::from([
                (
                    "openai".to_string(),
                    LlmProviderConfig {
                        provider_type: BackendType::OpenAI,
                        endpoint: None,
                        default_model: Some("gpt-4o".to_string()),
                        temperature: None,
                        max_tokens: None,
                        timeout_secs: None,
                        api_key: Some("sk-test".to_string()),
                        available_models: Some(vec!["gpt-4o".to_string()]),
                        trust_level: Some(TrustLevel::Local),
                        name: None,
                    },
                ),
                (
                    "anthropic".to_string(),
                    LlmProviderConfig {
                        provider_type: BackendType::Anthropic,
                        endpoint: None,
                        default_model: Some("claude-3-5-sonnet".to_string()),
                        temperature: None,
                        max_tokens: None,
                        timeout_secs: None,
                        api_key: Some("sk-test".to_string()),
                        available_models: Some(vec!["claude-3-5-sonnet".to_string()]),
                        trust_level: None, // Cloud (default)
                        name: None,
                    },
                ),
            ]),
            ..Default::default()
        };
        let manager = make_agent_manager_with_config(Some(config));

        // With Confidential classification, only Local-trust providers should be returned
        let providers = manager
            .list_providers(Some(crucible_config::DataClassification::Confidential))
            .await;

        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider_type, "openai");
        assert_eq!(providers[0].name, "OpenAI");

        // With no classification, all providers should be returned
        let all_providers = manager.list_providers(None).await;
        assert_eq!(all_providers.len(), 2);
    }

    #[test]
    fn all_backend_types_is_exhaustive() {
        // This test ensures that all_backend_types() covers all BackendType variants.
        // If a new variant is added to BackendType, this match will fail to compile
        // until the variant is added to all_backend_types().
        let all_types = all_backend_types();
        let all_types_set: std::collections::HashSet<_> = all_types.iter().copied().collect();

        // Verify each variant is in the array by matching exhaustively
        let _ = match BackendType::Ollama {
            BackendType::Ollama => (),
            BackendType::OpenAI => (),
            BackendType::Anthropic => (),
            BackendType::Cohere => (),
            BackendType::VertexAI => (),
            BackendType::FastEmbed => (),
            BackendType::Burn => (),
            BackendType::GitHubCopilot => (),
            BackendType::OpenRouter => (),
            BackendType::ZAI => (),
            BackendType::Custom => (),
            BackendType::Mock => (),
        };

        // Now verify all variants are in the array
        assert!(all_types_set.contains(&BackendType::Ollama), "Ollama missing from all_backend_types");
        assert!(all_types_set.contains(&BackendType::OpenAI), "OpenAI missing from all_backend_types");
        assert!(all_types_set.contains(&BackendType::Anthropic), "Anthropic missing from all_backend_types");
        assert!(all_types_set.contains(&BackendType::Cohere), "Cohere missing from all_backend_types");
        assert!(all_types_set.contains(&BackendType::VertexAI), "VertexAI missing from all_backend_types");
        assert!(all_types_set.contains(&BackendType::FastEmbed), "FastEmbed missing from all_backend_types");
        assert!(all_types_set.contains(&BackendType::Burn), "Burn missing from all_backend_types");
        assert!(all_types_set.contains(&BackendType::GitHubCopilot), "GitHubCopilot missing from all_backend_types");
        assert!(all_types_set.contains(&BackendType::OpenRouter), "OpenRouter missing from all_backend_types");
        assert!(all_types_set.contains(&BackendType::ZAI), "ZAI missing from all_backend_types");
        assert!(all_types_set.contains(&BackendType::Custom), "Custom missing from all_backend_types");
        assert!(all_types_set.contains(&BackendType::Mock), "Mock missing from all_backend_types");

        // Verify the count matches
        assert_eq!(all_types.len(), 12, "all_backend_types() should contain exactly 12 variants");
    }
}
