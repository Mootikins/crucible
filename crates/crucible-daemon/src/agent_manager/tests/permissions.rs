use super::*;

mod is_safe_tests {
    use super::*;

    #[test]
    fn read_only_tools_are_safe() {
        assert!(is_safe("read_file"));
        assert!(is_safe("glob"));
        assert!(is_safe("grep"));
        assert!(is_safe("read_note"));
        assert!(is_safe("read_metadata"));
        assert!(is_safe("text_search"));
        assert!(is_safe("property_search"));
        assert!(is_safe("semantic_search"));
        assert!(is_safe("get_kiln_info"));
        assert!(is_safe("list_notes"));
    }

    #[test]
    fn list_jobs_is_safe() {
        assert!(is_safe("list_jobs"), "list_jobs should be safe");
    }

    #[test]
    fn write_tools_are_not_safe() {
        assert!(!is_safe("write"));
        assert!(!is_safe("edit"));
        assert!(!is_safe("bash"));
        assert!(!is_safe("create_note"));
        assert!(!is_safe("update_note"));
        assert!(!is_safe("delete_note"));
    }

    #[test]
    fn unknown_tools_are_not_safe() {
        assert!(!is_safe("unknown_tool"));
        assert!(!is_safe(""));
        assert!(!is_safe("some_custom_tool"));
        assert!(!is_safe("fs_write_file")); // MCP prefixed tools
        assert!(!is_safe("gh_create_issue"));
    }

    #[test]
    fn delegate_session_is_not_safe() {
        assert!(!is_safe("delegate_session"));
    }

    #[test]
    fn cancel_job_is_not_safe() {
        assert!(!is_safe("cancel_job"));
    }
}

mod brief_resource_description_tests {
    use super::*;

    #[test]
    fn extracts_path_field() {
        let args = serde_json::json!({"path": "/home/user/file.txt"});
        assert_eq!(
            AgentManager::brief_resource_description(&args),
            "/home/user/file.txt"
        );
    }

    #[test]
    fn extracts_file_field() {
        let args = serde_json::json!({"file": "config.toml"});
        assert_eq!(
            AgentManager::brief_resource_description(&args),
            "config.toml"
        );
    }

    #[test]
    fn extracts_command_field() {
        let args = serde_json::json!({"command": "echo hello"});
        assert_eq!(
            AgentManager::brief_resource_description(&args),
            "echo hello"
        );
    }

    #[test]
    fn truncates_long_commands() {
        let long_cmd = "a".repeat(100);
        let args = serde_json::json!({"command": long_cmd});
        let result = AgentManager::brief_resource_description(&args);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 53); // 50 chars + "..."
    }

    #[test]
    fn extracts_name_field() {
        let args = serde_json::json!({"name": "my-note"});
        assert_eq!(AgentManager::brief_resource_description(&args), "my-note");
    }

    #[test]
    fn returns_empty_for_no_matching_fields() {
        let args = serde_json::json!({"other": "value"});
        assert_eq!(AgentManager::brief_resource_description(&args), "");
    }

    #[test]
    fn path_takes_precedence_over_other_fields() {
        let args = serde_json::json!({
            "path": "/path/to/file",
            "command": "some command",
            "name": "some name"
        });
        assert_eq!(
            AgentManager::brief_resource_description(&args),
            "/path/to/file"
        );
    }
}

mod pattern_matching_tests {
    use super::*;

    #[test]
    fn bash_command_matches_prefix() {
        let mut store = PatternStore::new();
        store.add_bash_pattern("npm install").unwrap();

        let args = serde_json::json!({"command": "npm install lodash"});
        assert!(AgentManager::check_pattern_match("bash", &args, &store));
    }

    #[test]
    fn bash_command_no_match() {
        let mut store = PatternStore::new();
        store.add_bash_pattern("npm install").unwrap();

        let args = serde_json::json!({"command": "rm -rf /"});
        assert!(!AgentManager::check_pattern_match("bash", &args, &store));
    }

    #[test]
    fn bash_command_missing_command_arg() {
        let store = PatternStore::new();
        let args = serde_json::json!({"other": "value"});
        assert!(!AgentManager::check_pattern_match("bash", &args, &store));
    }

    #[test]
    fn file_path_matches_prefix() {
        let mut store = PatternStore::new();
        store.add_file_pattern("src/").unwrap();

        let args = serde_json::json!({"path": "src/lib.rs"});
        assert!(AgentManager::check_pattern_match(
            "write_file",
            &args,
            &store
        ));
    }

    #[test]
    fn file_path_no_match() {
        let mut store = PatternStore::new();
        store.add_file_pattern("src/").unwrap();

        let args = serde_json::json!({"path": "tests/test.rs"});
        assert!(!AgentManager::check_pattern_match(
            "write_file",
            &args,
            &store
        ));
    }

    #[test]
    fn file_operations_check_file_patterns() {
        let mut store = PatternStore::new();
        store.add_file_pattern("notes/").unwrap();

        let args = serde_json::json!({"name": "notes/my-note.md"});

        assert!(AgentManager::check_pattern_match(
            "create_note",
            &args,
            &store
        ));
        assert!(AgentManager::check_pattern_match(
            "update_note",
            &args,
            &store
        ));
        assert!(AgentManager::check_pattern_match(
            "delete_note",
            &args,
            &store
        ));
    }

    #[test]
    fn tool_matches_always_allow() {
        let mut store = PatternStore::new();
        store.add_tool_pattern("custom_tool").unwrap();

        let args = serde_json::json!({});
        assert!(AgentManager::check_pattern_match(
            "custom_tool",
            &args,
            &store
        ));
    }

    #[test]
    fn tool_no_match() {
        let store = PatternStore::new();
        let args = serde_json::json!({});
        assert!(!AgentManager::check_pattern_match(
            "unknown_tool",
            &args,
            &store
        ));
    }

    #[test]
    fn empty_store_matches_nothing() {
        let store = PatternStore::new();

        let bash_args = serde_json::json!({"command": "npm install"});
        assert!(!AgentManager::check_pattern_match(
            "bash", &bash_args, &store
        ));

        let file_args = serde_json::json!({"path": "src/lib.rs"});
        assert!(!AgentManager::check_pattern_match(
            "write", &file_args, &store
        ));

        let tool_args = serde_json::json!({});
        assert!(!AgentManager::check_pattern_match(
            "custom_tool",
            &tool_args,
            &store
        ));
    }

    #[test]
    fn store_pattern_adds_bash_pattern() {
        let tmp = TempDir::new().unwrap();
        let project_path = tmp.path().to_string_lossy().to_string();

        AgentManager::store_pattern("bash", "cargo build", &project_path).unwrap();

        let store = PatternStore::load_sync(&project_path).unwrap();
        assert!(store.matches_bash("cargo build --release"));
    }

    #[test]
    fn store_pattern_adds_file_pattern() {
        let tmp = TempDir::new().unwrap();
        let project_path = tmp.path().to_string_lossy().to_string();

        AgentManager::store_pattern("write_file", "src/", &project_path).unwrap();

        let store = PatternStore::load_sync(&project_path).unwrap();
        assert!(store.matches_file("src/main.rs"));
    }

    #[test]
    fn store_pattern_adds_tool_pattern() {
        let tmp = TempDir::new().unwrap();
        let project_path = tmp.path().to_string_lossy().to_string();

        AgentManager::store_pattern("custom_tool", "custom_tool", &project_path).unwrap();

        let store = PatternStore::load_sync(&project_path).unwrap();
        assert!(store.matches_tool("custom_tool"));
    }

    #[test]
    fn store_pattern_rejects_star_pattern() {
        let tmp = TempDir::new().unwrap();
        let project_path = tmp.path().to_string_lossy().to_string();

        let result = AgentManager::store_pattern("bash", "*", &project_path);
        assert!(result.is_err());
    }
}

mod permission_channel_tests {
    use super::*;
    use crucible_core::interaction::{PermRequest, PermResponse};

    #[tokio::test]
    async fn await_permission_creates_pending_request() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_id = "test-session";
        let request = PermRequest::bash(["npm", "install"]);

        let (permission_id, _rx) = agent_manager.await_permission(session_id, request.clone());

        assert!(
            permission_id.starts_with("perm-"),
            "Permission ID should have perm- prefix"
        );

        let pending = agent_manager.get_pending_permission(session_id, &permission_id);
        assert!(pending.is_some(), "Pending permission should exist");
    }

    #[tokio::test]
    async fn respond_to_permission_allow_sends_response() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_id = "test-session";
        let request = PermRequest::bash(["npm", "install"]);

        let (permission_id, rx) = agent_manager.await_permission(session_id, request);

        // Respond with allow
        let result =
            agent_manager.respond_to_permission(session_id, &permission_id, PermResponse::allow());
        assert!(result.is_ok(), "respond_to_permission should succeed");

        // Verify response received
        let response = rx.await.expect("Should receive response");
        assert!(response.allowed, "Response should be allowed");
    }

    #[tokio::test]
    async fn respond_to_permission_deny_sends_response() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_id = "test-session";
        let request = PermRequest::bash(["rm", "-rf", "/"]);

        let (permission_id, rx) = agent_manager.await_permission(session_id, request);

        // Respond with deny
        let result =
            agent_manager.respond_to_permission(session_id, &permission_id, PermResponse::deny());
        assert!(result.is_ok(), "respond_to_permission should succeed");

        // Verify response received
        let response = rx.await.expect("Should receive response");
        assert!(!response.allowed, "Response should be denied");
    }

    #[tokio::test]
    async fn channel_drop_results_in_recv_error() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_id = "test-session";
        let request = PermRequest::bash(["npm", "install"]);

        let (permission_id, rx) = agent_manager.await_permission(session_id, request);

        // Remove the pending permission without responding (simulates cleanup/drop)
        agent_manager.pending_permissions.remove(session_id);

        // Verify the permission was removed
        let pending = agent_manager.get_pending_permission(session_id, &permission_id);
        assert!(pending.is_none(), "Pending permission should be removed");

        // The receiver should get an error when sender is dropped
        let result = rx.await;
        assert!(
            result.is_err(),
            "Receiver should error when sender is dropped"
        );
    }

    #[tokio::test]
    async fn respond_to_nonexistent_permission_returns_error() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let result = agent_manager.respond_to_permission(
            "nonexistent-session",
            "nonexistent-perm",
            PermResponse::allow(),
        );

        assert!(
            matches!(result, Err(AgentError::SessionNotFound(_))),
            "Should return SessionNotFound error"
        );
    }

    #[tokio::test]
    async fn respond_to_wrong_permission_id_returns_error() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_id = "test-session";
        let request = PermRequest::bash(["npm", "install"]);

        // Create a pending permission
        let (_permission_id, _rx) = agent_manager.await_permission(session_id, request);

        // Try to respond with wrong permission ID
        let result = agent_manager.respond_to_permission(
            session_id,
            "wrong-permission-id",
            PermResponse::allow(),
        );

        assert!(
            matches!(result, Err(AgentError::PermissionNotFound(_))),
            "Should return PermissionNotFound error"
        );
    }

    #[tokio::test]
    async fn list_pending_permissions_returns_all() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_id = "test-session";

        // Create multiple pending permissions
        let request1 = PermRequest::bash(["npm", "install"]);
        let request2 = PermRequest::write(["src", "main.rs"]);
        let request3 = PermRequest::tool("delete", serde_json::json!({"path": "/tmp/file"}));

        let (id1, _rx1) = agent_manager.await_permission(session_id, request1);
        let (id2, _rx2) = agent_manager.await_permission(session_id, request2);
        let (id3, _rx3) = agent_manager.await_permission(session_id, request3);

        let pending = agent_manager.list_pending_permissions(session_id);
        assert_eq!(pending.len(), 3, "Should have 3 pending permissions");

        let ids: Vec<_> = pending.iter().map(|(id, _)| id.clone()).collect();
        assert!(ids.contains(&id1), "Should contain first permission");
        assert!(ids.contains(&id2), "Should contain second permission");
        assert!(ids.contains(&id3), "Should contain third permission");
    }

    #[tokio::test]
    async fn list_pending_permissions_empty_for_unknown_session() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let pending = agent_manager.list_pending_permissions("unknown-session");
        assert!(
            pending.is_empty(),
            "Should return empty list for unknown session"
        );
    }

    #[tokio::test]
    async fn cleanup_session_removes_pending_permissions() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_id = "test-session";
        let request = PermRequest::bash(["npm", "install"]);

        let (permission_id, _rx) = agent_manager.await_permission(session_id, request);

        // Verify permission exists
        assert!(
            agent_manager
                .get_pending_permission(session_id, &permission_id)
                .is_some(),
            "Permission should exist before cleanup"
        );

        // Cleanup session
        agent_manager.cleanup_session(session_id);

        // Verify permission is removed
        assert!(
            agent_manager
                .get_pending_permission(session_id, &permission_id)
                .is_none(),
            "Permission should be removed after cleanup"
        );
    }

    #[tokio::test]
    async fn multiple_sessions_have_isolated_permissions() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session1 = "session-1";
        let session2 = "session-2";

        let request1 = PermRequest::bash(["npm", "install"]);
        let request2 = PermRequest::bash(["cargo", "build"]);

        let (id1, _rx1) = agent_manager.await_permission(session1, request1);
        let (id2, _rx2) = agent_manager.await_permission(session2, request2);

        // Each session should only see its own permissions
        let pending1 = agent_manager.list_pending_permissions(session1);
        let pending2 = agent_manager.list_pending_permissions(session2);

        assert_eq!(pending1.len(), 1, "Session 1 should have 1 permission");
        assert_eq!(pending2.len(), 1, "Session 2 should have 1 permission");

        assert_eq!(
            pending1[0].0, id1,
            "Session 1 should have its own permission"
        );
        assert_eq!(
            pending2[0].0, id2,
            "Session 2 should have its own permission"
        );

        // Cleanup session 1 should not affect session 2
        agent_manager.cleanup_session(session1);

        let pending1_after = agent_manager.list_pending_permissions(session1);
        let pending2_after = agent_manager.list_pending_permissions(session2);

        assert!(
            pending1_after.is_empty(),
            "Session 1 should have no permissions after cleanup"
        );
        assert_eq!(
            pending2_after.len(),
            1,
            "Session 2 should still have its permission"
        );
    }

    #[tokio::test]
    async fn test_switch_model_cross_provider() {
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

        // Create providers config with multiple providers
        let mut providers = std::collections::HashMap::new();
        providers.insert(
            "ollama".to_string(),
            LlmProviderConfig::builder(BackendType::Ollama)
                .endpoint("http://localhost:11434")
                .build(),
        );
        providers.insert(
            "zai".to_string(),
            LlmProviderConfig::builder(BackendType::Anthropic)
                .endpoint("https://api.zaiforge.com/v1")
                .build(),
        );
        let llm_config = LlmConfig {
            default: Some("ollama".to_string()),
            providers,
        };

        let agent_manager =
            create_test_agent_manager_with_providers(session_manager.clone(), llm_config);

        // Configure with ollama provider
        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        // Switch to zai/claude-sonnet-4
        agent_manager
            .switch_model(&session.id, "zai/claude-sonnet-4", None)
            .await
            .unwrap();

        let updated = session_manager.get_session(&session.id).unwrap();
        let agent = updated.agent.as_ref().unwrap();

        assert_eq!(agent.model, "claude-sonnet-4", "Model should be updated");
        assert_eq!(
            agent.provider_key.as_deref(),
            Some("zai"),
            "Provider key should be updated"
        );
        assert_eq!(
            agent.endpoint.as_deref(),
            Some("https://api.zaiforge.com/v1"),
            "Endpoint should be updated"
        );
        assert_eq!(
            agent.provider,
            BackendType::Anthropic,
            "Provider should be updated"
        );
    }

    #[tokio::test]
    async fn test_switch_model_unprefixed_same_provider() {
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

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        let before = session_manager.get_session(&session.id).unwrap();
        let before_provider = before.agent.as_ref().unwrap().provider;
        let before_endpoint = before.agent.as_ref().unwrap().endpoint.clone();

        // Switch to unprefixed model (should only change model, not provider)
        agent_manager
            .switch_model(&session.id, "llama3.3", None)
            .await
            .unwrap();

        let updated = session_manager.get_session(&session.id).unwrap();
        let agent = updated.agent.as_ref().unwrap();

        assert_eq!(agent.model, "llama3.3", "Model should be updated");
        assert_eq!(
            agent.provider, before_provider,
            "Provider should remain unchanged"
        );
        assert_eq!(
            agent.endpoint, before_endpoint,
            "Endpoint should remain unchanged"
        );
    }

    #[tokio::test]
    async fn test_switch_model_unknown_provider_prefix() {
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

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        let before = session_manager.get_session(&session.id).unwrap();
        let before_provider = before.agent.as_ref().unwrap().provider;

        agent_manager
            .switch_model(&session.id, "unknown/model", None)
            .await
            .unwrap();

        let updated = session_manager.get_session(&session.id).unwrap();
        let agent = updated.agent.as_ref().unwrap();

        assert_eq!(
            agent.model, "unknown/model",
            "Model should be set to full string"
        );
        assert_eq!(
            agent.provider, before_provider,
            "Provider should remain unchanged"
        );
    }

    #[tokio::test]
    async fn test_switch_model_cross_provider_invalidates_cache() {
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
            "ollama".to_string(),
            LlmProviderConfig::builder(BackendType::Ollama)
                .endpoint("http://localhost:11434")
                .build(),
        );
        providers.insert(
            "zai".to_string(),
            LlmProviderConfig::builder(BackendType::Anthropic)
                .endpoint("https://api.zaiforge.com/v1")
                .build(),
        );
        let llm_config = LlmConfig {
            default: Some("ollama".to_string()),
            providers,
        };

        let agent_manager =
            create_test_agent_manager_with_providers(session_manager.clone(), llm_config);

        agent_manager
            .configure_agent(&session.id, test_agent())
            .await
            .unwrap();

        agent_manager
            .switch_model(&session.id, "zai/claude-sonnet-4", None)
            .await
            .unwrap();

        assert!(
            !agent_manager.agent_cache.contains_key(&session.id),
            "Cache should be invalidated after cross-provider switch"
        );
    }
}

