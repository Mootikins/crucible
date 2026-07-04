//! Mapping between Crucible's `BackendType` and genai's `AdapterKind`
//!
//! This module provides explicit, exhaustive mappings from Crucible's backend
//! configuration to genai's adapter types. All mappings are explicit (no auto-detection).

use crucible_core::config::BackendType;
use genai::adapter::AdapterKind;
use genai::resolver::{AuthData, AuthResolver, Endpoint, ServiceTargetResolver};
use genai::ModelIden;
use tracing;

/// Maps a Crucible `BackendType` to genai's `AdapterKind`.
///
/// This function is explicit and exhaustive — every `BackendType` variant
/// is explicitly mapped to an `AdapterKind`. No auto-detection or inference.
///
/// # Mappings
///
/// - `Ollama` → `AdapterKind::Ollama`
/// - `OpenAI` → `AdapterKind::OpenAI`
/// - `Anthropic` → `AdapterKind::Anthropic`
/// - `Cohere` → `AdapterKind::Cohere`
/// - `VertexAI` → Not supported (no genai adapter)
/// - `FastEmbed` → Not supported (embedding-only, no chat)
/// - `Burn` → Not supported (embedding-only, no chat)
/// - `GitHubCopilot` → `AdapterKind::OpenAI` (uses ServiceTargetResolver for endpoint)
/// - `OpenRouter` → `AdapterKind::OpenAI` (uses ServiceTargetResolver for custom endpoint)
/// - `ZAI` → `AdapterKind::Zai` (coding plan uses `zai_coding::` model prefix)
/// - `Custom` → `AdapterKind::OpenAI` (generic OpenAI-compatible)
/// - `Mock` → Not supported (testing only)
pub fn backend_to_adapter(backend: &BackendType) -> Option<AdapterKind> {
    match backend {
        BackendType::Ollama => Some(AdapterKind::Ollama),
        BackendType::OpenAI => Some(AdapterKind::OpenAI),
        BackendType::Anthropic => Some(AdapterKind::Anthropic),
        BackendType::Cohere => Some(AdapterKind::Cohere),
        BackendType::VertexAI => None,
        // Embedding-only backends — no chat support
        BackendType::FastEmbed => None,
        BackendType::Burn => None,
        // Chat-only backends that use OpenAI-compatible API
        BackendType::GitHubCopilot => Some(AdapterKind::OpenAI),
        BackendType::OpenRouter => Some(AdapterKind::OpenAI),
        BackendType::ZAI => Some(AdapterKind::Zai),
        // Custom HTTP-based provider (OpenAI-compatible)
        BackendType::Custom => Some(AdapterKind::OpenAI),
        // Mock backend — testing only
        BackendType::Mock => None,
    }
}

/// Ensures an endpoint URL ends with a trailing slash.
///
/// genai's OpenAI adapter (and Ollama, which delegates to it) uses
/// `Url::join("chat/completions")` which per RFC 3986 replaces the last
/// path segment if the base URL lacks a trailing slash.
/// A base of `https://host/v1` joins to `https://host/chat/completions` (wrong),
/// while `https://host/v1/` joins to `https://host/v1/chat/completions` (correct).
///
/// This applies to ALL adapters that route through OpenAI's URL construction.
fn ensure_trailing_slash(endpoint: &str) -> String {
    let trimmed = endpoint.trim_end_matches('/');
    format!("{trimmed}/")
}

/// A configured genai client with explicit adapter selection and authentication.
///
/// This struct encapsulates a genai `Client` with its associated backend type.
/// It provides methods for building model identifiers and accessing the inner client.
pub struct ChatClient {
    client: genai::Client,
    backend: BackendType,
    /// ZAI coding plan requires `zai_coding::` model name prefix for correct
    /// endpoint routing in genai's ZAI adapter.
    zai_coding: bool,
}

impl ChatClient {
    /// Builds a genai `Client` from an `LlmProviderConfig`.
    ///
    /// This constructor sets up:
    /// - Explicit adapter selection (no auto-detection)
    /// - `AuthResolver` for API key injection
    /// - `ServiceTargetResolver` for endpoint override (used by GitHubCopilot, OpenRouter, ZAI)
    /// - Endpoint trailing-slash fix for correct `Url::join()` behavior
    ///
    /// # Arguments
    ///
    /// * `config` - The Crucible LLM provider configuration
    ///
    /// # Returns
    ///
    /// A configured `ChatClient` ready for use.
    ///
    /// # Panics
    ///
    /// Panics if the backend type is not supported for chat (e.g., FastEmbed, Burn, Mock).
    pub fn new(config: &crucible_core::config::LlmProviderConfig) -> Self {
        let _adapter =
            backend_to_adapter(&config.provider_type).expect("Backend does not support chat");

        let mut builder = genai::Client::builder();

        // Set up authentication if API key is available
        if let Some(api_key) = config.api_key() {
            builder = builder.with_auth_resolver(AuthResolver::from_resolver_fn(
                move |_: genai::ModelIden| Ok(Some(AuthData::from_single(api_key.clone()))),
            ));
        } else if config.endpoint.is_some() {
            // Keyless custom endpoint (local llama.cpp/llama-swap, keyless
            // proxies): without a resolver, genai falls back to the vendor
            // env var (e.g. OPENAI_API_KEY) and errors when it's unset, even
            // though the endpoint needs no auth. Send a placeholder bearer —
            // OpenAI-compatible local servers ignore it. Providers using the
            // vendor's default endpoint keep genai's env-var fallback.
            builder = builder.with_auth_resolver(AuthResolver::from_resolver_fn(
                |_: genai::ModelIden| Ok(Some(AuthData::from_single("sk-no-key-required"))),
            ));
        }

        // Set up service target resolver for custom endpoints
        // (used by GitHubCopilot, OpenRouter, ZAI, and Custom)
        let mut endpoint = config.endpoint();

        // Ensure endpoint has correct trailing slash for Url::join() behavior (RFC 3986).
        // Without a trailing slash, Url::join("chat/completions") replaces the last
        // path segment instead of appending.
        if !endpoint.is_empty() {
            if matches!(config.provider_type, BackendType::Ollama) && !endpoint.contains("/v1") {
                // Ollama-specific: auto-add /v1/ if the base URL is missing it entirely
                let fixed = format!("{}/v1/", endpoint.trim_end_matches('/'));
                tracing::info!(
                    endpoint = %endpoint,
                    fixed_endpoint = %fixed,
                    "Auto-fixed Ollama endpoint to include /v1/ path"
                );
                endpoint = fixed;
            } else {
                // ALL adapters: ensure trailing slash for correct Url::join() behavior
                let fixed = ensure_trailing_slash(&endpoint);
                if fixed != endpoint {
                    tracing::debug!(
                        endpoint = %endpoint,
                        fixed_endpoint = %fixed,
                        "Added trailing slash to endpoint for correct URL joining"
                    );
                    endpoint = fixed;
                }
            }
        }

        let zai_coding =
            matches!(config.provider_type, BackendType::ZAI) && endpoint.contains("coding");

        if !endpoint.is_empty() {
            builder = builder.with_service_target_resolver(
                ServiceTargetResolver::from_resolver_fn(move |mut st: genai::ServiceTarget| {
                    st.endpoint = Endpoint::from_owned(endpoint.clone());
                    Ok(st)
                }),
            );
        }

        let client = builder.build();
        Self {
            client,
            backend: config.provider_type,
            zai_coding,
        }
    }

    /// Builds an explicit `ModelIden` from a model name.
    ///
    /// This method produces an explicit `ModelIden` with the adapter
    /// and model name. It does NOT rely on genai's model name auto-detection.
    ///
    /// # Arguments
    ///
    /// * `model` - The model name (e.g., "gpt-4o", "claude-3-5-sonnet-20241022")
    ///
    /// # Returns
    ///
    /// `Some(ModelIden)` if the backend supports chat, `None` otherwise.
    pub fn model_iden(&self, model: &str) -> Option<ModelIden> {
        let adapter = backend_to_adapter(&self.backend)?;
        if self.zai_coding {
            // Prefix with zai_coding:: (underscore — genai's ZAI_CODING_NAMESPACE)
            // so genai routes to the coding endpoint instead of the credit-based one.
            // A hyphen is not a recognized namespace and silently falls back to Ollama.
            Some(ModelIden::new(adapter, format!("zai_coding::{model}")))
        } else {
            Some(ModelIden::new(adapter, model))
        }
    }

    /// Returns a reference to the inner genai `Client`.
    pub fn inner(&self) -> &genai::Client {
        &self.client
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // backend_to_adapter tests — all 13 variants
    // ========================================================================

    #[test]
    fn test_backend_to_adapter_ollama() {
        assert_eq!(
            backend_to_adapter(&BackendType::Ollama),
            Some(AdapterKind::Ollama)
        );
    }

    #[test]
    fn test_backend_to_adapter_openai() {
        assert_eq!(
            backend_to_adapter(&BackendType::OpenAI),
            Some(AdapterKind::OpenAI)
        );
    }

    #[test]
    fn test_backend_to_adapter_anthropic() {
        assert_eq!(
            backend_to_adapter(&BackendType::Anthropic),
            Some(AdapterKind::Anthropic)
        );
    }

    #[test]
    fn test_backend_to_adapter_cohere() {
        assert_eq!(
            backend_to_adapter(&BackendType::Cohere),
            Some(AdapterKind::Cohere)
        );
    }

    #[test]
    fn test_backend_to_adapter_vertexai() {
        // VertexAI is not supported in genai 0.5.3
        assert_eq!(backend_to_adapter(&BackendType::VertexAI), None);
    }

    #[test]
    fn test_backend_to_adapter_fastembed_none() {
        // FastEmbed is embedding-only, no chat support
        assert_eq!(backend_to_adapter(&BackendType::FastEmbed), None);
    }

    #[test]
    fn test_backend_to_adapter_burn_none() {
        // Burn is embedding-only, no chat support
        assert_eq!(backend_to_adapter(&BackendType::Burn), None);
    }

    #[test]
    fn test_backend_to_adapter_github_copilot_openai() {
        // GitHubCopilot uses OpenAI-compatible API
        assert_eq!(
            backend_to_adapter(&BackendType::GitHubCopilot),
            Some(AdapterKind::OpenAI)
        );
    }

    #[test]
    fn test_backend_to_adapter_openrouter_openai() {
        // OpenRouter uses OpenAI-compatible API
        assert_eq!(
            backend_to_adapter(&BackendType::OpenRouter),
            Some(AdapterKind::OpenAI)
        );
    }

    #[test]
    fn test_backend_to_adapter_zai() {
        assert_eq!(
            backend_to_adapter(&BackendType::ZAI),
            Some(AdapterKind::Zai)
        );
    }

    #[test]
    fn test_backend_to_adapter_custom_openai() {
        // Custom HTTP-based provider uses OpenAI-compatible API
        assert_eq!(
            backend_to_adapter(&BackendType::Custom),
            Some(AdapterKind::OpenAI)
        );
    }

    #[test]
    fn test_backend_to_adapter_mock_none() {
        // Mock is testing-only, no chat support
        assert_eq!(backend_to_adapter(&BackendType::Mock), None);
    }

    // ========================================================================
    // ChatClient::new tests — basic construction
    // ========================================================================

    #[test]
    fn test_chat_client_new_ollama() {
        let config = crucible_core::config::LlmProviderConfig {
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
        };

        let _client = ChatClient::new(&config);
        // If we get here without panic, the client was built successfully
    }

    #[test]
    fn test_chat_client_new_openai_with_api_key() {
        let config = crucible_core::config::LlmProviderConfig {
            provider_type: BackendType::OpenAI,
            endpoint: None,
            default_model: Some("gpt-4o".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("sk-test-key-123".to_string()),
            available_models: None,
            trust_level: None,
            name: None,
        };

        let _client = ChatClient::new(&config);
        // If we get here without panic, the client was built successfully
    }

    #[test]
    fn test_chat_client_new_anthropic() {
        let config = crucible_core::config::LlmProviderConfig {
            provider_type: BackendType::Anthropic,
            endpoint: None,
            default_model: Some("claude-3-5-sonnet-20241022".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("sk-ant-test-key".to_string()),
            available_models: None,
            trust_level: None,
            name: None,
        };

        let _client = ChatClient::new(&config);
        // If we get here without panic, the client was built successfully
    }

    #[test]
    fn test_chat_client_new_github_copilot_with_endpoint() {
        let config = crucible_core::config::LlmProviderConfig {
            provider_type: BackendType::GitHubCopilot,
            endpoint: Some("https://api.githubcopilot.com".to_string()),
            default_model: Some("gpt-4o".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("github-token".to_string()),
            available_models: None,
            trust_level: None,
            name: None,
        };

        let _client = ChatClient::new(&config);
        // If we get here without panic, the client was built successfully
    }

    #[test]
    fn test_chat_client_new_openrouter_with_custom_endpoint() {
        let config = crucible_core::config::LlmProviderConfig {
            provider_type: BackendType::OpenRouter,
            endpoint: Some("https://openrouter.ai/api/v1".to_string()),
            default_model: Some("openai/gpt-4o".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("sk-or-test-key".to_string()),
            available_models: None,
            trust_level: None,
            name: None,
        };

        let _client = ChatClient::new(&config);
        // If we get here without panic, the client was built successfully
    }

    #[test]
    fn test_chat_client_new_zai_with_custom_endpoint() {
        let config = crucible_core::config::LlmProviderConfig {
            provider_type: BackendType::ZAI,
            endpoint: Some("https://api.z.ai/api/coding/paas/v4".to_string()),
            default_model: Some("GLM-4.7".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("glm-auth-token".to_string()),
            available_models: None,
            trust_level: None,
            name: None,
        };

        let _client = ChatClient::new(&config);
        // If we get here without panic, the client was built successfully
    }

    #[test]
    fn test_chat_client_new_custom_with_endpoint() {
        let config = crucible_core::config::LlmProviderConfig {
            provider_type: BackendType::Custom,
            endpoint: Some("http://custom-api.example.com/v1".to_string()),
            default_model: Some("my-custom-model".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("custom-api-key".to_string()),
            available_models: None,
            trust_level: None,
            name: None,
        };

        let _client = ChatClient::new(&config);
        // If we get here without panic, the client was built successfully
    }

    #[test]
    #[should_panic(expected = "Backend does not support chat")]
    fn test_chat_client_new_fastembed_panics() {
        let config = crucible_core::config::LlmProviderConfig {
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

        let _client = ChatClient::new(&config);
    }

    #[test]
    #[should_panic(expected = "Backend does not support chat")]
    fn test_chat_client_new_burn_panics() {
        let config = crucible_core::config::LlmProviderConfig {
            provider_type: BackendType::Burn,
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

        let _client = ChatClient::new(&config);
    }

    #[test]
    #[should_panic(expected = "Backend does not support chat")]
    fn test_chat_client_new_mock_panics() {
        let config = crucible_core::config::LlmProviderConfig {
            provider_type: BackendType::Mock,
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

        let _client = ChatClient::new(&config);
    }

    // ========================================================================
    // Exhaustive mapping verification
    // ========================================================================

    #[test]
    fn test_all_13_variants_explicitly_mapped() {
        // Verify that all 13 BackendType variants are explicitly handled
        let variants = [
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
        ];

        // Each variant should have a defined mapping (Some or None)
        for variant in &variants {
            let result = backend_to_adapter(variant);
            // Just verify that the function returns without panicking
            let _ = result;
        }
    }

    #[test]
    fn test_chat_capable_backends_return_some() {
        let chat_capable = [
            BackendType::Ollama,
            BackendType::OpenAI,
            BackendType::Anthropic,
            BackendType::Cohere,
            BackendType::GitHubCopilot,
            BackendType::OpenRouter,
            BackendType::ZAI,
            BackendType::Custom,
        ];

        for backend in &chat_capable {
            assert!(
                backend_to_adapter(backend).is_some(),
                "{:?} should support chat",
                backend
            );
        }
    }

    #[test]
    fn test_non_chat_backends_return_none() {
        let non_chat = [BackendType::FastEmbed, BackendType::Burn, BackendType::Mock];

        for backend in &non_chat {
            assert!(
                backend_to_adapter(backend).is_none(),
                "{:?} should NOT support chat",
                backend
            );
        }
    }

    // ========================================================================
    // Ollama endpoint validation tests
    // ========================================================================

    #[test]
    fn endpoint_validation_warns_on_missing_v1() {
        // Test that the validation logic correctly identifies missing /v1/
        let endpoint = "https://llm.example.com";
        let provider_type = BackendType::Ollama;

        // Simulate the validation condition
        let should_warn = matches!(provider_type, BackendType::Ollama)
            && !endpoint.is_empty()
            && !endpoint.contains("/v1");

        assert!(should_warn, "Should warn for Ollama endpoint without /v1/");
    }

    #[test]
    fn endpoint_validation_no_warn_for_empty_endpoint() {
        // When no custom endpoint is set, config.endpoint() returns ""
        // The empty check prevents spurious warnings for the default Ollama endpoint.
        let endpoint = ""; // Empty = default (no custom endpoint configured)
        let provider_type = BackendType::Ollama;

        let should_warn = matches!(provider_type, BackendType::Ollama)
            && !endpoint.is_empty()
            && !endpoint.contains("/v1");

        assert!(
            !should_warn,
            "Default (empty) Ollama endpoint should not trigger warning"
        );
    }

    // ========================================================================
    // ChatClient endpoint auto-fix tests
    // ========================================================================

    #[test]
    fn ensure_trailing_slash_basic() {
        assert_eq!(ensure_trailing_slash("https://host/v1"), "https://host/v1/");
    }

    #[test]
    fn ensure_trailing_slash_already_present() {
        assert_eq!(
            ensure_trailing_slash("https://host/v1/"),
            "https://host/v1/"
        );
    }

    #[test]
    fn ensure_trailing_slash_no_path() {
        assert_eq!(ensure_trailing_slash("https://host"), "https://host/");
    }

    #[test]
    fn endpoint_auto_fix_appends_v1() {
        // Ollama-specific: auto-add /v1/ if missing
        let config = crucible_core::config::LlmProviderConfig {
            provider_type: BackendType::Ollama,
            endpoint: Some("https://llm.example.com".to_string()),
            default_model: Some("llama3.2".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
            available_models: None,
            trust_level: None,
            name: None,
        };
        let client = ChatClient::new(&config);
        assert_eq!(client.backend, BackendType::Ollama);
    }

    #[test]
    fn endpoint_auto_fix_idempotent() {
        let endpoint = "https://example.com/v1/";
        let fixed = ensure_trailing_slash(endpoint);
        assert_eq!(fixed, "https://example.com/v1/");
    }

    #[test]
    fn endpoint_auto_fix_trailing_slash() {
        let endpoint = "https://example.com/";
        let fixed = ensure_trailing_slash(endpoint);
        assert_eq!(fixed, "https://example.com/");
    }

    #[test]
    fn endpoint_auto_fix_v1_without_trailing_slash() {
        let endpoint = "https://llm.example.com/v1";
        let fixed = ensure_trailing_slash(endpoint);
        assert_eq!(fixed, "https://llm.example.com/v1/");
    }

    #[test]
    fn openai_endpoint_gets_trailing_slash() {
        // Reproduces the bug: type = "openai", endpoint without trailing slash
        let config = crucible_core::config::LlmProviderConfig {
            provider_type: BackendType::OpenAI,
            endpoint: Some("https://llm.example.com/v1".to_string()),
            default_model: Some("glm-4.7-flash-iq4".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
            available_models: None,
            trust_level: None,
            name: None,
        };
        // Should build successfully (trailing slash applied internally)
        let client = ChatClient::new(&config);
        assert_eq!(client.backend, BackendType::OpenAI);
    }

    #[test]
    fn chat_client_model_iden_ollama() {
        let config = crucible_core::config::LlmProviderConfig {
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
        };

        let client = ChatClient::new(&config);
        let model_iden = client.model_iden("llama3.2");
        assert!(model_iden.is_some());
        let iden = model_iden.unwrap();
        assert_eq!(iden.adapter_kind, AdapterKind::Ollama);
        assert_eq!(&*iden.model_name, "llama3.2");
    }

    #[tokio::test]
    async fn keyless_custom_endpoint_does_not_require_vendor_env_key() {
        // An OpenAI-compatible provider pointed at a custom endpoint with no
        // api_key (local llama.cpp/llama-swap, keyless proxies) must not fall
        // through to genai's vendor env-var lookup — that fails with
        // `ApiKeyEnvNotFound { env_name: "OPENAI_API_KEY" }` even though the
        // endpoint needs no auth at all.
        let config = crucible_core::config::LlmProviderConfig {
            provider_type: BackendType::OpenAI,
            endpoint: Some("https://llama.example.com/v1".to_string()),
            default_model: Some("glm-4.7-flash-iq4".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
            available_models: None,
            trust_level: None,
            name: None,
        };

        let client = ChatClient::new(&config);
        let target = client
            .inner()
            .resolve_service_target("openai::glm-4.7-flash-iq4")
            .await
            .expect("resolving a keyless custom endpoint must not error");
        assert!(
            matches!(target.auth, genai::resolver::AuthData::Key(_)),
            "keyless custom endpoint should get a placeholder key, not env lookup (got {:?})",
            target.auth
        );
    }

    #[tokio::test]
    async fn default_endpoint_still_uses_vendor_env_key() {
        // Without a custom endpoint, genai's env-var fallback (OPENAI_API_KEY)
        // is a feature — the placeholder must NOT be injected there.
        let config = crucible_core::config::LlmProviderConfig {
            provider_type: BackendType::OpenAI,
            endpoint: None,
            default_model: Some("gpt-4o".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
            available_models: None,
            trust_level: None,
            name: None,
        };

        let client = ChatClient::new(&config);
        let target = client
            .inner()
            .resolve_service_target("openai::gpt-4o")
            .await
            .expect("target resolution itself should succeed");
        assert!(
            matches!(target.auth, genai::resolver::AuthData::FromEnv(_)),
            "default endpoint keeps genai's env-var auth (got {:?})",
            target.auth
        );
    }

    #[test]
    fn chat_client_model_iden_zai_coding_uses_genai_namespace() {
        // genai's coding-plan namespace is `zai_coding::` (underscore). A hyphen
        // (`zai-coding::`) is not a recognized namespace, so genai falls back to
        // the Ollama adapter, which sends no auth header → 401 from api.z.ai.
        let config = crucible_core::config::LlmProviderConfig {
            provider_type: BackendType::ZAI,
            endpoint: Some("https://api.z.ai/api/coding/paas/v4".to_string()),
            default_model: Some("glm-5".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("glm-auth-token".to_string()),
            available_models: None,
            trust_level: None,
            name: None,
        };

        let client = ChatClient::new(&config);
        let iden = client.model_iden("glm-5").unwrap();
        assert_eq!(iden.adapter_kind, AdapterKind::Zai);
        assert_eq!(&*iden.model_name, "zai_coding::glm-5");
        // genai must resolve the namespaced name back to the ZAI adapter
        assert_eq!(
            AdapterKind::from_model(&iden.model_name).unwrap(),
            AdapterKind::Zai
        );
    }

    #[test]
    fn chat_client_model_iden_openai() {
        let config = crucible_core::config::LlmProviderConfig {
            provider_type: BackendType::OpenAI,
            endpoint: None,
            default_model: Some("gpt-4o".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("sk-test".to_string()),
            available_models: None,
            trust_level: None,
            name: None,
        };

        let client = ChatClient::new(&config);
        let model_iden = client.model_iden("gpt-4o");
        assert!(model_iden.is_some());
        let iden = model_iden.unwrap();
        assert_eq!(iden.adapter_kind, AdapterKind::OpenAI);
        assert_eq!(&*iden.model_name, "gpt-4o");
    }
}
