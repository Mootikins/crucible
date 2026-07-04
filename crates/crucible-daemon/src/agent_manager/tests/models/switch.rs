use super::super::*;

#[tokio::test]
async fn test_switch_model_zai_llm_config() {
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};

    let (_tmp, session_manager, session) = setup_session_manager().await;

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
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};

    let (_tmp, session_manager, session) = setup_session_manager().await;

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
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};

    let (_tmp, session_manager, session) = setup_session_manager().await;

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
    let (_tmp, session_manager, session) = setup_session_manager().await;

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
    let (_tmp, session_manager, session) = setup_session_manager().await;

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
async fn test_switch_model_to_zai_provider() {
    use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};
    use std::collections::HashMap;

    let (_tmp, session_manager, session) = setup_session_manager().await;

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
