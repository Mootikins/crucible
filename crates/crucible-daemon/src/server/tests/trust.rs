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
    let ctx = RpcContext::for_test(
        km.clone(),
        sm.clone(),
        am.clone(),
        pm.clone(),
        event_tx.clone(),
        llm_config.clone(),
        tmp.path().to_path_buf(),
    );
    let response = handle_session_create(request, &ctx).await;
    let error = response.error.expect("expected trust-level rejection");

    assert_eq!(error.code, INVALID_PARAMS);
    assert!(error.message.contains("insufficient"));
    assert!(error.message.contains("cloud"));
    assert!(error.message.contains("confidential"));
    assert_eq!(sm.list_sessions().len(), 0);
}

/// The plugin door answers the same as the RPC door.
///
/// `cru.sessions.create` used to call `SessionManager::create_session`
/// directly, so a plugin could open a cloud-provider session on a confidential
/// kiln that `session.create` would have refused — same daemon, same socket,
/// two different answers. It now runs `create_session_resolved`, which is what
/// makes this test a sibling of the one above rather than a near-copy.
#[tokio::test]
async fn bridge_create_refuses_a_cloud_provider_on_a_confidential_kiln() {
    use crucible_lua::DaemonSessionApi;

    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    let kiln = workspace.join("notes");
    std::fs::create_dir_all(&kiln).unwrap();
    write_workspace_config(&workspace, "./notes", Some("confidential"));

    let llm_config = Some(build_llm_config(
        "cloud",
        crucible_core::config::BackendType::OpenAI,
    ));

    let sm = Arc::new(SessionManager::with_storage(Arc::new(
        FileSessionStorage::new(),
    )));
    let pm = Arc::new(ProjectManager::new(tmp.path().join("projects.json")));
    let km = Arc::new(KilnManager::new());
    let (event_tx, _event_rx) = broadcast::channel(16);
    let am = test_agent_manager(km.clone(), sm.clone(), event_tx.clone(), llm_config.clone());
    let bridge = crate::session_bridge::DaemonSessionBridge::new(Arc::new(RpcContext::for_test(
        km,
        sm.clone(),
        am,
        pm,
        event_tx,
        llm_config,
        tmp.path().to_path_buf(),
    )));

    let err = bridge
        .create_session(json!({
            "type": "chat",
            "kiln": kiln,
            "workspace": workspace,
            "provider_key": "cloud",
        }))
        .await
        .expect_err("a plugin must not reach a kiln an RPC client cannot");

    assert!(err.contains("insufficient"), "got: {err}");
    assert!(err.contains("cloud"), "got: {err}");
    assert!(err.contains("confidential"), "got: {err}");
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
    let ctx = RpcContext::for_test(
        km.clone(),
        sm.clone(),
        am.clone(),
        pm.clone(),
        event_tx.clone(),
        llm_config.clone(),
        tmp.path().to_path_buf(),
    );
    let response = handle_session_create(request, &ctx).await;

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
    let ctx = RpcContext::for_test(
        km.clone(),
        sm.clone(),
        am.clone(),
        pm.clone(),
        event_tx.clone(),
        llm_config.clone(),
        tmp.path().to_path_buf(),
    );
    let response = handle_session_create(request, &ctx).await;

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
    let ctx = RpcContext::for_test(
        km.clone(),
        sm.clone(),
        am.clone(),
        pm.clone(),
        event_tx.clone(),
        llm_config.clone(),
        tmp.path().to_path_buf(),
    );
    let response = handle_session_create(request, &ctx).await;
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
    let params: crate::rpc_client::SessionCreateRequest = serde_json::from_value(json!({
        "agent_type": "acp",
        "kiln": "/tmp/kiln"
    }))
    .unwrap();
    // Even with a Local-trust provider in config, ACP always returns Cloud
    let llm_config = Some(build_llm_config_with_trust(
        "local-provider",
        crucible_core::config::BackendType::Mock,
        Some(crucible_core::config::TrustLevel::Local),
    ));
    let result = resolve_provider_trust_level_for_create(&params, &llm_config);
    assert_eq!(result, crucible_core::config::TrustLevel::Cloud);
}

#[test]
fn provider_trust_bare_backend_name_cloud() {
    let params: crate::rpc_client::SessionCreateRequest = serde_json::from_value(json!({
        "provider": "ollama",
        "kiln": "/tmp/kiln"
    }))
    .unwrap();
    let result = resolve_provider_trust_level_for_create(&params, &None);
    assert_eq!(result, crucible_core::config::TrustLevel::Cloud);
}

#[test]
fn provider_trust_bare_backend_name_local() {
    let params: crate::rpc_client::SessionCreateRequest = serde_json::from_value(json!({
        "provider": "fastembed",
        "kiln": "/tmp/kiln"
    }))
    .unwrap();
    let result = resolve_provider_trust_level_for_create(&params, &None);
    assert_eq!(result, crucible_core::config::TrustLevel::Local);
}

#[test]
fn provider_trust_default_provider_fallback() {
    // No agent_type, no provider_key, no provider → falls back to default provider in llm_config
    let params: crate::rpc_client::SessionCreateRequest = serde_json::from_value(json!({
        "kiln": "/tmp/kiln"
    }))
    .unwrap();
    // Build config where default provider is Local trust
    let llm_config = Some(build_llm_config_with_trust(
        "my-local",
        crucible_core::config::BackendType::Mock,
        Some(crucible_core::config::TrustLevel::Local),
    ));
    let result = resolve_provider_trust_level_for_create(&params, &llm_config);
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

/// Switching to a provider the kiln's classification does not permit is
/// refused.
///
/// The trust gate is attach-time by design — `tools/search.rs` passes
/// `provider_trust: None` precisely because "connected kilns pass the trust
/// gate at attach time". That is sound only if attach-time state cannot be
/// invalidated later, and `session.switch_model` invalidated it: create on a
/// local provider with a confidential kiln (gate passes), switch to a cloud
/// provider (nothing re-checked), and the kiln's contents are then retrievable
/// by a provider that was never cleared for them.
#[tokio::test]
async fn switching_to_an_untrusted_provider_is_refused_while_a_confidential_kiln_is_attached() {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    let kiln = workspace.join("notes");
    std::fs::create_dir_all(&kiln).unwrap();
    write_workspace_config(&workspace, "./notes", Some("confidential"));

    // Two providers: a local one that may see confidential data, and a cloud
    // one that may not.
    let mut llm_config = build_llm_config_with_trust(
        "local",
        crucible_core::config::BackendType::Ollama,
        Some(crucible_core::config::TrustLevel::Local),
    );
    let cloud = build_llm_config_with_trust(
        "cloud",
        crucible_core::config::BackendType::OpenAI,
        Some(crucible_core::config::TrustLevel::Cloud),
    );
    llm_config.providers.extend(cloud.providers);
    let llm_config = Some(llm_config);

    let storage = Arc::new(FileSessionStorage::new());
    let sm = Arc::new(SessionManager::with_storage(storage));
    let pm = Arc::new(ProjectManager::new(tmp.path().join("projects.json")));
    let km = Arc::new(KilnManager::new());
    let (event_tx, _rx) = broadcast::channel(16);
    let am = test_agent_manager(km.clone(), sm.clone(), event_tx.clone(), llm_config.clone());

    let ctx = RpcContext::for_test(
        km.clone(),
        sm.clone(),
        am.clone(),
        pm.clone(),
        event_tx.clone(),
        llm_config.clone(),
        tmp.path().to_path_buf(),
    );
    let response =
        handle_session_create(create_session_request(&kiln, &workspace, "local"), &ctx).await;
    assert!(
        response.error.is_none(),
        "a local provider must be allowed a confidential kiln: {:?}",
        response.error
    );
    let session_id = sm.list_sessions()[0].id.clone();

    // `session.create` does not configure an agent; `switch_model` needs one.
    let agent = crucible_core::session::SessionAgent {
        mode: None,
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("local".to_string()),
        provider: crucible_core::config::BackendType::Ollama,
        model: "llama3.2".to_string(),
        system_prompt: "trust test".to_string(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: std::collections::HashMap::new(),
        mcp_servers: Vec::new(),
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
        precognition_results: 5,
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: Default::default(),
        context_window: None,
        output_validation: Default::default(),
        validation_retries: 3,
        autocompact_threshold: None,
        tool_policy: None,
    };
    am.configure_agent(&session_id, agent)
        .await
        .expect("configure agent");

    let err = am
        .switch_model(&session_id, "cloud/gpt-4", None)
        .await
        .expect_err("switching to an untrusted provider must be refused");

    let message = err.to_string();
    assert!(
        message.contains("confidential"),
        "the refusal must name the classification that blocked it: {message}"
    );

    // And the session keeps the provider it had, rather than half-switching.
    let session = sm.get_session(&session_id).expect("session survives");
    let agent = session.agent.expect("agent config survives");
    assert_eq!(
        agent.provider_key.as_deref(),
        Some("local"),
        "a refused switch must leave the session on its original provider"
    );
}

/// The gate is not a blanket ban on switching: an unclassified kiln, or a
/// target the new provider does satisfy, still switches.
#[tokio::test]
async fn switching_providers_is_allowed_when_the_kiln_permits_it() {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    let kiln = workspace.join("notes");
    std::fs::create_dir_all(&kiln).unwrap();
    // `public` is satisfied by any provider.
    write_workspace_config(&workspace, "./notes", Some("public"));

    let mut llm_config = build_llm_config_with_trust(
        "local",
        crucible_core::config::BackendType::Ollama,
        Some(crucible_core::config::TrustLevel::Local),
    );
    let cloud = build_llm_config_with_trust(
        "cloud",
        crucible_core::config::BackendType::OpenAI,
        Some(crucible_core::config::TrustLevel::Cloud),
    );
    llm_config.providers.extend(cloud.providers);
    let llm_config = Some(llm_config);

    let storage = Arc::new(FileSessionStorage::new());
    let sm = Arc::new(SessionManager::with_storage(storage));
    let pm = Arc::new(ProjectManager::new(tmp.path().join("projects.json")));
    let km = Arc::new(KilnManager::new());
    let (event_tx, _rx) = broadcast::channel(16);
    let am = test_agent_manager(km.clone(), sm.clone(), event_tx.clone(), llm_config.clone());

    let ctx = RpcContext::for_test(
        km.clone(),
        sm.clone(),
        am.clone(),
        pm.clone(),
        event_tx.clone(),
        llm_config.clone(),
        tmp.path().to_path_buf(),
    );
    let response =
        handle_session_create(create_session_request(&kiln, &workspace, "local"), &ctx).await;
    assert!(response.error.is_none(), "{:?}", response.error);
    let session_id = sm.list_sessions()[0].id.clone();

    let agent = crucible_core::session::SessionAgent {
        mode: None,
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("local".to_string()),
        provider: crucible_core::config::BackendType::Ollama,
        model: "llama3.2".to_string(),
        system_prompt: "trust test".to_string(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: std::collections::HashMap::new(),
        mcp_servers: Vec::new(),
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
        precognition_results: 5,
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: Default::default(),
        context_window: None,
        output_validation: Default::default(),
        validation_retries: 3,
        autocompact_threshold: None,
        tool_policy: None,
    };
    am.configure_agent(&session_id, agent)
        .await
        .expect("configure agent");

    am.switch_model(&session_id, "cloud/gpt-4", None)
        .await
        .expect("a public kiln must not block a cloud provider");
}

/// A confidential kiln arriving in `connect_kilns` is refused, and refused
/// *before* anything is written.
///
/// Two gates could catch this and only one is early enough.
/// `validate_trust_level` classifies the primary kiln alone, so a confidential
/// kiln attached at create never reached it — and `tools/search.rs` then passes
/// `provider_trust: None` on the strength of "connected kilns pass the trust
/// gate at attach time". The gate inside `configure_agent` does see connected
/// kilns, but create calls it *after* `create_session` has persisted the row,
/// so catching it there would answer 422 and still leave an agent-less session
/// listed forever, answering `NoAgentConfigured`.
///
/// The `list_sessions()` assertion is the half that regresses silently: the
/// refusal keeps working when the check moves back too late, and only the
/// leftover row shows it.
#[tokio::test]
async fn a_confidential_connected_kiln_is_refused_without_creating_a_session() {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    let public = workspace.join("public");
    let secret = workspace.join("secret");
    std::fs::create_dir_all(&public).unwrap();
    std::fs::create_dir_all(&secret).unwrap();

    // Only the *connected* kiln is confidential; the primary one is not, so
    // the create-time classification check passes and cannot be what refuses.
    let crucible_dir = workspace.join(".crucible");
    std::fs::create_dir_all(&crucible_dir).unwrap();
    std::fs::write(
        crucible_dir.join("project.toml"),
        "[[kilns]]\npath = \"./public\"\n\n\
         [[kilns]]\npath = \"./secret\"\ndata_classification = \"confidential\"\n",
    )
    .unwrap();

    let llm_config = Some(build_llm_config(
        "cloud",
        crucible_core::config::BackendType::OpenAI,
    ));
    let request: Request = serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "session.create",
        "params": {
            "type": "chat",
            "kiln": public,
            "workspace": workspace,
            "connect_kilns": [secret],
            "provider_key": "cloud",
            // The web always sets this, so this is its default create shape.
            "configure_agent": true,
        }
    }))
    .unwrap();

    let storage = Arc::new(FileSessionStorage::new());
    let sm = Arc::new(SessionManager::with_storage(storage));
    let pm = Arc::new(ProjectManager::new(tmp.path().join("projects.json")));
    let km = Arc::new(KilnManager::new());

    let (event_tx, _event_rx) = broadcast::channel(16);
    let am = test_agent_manager(km.clone(), sm.clone(), event_tx.clone(), llm_config.clone());
    let ctx = RpcContext::for_test(
        km.clone(),
        sm.clone(),
        am.clone(),
        pm.clone(),
        event_tx.clone(),
        llm_config.clone(),
        tmp.path().to_path_buf(),
    );

    let response = handle_session_create(request, &ctx).await;
    let error = response
        .error
        .expect("a confidential connected kiln must refuse a cloud provider");
    assert_eq!(error.code, INVALID_PARAMS);
    assert!(error.message.contains("insufficient"), "{}", error.message);
    assert_eq!(
        sm.list_sessions().len(),
        0,
        "the refusal must not leave a session behind"
    );
}
