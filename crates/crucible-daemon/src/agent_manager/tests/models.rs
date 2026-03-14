use super::*;
#[tokio::test]
async fn test_list_models_returns_all_providers() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};
    use std::collections::HashMap;

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let mut providers = HashMap::new();
    providers.insert(
        "ollama".to_string(),
        LlmProviderConfig::builder(BackendType::Ollama)
            .endpoint("http://localhost:11434")
            .available_models(vec!["llama3.2".to_string(), "qwen2.5".to_string()])
            .build(),
    );
    providers.insert(
        "openai".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["gpt-4".to_string(), "gpt-3.5-turbo".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("ollama".to_string()),
        providers,
    };

    let (event_tx, _) = broadcast::channel(16);
    let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
    let agent_manager = AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: session_manager.clone(),
        background_manager,
        mcp_gateway: None,
        llm_config: Some(llm_config),
        acp_config: None,
        permission_config: None,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(std::path::PathBuf::from("/tmp"))),
    });

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

    assert!(
        models.iter().any(|m| m.starts_with("openai/")),
        "Should have openai/ prefixed models, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openai/gpt-4".to_string()),
        "Should contain openai/gpt-4, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openai/gpt-3.5-turbo".to_string()),
        "Should contain openai/gpt-3.5-turbo, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_trust_excludes_cloud_for_confidential_kiln() {
    use crucible_config::{
        BackendType, DataClassification, LlmConfig, LlmProviderConfig, TrustLevel,
    };
    use std::collections::HashMap;

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let mut local = LlmProviderConfig::builder(BackendType::Custom)
        .available_models(vec!["local-model".to_string()])
        .build();
    local.trust_level = Some(TrustLevel::Local);

    let mut cloud = LlmProviderConfig::builder(BackendType::OpenAI)
        .available_models(vec!["gpt-4o".to_string()])
        .build();
    cloud.trust_level = Some(TrustLevel::Cloud);

    let mut providers = HashMap::new();
    providers.insert("local-custom".to_string(), local);
    providers.insert("cloud-openai".to_string(), cloud);

    let llm_config = LlmConfig {
        default: Some("local-custom".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager
        .list_models(&session.id, Some(DataClassification::Confidential))
        .await
        .unwrap();

    assert!(
        models.contains(&"local-custom/local-model".to_string()),
        "Confidential should keep Local provider models, got: {:?}",
        models
    );
    assert!(
        !models.iter().any(|m| m.starts_with("cloud-openai/")),
        "Confidential should exclude Cloud provider models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_trust_returns_all_for_public_kiln() {
    use crucible_config::{
        BackendType, DataClassification, LlmConfig, LlmProviderConfig, TrustLevel,
    };
    use std::collections::HashMap;

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let mut local = LlmProviderConfig::builder(BackendType::Custom)
        .available_models(vec!["local-model".to_string()])
        .build();
    local.trust_level = Some(TrustLevel::Local);

    let mut cloud = LlmProviderConfig::builder(BackendType::OpenAI)
        .available_models(vec!["gpt-4o".to_string()])
        .build();
    cloud.trust_level = Some(TrustLevel::Cloud);

    let mut providers = HashMap::new();
    providers.insert("local-custom".to_string(), local);
    providers.insert("cloud-openai".to_string(), cloud);

    let llm_config = LlmConfig {
        default: Some("local-custom".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager
        .list_models(&session.id, Some(DataClassification::Public))
        .await
        .unwrap();

    assert!(
        models.contains(&"local-custom/local-model".to_string()),
        "Public should include Local provider models, got: {:?}",
        models
    );
    assert!(
        models.contains(&"cloud-openai/gpt-4o".to_string()),
        "Public should include Cloud provider models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_trust_returns_all_when_no_classification() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig, TrustLevel};
    use std::collections::HashMap;

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let mut local = LlmProviderConfig::builder(BackendType::Custom)
        .available_models(vec!["local-model".to_string()])
        .build();
    local.trust_level = Some(TrustLevel::Local);

    let mut cloud = LlmProviderConfig::builder(BackendType::OpenAI)
        .available_models(vec!["gpt-4o".to_string()])
        .build();
    cloud.trust_level = Some(TrustLevel::Cloud);

    let mut providers = HashMap::new();
    providers.insert("local-custom".to_string(), local);
    providers.insert("cloud-openai".to_string(), cloud);

    let llm_config = LlmConfig {
        default: Some("local-custom".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

    assert!(
        models.contains(&"local-custom/local-model".to_string()),
        "No classification should include Local provider models, got: {:?}",
        models
    );
    assert!(
        models.contains(&"cloud-openai/gpt-4o".to_string()),
        "No classification should include Cloud provider models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_trust_includes_cloud_for_internal_kiln() {
    use crucible_config::{
        BackendType, DataClassification, LlmConfig, LlmProviderConfig, TrustLevel,
    };
    use std::collections::HashMap;

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let mut local = LlmProviderConfig::builder(BackendType::Custom)
        .available_models(vec!["local-model".to_string()])
        .build();
    local.trust_level = Some(TrustLevel::Local);

    let mut cloud = LlmProviderConfig::builder(BackendType::OpenAI)
        .available_models(vec!["gpt-4o".to_string()])
        .build();
    cloud.trust_level = Some(TrustLevel::Cloud);

    let mut untrusted = LlmProviderConfig::builder(BackendType::Custom)
        .available_models(vec!["unsafe-model".to_string()])
        .build();
    untrusted.trust_level = Some(TrustLevel::Untrusted);

    let mut providers = HashMap::new();
    providers.insert("local-custom".to_string(), local);
    providers.insert("cloud-openai".to_string(), cloud);
    providers.insert("untrusted-custom".to_string(), untrusted);

    let llm_config = LlmConfig {
        default: Some("local-custom".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager
        .list_models(&session.id, Some(DataClassification::Internal))
        .await
        .unwrap();

    assert!(
        models.contains(&"local-custom/local-model".to_string()),
        "Internal should include Local provider models, got: {:?}",
        models
    );
    assert!(
        models.contains(&"cloud-openai/gpt-4o".to_string()),
        "Internal should include Cloud provider models, got: {:?}",
        models
    );
    assert!(
        !models.iter().any(|m| m.starts_with("untrusted-custom/")),
        "Internal should exclude Untrusted provider models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_all_chat_backends_with_explicit_models() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};
    use std::collections::HashMap;

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    // With available_models set, discover_models short-circuits without HTTP.
    // The mock server is kept for the endpoint URL but never contacted.
    let (ollama_endpoint, ollama_server) = start_mock_ollama_tags_server(vec!["llama3.2"]).await;

    let mut providers = HashMap::new();
    providers.insert(
        "ollama-local".to_string(),
        LlmProviderConfig::builder(BackendType::Ollama)
            .endpoint(ollama_endpoint)
            .available_models(vec!["llama3.2".to_string()])
            .build(),
    );
    providers.insert(
        "openai-main".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["gpt-4o".to_string()])
            .build(),
    );
    providers.insert(
        "anthropic-main".to_string(),
        LlmProviderConfig::builder(BackendType::Anthropic)
            .available_models(vec!["claude-sonnet-4-20250514".to_string()])
            .build(),
    );
    providers.insert(
        "cohere-main".to_string(),
        LlmProviderConfig::builder(BackendType::Cohere)
            .available_models(vec!["command-r-plus".to_string()])
            .build(),
    );
    providers.insert(
        "vertex-main".to_string(),
        LlmProviderConfig::builder(BackendType::VertexAI)
            .available_models(vec!["gemini-1.5-pro".to_string()])
            .build(),
    );
    providers.insert(
        "copilot-main".to_string(),
        LlmProviderConfig::builder(BackendType::GitHubCopilot)
            .available_models(vec!["gpt-4o".to_string()])
            .build(),
    );
    providers.insert(
        "openrouter-main".to_string(),
        LlmProviderConfig::builder(BackendType::OpenRouter)
            .available_models(vec!["openai/gpt-4o".to_string()])
            .build(),
    );
    providers.insert(
        "zai-main".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .available_models(vec!["GLM-4.7".to_string()])
            .build(),
    );
    providers.insert(
        "custom-main".to_string(),
        LlmProviderConfig::builder(BackendType::Custom)
            .available_models(vec!["my-custom-model".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("ollama-local".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    ollama_server.abort(); // Server never receives request with available_models set

    let expected_models = [
        "ollama-local/llama3.2",
        "openai-main/gpt-4o",
        "anthropic-main/claude-sonnet-4-20250514",
        "cohere-main/command-r-plus",
        "vertex-main/gemini-1.5-pro",
        "copilot-main/gpt-4o",
        "openrouter-main/openai/gpt-4o",
        "zai-main/GLM-4.7",
        "custom-main/my-custom-model",
    ];

    for expected in expected_models {
        assert!(
            models.contains(&expected.to_string()),
            "Missing model {expected}, got: {:?}",
            models
        );
    }
}

#[tokio::test]
async fn test_list_models_discovery_failure_returns_empty() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};
    use std::collections::HashMap;

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    // Use dead endpoints to force discovery failure
    let dead_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead_addr = dead_listener.local_addr().unwrap();
    drop(dead_listener);
    let dead_endpoint = format!("http://{}", dead_addr);

    let mut providers = HashMap::new();
    providers.insert(
        "anthropic-dead".to_string(),
        LlmProviderConfig::builder(BackendType::Anthropic)
            .endpoint(&dead_endpoint)
            .build(),
    );
    providers.insert(
        "openai-dead".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .endpoint(&dead_endpoint)
            .build(),
    );
    providers.insert(
        "zai-dead".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .endpoint(&dead_endpoint)
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("anthropic-dead".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

    // Without available_models and with dead endpoints, all providers return empty
    assert!(
        models.is_empty(),
        "Failed discovery without available_models should return empty, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_count_matches_sum() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};
    use std::collections::HashMap;

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let mut providers = HashMap::new();
    providers.insert(
        "openai-count".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["gpt-4o".to_string(), "o3-mini".to_string()])
            .build(),
    );
    providers.insert(
        "anthropic-count".to_string(),
        LlmProviderConfig::builder(BackendType::Anthropic)
            .available_models(vec!["claude-3-7-sonnet-20250219".to_string()])
            .build(),
    );
    providers.insert(
        "zai-count".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .available_models(vec!["GLM-5".to_string(), "GLM-4.7".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-count".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    let expected_total = 2 + 1 + 2;

    assert_eq!(
        models.len(),
        expected_total,
        "Expected {} models total, got {:?}",
        expected_total,
        models
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn test_list_models_no_llm_config() {
    let _env_lock = ENV_LOCK.lock().expect("env lock poisoned");
    let _env_guards = clear_provider_env();

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let (event_tx, _) = broadcast::channel(16);
    let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
    let agent_manager = AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: session_manager.clone(),
        background_manager,
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        permission_config: None,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(std::path::PathBuf::from("/tmp"))),
    });

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

    assert!(
        models.is_empty()
            || models
                .iter()
                .all(|m| m.starts_with("[error]") || !m.contains('/')),
        "Should not prefix models when llm_config is None"
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn test_list_models_includes_env_discovered_providers() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};
    use std::collections::HashMap;

    let _env_lock = ENV_LOCK.lock().expect("env lock poisoned");
    let _env_guards = clear_provider_env();
    let _glm_guard = EnvVarGuard::set("GLM_AUTH_TOKEN", "test-token".to_string());

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    // Configure ZAI provider with static available_models (no network needed)
    let mut providers = HashMap::new();
    providers.insert(
        "zai".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .available_models(vec!["glm-4".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: None,
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    assert!(
        models.iter().any(|model| model.starts_with("zai/")),
        "Expected ZAI models with zai/ prefix, got: {:?}",
        models
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn test_list_models_classification_filters_env_providers() {
    use crucible_config::{
        BackendType, DataClassification, LlmConfig, LlmProviderConfig, TrustLevel,
    };
    use std::collections::HashMap;

    let _env_lock = ENV_LOCK.lock().expect("env lock poisoned");
    let _env_guards = clear_provider_env();
    let _glm_guard = EnvVarGuard::set("GLM_AUTH_TOKEN", "test-token".to_string());

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    // Configure ZAI provider with Cloud trust level (simulates env-discovered provider)
    let mut providers = HashMap::new();
    let mut zai_config = LlmProviderConfig::builder(BackendType::ZAI)
        .available_models(vec!["glm-4".to_string()])
        .build();
    zai_config.trust_level = Some(TrustLevel::Cloud);
    providers.insert("zai".to_string(), zai_config);

    let llm_config = LlmConfig {
        default: None,
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager
        .list_models(&session.id, Some(DataClassification::Confidential))
        .await
        .unwrap();
    assert!(
        !models.iter().any(|model| model.starts_with("zai/")),
        "Expected Confidential classification to filter Cloud providers, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_prefixes_with_provider_key() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};
    use std::collections::HashMap;

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let mut providers = HashMap::new();
    providers.insert(
        "anthropic".to_string(),
        LlmProviderConfig::builder(BackendType::Anthropic)
            .available_models(vec!["claude-3-opus".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("zai-coding".to_string()),
        providers,
    };

    let (event_tx, _) = broadcast::channel(16);
    let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
    let agent_manager = AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: session_manager.clone(),
        background_manager,
        mcp_gateway: None,
        llm_config: Some(llm_config),
        acp_config: None,
        permission_config: None,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(std::path::PathBuf::from("/tmp"))),
    });

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

    assert!(
        models.contains(&"anthropic/claude-3-opus".to_string()),
        "Should prefix with provider key: {:?}",
        models
    );
}

#[test]
fn test_openai_compatible_parses_data_models_prefers_id() {
    let payload = serde_json::json!({
        "data": [
            { "id": "gpt-4o", "name": "ignored-name" },
            { "name": "fallback-name" },
            { "id": "gpt-4o-mini" }
        ]
    });

    let models =
        crate::provider::model_listing::openai_compat::parse_models_response(&payload.to_string())
            .unwrap();

    assert_eq!(
        models,
        vec![
            "gpt-4o".to_string(),
            "fallback-name".to_string(),
            "gpt-4o-mini".to_string()
        ]
    );
}

#[test]
fn test_openai_compatible_parses_models_fallback_shape() {
    let payload = serde_json::json!({
        "models": [
            { "name": "llama-3.1-70b" },
            { "id": "deepseek-chat" }
        ]
    });

    let models =
        crate::provider::model_listing::openai_compat::parse_models_response(&payload.to_string())
            .unwrap();

    assert_eq!(
        models,
        vec!["llama-3.1-70b".to_string(), "deepseek-chat".to_string()]
    );
}

#[test]
fn test_openai_compatible_missing_both_keys_errors() {
    // Neither 'data' nor 'models' key → error
    let payload = serde_json::json!({
        "other_key": []
    });

    let result =
        crate::provider::model_listing::openai_compat::parse_models_response(&payload.to_string());

    assert!(result.is_err());
}

#[tokio::test]
async fn test_openai_compatible_http_includes_auth_header_and_trims_endpoint() {
    let (endpoint, server) = start_mock_openai_models_server(
        200,
        serde_json::json!({
            "data": [
                { "id": "gpt-4o" },
                { "name": "gpt-4.1-mini" }
            ]
        }),
        Some("test-key"),
    )
    .await;

    let models =
        crate::provider::model_listing::openai_compat::list_models(&(endpoint + "/"), "test-key")
            .await
            .unwrap();
    server.await.unwrap();

    assert_eq!(
        models,
        vec!["gpt-4o".to_string(), "gpt-4.1-mini".to_string()]
    );
}

#[tokio::test]
async fn test_openai_compatible_non_success_status_returns_error() {
    let (endpoint, server) = start_mock_openai_models_server(
        503,
        serde_json::json!({ "error": "service unavailable" }),
        None,
    )
    .await;

    let result = crate::provider::model_listing::openai_compat::list_models(&endpoint, "").await;
    server.await.unwrap();

    assert!(result.is_err());
}

#[tokio::test]
async fn test_openai_compatible_connection_failure_returns_error() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let endpoint = format!("http://{}", addr);
    let result = crate::provider::model_listing::openai_compat::list_models(&endpoint, "").await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_parse_provider_model_llm_config_found() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

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
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

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
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

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
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

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
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

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
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

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

#[tokio::test]
async fn test_switch_model_zai_llm_config() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

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
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    // Switch to zai-coding/GLM-4.7
    agent_manager
        .switch_model(&session.id, "zai-coding/GLM-4.7", None)
        .await
        .unwrap();

    let updated = session_manager.get_session(&session.id).unwrap();
    let agent = updated.agent.as_ref().unwrap();

    assert_eq!(agent.model, "GLM-4.7", "Model should be updated");
    assert_eq!(
        agent.provider,
        BackendType::ZAI,
        "Provider should be set to zai via as_str()"
    );
    assert_eq!(
        agent.provider_key.as_deref(),
        Some("zai-coding"),
        "Provider key should be set"
    );
    assert_eq!(
        agent.endpoint.as_deref(),
        Some("https://api.z.ai/api/coding/paas/v4"),
        "Endpoint should be updated from llm_config"
    );
}

#[tokio::test]
async fn test_switch_model_legacy_still_works() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let mut llm_providers = std::collections::HashMap::new();
    llm_providers.insert(
        "local".to_string(),
        LlmProviderConfig::builder(BackendType::Ollama)
            .endpoint("http://localhost:11434")
            .build(),
    );
    llm_providers.insert(
        "zai-coding".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .endpoint("https://api.z.ai/api/coding/paas/v4")
            .build(),
    );
    let llm_config = LlmConfig {
        default: None,
        providers: llm_providers,
    };

    let agent_manager = create_test_agent_manager_with_both(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    // Switch using legacy config key
    agent_manager
        .switch_model(&session.id, "local/llama3.3", None)
        .await
        .unwrap();

    let updated = session_manager.get_session(&session.id).unwrap();
    let agent = updated.agent.as_ref().unwrap();

    assert_eq!(agent.model, "llama3.3", "Model should be updated");
    assert_eq!(
        agent.provider,
        BackendType::Ollama,
        "Provider should be set from llm config"
    );
    assert_eq!(
        agent.provider_key.as_deref(),
        Some("local"),
        "Provider key should be set"
    );
    assert_eq!(
        agent.endpoint.as_deref(),
        Some("http://localhost:11434"),
        "Endpoint should come from llm config"
    );
}

#[tokio::test]
async fn test_switch_model_llm_config_invalidates_cache() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let mut providers = std::collections::HashMap::new();
    providers.insert(
        "zai-coding".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .endpoint("https://api.z.ai/api/coding/paas/v4")
            .build(),
    );

    let llm_config = LlmConfig {
        default: None,
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    agent_manager
        .switch_model(&session.id, "zai-coding/GLM-4.7", None)
        .await
        .unwrap();

    assert!(
        !agent_manager.agent_cache.contains_key(&session.id),
        "Cache should be invalidated after llm_config cross-provider switch"
    );
}

#[tokio::test]
async fn test_switch_model_unknown_provider_prefix() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    agent_manager
        .switch_model(&session.id, "unknown-provider/model", None)
        .await
        .unwrap();

    let updated = session_manager.get_session(&session.id).unwrap();
    let agent = updated.agent.as_ref().unwrap();

    assert_eq!(
        agent.model, "unknown-provider/model",
        "Unknown provider should be treated as model name"
    );
    assert_eq!(
        agent.provider,
        BackendType::Ollama,
        "Provider should remain unchanged (default)"
    );
    assert_eq!(
        agent.provider_key.as_deref(),
        Some("ollama"),
        "Provider key should remain unchanged"
    );
}

#[tokio::test]
async fn test_switch_model_org_slash_model_format() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let agent_manager = create_test_agent_manager(session_manager.clone());

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    agent_manager
        .switch_model(&session.id, "meta-llama/llama-3.2-1b", None)
        .await
        .unwrap();

    let updated = session_manager.get_session(&session.id).unwrap();
    let agent = updated.agent.as_ref().unwrap();

    assert_eq!(
        agent.model, "meta-llama/llama-3.2-1b",
        "Org/model format should be treated as full model name"
    );
    assert_eq!(
        agent.provider,
        BackendType::Ollama,
        "Provider should remain unchanged (default)"
    );
}

#[tokio::test]
async fn test_list_models_multi_provider_with_zai() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};
    use std::collections::HashMap;

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let mut providers = HashMap::new();
    providers.insert(
        "zai-coding".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .endpoint("https://api.z.ai/api/coding/paas/v4")
            .available_models(vec![
                "GLM-5".to_string(),
                "GLM-4.7".to_string(),
                "GLM-4.5-Air".to_string(),
            ])
            .build(),
    );
    providers.insert(
        "openai".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["gpt-4".to_string(), "gpt-3.5-turbo".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("ollama".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

    // Verify ZAI models are present with correct prefix
    assert!(
        models.iter().any(|m| m.starts_with("zai-coding/")),
        "Should have zai-coding/ prefixed models, got: {:?}",
        models
    );
    assert!(
        models.contains(&"zai-coding/GLM-5".to_string()),
        "Should contain zai-coding/GLM-5, got: {:?}",
        models
    );
    assert!(
        models.contains(&"zai-coding/GLM-4.7".to_string()),
        "Should contain zai-coding/GLM-4.7, got: {:?}",
        models
    );
    assert!(
        models.contains(&"zai-coding/GLM-4.5-Air".to_string()),
        "Should contain zai-coding/GLM-4.5-Air, got: {:?}",
        models
    );

    // Verify OpenAI models are also present
    assert!(
        models.contains(&"openai/gpt-4".to_string()),
        "Should contain openai/gpt-4, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_legacy_providers_config() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let mut providers = std::collections::HashMap::new();
    providers.insert(
        "openai".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec![
                "gpt-4".to_string(),
                "text-embedding-3-small".to_string(),
            ])
            .build(),
    );
    providers.insert(
        "anthropic".to_string(),
        LlmProviderConfig::builder(BackendType::Anthropic)
            .available_models(vec!["claude-3-opus".to_string()])
            .build(),
    );
    let llm_config = LlmConfig {
        default: Some("openai".to_string()),
        providers,
    };
    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

    assert!(
        models.contains(&"openai/gpt-4".to_string()),
        "Should contain openai/gpt-4 from legacy config, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openai/text-embedding-3-small".to_string()),
        "Should contain openai/text-embedding-3-small from legacy config, got: {:?}",
        models
    );
    assert!(
        models.contains(&"anthropic/claude-3-opus".to_string()),
        "Should contain anthropic/claude-3-opus from legacy config, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_both_configs() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};
    use std::collections::HashMap;

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let mut llm_providers = HashMap::new();
    llm_providers.insert(
        "legacy-openai".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["gpt-3.5-turbo".to_string()])
            .build(),
    );
    llm_providers.insert(
        "new-anthropic".to_string(),
        LlmProviderConfig::builder(BackendType::Anthropic)
            .available_models(vec!["claude-sonnet-4".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("new-anthropic".to_string()),
        providers: llm_providers,
    };

    let agent_manager = create_test_agent_manager_with_both(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

    assert!(
        models.contains(&"new-anthropic/claude-sonnet-4".to_string()),
        "Should contain new-anthropic/claude-sonnet-4 from LlmConfig, got: {:?}",
        models
    );
    assert!(
        models.contains(&"legacy-openai/gpt-3.5-turbo".to_string()),
        "Should contain legacy-openai/gpt-3.5-turbo, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_switch_model_to_zai_provider() {
    use crucible_config::{BackendType, LlmConfig, LlmProviderConfig};
    use std::collections::HashMap;

    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let mut providers = HashMap::new();
    providers.insert(
        "openai".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .model("gpt-4")
            .build(),
    );
    providers.insert(
        "zai-coding".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .endpoint("https://api.z.ai/api/coding/paas/v4")
            .available_models(vec![
                "GLM-5".to_string(),
                "GLM-4.7".to_string(),
                "GLM-4.5-Air".to_string(),
            ])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    // Configure with OpenAI provider
    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    // Switch to ZAI provider with GLM-4.7 model
    agent_manager
        .switch_model(&session.id, "zai-coding/GLM-4.7", None)
        .await
        .unwrap();

    // Verify the agent was updated
    let updated = session_manager.get_session(&session.id).unwrap();
    let agent = updated.agent.as_ref().unwrap();

    assert_eq!(
        agent.provider_key.as_deref(),
        Some("zai-coding"),
        "Provider key should be updated to zai-coding"
    );
    assert_eq!(agent.model, "GLM-4.7", "Model should be updated to GLM-4.7");
    assert_eq!(
        agent.endpoint.as_deref(),
        Some("https://api.z.ai/api/coding/paas/v4"),
        "Endpoint should be updated to ZAI Coding Plan endpoint"
    );

    // Verify cache was invalidated
    assert!(
        !agent_manager.agent_cache.contains_key(&session.id),
        "Cache should be invalidated after cross-provider switch to ZAI"
    );
}

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
