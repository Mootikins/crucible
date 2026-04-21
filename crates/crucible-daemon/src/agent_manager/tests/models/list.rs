use super::super::*;

#[tokio::test]
async fn test_list_models_returns_all_providers() {
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};
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
    use crucible_core::config::{
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
    use crucible_core::config::{
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
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig, TrustLevel};
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
    use crucible_core::config::{
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
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};
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
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};
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
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};
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
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};
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
    use crucible_core::config::{
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
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};
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

#[tokio::test]
async fn test_list_models_multi_provider_with_zai() {
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};
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
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};

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
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};
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
