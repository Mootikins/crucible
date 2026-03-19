use crucible_config::BackendType;
use crucible_core::session::{SessionAgent, SessionType};
use crucible_daemon::background_manager::BackgroundJobManager;
use crucible_daemon::tools::workspace::WorkspaceTools;
use crucible_daemon::{
    AgentManager, AgentManagerParams, FileSessionStorage, KilnManager, SessionManager,
};
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::broadcast;

fn make_agent_manager() -> (AgentManager, Arc<SessionManager>, TempDir) {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let (event_tx, _) = broadcast::channel(16);
    let bg = Arc::new(BackgroundJobManager::new(event_tx));
    let agent_manager = AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: session_manager.clone(),
        background_manager: bg,
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        permission_config: None,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(std::path::PathBuf::from("/tmp"))),
    });
    (agent_manager, session_manager, tmp)
}

fn make_session_agent(
    agent_type: &str,
    agent_name: Option<&str>,
    provider: BackendType,
) -> SessionAgent {
    SessionAgent {
        agent_type: agent_type.to_string(),
        agent_name: agent_name.map(|s| s.to_string()),
        provider_key: None,
        provider,
        model: "test-model".to_string(),
        system_prompt: "You are helpful.".to_string(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
    }
}

#[tokio::test]
async fn unknown_agent_type_stores_config_without_error() {
    let (agent_manager, session_manager, tmp) = make_agent_manager();

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

    let agent = make_session_agent("unsupported_alien_type", None, BackendType::Mock);
    let result = agent_manager.configure_agent(&session.id, agent).await;

    assert!(
        result.is_ok(),
        "configure_agent should succeed for unknown type"
    );

    let retrieved = session_manager.get_session(&session.id).unwrap();
    assert!(
        retrieved.agent.is_some(),
        "Agent should be stored in session"
    );
    assert_eq!(
        retrieved.agent.unwrap().agent_type,
        "unsupported_alien_type"
    );
}

#[tokio::test]
async fn unsupported_agent_type_fails_at_send_message_time() {
    let (agent_manager, session_manager, tmp) = make_agent_manager();

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

    let agent = make_session_agent("unsupported_alien_type", None, BackendType::Mock);
    agent_manager
        .configure_agent(&session.id, agent)
        .await
        .unwrap();

    let (event_tx, _) = broadcast::channel(16);
    let result = agent_manager
        .send_message(&session.id, "test message".to_string(), &event_tx, true, None)
        .await;

    assert!(
        result.is_err(),
        "send_message should fail for unsupported agent type"
    );
    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(
        err_msg.contains("unsupported") || err_msg.contains("Unsupported"),
        "Error should mention unsupported type, got: {}",
        err_msg
    );
}

#[tokio::test]
async fn acp_agent_type_with_agent_name_stores_successfully() {
    let (agent_manager, session_manager, tmp) = make_agent_manager();

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

    let agent = make_session_agent("acp", Some("test-agent"), BackendType::Mock);
    let result = agent_manager.configure_agent(&session.id, agent).await;

    assert!(
        result.is_ok(),
        "configure_agent should succeed for ACP with agent_name"
    );

    let retrieved = session_manager.get_session(&session.id).unwrap();
    assert!(
        retrieved.agent.is_some(),
        "Agent should be stored in session"
    );
    let stored_agent = retrieved.agent.unwrap();
    assert_eq!(stored_agent.agent_type, "acp");
    assert_eq!(stored_agent.agent_name, Some("test-agent".to_string()));
}

#[tokio::test]
async fn internal_agent_type_with_mock_provider_stores_successfully() {
    let (agent_manager, session_manager, tmp) = make_agent_manager();

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

    let agent = make_session_agent("internal", None, BackendType::Mock);
    let result = agent_manager.configure_agent(&session.id, agent).await;

    assert!(
        result.is_ok(),
        "configure_agent should succeed for internal with Mock provider"
    );

    let retrieved = session_manager.get_session(&session.id).unwrap();
    assert!(
        retrieved.agent.is_some(),
        "Agent should be stored in session"
    );
    let stored_agent = retrieved.agent.unwrap();
    assert_eq!(stored_agent.agent_type, "internal");
    assert_eq!(stored_agent.provider, BackendType::Mock);
}

#[tokio::test]
async fn internal_agent_type_with_ollama_stores_config() {
    let (agent_manager, session_manager, tmp) = make_agent_manager();

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

    let agent = make_session_agent("internal", None, BackendType::Ollama);
    let result = agent_manager.configure_agent(&session.id, agent).await;

    assert!(
        result.is_ok(),
        "configure_agent should succeed for internal with Ollama"
    );

    let retrieved = session_manager.get_session(&session.id).unwrap();
    assert!(
        retrieved.agent.is_some(),
        "Agent should be stored in session"
    );
    let stored_agent = retrieved.agent.unwrap();
    assert_eq!(stored_agent.agent_type, "internal");
    assert_eq!(stored_agent.provider, BackendType::Ollama);
}

#[tokio::test]
async fn configure_agent_routing_decision_acp_vs_internal() {
    let (agent_manager, session_manager, tmp) = make_agent_manager();

    let session1 = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    let session2 = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let acp_agent = make_session_agent("acp", Some("test-acp"), BackendType::Mock);
    agent_manager
        .configure_agent(&session1.id, acp_agent)
        .await
        .unwrap();

    let internal_agent = make_session_agent("internal", None, BackendType::Mock);
    agent_manager
        .configure_agent(&session2.id, internal_agent)
        .await
        .unwrap();

    let retrieved1 = session_manager.get_session(&session1.id).unwrap();
    let retrieved2 = session_manager.get_session(&session2.id).unwrap();

    assert_eq!(retrieved1.agent.as_ref().unwrap().agent_type, "acp");
    assert_eq!(retrieved2.agent.as_ref().unwrap().agent_type, "internal");

    assert!(retrieved1.agent.as_ref().unwrap().agent_name.is_some());
    assert!(retrieved2.agent.as_ref().unwrap().agent_name.is_none());
}
