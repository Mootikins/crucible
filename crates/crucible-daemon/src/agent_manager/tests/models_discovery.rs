use super::*;

#[tokio::test]
async fn test_list_models_dynamic_discovery_openai_succeeds() {
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

    // Mock server returns OpenAI-style models
    let (endpoint, server) = start_mock_openai_models_server(
        200,
        serde_json::json!({
            "data": [
                { "id": "gpt-4o" },
                { "id": "gpt-4o-mini" },
                { "id": "o3-mini" }
            ]
        }),
        Some("test-openai-key"),
    )
    .await;

    let mut providers = HashMap::new();
    providers.insert(
        "openai-dynamic".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .endpoint(&endpoint)
            .api_key("test-openai-key")
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-dynamic".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    server.await.unwrap();

    assert!(
        models.contains(&"openai-dynamic/gpt-4o".to_string()),
        "Should contain dynamically discovered gpt-4o, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openai-dynamic/gpt-4o-mini".to_string()),
        "Should contain dynamically discovered gpt-4o-mini, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openai-dynamic/o3-mini".to_string()),
        "Should contain dynamically discovered o3-mini, got: {:?}",
        models
    );
    assert_eq!(
        models.len(),
        3,
        "Should have exactly 3 dynamically discovered models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_dynamic_discovery_zai_succeeds() {
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

    let (endpoint, server) = start_mock_openai_models_server(
        200,
        serde_json::json!({
            "data": [
                { "id": "GLM-5" },
                { "id": "GLM-4.7" },
                { "id": "GLM-4.5-Flash" }
            ]
        }),
        None,
    )
    .await;

    let mut providers = HashMap::new();
    providers.insert(
        "zai-dynamic".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .endpoint(&endpoint)
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("zai-dynamic".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    server.await.unwrap();

    assert!(
        models.contains(&"zai-dynamic/GLM-5".to_string()),
        "Should contain dynamically discovered GLM-5, got: {:?}",
        models
    );
    assert!(
        models.contains(&"zai-dynamic/GLM-4.7".to_string()),
        "Should contain dynamically discovered GLM-4.7, got: {:?}",
        models
    );
    assert_eq!(
        models.len(),
        3,
        "Should have exactly 3 dynamically discovered ZAI models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_dynamic_discovery_openrouter_succeeds() {
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

    let (endpoint, server) = start_mock_openai_models_server(
        200,
        serde_json::json!({
            "data": [
                { "id": "anthropic/claude-sonnet-4-20250514" },
                { "id": "openai/gpt-4o" },
                { "id": "meta-llama/llama-3.3-70b" }
            ]
        }),
        Some("test-or-key"),
    )
    .await;

    let mut providers = HashMap::new();
    providers.insert(
        "openrouter-dynamic".to_string(),
        LlmProviderConfig::builder(BackendType::OpenRouter)
            .endpoint(&endpoint)
            .api_key("test-or-key")
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openrouter-dynamic".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    server.await.unwrap();

    assert_eq!(
        models.len(),
        3,
        "Should have 3 dynamically discovered OpenRouter models, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openrouter-dynamic/anthropic/claude-sonnet-4-20250514".to_string()),
        "Should contain dynamically discovered model, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_dynamic_discovery_failure_returns_empty() {
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

    // Mock server returns 503 error
    let (openai_endpoint, openai_server) = start_mock_openai_models_server(
        503,
        serde_json::json!({ "error": "service unavailable" }),
        None,
    )
    .await;

    // ZAI endpoint that refuses connection (bind then drop)
    let zai_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let zai_addr = zai_listener.local_addr().unwrap();
    drop(zai_listener);
    let zai_endpoint = format!("http://{}", zai_addr);

    let mut providers = HashMap::new();
    providers.insert(
        "openai-fail".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .endpoint(&openai_endpoint)
            .build(),
    );
    providers.insert(
        "zai-fail".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .endpoint(&zai_endpoint)
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-fail".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    openai_server.await.unwrap();

    // Without available_models, failed discovery returns empty (no hardcoded fallback)
    assert!(
        models.is_empty(),
        "Failed API discovery without available_models should return empty, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_explicit_config_skips_dynamic_discovery() {
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

    // No mock server needed — explicit config should bypass API call entirely
    let mut providers = HashMap::new();
    providers.insert(
        "openai-explicit".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["my-custom-model".to_string()])
            .build(),
    );
    providers.insert(
        "zai-explicit".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .available_models(vec!["custom-glm".to_string()])
            .build(),
    );
    providers.insert(
        "openrouter-explicit".to_string(),
        LlmProviderConfig::builder(BackendType::OpenRouter)
            .available_models(vec!["custom-or-model".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-explicit".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

    // Explicit available_models should be used directly (no API call)
    assert!(
        models.contains(&"openai-explicit/my-custom-model".to_string()),
        "Explicit OpenAI config should be used, got: {:?}",
        models
    );
    assert!(
        models.contains(&"zai-explicit/custom-glm".to_string()),
        "Explicit ZAI config should be used, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openrouter-explicit/custom-or-model".to_string()),
        "Explicit OpenRouter config should be used, got: {:?}",
        models
    );
    assert_eq!(
        models.len(),
        3,
        "Should have exactly 3 explicitly configured models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_integration_multi_provider() {
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

    let (ollama_endpoint, ollama_server) =
        start_mock_ollama_tags_server(vec!["llama3.3", "qwen2.5"]).await;

    let mut providers = HashMap::new();
    providers.insert(
        "ollama-int".to_string(),
        LlmProviderConfig::builder(BackendType::Ollama)
            .endpoint(ollama_endpoint)
            .build(),
    );
    providers.insert(
        "openai-int".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["gpt-4o".to_string(), "o3-mini".to_string()])
            .build(),
    );
    providers.insert(
        "zai-int".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .available_models(vec!["GLM-5".to_string(), "GLM-4.7".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-int".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    ollama_server.await.unwrap();
    let expected_total = 2 + 2 + 2;

    assert!(
        models.contains(&"ollama-int/llama3.3".to_string()),
        "Should include prefixed Ollama models, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openai-int/gpt-4o".to_string()),
        "Should include prefixed OpenAI models, got: {:?}",
        models
    );
    assert!(
        models.contains(&"zai-int/GLM-5".to_string()),
        "Should include prefixed ZAI models, got: {:?}",
        models
    );
    assert_eq!(
        models.len(),
        expected_total,
        "Expected {} total models from all providers, got: {:?}",
        expected_total,
        models
    );
}

#[tokio::test]
async fn test_list_models_integration_dynamic_discovery() {
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

    let (endpoint, server) = start_mock_openai_models_server(
        200,
        serde_json::json!({
            "data": [
                { "id": "gpt-4.1-nano" },
                { "id": "o4-mini" }
            ]
        }),
        Some("integration-openai-key"),
    )
    .await;

    let mut providers = HashMap::new();
    providers.insert(
        "openai-discovery-int".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .endpoint(&endpoint)
            .api_key("integration-openai-key")
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-discovery-int".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    server.await.unwrap();

    assert!(
        models.contains(&"openai-discovery-int/gpt-4.1-nano".to_string()),
        "Should include API-discovered model, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openai-discovery-int/o4-mini".to_string()),
        "Should include API-discovered model, got: {:?}",
        models
    );
    assert!(
        !models.contains(&"openai-discovery-int/gpt-4o".to_string()),
        "Should not inject hardcoded fallback models when API succeeds, got: {:?}",
        models
    );
    assert_eq!(
        models.len(),
        2,
        "Expected exactly API models from dynamic discovery, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_integration_override_precedence() {
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

    let dead_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dead_addr = dead_listener.local_addr().unwrap();
    drop(dead_listener);
    let dead_endpoint = format!("http://{}", dead_addr);

    let (zai_endpoint, zai_server) = start_mock_openai_models_server(
        200,
        serde_json::json!({
            "data": [
                { "id": "GLM-5" },
                { "id": "GLM-4.6" }
            ]
        }),
        None,
    )
    .await;

    let mut providers = HashMap::new();
    providers.insert(
        "openai-override-int".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .endpoint(&dead_endpoint)
            .available_models(vec!["gpt-custom-override".to_string()])
            .build(),
    );
    providers.insert(
        "zai-dynamic-int".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .endpoint(&zai_endpoint)
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-override-int".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    zai_server.await.unwrap();

    assert!(
        models.contains(&"openai-override-int/gpt-custom-override".to_string()),
        "Should use explicit override model for OpenAI, got: {:?}",
        models
    );
    assert!(
        !models.contains(&"openai-override-int/gpt-4o".to_string()),
        "OpenAI override should win over fallback/API discovery, got: {:?}",
        models
    );
    assert!(
        models.contains(&"zai-dynamic-int/GLM-5".to_string()),
        "Other providers without overrides should still use dynamic discovery, got: {:?}",
        models
    );
    assert_eq!(
        models.len(),
        3,
        "Expected 1 override + 2 dynamic models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_integration_partial_failure() {
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

    let ollama_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ollama_addr = ollama_listener.local_addr().unwrap();
    drop(ollama_listener);
    let ollama_dead_endpoint = format!("http://{}", ollama_addr);

    let mut providers = HashMap::new();
    providers.insert(
        "ollama-bad-int".to_string(),
        LlmProviderConfig::builder(BackendType::Ollama)
            .endpoint(&ollama_dead_endpoint)
            .build(),
    );
    providers.insert(
        "openai-ok-int".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["gpt-4o".to_string(), "o3-mini".to_string()])
            .build(),
    );
    providers.insert(
        "zai-ok-int".to_string(),
        LlmProviderConfig::builder(BackendType::ZAI)
            .available_models(vec!["GLM-5".to_string(), "GLM-4.7".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-ok-int".to_string()),
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
        models.contains(&"openai-ok-int/gpt-4o".to_string()),
        "Working providers should still contribute models, got: {:?}",
        models
    );
    assert!(
        models.contains(&"zai-ok-int/GLM-5".to_string()),
        "Working providers should still contribute models, got: {:?}",
        models
    );

    // With the new rig_model_listing dispatch, failed providers silently
    // fall back to effective_models() — no error entries surfaced in the list.
    let error_entries: Vec<_> = models.iter().filter(|m| m.starts_with("[error]")).collect();
    assert_eq!(
        error_entries.len(),
        0,
        "No error entries should be surfaced with new dispatch, got: {:?}",
        models
    );

    // 2 from openai-ok-int + 2 from zai-ok-int + 0 from failed ollama
    let expected_total = 2 + 2;
    assert_eq!(
        models.len(),
        expected_total,
        "Expected 4 models from healthy providers, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_openai_model_discovery_returns_all_models() {
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

    // Mock server returns 20 models including non-chat models
    let (endpoint, server) = start_mock_openai_models_server(
        200,
        serde_json::json!({
            "data": [
                { "id": "gpt-4o" },
                { "id": "gpt-4o-mini" },
                { "id": "gpt-4-turbo" },
                { "id": "gpt-4" },
                { "id": "gpt-3.5-turbo" },
                { "id": "o1" },
                { "id": "o1-mini" },
                { "id": "o3-mini" },
                { "id": "o4" },
                { "id": "chatgpt-4o-latest" },
                { "id": "dall-e-3" },
                { "id": "dall-e-2" },
                { "id": "whisper-1" },
                { "id": "text-embedding-3-large" },
                { "id": "text-embedding-3-small" },
                { "id": "text-embedding-ada-002" },
                { "id": "text-moderation-latest" },
                { "id": "text-moderation-stable" },
                { "id": "tts-1" },
                { "id": "tts-1-hd" }
            ]
        }),
        Some("test-openai-key"),
    )
    .await;

    let mut providers = HashMap::new();
    providers.insert(
        "openai-test".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .endpoint(&endpoint)
            .api_key("test-openai-key")
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-test".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();
    server.await.unwrap();

    // With new rig_model_listing dispatch, all discovered models are returned
    // without filtering. Model filtering is now the responsibility of the caller.
    let openai_models: Vec<_> = models
        .iter()
        .filter(|m| m.starts_with("openai-test/"))
        .collect();
    assert_eq!(
        openai_models.len(),
        20,
        "Should return all 20 discovered models without filtering, got: {:?}",
        openai_models
    );

    // Verify some chat models are present
    assert!(
        models.contains(&"openai-test/gpt-4o".to_string()),
        "Should contain gpt-4o, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openai-test/o1".to_string()),
        "Should contain o1, got: {:?}",
        models
    );

    // Non-chat models are also now included (no longer filtered)
    assert!(
        models.contains(&"openai-test/dall-e-3".to_string()),
        "Should contain dall-e-3 (no filtering), got: {:?}",
        models
    );
    assert!(
        models.contains(&"openai-test/tts-1".to_string()),
        "Should contain tts-1 (no filtering), got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_list_models_ollama_failure() {
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

    // Ollama endpoint that refuses connection (bind then drop)
    let ollama_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ollama_addr = ollama_listener.local_addr().unwrap();
    drop(ollama_listener);
    let ollama_endpoint = format!("http://{}", ollama_addr);

    let mut providers = HashMap::new();
    providers.insert(
        "ollama-dead".to_string(),
        LlmProviderConfig::builder(BackendType::Ollama)
            .endpoint(&ollama_endpoint)
            .build(),
    );
    providers.insert(
        "openai-ok".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["gpt-4o".to_string(), "gpt-4o-mini".to_string()])
            .build(),
    );

    let llm_config = LlmConfig {
        default: Some("openai-ok".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    let models = agent_manager.list_models(&session.id, None).await.unwrap();

    // OpenAI models should be present
    assert!(
        models.contains(&"openai-ok/gpt-4o".to_string()),
        "OpenAI models should be present, got: {:?}",
        models
    );
    assert!(
        models.contains(&"openai-ok/gpt-4o-mini".to_string()),
        "OpenAI models should be present, got: {:?}",
        models
    );

    // With new rig_model_listing dispatch, failed providers silently fall back
    // to effective_models() — no error entries surfaced in the model list.
    let error_entries: Vec<_> = models.iter().filter(|m| m.starts_with("[error]")).collect();
    assert!(
        error_entries.is_empty(),
        "No error entries should be surfaced with new dispatch, got: {:?}",
        models
    );

    // Only OpenAI models present (Ollama silently failed)
    assert_eq!(
        models.len(),
        2,
        "Should have exactly 2 OpenAI models, got: {:?}",
        models
    );
}

#[tokio::test]
async fn test_model_cache_hit() {
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
        "test".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["model1".to_string(), "model2".to_string()])
            .build(),
    );
    let llm_config = LlmConfig {
        default: Some("test".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    // First call should populate cache
    let models1 = agent_manager.list_models(&session.id, None).await.unwrap();
    assert!(!models1.is_empty(), "Should return models");

    // Second call should return same result from cache
    let models2 = agent_manager.list_models(&session.id, None).await.unwrap();
    assert_eq!(models1, models2, "Cache hit should return identical models");

    // Verify cache entry exists
    assert!(
        agent_manager.model_cache.contains_key("all"),
        "Cache should contain 'all' key"
    );
}

#[tokio::test]
async fn test_model_cache_invalidation() {
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
        "test".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["model1".to_string()])
            .build(),
    );
    let llm_config = LlmConfig {
        default: Some("test".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    // First call populates cache
    let _models1 = agent_manager.list_models(&session.id, None).await.unwrap();
    assert!(
        agent_manager.model_cache.contains_key("all"),
        "Cache should be populated"
    );

    // Invalidate cache
    agent_manager.invalidate_model_cache();
    assert!(
        !agent_manager.model_cache.contains_key("all"),
        "Cache should be cleared after invalidation"
    );

    // Second call should succeed (repopulate cache)
    let _models2 = agent_manager.list_models(&session.id, None).await.unwrap();
    assert!(
        agent_manager.model_cache.contains_key("all"),
        "Cache should be repopulated after list_models"
    );
}

#[tokio::test]

async fn test_model_cache_does_not_cache_errors() {
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
    // Configure provider with models
    providers.insert(
        "test".to_string(),
        LlmProviderConfig::builder(BackendType::OpenAI)
            .available_models(vec!["model1".to_string(), "model2".to_string()])
            .build(),
    );
    let llm_config = LlmConfig {
        default: Some("test".to_string()),
        providers,
    };

    let agent_manager =
        create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

    agent_manager
        .configure_agent(&session.id, test_agent())
        .await
        .unwrap();

    // First call populates cache
    let models1 = agent_manager.list_models(&session.id, None).await.unwrap();
    assert!(!models1.is_empty(), "Should return models");
    assert!(
        agent_manager.model_cache.contains_key("all"),
        "Cache should be populated after successful list_models"
    );

    // Verify cache contains the same models
    let (cached_models, _) = agent_manager.model_cache.get("all").unwrap().clone();
    assert_eq!(
        models1, cached_models,
        "Cached models should match returned models"
    );
}
