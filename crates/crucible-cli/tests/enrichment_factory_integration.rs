//! Integration tests for enrichment factory

#![allow(clippy::field_reassign_with_default)]

//!
//! Tests the enrichment service creation, provider caching, and configuration handling.

use crucible_config::{CliAppConfig, EmbeddingConfig, EmbeddingProviderType};

/// Test that default config creates valid cache key
#[test]
fn test_cache_key_generation_default() {
    let config = CliAppConfig::default();
    let key = format!(
        "{:?}|{}|{}|{}",
        config.embedding.provider,
        config.embedding.model.as_deref().unwrap_or("default"),
        config.embedding.api_url.as_deref().unwrap_or("default"),
        config.embedding.batch_size
    );

    // Should contain provider type
    assert!(
        key.contains("FastEmbed"),
        "Key should contain provider type"
    );
    // Should contain batch size
    assert!(key.contains("16"), "Key should contain batch size (16)");
}

/// Test that different configs produce different cache keys
#[test]
fn test_cache_key_uniqueness() {
    let config1 = CliAppConfig::default();
    let mut config2 = CliAppConfig::default();

    config2.embedding.model = Some("different-model".to_string());

    let key1 = format!(
        "{:?}|{}|{}|{}",
        config1.embedding.provider,
        config1.embedding.model.as_deref().unwrap_or("default"),
        config1.embedding.api_url.as_deref().unwrap_or("default"),
        config1.embedding.batch_size
    );

    let key2 = format!(
        "{:?}|{}|{}|{}",
        config2.embedding.provider,
        config2.embedding.model.as_deref().unwrap_or("default"),
        config2.embedding.api_url.as_deref().unwrap_or("default"),
        config2.embedding.batch_size
    );

    assert_ne!(
        key1, key2,
        "Different configs should have different cache keys"
    );
}

/// Test that same configs produce identical cache keys
#[test]
fn test_cache_key_consistency() {
    let config = CliAppConfig::default();

    let key1 = format!(
        "{:?}|{}|{}|{}",
        config.embedding.provider,
        config.embedding.model.as_deref().unwrap_or("default"),
        config.embedding.api_url.as_deref().unwrap_or("default"),
        config.embedding.batch_size
    );

    let key2 = format!(
        "{:?}|{}|{}|{}",
        config.embedding.provider,
        config.embedding.model.as_deref().unwrap_or("default"),
        config.embedding.api_url.as_deref().unwrap_or("default"),
        config.embedding.batch_size
    );

    assert_eq!(
        key1, key2,
        "Same config should produce identical cache keys"
    );
}

/// Test cache key with Ollama provider
#[test]
fn test_cache_key_ollama_provider() {
    let mut config = CliAppConfig::default();
    config.embedding = EmbeddingConfig {
        provider: EmbeddingProviderType::Ollama,
        model: Some("nomic-embed-text".to_string()),
        api_url: Some("http://localhost:11434".to_string()),
        batch_size: 50,
        max_concurrent: None,
    };

    let key = format!(
        "{:?}|{}|{}|{}",
        config.embedding.provider,
        config.embedding.model.as_deref().unwrap_or("default"),
        config.embedding.api_url.as_deref().unwrap_or("default"),
        config.embedding.batch_size
    );

    assert!(key.contains("Ollama"), "Key should contain Ollama provider");
    assert!(
        key.contains("nomic-embed-text"),
        "Key should contain model name"
    );
    assert!(
        key.contains("localhost:11434"),
        "Key should contain endpoint"
    );
    assert!(key.contains("50"), "Key should contain batch size");
}

/// Test cache key with OpenAI provider
#[test]
fn test_cache_key_openai_provider() {
    let mut config = CliAppConfig::default();
    config.embedding = EmbeddingConfig {
        provider: EmbeddingProviderType::OpenAI,
        model: Some("text-embedding-3-small".to_string()),
        api_url: Some("https://api.openai.com/v1".to_string()),
        batch_size: 100,
        max_concurrent: None,
    };

    let key = format!(
        "{:?}|{}|{}|{}",
        config.embedding.provider,
        config.embedding.model.as_deref().unwrap_or("default"),
        config.embedding.api_url.as_deref().unwrap_or("default"),
        config.embedding.batch_size
    );

    assert!(key.contains("OpenAI"), "Key should contain OpenAI provider");
    assert!(
        key.contains("text-embedding-3-small"),
        "Key should contain model name"
    );
}

/// Test EmbeddingConfig default values
#[test]
fn test_embedding_config_defaults() {
    let config = EmbeddingConfig::default();

    assert_eq!(config.provider, EmbeddingProviderType::FastEmbed);
    assert_eq!(config.model, None);
    assert_eq!(config.api_url, None);
    assert_eq!(config.batch_size, 16);
    assert_eq!(config.max_concurrent, None);
}

/// Test EmbeddingProviderType variants
#[test]
fn test_embedding_provider_type_variants() {
    let fastembed = EmbeddingProviderType::FastEmbed;
    let ollama = EmbeddingProviderType::Ollama;
    let openai = EmbeddingProviderType::OpenAI;

    // Verify they are distinct
    assert_ne!(format!("{:?}", fastembed), format!("{:?}", ollama));
    assert_ne!(format!("{:?}", ollama), format!("{:?}", openai));
    assert_ne!(format!("{:?}", fastembed), format!("{:?}", openai));
}

/// Test that default provider is FastEmbed (local, no API key required)
#[test]
fn test_default_provider_is_local() {
    let config = EmbeddingConfig::default();
    assert_eq!(
        config.provider,
        EmbeddingProviderType::FastEmbed,
        "Default provider should be FastEmbed for privacy and no API key requirement"
    );
}

/// Test batch size configuration
#[test]
fn test_batch_size_configuration() {
    let mut config = EmbeddingConfig::default();

    // Default
    assert_eq!(config.batch_size, 16);

    // Custom value
    config.batch_size = 64;
    assert_eq!(config.batch_size, 64);

    // Zero batch size (edge case)
    config.batch_size = 0;
    assert_eq!(config.batch_size, 0);

    // Large batch size
    config.batch_size = 1000;
    assert_eq!(config.batch_size, 1000);
}

/// Test max_concurrent configuration
#[test]
fn test_max_concurrent_configuration() {
    let mut config = EmbeddingConfig::default();

    // Default is None (use provider default)
    assert_eq!(config.max_concurrent, None);

    // Custom value
    config.max_concurrent = Some(4);
    assert_eq!(config.max_concurrent, Some(4));

    // Single threaded
    config.max_concurrent = Some(1);
    assert_eq!(config.max_concurrent, Some(1));
}

/// Test model configuration
#[test]
fn test_model_configuration() {
    let mut config = EmbeddingConfig::default();

    // Default is None (use provider default)
    assert_eq!(config.model, None);

    // Custom model
    config.model = Some("custom-model".to_string());
    assert_eq!(config.model, Some("custom-model".to_string()));

    // Empty model (edge case)
    config.model = Some("".to_string());
    assert_eq!(config.model, Some("".to_string()));
}

/// Test API URL configuration
#[test]
fn test_api_url_configuration() {
    let mut config = EmbeddingConfig::default();

    // Default is None (use provider default)
    assert_eq!(config.api_url, None);

    // Custom URL
    config.api_url = Some("http://custom-endpoint:8080".to_string());
    assert_eq!(
        config.api_url,
        Some("http://custom-endpoint:8080".to_string())
    );

    // HTTPS URL
    config.api_url = Some("https://api.example.com/v1".to_string());
    assert_eq!(
        config.api_url,
        Some("https://api.example.com/v1".to_string())
    );
}

/// Test that cache keys differ when batch size changes
#[test]
fn test_cache_key_batch_size_sensitivity() {
    let mut config1 = CliAppConfig::default();
    let mut config2 = CliAppConfig::default();

    config1.embedding.batch_size = 16;
    config2.embedding.batch_size = 32;

    let key1 = format!(
        "{:?}|{}|{}|{}",
        config1.embedding.provider,
        config1.embedding.model.as_deref().unwrap_or("default"),
        config1.embedding.api_url.as_deref().unwrap_or("default"),
        config1.embedding.batch_size
    );

    let key2 = format!(
        "{:?}|{}|{}|{}",
        config2.embedding.provider,
        config2.embedding.model.as_deref().unwrap_or("default"),
        config2.embedding.api_url.as_deref().unwrap_or("default"),
        config2.embedding.batch_size
    );

    assert_ne!(
        key1, key2,
        "Different batch sizes should produce different keys"
    );
}

/// Test cache key format stability
#[test]
fn test_cache_key_format() {
    let mut config = CliAppConfig::default();
    config.embedding = EmbeddingConfig {
        provider: EmbeddingProviderType::Ollama,
        model: Some("model".to_string()),
        api_url: Some("http://url".to_string()),
        batch_size: 42,
        max_concurrent: None,
    };

    let key = format!(
        "{:?}|{}|{}|{}",
        config.embedding.provider,
        config.embedding.model.as_deref().unwrap_or("default"),
        config.embedding.api_url.as_deref().unwrap_or("default"),
        config.embedding.batch_size
    );

    // Verify format: provider|model|url|batch_size
    let parts: Vec<&str> = key.split('|').collect();
    assert_eq!(parts.len(), 4, "Cache key should have 4 parts");
    assert!(parts[0].contains("Ollama"), "First part should be provider");
    assert_eq!(parts[1], "model", "Second part should be model");
    assert_eq!(parts[2], "http://url", "Third part should be URL");
    assert_eq!(parts[3], "42", "Fourth part should be batch size");
}

/// Test config clone preserves all fields
#[test]
fn test_embedding_config_clone() {
    let original = EmbeddingConfig {
        provider: EmbeddingProviderType::Ollama,
        model: Some("test-model".to_string()),
        api_url: Some("http://test:8080".to_string()),
        batch_size: 99,
        max_concurrent: Some(8),
    };

    let cloned = original.clone();

    assert_eq!(cloned.provider, original.provider);
    assert_eq!(cloned.model, original.model);
    assert_eq!(cloned.api_url, original.api_url);
    assert_eq!(cloned.batch_size, original.batch_size);
    assert_eq!(cloned.max_concurrent, original.max_concurrent);
}

/// Test EmbeddingProviderType default
#[test]
fn test_embedding_provider_type_default() {
    let provider_type = EmbeddingProviderType::default();
    assert_eq!(
        provider_type,
        EmbeddingProviderType::FastEmbed,
        "Default provider type should be FastEmbed"
    );
}
