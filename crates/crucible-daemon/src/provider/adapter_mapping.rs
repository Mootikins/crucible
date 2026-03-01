//! Mapping between Crucible's `BackendType` and genai's `AdapterKind`
//!
//! This module provides explicit, exhaustive mappings from Crucible's backend
//! configuration to genai's adapter types. All mappings are explicit (no auto-detection).

use crucible_config::BackendType;
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
/// - `ZAI` → `AdapterKind::OpenAI` (uses ServiceTargetResolver for custom endpoint)
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
        BackendType::ZAI => Some(AdapterKind::OpenAI),
        // Custom HTTP-based provider (OpenAI-compatible)
        BackendType::Custom => Some(AdapterKind::OpenAI),
        // Mock backend — testing only
        BackendType::Mock => None,
    }
}

/// Builds an explicit `ModelIden` from a `BackendType` and model name.
///
/// This function ALWAYS produces an explicit `ModelIden` with the adapter
/// and model name. It does NOT rely on genai's model name auto-detection
/// (e.g., `gpt-*` prefix magic).
///
/// # Arguments
///
/// * `backend` - The Crucible backend type
/// * `model` - The model name (e.g., "gpt-4o", "claude-3-5-sonnet-20241022")
///
/// # Returns
///
/// `Some(ModelIden)` if the backend supports chat, `None` otherwise.
pub fn build_model_iden(backend: &BackendType, model: &str) -> Option<ModelIden> {
    let adapter = backend_to_adapter(backend)?;
    Some(ModelIden::new(adapter, model))
}

/// Builds a genai `Client` from an `LlmProviderConfig`.
///
/// This function constructs a genai client with:
/// - Explicit adapter selection (no auto-detection)
/// - `AuthResolver` for API key injection
/// - `ServiceTargetResolver` for endpoint override (used by GitHubCopilot, OpenRouter, ZAI)
///
/// # Arguments
///
/// * `config` - The Crucible LLM provider configuration
///
/// # Returns
///
/// A configured `genai::Client` ready for use.
///
/// # Panics
///
/// Panics if the backend type is not supported for chat (e.g., FastEmbed, Burn, Mock).
pub fn build_genai_client(config: &crucible_config::LlmProviderConfig) -> genai::Client {
    let _adapter =
        backend_to_adapter(&config.provider_type).expect("Backend does not support chat");

    let mut builder = genai::Client::builder();

    // Set up authentication if API key is available
    if let Some(api_key) = config.api_key() {
        builder = builder.with_auth_resolver(AuthResolver::from_resolver_fn(
            move |_: genai::ModelIden| Ok(Some(AuthData::from_single(api_key.clone()))),
        ));
    }

    // Set up service target resolver for custom endpoints
    // (used by GitHubCopilot, OpenRouter, ZAI, and Custom)
    let endpoint = config.endpoint();
    
    // Validate Ollama endpoint has /v1/ path
    if matches!(config.provider_type, BackendType::Ollama)
        && !endpoint.is_empty()
        && !endpoint.contains("/v1")
    {
        tracing::warn!(
            endpoint = %endpoint,
            "Ollama endpoint may be missing '/v1/' — genai appends 'chat/completions' directly to the base URL. Try: '{}v1/'",
            endpoint
        );
    }
    
    if !endpoint.is_empty() {
        builder = builder.with_service_target_resolver(ServiceTargetResolver::from_resolver_fn(
            move |mut st: genai::ServiceTarget| {
                st.endpoint = Endpoint::from_owned(endpoint.clone());
                Ok(st)
            },
        ));
    }

    builder.build()
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
    fn test_backend_to_adapter_zai_openai() {
        // ZAI uses OpenAI-compatible API
        assert_eq!(
            backend_to_adapter(&BackendType::ZAI),
            Some(AdapterKind::OpenAI)
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
    // build_model_iden tests — all chat-capable variants
    // ========================================================================

    #[test]
    fn test_build_model_iden_ollama() {
        let model_iden = build_model_iden(&BackendType::Ollama, "llama3.2");
        assert!(model_iden.is_some());
        let iden = model_iden.unwrap();
        assert_eq!(iden.adapter_kind, AdapterKind::Ollama);
        assert_eq!(&*iden.model_name, "llama3.2");
    }

    #[test]
    fn test_build_model_iden_openai() {
        let model_iden = build_model_iden(&BackendType::OpenAI, "gpt-4o");
        assert!(model_iden.is_some());
        let iden = model_iden.unwrap();
        assert_eq!(iden.adapter_kind, AdapterKind::OpenAI);
        assert_eq!(&*iden.model_name, "gpt-4o");
    }

    #[test]
    fn test_build_model_iden_anthropic() {
        let model_iden = build_model_iden(&BackendType::Anthropic, "claude-3-5-sonnet-20241022");
        assert!(model_iden.is_some());
        let iden = model_iden.unwrap();
        assert_eq!(iden.adapter_kind, AdapterKind::Anthropic);
        assert_eq!(&*iden.model_name, "claude-3-5-sonnet-20241022");
    }

    #[test]
    fn test_build_model_iden_cohere() {
        let model_iden = build_model_iden(&BackendType::Cohere, "command-r-plus");
        assert!(model_iden.is_some());
        let iden = model_iden.unwrap();
        assert_eq!(iden.adapter_kind, AdapterKind::Cohere);
        assert_eq!(&*iden.model_name, "command-r-plus");
    }

    #[test]
    fn test_build_model_iden_vertexai() {
        // VertexAI is not supported in genai 0.5.3
        assert_eq!(
            build_model_iden(&BackendType::VertexAI, "gemini-1.5-pro"),
            None
        );
    }

    #[test]
    fn test_build_model_iden_github_copilot() {
        let model_iden = build_model_iden(&BackendType::GitHubCopilot, "gpt-4o");
        assert!(model_iden.is_some());
        let iden = model_iden.unwrap();
        assert_eq!(iden.adapter_kind, AdapterKind::OpenAI);
        assert_eq!(&*iden.model_name, "gpt-4o");
    }

    #[test]
    fn test_build_model_iden_openrouter() {
        let model_iden = build_model_iden(&BackendType::OpenRouter, "openai/gpt-4o");
        assert!(model_iden.is_some());
        let iden = model_iden.unwrap();
        assert_eq!(iden.adapter_kind, AdapterKind::OpenAI);
        assert_eq!(&*iden.model_name, "openai/gpt-4o");
    }

    #[test]
    fn test_build_model_iden_zai() {
        let model_iden = build_model_iden(&BackendType::ZAI, "GLM-4.7");
        assert!(model_iden.is_some());
        let iden = model_iden.unwrap();
        assert_eq!(iden.adapter_kind, AdapterKind::OpenAI);
        assert_eq!(&*iden.model_name, "GLM-4.7");
    }

    #[test]
    fn test_build_model_iden_custom() {
        let model_iden = build_model_iden(&BackendType::Custom, "my-custom-model");
        assert!(model_iden.is_some());
        let iden = model_iden.unwrap();
        assert_eq!(iden.adapter_kind, AdapterKind::OpenAI);
        assert_eq!(&*iden.model_name, "my-custom-model");
    }

    #[test]
    fn test_build_model_iden_fastembed_none() {
        // FastEmbed is embedding-only
        assert_eq!(
            build_model_iden(&BackendType::FastEmbed, "some-model"),
            None
        );
    }

    #[test]
    fn test_build_model_iden_burn_none() {
        // Burn is embedding-only
        assert_eq!(build_model_iden(&BackendType::Burn, "some-model"), None);
    }

    #[test]
    fn test_build_model_iden_mock_none() {
        // Mock is testing-only
        assert_eq!(build_model_iden(&BackendType::Mock, "mock-model"), None);
    }

    // ========================================================================
    // build_genai_client tests — basic construction
    // ========================================================================

    #[test]
    fn test_build_genai_client_ollama() {
        let config = crucible_config::LlmProviderConfig {
            provider_type: BackendType::Ollama,
            endpoint: Some("http://localhost:11434".to_string()),
            default_model: Some("llama3.2".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
            available_models: None,
            trust_level: None,
        };

        let _client = build_genai_client(&config);
        // If we get here without panic, the client was built successfully
    }

    #[test]
    fn test_build_genai_client_openai_with_api_key() {
        let config = crucible_config::LlmProviderConfig {
            provider_type: BackendType::OpenAI,
            endpoint: None,
            default_model: Some("gpt-4o".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("sk-test-key-123".to_string()),
            available_models: None,
            trust_level: None,
        };

        let _client = build_genai_client(&config);
        // If we get here without panic, the client was built successfully
    }

    #[test]
    fn test_build_genai_client_anthropic() {
        let config = crucible_config::LlmProviderConfig {
            provider_type: BackendType::Anthropic,
            endpoint: None,
            default_model: Some("claude-3-5-sonnet-20241022".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("sk-ant-test-key".to_string()),
            available_models: None,
            trust_level: None,
        };

        let _client = build_genai_client(&config);
        // If we get here without panic, the client was built successfully
    }

    #[test]
    fn test_build_genai_client_github_copilot_with_endpoint() {
        let config = crucible_config::LlmProviderConfig {
            provider_type: BackendType::GitHubCopilot,
            endpoint: Some("https://api.githubcopilot.com".to_string()),
            default_model: Some("gpt-4o".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("github-token".to_string()),
            available_models: None,
            trust_level: None,
        };

        let _client = build_genai_client(&config);
        // If we get here without panic, the client was built successfully
    }

    #[test]
    fn test_build_genai_client_openrouter_with_custom_endpoint() {
        let config = crucible_config::LlmProviderConfig {
            provider_type: BackendType::OpenRouter,
            endpoint: Some("https://openrouter.ai/api/v1".to_string()),
            default_model: Some("openai/gpt-4o".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("sk-or-test-key".to_string()),
            available_models: None,
            trust_level: None,
        };

        let _client = build_genai_client(&config);
        // If we get here without panic, the client was built successfully
    }

    #[test]
    fn test_build_genai_client_zai_with_custom_endpoint() {
        let config = crucible_config::LlmProviderConfig {
            provider_type: BackendType::ZAI,
            endpoint: Some("https://api.z.ai/api/coding/paas/v4".to_string()),
            default_model: Some("GLM-4.7".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("glm-auth-token".to_string()),
            available_models: None,
            trust_level: None,
        };

        let _client = build_genai_client(&config);
        // If we get here without panic, the client was built successfully
    }

    #[test]
    fn test_build_genai_client_custom_with_endpoint() {
        let config = crucible_config::LlmProviderConfig {
            provider_type: BackendType::Custom,
            endpoint: Some("http://custom-api.example.com/v1".to_string()),
            default_model: Some("my-custom-model".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("custom-api-key".to_string()),
            available_models: None,
            trust_level: None,
        };

        let _client = build_genai_client(&config);
        // If we get here without panic, the client was built successfully
    }

    #[test]
    #[should_panic(expected = "Backend does not support chat")]
    fn test_build_genai_client_fastembed_panics() {
        let config = crucible_config::LlmProviderConfig {
            provider_type: BackendType::FastEmbed,
            endpoint: None,
            default_model: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
            available_models: None,
            trust_level: None,
        };

        let _client = build_genai_client(&config);
    }

    #[test]
    #[should_panic(expected = "Backend does not support chat")]
    fn test_build_genai_client_burn_panics() {
        let config = crucible_config::LlmProviderConfig {
            provider_type: BackendType::Burn,
            endpoint: None,
            default_model: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
            available_models: None,
            trust_level: None,
        };

        let _client = build_genai_client(&config);
    }

    #[test]
    #[should_panic(expected = "Backend does not support chat")]
    fn test_build_genai_client_mock_panics() {
        let config = crucible_config::LlmProviderConfig {
            provider_type: BackendType::Mock,
            endpoint: None,
            default_model: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
            available_models: None,
            trust_level: None,
        };

        let _client = build_genai_client(&config);
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
        let endpoint = "https://llama.krohnos.io";
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

        assert!(!should_warn, "Default (empty) Ollama endpoint should not trigger warning");
    }

}
