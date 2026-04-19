use super::super::*;

#[tokio::test]
async fn test_resolve_provider_config_from_llm_config() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let mut providers = std::collections::HashMap::new();
    providers.insert(
        "zai-coding".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .endpoint("https://api.z.ai/api/coding/paas/v4")
            .api_key("test-key-123")
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("zai-coding".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    let resolved = agent_manager.resolve_provider_config("zai-coding");
    assert!(resolved.is_some(), "Should resolve from llm_config");
    let resolved = resolved.unwrap();
    assert_eq!(resolved.provider_type, BackendType::ZAI);
    assert_eq!(
        resolved.endpoint.as_deref(),
        Some("https://api.z.ai/api/coding/paas/v4")
    );
    assert_eq!(resolved.api_key.as_deref(), Some("test-key-123"));
    assert_eq!(resolved.source, "llm_config");
}

#[tokio::test]
async fn test_resolve_provider_config_from_providers_config() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let mut providers = std::collections::HashMap::new();
    providers.insert(
        "local".to_string(),
        LlmProviderConfig::builder(BackendType::Ollama)
            .endpoint("http://localhost:11434")
            .api_key("ollama-key")
            .build(),
    );
    let llm_config = LlmConfig {
        default: Some("local".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_providers(session_manager.clone(), llm_config);

    let resolved = agent_manager.resolve_provider_config("local");
    assert!(resolved.is_some(), "Should resolve from llm_config");
    let resolved = resolved.unwrap();
    assert_eq!(resolved.provider_type, BackendType::Ollama);
    assert_eq!(resolved.endpoint.as_deref(), Some("http://localhost:11434"));
    assert_eq!(resolved.api_key.as_deref(), Some("ollama-key"));
    assert_eq!(resolved.source, "llm_config");
}

#[tokio::test]
async fn test_resolve_provider_config_does_not_use_legacy_providers_config() {
    use crucible_config::LlmConfig;

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let llm_config = LlmConfig::default();
    let agent_manager = create_test_agent_manager_with_providers(session_manager, llm_config);

    let resolved = agent_manager.resolve_provider_config("legacy");
    assert!(
        resolved.is_none(),
        "legacy providers config should not be used for resolution"
    );
}

#[tokio::test]
async fn test_resolve_provider_config_not_found() {
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let agent_manager = create_test_agent_manager(session_manager);

    let resolved = agent_manager.resolve_provider_config("nonexistent");
    assert!(
        resolved.is_none(),
        "Should return None when provider not in either config"
    );
}

#[tokio::test]
async fn test_resolve_provider_config_llm_config_wins_over_providers_config() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let mut llm_providers = std::collections::HashMap::new();
    llm_providers.insert(
        "shared".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .endpoint("https://api.openai.com/v1")
            .api_key("openai-key")
            .build(),
    );
    let llm_config = LlmConfig {
        default: None,
        providers: llm_providers,
    };

    let agent_manager = create_test_agent_manager_with_both(session_manager.clone(), llm_config);

    let resolved = agent_manager.resolve_provider_config("shared");
    assert!(resolved.is_some(), "Should resolve when in both configs");
    let resolved = resolved.unwrap();
    assert_eq!(
        resolved.source, "llm_config",
        "LlmConfig should take priority"
    );
    assert_eq!(resolved.provider_type, BackendType::OpenAI);
    assert_eq!(
        resolved.endpoint.as_deref(),
        Some("https://api.openai.com/v1")
    );
    assert_eq!(resolved.api_key.as_deref(), Some("openai-key"));
}
