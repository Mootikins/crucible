//! Integration tests for text generation factory functions

use crucible_config::{EffectiveLlmConfig, LlmConfig, LlmProviderConfig, LlmProviderType};
use crucible_llm::text_generation::{
    from_config_by_name, from_effective_config, from_provider_config,
};
use std::collections::HashMap;

#[tokio::test]
async fn test_factory_from_provider_config_ollama() {
    let config = LlmProviderConfig {
        provider_type: LlmProviderType::Ollama,
        endpoint: Some("http://localhost:11434".to_string()),
        default_model: Some("llama3.2".to_string()),
        temperature: Some(0.7),
        max_tokens: Some(4096),
        timeout_secs: Some(120),
        api_key: None,
    };

    let provider = from_provider_config(&config).await;
    assert!(provider.is_ok(), "Failed to create provider from config");

    let provider = provider.unwrap();
    assert_eq!(provider.provider_name(), "Ollama");
    assert_eq!(provider.default_model(), "llama3.2");
}

#[tokio::test]
async fn test_factory_from_effective_config_ollama() {
    let effective = EffectiveLlmConfig {
        key: "local".to_string(),
        provider_type: LlmProviderType::Ollama,
        endpoint: "http://localhost:11434".to_string(),
        model: "llama3.2".to_string(),
        temperature: 0.7,
        max_tokens: 4096,
        timeout_secs: 120,
        api_key: None,
    };

    let provider = from_effective_config(&effective).await;
    assert!(
        provider.is_ok(),
        "Failed to create provider from effective config"
    );

    let provider = provider.unwrap();
    assert_eq!(provider.provider_name(), "Ollama");
    assert_eq!(provider.default_model(), "llama3.2");
}

#[tokio::test]
async fn test_factory_from_config_by_name() {
    let mut providers = HashMap::new();
    providers.insert(
        "local".to_string(),
        LlmProviderConfig {
            provider_type: LlmProviderType::Ollama,
            endpoint: Some("http://localhost:11434".to_string()),
            default_model: Some("llama3.2".to_string()),
            temperature: Some(0.7),
            max_tokens: Some(4096),
            timeout_secs: Some(120),
            api_key: None,
        },
    );

    let llm_config = LlmConfig {
        default: Some("local".to_string()),
        providers,
    };

    let config = crucible_config::Config {
        llm: Some(llm_config),
        ..Default::default()
    };

    let provider = from_config_by_name(&config, "local").await;
    assert!(provider.is_ok(), "Failed to create provider by name");

    let provider = provider.unwrap();
    assert_eq!(provider.provider_name(), "Ollama");
    assert_eq!(provider.default_model(), "llama3.2");
}

#[tokio::test]
async fn test_factory_from_config_by_name_not_found() {
    let providers = HashMap::new();

    let llm_config = LlmConfig {
        default: None,
        providers,
    };

    let config = crucible_config::Config {
        llm: Some(llm_config),
        ..Default::default()
    };

    let provider = from_config_by_name(&config, "nonexistent").await;
    assert!(provider.is_err(), "Expected error for nonexistent provider");
}

#[tokio::test]
async fn test_factory_openai_requires_api_key() {
    let config = LlmProviderConfig {
        provider_type: LlmProviderType::OpenAI,
        endpoint: Some("https://api.openai.com/v1".to_string()),
        default_model: Some("gpt-4o".to_string()),
        temperature: Some(0.7),
        max_tokens: Some(4096),
        timeout_secs: Some(120),
        api_key: None, // No API key
    };

    let provider = from_provider_config(&config).await;
    assert!(provider.is_err(), "Expected error for missing API key");
}
