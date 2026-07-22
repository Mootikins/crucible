use super::super::*;

#[tokio::test]
async fn test_parse_provider_model_llm_config_found() {
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let mut providers = std::collections::HashMap::new();
    providers.insert(
        "zai-coding".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .endpoint("https://api.z.ai/api/coding/paas/v4")
            .available_models(vec!["GLM-4.7".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("zai-coding".to_string()),
        providers,
        models: Default::default(),
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    let (provider_key, model_name) = agent_manager.parse_provider_model("zai-coding/GLM-4.7");
    assert_eq!(
        provider_key.as_deref(),
        Some("zai-coding"),
        "Should find provider key in llm_config"
    );
    assert_eq!(model_name, "GLM-4.7", "Model name should be extracted");
}

#[tokio::test]
async fn test_parse_provider_model_llm_config_not_found() {
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let mut providers = std::collections::HashMap::new();
    providers.insert(
        "zai-coding".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI).build(),
    );

    let llm_config = LlmConfig {
        default: None,
        providers,
        models: Default::default(),
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    let (provider_key, model_name) = agent_manager.parse_provider_model("unknown/model");
    assert_eq!(
        provider_key, None,
        "Should return None when prefix not in either config"
    );
    assert_eq!(
        model_name, "unknown/model",
        "Should return full string as model"
    );
}

#[tokio::test]
async fn test_parse_provider_model_legacy_takes_precedence() {
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let mut llm_providers = std::collections::HashMap::new();
    llm_providers.insert(
        "local".to_string(),
        LlmProviderConfig::builder(BackendType::Ollama)
            .endpoint("http://different:11434")
            .build(),
    );
    let llm_config = LlmConfig {
        default: None,
        providers: llm_providers,
        models: Default::default(),
    };

    let agent_manager = create_test_agent_manager_with_both(session_manager.clone(), llm_config);

    let (provider_key, model_name) = agent_manager.parse_provider_model("local/llama3.2");
    assert_eq!(
        provider_key.as_deref(),
        Some("local"),
        "Configured provider key should be detected"
    );
    assert_eq!(model_name, "llama3.2");
}

#[tokio::test]
async fn test_parse_provider_model_empty_string() {
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let agent_manager = create_test_agent_manager(session_manager);

    let (provider_key, model_name) = agent_manager.parse_provider_model("");
    assert_eq!(
        provider_key, None,
        "Empty string should return None provider"
    );
    assert_eq!(
        model_name, "",
        "Empty string should return empty model name"
    );
}

#[tokio::test]
async fn test_parse_provider_model_trailing_slash() {
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let mut providers = std::collections::HashMap::new();
    providers.insert(
        "provider".to_string(),
        LlmProviderConfig::builder(BackendType::Ollama)
            .endpoint("http://localhost:11434")
            .build(),
    );
    let llm_config = LlmConfig {
        default: Some("provider".to_string()),
        providers,
        models: Default::default(),
    };

    let agent_manager =
        create_test_agent_manager_with_providers(session_manager.clone(), llm_config);

    let (provider_key, model_name) = agent_manager.parse_provider_model("provider/");
    assert_eq!(
        provider_key.as_deref(),
        Some("provider"),
        "Trailing slash should still parse provider"
    );
    assert_eq!(
        model_name, "",
        "Trailing slash should result in empty model name"
    );
}

#[tokio::test]
async fn test_parse_provider_model_whitespace() {
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let mut providers = std::collections::HashMap::new();
    providers.insert(
        "provider".to_string(),
        LlmProviderConfig::builder(BackendType::Ollama)
            .endpoint("http://localhost:11434")
            .build(),
    );
    let llm_config = LlmConfig {
        default: Some("provider".to_string()),
        providers,
        models: Default::default(),
    };

    let agent_manager =
        create_test_agent_manager_with_providers(session_manager.clone(), llm_config);

    let (provider_key, model_name) = agent_manager.parse_provider_model("  provider/model  ");
    assert_eq!(
        provider_key, None,
        "Whitespace prefix prevents provider match (no trimming in parse)"
    );
    assert_eq!(
        model_name, "  provider/model  ",
        "Full string with whitespace returned as model"
    );
}

#[tokio::test]
async fn test_parse_provider_model_case_sensitivity() {
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};

    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let mut providers = std::collections::HashMap::new();
    providers.insert(
        "ollama".to_string(),
        LlmProviderConfig::builder(BackendType::Ollama)
            .endpoint("http://localhost:11434")
            .build(),
    );
    let llm_config = LlmConfig {
        default: Some("ollama".to_string()),
        providers,
        models: Default::default(),
    };

    let agent_manager =
        create_test_agent_manager_with_providers(session_manager.clone(), llm_config);

    let (provider_key, model_name) = agent_manager.parse_provider_model("ollama/model");
    assert_eq!(
        provider_key.as_deref(),
        Some("ollama"),
        "Lowercase should match"
    );
    assert_eq!(model_name, "model");

    let (provider_key, model_name) = agent_manager.parse_provider_model("OLLAMA/model");
    assert_eq!(
        provider_key, None,
        "Uppercase should not match (case-sensitive)"
    );
    assert_eq!(model_name, "OLLAMA/model", "Full string returned as model");
}
