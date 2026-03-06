//! Agent factory configuration tests

#![allow(clippy::field_reassign_with_default)]

//!
//! Tests that verify agent creation with various LLM provider configurations.
//! These tests validate that configurations are properly parsed, validated, and
//! used to create internal agents with the correct settings.

use crucible_cli::factories::AgentInitParams;
use crucible_config::{BackendType, CliAppConfig, LlmConfig, LlmProviderConfig};
use std::collections::HashMap;

/// Helper to create a minimal CliAppConfig for testing
fn create_agent_factory_test_config() -> CliAppConfig {
    CliAppConfig {
        kiln_path: std::env::temp_dir().join("crucible_test_kiln"),
        ..Default::default()
    }
}

/// Helper to create CliAppConfig with custom chat provider
fn create_config_with_provider(provider: BackendType, model: Option<String>) -> CliAppConfig {
    let mut config = create_agent_factory_test_config();
    config.chat.model = model;
    config.llm.default = Some("default".to_string());
    config.llm.providers.insert(
        "default".to_string(),
        LlmProviderConfig::builder(provider).build(),
    );
    config
}

/// Helper to create CliAppConfig with named LLM providers
fn create_config_with_named_providers(
    default_key: Option<String>,
    providers: HashMap<String, LlmProviderConfig>,
) -> CliAppConfig {
    let mut config = create_agent_factory_test_config();
    config.llm = LlmConfig {
        default: default_key,
        providers,
    };
    config
}

// ============================================================================
// Configuration Defaults Tests
// ============================================================================

#[test]
fn test_default_config_has_sensible_values() {
    let config = create_agent_factory_test_config();

    // Chat config should have defaults
    assert_eq!(config.ollama_endpoint(), "http://localhost:11434");
    assert_eq!(config.chat.chat_model(), "llama3.2");
    assert_eq!(config.chat.temperature(), 0.7);
    assert_eq!(config.chat.max_tokens(), 2048);
}

#[test]
fn test_custom_chat_config_values() {
    let mut config = create_agent_factory_test_config();
    config.chat.model = Some("custom-model".to_string());
    config.chat.endpoint = Some("http://custom:8080".to_string());
    config.chat.temperature = Some(0.9);
    config.chat.max_tokens = Some(4096);

    assert_eq!(config.chat.chat_model(), "custom-model");
    assert_eq!(config.chat.endpoint.as_deref(), Some("http://custom:8080"));
    assert_eq!(config.chat.temperature(), 0.9);
    assert_eq!(config.chat.max_tokens(), 4096);
}

// ============================================================================
// Named Provider Configuration Tests
// ============================================================================

#[test]
fn test_llm_config_with_single_ollama_provider() {
    let mut providers = HashMap::new();
    providers.insert(
        "local".to_string(),
        LlmProviderConfig {
            provider_type: BackendType::Ollama,
            endpoint: Some("http://localhost:11434".to_string()),
            default_model: Some("llama3.2".to_string()),
            temperature: Some(0.7),
            max_tokens: Some(4096),
            timeout_secs: Some(120),
            api_key: None,
            available_models: None,
            trust_level: None,
        },
    );

    let config = create_config_with_named_providers(Some("local".to_string()), providers);

    assert!(config.llm.has_providers());
    assert_eq!(config.llm.provider_keys().len(), 1);

    let (key, provider) = config.llm.default_provider().unwrap();
    assert_eq!(key, "local");
    assert_eq!(provider.provider_type, BackendType::Ollama);
    assert_eq!(provider.endpoint(), "http://localhost:11434");
    assert_eq!(provider.model(), "llama3.2");
}

#[test]
fn test_llm_config_with_multiple_providers() {
    let mut providers = HashMap::new();

    providers.insert(
        "local-ollama".to_string(),
        LlmProviderConfig {
            provider_type: BackendType::Ollama,
            endpoint: Some("http://localhost:11434".to_string()),
            default_model: Some("llama3.2".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
            available_models: None,
            trust_level: None,
        },
    );

    providers.insert(
        "remote-openai".to_string(),
        LlmProviderConfig {
            provider_type: BackendType::OpenAI,
            endpoint: None, // Use default
            default_model: Some("gpt-4o".to_string()),
            temperature: Some(0.5),
            max_tokens: Some(8192),
            timeout_secs: Some(300),
            api_key: Some("OPENAI_API_KEY".to_string()),
            available_models: None,
            trust_level: None,
        },
    );

    let config = create_config_with_named_providers(Some("local-ollama".to_string()), providers);

    assert!(config.llm.has_providers());
    assert_eq!(config.llm.provider_keys().len(), 2);

    // Default should be local-ollama
    let (key, _) = config.llm.default_provider().unwrap();
    assert_eq!(key, "local-ollama");

    // Can retrieve OpenAI provider by name
    let openai = config.llm.get_provider("remote-openai").unwrap();
    assert_eq!(openai.provider_type, BackendType::OpenAI);
    assert_eq!(openai.endpoint(), "https://api.openai.com/v1");
    assert_eq!(openai.model(), "gpt-4o");
}

#[test]
fn test_llm_config_provider_not_found() {
    let providers = HashMap::new();
    let config = create_config_with_named_providers(None, providers);

    assert!(!config.llm.has_providers());
    assert!(config.llm.get_provider("nonexistent").is_none());
    assert!(config.llm.default_provider().is_none());
}

#[test]
fn test_llm_config_invalid_default_provider() {
    let mut providers = HashMap::new();
    providers.insert(
        "local".to_string(),
        LlmProviderConfig {
            provider_type: BackendType::Ollama,
            endpoint: None,
            default_model: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
            available_models: None,
            trust_level: None,
        },
    );

    // Default points to non-existent provider
    let config = create_config_with_named_providers(Some("nonexistent".to_string()), providers);

    assert!(config.llm.has_providers());
    assert!(config.llm.default_provider().is_none()); // Should return None
}

// ============================================================================
// Provider Type Tests
// ============================================================================

#[test]
fn test_provider_type_ollama_defaults() {
    let provider = LlmProviderConfig {
        provider_type: BackendType::Ollama,
        endpoint: None,
        default_model: None,
        temperature: None,
        max_tokens: None,
        timeout_secs: None,
        api_key: None,
        available_models: None,
        trust_level: None,
    };

    assert_eq!(provider.endpoint(), "http://localhost:11434");
    assert_eq!(provider.model(), "llama3.2");
    assert_eq!(provider.temperature(), 0.7);
    assert_eq!(provider.max_tokens(), 4096);
    assert_eq!(provider.timeout_secs(), 120);
}

#[test]
fn test_provider_type_openai_defaults() {
    let provider = LlmProviderConfig {
        provider_type: BackendType::OpenAI,
        endpoint: None,
        default_model: None,
        temperature: None,
        max_tokens: None,
        timeout_secs: None,
        api_key: None,
        available_models: None,
        trust_level: None,
    };

    assert_eq!(provider.endpoint(), "https://api.openai.com/v1");
    assert_eq!(provider.model(), "gpt-4o");
    assert_eq!(provider.temperature(), 0.7);
    assert_eq!(provider.max_tokens(), 4096);
    assert_eq!(provider.timeout_secs(), 120);
}

#[test]
fn test_provider_type_anthropic_defaults() {
    let provider = LlmProviderConfig {
        provider_type: BackendType::Anthropic,
        endpoint: None,
        default_model: None,
        temperature: None,
        max_tokens: None,
        timeout_secs: None,
        api_key: None,
        available_models: None,
        trust_level: None,
    };

    assert_eq!(provider.endpoint(), "https://api.anthropic.com/v1");
    assert_eq!(provider.model(), "claude-3-5-sonnet-20241022");
    assert_eq!(provider.temperature(), 0.7);
    assert_eq!(provider.max_tokens(), 4096);
    assert_eq!(provider.timeout_secs(), 120);
}

#[test]
fn test_provider_custom_overrides() {
    let provider = LlmProviderConfig {
        provider_type: BackendType::Ollama,
        endpoint: Some("http://192.168.1.100:11434".to_string()),
        default_model: Some("llama3.1:70b".to_string()),
        temperature: Some(0.9),
        max_tokens: Some(8192),
        timeout_secs: Some(300),
        api_key: None,
        available_models: None,
        trust_level: None,
    };

    assert_eq!(provider.endpoint(), "http://192.168.1.100:11434");
    assert_eq!(provider.model(), "llama3.1:70b");
    assert_eq!(provider.temperature(), 0.9);
    assert_eq!(provider.max_tokens(), 8192);
    assert_eq!(provider.timeout_secs(), 300);
}

// ============================================================================
// Agent Creation Tests
// ============================================================================

// ============================================================================
// Model Name Propagation Tests
// ============================================================================

#[test]
fn test_model_name_from_chat_config() {
    let config = create_config_with_provider(
        BackendType::Ollama,
        Some("test-model-from-chat".to_string()),
    );

    assert_eq!(config.chat.chat_model(), "test-model-from-chat");
}

#[test]
fn test_model_name_fallback_to_default() {
    let config = create_config_with_provider(BackendType::Ollama, None);

    // Should use default Ollama model
    assert_eq!(config.chat.chat_model(), "llama3.2");
}

#[test]
fn test_model_name_from_named_provider() {
    let mut providers = HashMap::new();
    providers.insert(
        "custom".to_string(),
        LlmProviderConfig {
            provider_type: BackendType::Ollama,
            endpoint: None,
            default_model: Some("custom-provider-model".to_string()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
            available_models: None,
            trust_level: None,
        },
    );

    let config = create_config_with_named_providers(Some("custom".to_string()), providers);

    let provider = config.llm.get_provider("custom").unwrap();
    assert_eq!(provider.model(), "custom-provider-model");
}

// ============================================================================
// Context Token Configuration Tests
// ============================================================================

#[test]
fn test_agent_params_accept_max_context_tokens() {
    let params = AgentInitParams::new().with_max_context_tokens(8192);
    assert_eq!(params.max_context_tokens, Some(8192));
}

#[test]
fn test_agent_params_default_max_context_tokens_is_none() {
    let params = AgentInitParams::new();
    assert_eq!(params.max_context_tokens, None);
}

// ============================================================================
// Edge Cases and Error Handling
// ============================================================================

#[test]
fn test_empty_llm_config() {
    let config = create_config_with_named_providers(None, HashMap::new());

    assert!(!config.llm.has_providers());
    assert_eq!(config.llm.provider_keys().len(), 0);
}

#[test]
fn test_chat_provider_variants() {
    // Test all provider enum variants
    let ollama = BackendType::Ollama;
    let openai = BackendType::OpenAI;
    let anthropic = BackendType::Anthropic;

    assert_eq!(ollama, BackendType::Ollama);
    assert_eq!(openai, BackendType::OpenAI);
    assert_eq!(anthropic, BackendType::Anthropic);

    // Test inequality
    assert_ne!(ollama, openai);
    assert_ne!(openai, anthropic);
    assert_ne!(ollama, anthropic);
}

#[test]
fn test_llm_provider_type_variants() {
    // Test all provider type enum variants
    let ollama = BackendType::Ollama;
    let openai = BackendType::OpenAI;
    let anthropic = BackendType::Anthropic;

    assert_eq!(ollama, BackendType::Ollama);
    assert_eq!(openai, BackendType::OpenAI);
    assert_eq!(anthropic, BackendType::Anthropic);

    // Test inequality
    assert_ne!(ollama, openai);
    assert_ne!(openai, anthropic);
    assert_ne!(ollama, anthropic);
}

#[test]
fn test_config_temperature_boundary_values() {
    let mut config = create_agent_factory_test_config();

    // Min temperature (0.0)
    config.chat.temperature = Some(0.0);
    assert_eq!(config.chat.temperature(), 0.0);

    // Max reasonable temperature (2.0)
    config.chat.temperature = Some(2.0);
    assert_eq!(config.chat.temperature(), 2.0);

    // Default temperature
    config.chat.temperature = None;
    assert_eq!(config.chat.temperature(), 0.7);
}

#[test]
fn test_config_max_tokens_boundary_values() {
    let mut config = create_agent_factory_test_config();

    // Small value
    config.chat.max_tokens = Some(1);
    assert_eq!(config.chat.max_tokens(), 1);

    // Large value (128K tokens - Claude 3 territory)
    config.chat.max_tokens = Some(128_000);
    assert_eq!(config.chat.max_tokens(), 128_000);

    // Default
    config.chat.max_tokens = None;
    assert_eq!(config.chat.max_tokens(), 2048);
}

#[test]
fn test_config_timeout_boundary_values() {
    let mut config = create_agent_factory_test_config();

    // Short timeout
    config.chat.timeout_secs = Some(1);
    assert_eq!(config.chat.timeout_secs(), 1);

    // Long timeout (1 hour)
    config.chat.timeout_secs = Some(3600);
    assert_eq!(config.chat.timeout_secs(), 3600);

    // Default
    config.chat.timeout_secs = None;
    assert_eq!(config.chat.timeout_secs(), 120);
}

// ============================================================================
// API Key Configuration Tests
// ============================================================================

#[test]
fn test_provider_api_key_direct_value() {
    // With new model, api_key is the resolved value (not an env var name)
    let provider = LlmProviderConfig {
        provider_type: BackendType::OpenAI,
        endpoint: None,
        default_model: None,
        temperature: None,
        max_tokens: None,
        timeout_secs: None,
        api_key: Some("sk-test-key-12345".to_string()),
        available_models: None,
        trust_level: None,
    };

    assert_eq!(provider.api_key(), Some("sk-test-key-12345".to_string()));
}

#[test]
fn test_provider_no_api_key_configured() {
    let provider = LlmProviderConfig {
        provider_type: BackendType::Ollama,
        endpoint: None,
        default_model: None,
        temperature: None,
        max_tokens: None,
        timeout_secs: None,
        api_key: None,
        available_models: None,
        trust_level: None,
    };

    // Should return None if no api_key configured
    assert_eq!(provider.api_key(), None);
}

// ============================================================================
// Integration-style Configuration Tests
// ============================================================================

#[test]
fn test_realistic_ollama_config() {
    let mut providers = HashMap::new();
    providers.insert(
        "local-llama".to_string(),
        LlmProviderConfig {
            provider_type: BackendType::Ollama,
            endpoint: Some("http://localhost:11434".to_string()),
            default_model: Some("llama3.2:latest".to_string()),
            temperature: Some(0.7),
            max_tokens: Some(4096),
            timeout_secs: Some(120),
            api_key: None,
            available_models: None,
            trust_level: None,
        },
    );

    let config = create_config_with_named_providers(Some("local-llama".to_string()), providers);

    let (key, provider) = config.llm.default_provider().unwrap();
    assert_eq!(key, "local-llama");
    assert_eq!(provider.provider_type, BackendType::Ollama);
    assert_eq!(provider.endpoint(), "http://localhost:11434");
    assert_eq!(provider.model(), "llama3.2:latest");
}

#[test]
fn test_realistic_openai_config() {
    let mut providers = HashMap::new();
    providers.insert(
        "openai-gpt4".to_string(),
        LlmProviderConfig {
            provider_type: BackendType::OpenAI,
            endpoint: None, // Use default
            default_model: Some("gpt-4o".to_string()),
            temperature: Some(0.5),
            max_tokens: Some(8192),
            timeout_secs: Some(300),
            api_key: Some("OPENAI_API_KEY".to_string()),
            available_models: None,
            trust_level: None,
        },
    );

    let config = create_config_with_named_providers(Some("openai-gpt4".to_string()), providers);

    let (key, provider) = config.llm.default_provider().unwrap();
    assert_eq!(key, "openai-gpt4");
    assert_eq!(provider.provider_type, BackendType::OpenAI);
    assert_eq!(provider.endpoint(), "https://api.openai.com/v1");
    assert_eq!(provider.model(), "gpt-4o");
    assert_eq!(provider.temperature(), 0.5);
    assert_eq!(provider.max_tokens(), 8192);
}

#[test]
fn test_realistic_multi_provider_config() {
    let mut providers = HashMap::new();

    // Local development with Ollama
    providers.insert(
        "dev".to_string(),
        LlmProviderConfig {
            provider_type: BackendType::Ollama,
            endpoint: Some("http://localhost:11434".to_string()),
            default_model: Some("llama3.2".to_string()),
            temperature: Some(0.7),
            max_tokens: Some(4096),
            timeout_secs: Some(120),
            api_key: None,
            available_models: None,
            trust_level: None,
        },
    );

    // Production with OpenAI
    providers.insert(
        "prod".to_string(),
        LlmProviderConfig {
            provider_type: BackendType::OpenAI,
            endpoint: None, // Use default
            default_model: Some("gpt-4o".to_string()),
            temperature: Some(0.5),
            max_tokens: Some(8192),
            timeout_secs: Some(300),
            api_key: Some("OPENAI_API_KEY".to_string()),
            available_models: None,
            trust_level: None,
        },
    );

    // Alternative Anthropic
    providers.insert(
        "claude".to_string(),
        LlmProviderConfig {
            provider_type: BackendType::Anthropic,
            endpoint: None,
            default_model: Some("claude-3-5-sonnet-20241022".to_string()),
            temperature: Some(0.7),
            max_tokens: Some(4096),
            timeout_secs: Some(300),
            api_key: Some("ANTHROPIC_API_KEY".to_string()),
            available_models: None,
            trust_level: None,
        },
    );

    let config = create_config_with_named_providers(Some("dev".to_string()), providers);

    // Should have all three providers
    assert_eq!(config.llm.provider_keys().len(), 3);

    // Default should be dev
    let (key, _) = config.llm.default_provider().unwrap();
    assert_eq!(key, "dev");

    // All providers should be accessible
    assert!(config.llm.get_provider("dev").is_some());
    assert!(config.llm.get_provider("prod").is_some());
    assert!(config.llm.get_provider("claude").is_some());
}
