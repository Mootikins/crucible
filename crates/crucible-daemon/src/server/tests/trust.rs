use super::*;

#[tokio::test]
async fn cloud_provider_confidential_kiln_returns_insufficient_error() {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    let kiln = workspace.join("notes");
    std::fs::create_dir_all(&kiln).unwrap();
    write_workspace_config(&workspace, "./notes", Some("confidential"));

    let llm_config = Some(build_llm_config(
        "cloud",
        crucible_core::config::BackendType::OpenAI,
    ));
    let request = create_session_request(&kiln, &workspace, "cloud");

    let storage = Arc::new(FileSessionStorage::new());
    let sm = Arc::new(SessionManager::with_storage(storage));
    let pm = Arc::new(ProjectManager::new(tmp.path().join("projects.json")));
    let km = Arc::new(KilnManager::new());

    let (event_tx, _event_rx) = broadcast::channel(16);
    let am = test_agent_manager(km.clone(), sm.clone(), event_tx.clone(), llm_config.clone());
    let response =
        handle_session_create(request, &sm, &pm, &llm_config, &km, &event_tx, &am, None).await;
    let error = response.error.expect("expected trust-level rejection");

    assert_eq!(error.code, INVALID_PARAMS);
    assert!(error.message.contains("insufficient"));
    assert!(error.message.contains("cloud"));
    assert!(error.message.contains("confidential"));
    assert_eq!(sm.list_sessions().len(), 0);
}

#[tokio::test]
async fn local_provider_confidential_kiln_allows_session_creation() {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    let kiln = workspace.join("notes");
    std::fs::create_dir_all(&kiln).unwrap();
    write_workspace_config(&workspace, "./notes", Some("confidential"));

    let llm_config = Some(build_llm_config(
        "local",
        crucible_core::config::BackendType::Mock,
    ));
    let request = create_session_request(&kiln, &workspace, "local");

    let storage = Arc::new(FileSessionStorage::new());
    let sm = Arc::new(SessionManager::with_storage(storage));
    let pm = Arc::new(ProjectManager::new(tmp.path().join("projects.json")));
    let km = Arc::new(KilnManager::new());

    let (event_tx, _event_rx) = broadcast::channel(16);
    let am = test_agent_manager(km.clone(), sm.clone(), event_tx.clone(), llm_config.clone());
    let response =
        handle_session_create(request, &sm, &pm, &llm_config, &km, &event_tx, &am, None).await;

    assert!(response.error.is_none());
    assert!(response.result.is_some());
    assert_eq!(sm.list_sessions().len(), 1);
}

#[tokio::test]
async fn cloud_provider_public_or_missing_classification_allows_session_creation() {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    let kiln = workspace.join("notes");
    std::fs::create_dir_all(&kiln).unwrap();
    write_workspace_config(&workspace, "./notes", None);

    let llm_config = Some(build_llm_config(
        "cloud",
        crucible_core::config::BackendType::OpenAI,
    ));
    let request = create_session_request(&kiln, &workspace, "cloud");

    let storage = Arc::new(FileSessionStorage::new());
    let sm = Arc::new(SessionManager::with_storage(storage));
    let pm = Arc::new(ProjectManager::new(tmp.path().join("projects.json")));
    let km = Arc::new(KilnManager::new());

    let (event_tx, _event_rx) = broadcast::channel(16);
    let am = test_agent_manager(km.clone(), sm.clone(), event_tx.clone(), llm_config.clone());
    let response =
        handle_session_create(request, &sm, &pm, &llm_config, &km, &event_tx, &am, None).await;

    assert!(response.error.is_none());
    assert!(response.result.is_some());
    assert_eq!(sm.list_sessions().len(), 1);
}

#[tokio::test]
async fn untrusted_provider_internal_kiln_returns_error() {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    let kiln = workspace.join("notes");
    std::fs::create_dir_all(&kiln).unwrap();
    write_workspace_config(&workspace, "./notes", Some("internal"));

    let llm_config = Some(build_llm_config_with_trust(
        "untrusted",
        crucible_core::config::BackendType::Custom,
        Some(crucible_core::config::TrustLevel::Untrusted),
    ));
    let request = create_session_request(&kiln, &workspace, "untrusted");

    let storage = Arc::new(FileSessionStorage::new());
    let sm = Arc::new(SessionManager::with_storage(storage));
    let pm = Arc::new(ProjectManager::new(tmp.path().join("projects.json")));
    let km = Arc::new(KilnManager::new());

    let (event_tx, _event_rx) = broadcast::channel(16);
    let am = test_agent_manager(km.clone(), sm.clone(), event_tx.clone(), llm_config.clone());
    let response =
        handle_session_create(request, &sm, &pm, &llm_config, &km, &event_tx, &am, None).await;
    let error = response.error.expect("expected trust-level rejection");

    assert_eq!(error.code, INVALID_PARAMS);
    assert!(error.message.contains("insufficient"));
    assert!(error.message.contains("untrusted"));
    assert!(error.message.contains("internal"));
    assert_eq!(sm.list_sessions().len(), 0);
}

// Tests for resolve_provider_trust_level_for_create
#[test]
fn provider_trust_acp_agent_always_cloud() {
    let req: Request = serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "session.create",
        "params": {
            "agent_type": "acp",
            "kiln": "/tmp/kiln"
        }
    }))
    .unwrap();
    // Even with a Local-trust provider in config, ACP always returns Cloud
    let llm_config = Some(build_llm_config_with_trust(
        "local-provider",
        crucible_core::config::BackendType::Mock,
        Some(crucible_core::config::TrustLevel::Local),
    ));
    let result = resolve_provider_trust_level_for_create(&req, &llm_config);
    assert_eq!(result, crucible_core::config::TrustLevel::Cloud);
}

#[test]
fn provider_trust_bare_backend_name_cloud() {
    let req: Request = serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "session.create",
        "params": {
            "provider": "ollama",
            "kiln": "/tmp/kiln"
        }
    }))
    .unwrap();
    let result = resolve_provider_trust_level_for_create(&req, &None);
    assert_eq!(result, crucible_core::config::TrustLevel::Cloud);
}

#[test]
fn provider_trust_bare_backend_name_local() {
    let req: Request = serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "session.create",
        "params": {
            "provider": "fastembed",
            "kiln": "/tmp/kiln"
        }
    }))
    .unwrap();
    let result = resolve_provider_trust_level_for_create(&req, &None);
    assert_eq!(result, crucible_core::config::TrustLevel::Local);
}

#[test]
fn provider_trust_default_provider_fallback() {
    // No agent_type, no provider_key, no provider → falls back to default provider in llm_config
    let req: Request = serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "session.create",
        "params": {
            "kiln": "/tmp/kiln"
        }
    }))
    .unwrap();
    // Build config where default provider is Local trust
    let llm_config = Some(build_llm_config_with_trust(
        "my-local",
        crucible_core::config::BackendType::Mock,
        Some(crucible_core::config::TrustLevel::Local),
    ));
    let result = resolve_provider_trust_level_for_create(&req, &llm_config);
    assert_eq!(result, crucible_core::config::TrustLevel::Local);
}

// Tests for resolve_kiln_classification_for_create wrapper
#[test]
fn kiln_classification_workspace_none_returns_none() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().join("kiln");
    std::fs::create_dir_all(&kiln).unwrap();
    // No workspace.toml at kiln dir → returns None (no silent default)
    let result = resolve_kiln_classification_for_create(&kiln, None);
    assert_eq!(result, None);
}

#[test]
fn kiln_classification_relative_path_matches() {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    let kiln = workspace.join("notes");
    std::fs::create_dir_all(&kiln).unwrap();
    write_workspace_config(&workspace, "./notes", Some("internal"));
    let result = resolve_kiln_classification_for_create(&kiln, Some(&workspace));
    assert_eq!(
        result,
        Some(crucible_core::config::DataClassification::Internal)
    );
}
