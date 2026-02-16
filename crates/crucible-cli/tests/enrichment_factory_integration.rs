#![allow(clippy::field_reassign_with_default)]

use crucible_config::{BackendType, CliAppConfig, LlmProviderConfig};

fn cache_key_from_llm(config: &CliAppConfig) -> String {
    if let Ok(provider) = config.effective_llm_provider() {
        format!(
            "{:?}|{}|{}|{}",
            provider.provider_type, provider.model, provider.endpoint, provider.max_tokens
        )
    } else {
        "none|default|default|0".to_string()
    }
}

fn set_default_provider(config: &mut CliAppConfig, name: &str, provider: LlmProviderConfig) {
    config.llm.default = Some(name.to_string());
    config.llm.providers.insert(name.to_string(), provider);
}

#[test]
fn test_cache_key_generation_default() {
    let mut config = CliAppConfig::default();
    set_default_provider(
        &mut config,
        "local",
        LlmProviderConfig::builder(BackendType::FastEmbed).build(),
    );

    let key = cache_key_from_llm(&config);

    assert!(key.contains("FastEmbed"));
}

#[test]
fn test_cache_key_uniqueness() {
    let mut config1 = CliAppConfig::default();
    let mut config2 = CliAppConfig::default();

    set_default_provider(
        &mut config1,
        "local",
        LlmProviderConfig::builder(BackendType::OpenAI)
            .model("text-embedding-3-small")
            .build(),
    );
    set_default_provider(
        &mut config2,
        "local",
        LlmProviderConfig::builder(BackendType::OpenAI)
            .model("text-embedding-3-large")
            .build(),
    );

    let key1 = cache_key_from_llm(&config1);
    let key2 = cache_key_from_llm(&config2);

    assert_ne!(key1, key2);
}

#[test]
fn test_cache_key_consistency() {
    let mut config = CliAppConfig::default();
    set_default_provider(
        &mut config,
        "local",
        LlmProviderConfig::builder(BackendType::OpenAI)
            .model("text-embedding-3-small")
            .endpoint("https://api.openai.com/v1")
            .max_tokens(4096)
            .build(),
    );

    let key1 = cache_key_from_llm(&config);
    let key2 = cache_key_from_llm(&config);

    assert_eq!(key1, key2);
}

#[test]
fn test_cache_key_ollama_provider() {
    let mut config = CliAppConfig::default();
    set_default_provider(
        &mut config,
        "ollama",
        LlmProviderConfig::builder(BackendType::Ollama)
            .model("nomic-embed-text")
            .endpoint("http://localhost:11434")
            .max_tokens(50)
            .build(),
    );

    let key = cache_key_from_llm(&config);

    assert!(key.contains("Ollama"));
    assert!(key.contains("nomic-embed-text"));
    assert!(key.contains("localhost:11434"));
    assert!(key.contains("50"));
}

#[test]
fn test_cache_key_openai_provider() {
    let mut config = CliAppConfig::default();
    set_default_provider(
        &mut config,
        "openai",
        LlmProviderConfig::builder(BackendType::OpenAI)
            .model("text-embedding-3-small")
            .endpoint("https://api.openai.com/v1")
            .max_tokens(100)
            .build(),
    );

    let key = cache_key_from_llm(&config);

    assert!(key.contains("OpenAI"));
    assert!(key.contains("text-embedding-3-small"));
}

#[test]
fn test_llm_provider_type_variants() {
    let fastembed = BackendType::FastEmbed;
    let ollama = BackendType::Ollama;
    let openai = BackendType::OpenAI;

    assert_ne!(format!("{:?}", fastembed), format!("{:?}", ollama));
    assert_ne!(format!("{:?}", ollama), format!("{:?}", openai));
    assert_ne!(format!("{:?}", fastembed), format!("{:?}", openai));
}

#[test]
fn test_default_provider_is_local() {
    let mut config = CliAppConfig::default();
    set_default_provider(
        &mut config,
        "local",
        LlmProviderConfig::builder(BackendType::FastEmbed).build(),
    );

    let provider = config.effective_llm_provider().unwrap();
    assert_eq!(provider.provider_type, BackendType::FastEmbed);
}

#[test]
fn test_model_configuration() {
    let mut config = CliAppConfig::default();
    set_default_provider(
        &mut config,
        "local",
        LlmProviderConfig::builder(BackendType::OpenAI)
            .model("custom-model")
            .build(),
    );

    let provider = config.effective_llm_provider().unwrap();
    assert_eq!(provider.model, "custom-model".to_string());
}

#[test]
fn test_api_url_configuration() {
    let mut config = CliAppConfig::default();
    set_default_provider(
        &mut config,
        "local",
        LlmProviderConfig::builder(BackendType::OpenAI)
            .endpoint("http://custom-endpoint:8080")
            .build(),
    );

    let provider = config.effective_llm_provider().unwrap();
    assert_eq!(provider.endpoint, "http://custom-endpoint:8080".to_string());
}

#[test]
fn test_cache_key_max_tokens_sensitivity() {
    let mut config1 = CliAppConfig::default();
    let mut config2 = CliAppConfig::default();

    set_default_provider(
        &mut config1,
        "local",
        LlmProviderConfig::builder(BackendType::Ollama)
            .max_tokens(16)
            .build(),
    );
    set_default_provider(
        &mut config2,
        "local",
        LlmProviderConfig::builder(BackendType::Ollama)
            .max_tokens(32)
            .build(),
    );

    let key1 = cache_key_from_llm(&config1);
    let key2 = cache_key_from_llm(&config2);

    assert_ne!(key1, key2);
}

#[test]
fn test_cache_key_format() {
    let mut config = CliAppConfig::default();
    set_default_provider(
        &mut config,
        "provider",
        LlmProviderConfig::builder(BackendType::Ollama)
            .model("model")
            .endpoint("http://url")
            .max_tokens(42)
            .build(),
    );

    let key = cache_key_from_llm(&config);

    let parts: Vec<&str> = key.split('|').collect();
    assert_eq!(parts.len(), 4);
    assert!(parts[0].contains("Ollama"));
    assert_eq!(parts[1], "model");
    assert_eq!(parts[2], "http://url");
    assert_eq!(parts[3], "42");
}

#[test]
fn test_llm_provider_config_clone() {
    let original = LlmProviderConfig::builder(BackendType::Ollama)
        .model("test-model")
        .endpoint("http://test:8080")
        .max_tokens(99)
        .build();

    let cloned = original.clone();

    assert_eq!(cloned.provider_type, original.provider_type);
    assert_eq!(cloned.default_model, original.default_model);
    assert_eq!(cloned.endpoint, original.endpoint);
    assert_eq!(cloned.max_tokens, original.max_tokens);
}

#[test]
fn test_llm_provider_type_default() {
    let provider_type = BackendType::default();
    assert_eq!(provider_type, BackendType::FastEmbed);
}
