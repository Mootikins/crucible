use super::*;

mod is_safe_tests {
    use super::*;
    use test_case::test_case;

    #[test_case(
        &[
            "read_file", "glob", "grep", "read_note", "read_metadata",
            "text_search", "property_search", "semantic_search",
            "get_kiln_info", "list_notes",
        ],
        true;
        "read_only_tools_are_safe"
    )]
    #[test_case(&["list_jobs"], true; "list_jobs_is_safe")]
    #[test_case(
        &["write", "edit", "bash", "create_note", "update_note", "delete_note"],
        false;
        "write_tools_are_not_safe"
    )]
    #[test_case(
        &["unknown_tool", "", "some_custom_tool", "fs_write_file", "gh_create_issue"],
        false;
        "unknown_tools_are_not_safe"
    )]
    #[test_case(&["delegate_session"], false; "delegate_session_is_not_safe")]
    #[test_case(&["cancel_job"], false; "cancel_job_is_not_safe")]
    fn is_safe_classifies_tools(tools: &[&str], expected_safe: bool) {
        for tool in tools {
            assert_eq!(
                is_safe(tool),
                expected_safe,
                "is_safe({tool:?}) should be {expected_safe}",
            );
        }
    }
}

mod brief_resource_description_tests {
    use super::*;
    use test_case::test_case;

    #[test_case(
        serde_json::json!({"path": "/home/user/file.txt"}),
        "/home/user/file.txt";
        "extracts_path_field"
    )]
    #[test_case(
        serde_json::json!({"file": "config.toml"}),
        "config.toml";
        "extracts_file_field"
    )]
    #[test_case(
        serde_json::json!({"command": "echo hello"}),
        "echo hello";
        "extracts_command_field"
    )]
    #[test_case(
        serde_json::json!({"name": "my-note"}),
        "my-note";
        "extracts_name_field"
    )]
    #[test_case(
        serde_json::json!({"other": "value"}),
        "";
        "returns_empty_for_no_matching_fields"
    )]
    fn brief_extracts_known_field(args: serde_json::Value, expected: &str) {
        assert_eq!(AgentManager::brief_resource_description(&args), expected);
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
    use test_case::test_case;

    #[test_case(
        "bash",
        serde_json::json!({"command": "npm install lodash"}),
        Some(("bash", "npm install")),
        true;
        "bash_command_matches_prefix"
    )]
    #[test_case(
        "bash",
        serde_json::json!({"command": "rm -rf /"}),
        Some(("bash", "npm install")),
        false;
        "bash_command_no_match"
    )]
    #[test_case(
        "bash",
        serde_json::json!({"other": "value"}),
        None,
        false;
        "bash_command_missing_command_arg"
    )]
    #[test_case(
        "write_file",
        serde_json::json!({"path": "src/lib.rs"}),
        Some(("file", "src/")),
        true;
        "file_path_matches_prefix"
    )]
    #[test_case(
        "write_file",
        serde_json::json!({"path": "tests/test.rs"}),
        Some(("file", "src/")),
        false;
        "file_path_no_match"
    )]
    #[test_case(
        "custom_tool",
        serde_json::json!({}),
        Some(("tool", "custom_tool")),
        true;
        "tool_matches_always_allow"
    )]
    #[test_case(
        "unknown_tool",
        serde_json::json!({}),
        None,
        false;
        "tool_no_match"
    )]
    fn check_pattern_match_outcomes(
        tool: &str,
        args: serde_json::Value,
        store_setup: Option<(&str, &str)>,
        expected: bool,
    ) {
        let mut store = PatternStore::new();
        if let Some((kind, pattern)) = store_setup {
            match kind {
                "bash" => store.add_bash_pattern(pattern).unwrap(),
                "file" => store.add_file_pattern(pattern).unwrap(),
                "tool" => store.add_tool_pattern(pattern).unwrap(),
                other => unreachable!("unknown store pattern kind: {other}"),
            }
        }
        assert_eq!(
            AgentManager::check_pattern_match(tool, &args, &store),
            expected,
        );
    }

    #[test]
    fn file_operations_check_file_patterns() {
        let mut store = PatternStore::new();
        store.add_file_pattern("notes/").unwrap();

        let args = serde_json::json!({"name": "notes/my-note.md"});

        assert!(AgentManager::check_pattern_match(
            "create_note", &args, &store
        ));
        assert!(AgentManager::check_pattern_match(
            "update_note", &args, &store
        ));
        assert!(AgentManager::check_pattern_match(
            "delete_note", &args, &store
        ));
    }

    #[test]
    fn empty_store_matches_nothing() {
        let store = PatternStore::new();

        let bash_args = serde_json::json!({"command": "npm install"});
        assert!(!AgentManager::check_pattern_match("bash", &bash_args, &store));

        let file_args = serde_json::json!({"path": "src/lib.rs"});
        assert!(!AgentManager::check_pattern_match("write", &file_args, &store));

        let tool_args = serde_json::json!({});
        assert!(!AgentManager::check_pattern_match(
            "custom_tool", &tool_args, &store
        ));
    }

    #[test_case("bash", "cargo build", "cargo build --release", true; "store_pattern_adds_bash_pattern")]
    #[test_case("write_file", "src/", "src/main.rs", true; "store_pattern_adds_file_pattern")]
    #[test_case("custom_tool", "custom_tool", "custom_tool", true; "store_pattern_adds_tool_pattern")]
    #[test_case("bash", "*", "", false; "store_pattern_rejects_star_pattern")]
    fn store_pattern_outcomes(
        kind: &str,
        pattern: &str,
        sample: &str,
        should_succeed: bool,
    ) {
        let tmp = TempDir::new().unwrap();
        let project_path = tmp.path().to_string_lossy().to_string();

        let result = AgentManager::store_pattern(kind, pattern, &project_path);

        if should_succeed {
            result.unwrap();
            let store = PatternStore::load_sync(&project_path).unwrap();
            match kind {
                "bash" => assert!(store.matches_bash(sample), "matches_bash({sample:?})"),
                "write_file" => assert!(store.matches_file(sample), "matches_file({sample:?})"),
                _ => assert!(store.matches_tool(sample), "matches_tool({sample:?})"),
            }
        } else {
            assert!(result.is_err());
        }
    }
}

mod permission_channel_tests {
    use super::*;
    use crucible_core::interaction::{PermRequest, PermResponse};
    use test_case::test_case;

    #[derive(Clone, Copy)]
    enum RespondScenario {
        Allow,
        Deny,
        NonexistentSession,
        WrongPermissionId,
    }

    #[test_case(RespondScenario::Allow; "respond_to_permission_allow_sends_response")]
    #[test_case(RespondScenario::Deny; "respond_to_permission_deny_sends_response")]
    #[test_case(RespondScenario::NonexistentSession; "respond_to_nonexistent_permission_returns_error")]
    #[test_case(RespondScenario::WrongPermissionId; "respond_to_wrong_permission_id_returns_error")]
    #[tokio::test]
    async fn respond_to_permission_outcomes(scenario: RespondScenario) {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        // NonexistentSession has no awaited permission; the other three await one.
        let awaited = match scenario {
            RespondScenario::NonexistentSession => None,
            RespondScenario::Allow | RespondScenario::WrongPermissionId => Some(
                agent_manager.await_permission("test-session", PermRequest::bash(["npm", "install"])),
            ),
            RespondScenario::Deny => Some(
                agent_manager.await_permission("test-session", PermRequest::bash(["rm", "-rf", "/"])),
            ),
        };

        let (session_id, id_to_use, response) = match scenario {
            RespondScenario::Allow => (
                "test-session",
                awaited.as_ref().unwrap().0.as_str(),
                PermResponse::allow(),
            ),
            RespondScenario::Deny => (
                "test-session",
                awaited.as_ref().unwrap().0.as_str(),
                PermResponse::deny(),
            ),
            RespondScenario::NonexistentSession => {
                ("nonexistent-session", "nonexistent-perm", PermResponse::allow())
            }
            RespondScenario::WrongPermissionId => {
                ("test-session", "wrong-permission-id", PermResponse::allow())
            }
        };

        let result = agent_manager.respond_to_permission(session_id, id_to_use, response);

        match scenario {
            RespondScenario::Allow | RespondScenario::Deny => {
                assert!(result.is_ok(), "respond_to_permission should succeed");
                let rx = awaited.unwrap().1;
                let response = rx.await.expect("Should receive response");
                let expected_allowed = matches!(scenario, RespondScenario::Allow);
                assert_eq!(
                    response.allowed, expected_allowed,
                    "Response allowed flag should match scenario",
                );
            }
            RespondScenario::NonexistentSession => {
                assert!(
                    matches!(result, Err(AgentError::SessionNotFound(_))),
                    "Should return SessionNotFound error"
                );
            }
            RespondScenario::WrongPermissionId => {
                assert!(
                    matches!(result, Err(AgentError::PermissionNotFound(_))),
                    "Should return PermissionNotFound error"
                );
            }
        }
    }

    #[derive(Clone, Copy)]
    enum LifecycleAction {
        AwaitCreates,
        CleanupRemoves,
    }

    #[test_case(LifecycleAction::AwaitCreates; "await_permission_creates_pending_request")]
    #[test_case(LifecycleAction::CleanupRemoves; "cleanup_session_removes_pending_permissions")]
    #[tokio::test]
    async fn permission_lifecycle(action: LifecycleAction) {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_id = "test-session";
        let request = PermRequest::bash(["npm", "install"]);

        let (permission_id, _rx) = agent_manager.await_permission(session_id, request);
        assert!(
            permission_id.starts_with("perm-"),
            "Permission ID should have perm- prefix"
        );

        match action {
            LifecycleAction::AwaitCreates => {
                let pending = agent_manager.get_pending_permission(session_id, &permission_id);
                assert!(pending.is_some(), "Pending permission should exist");
            }
            LifecycleAction::CleanupRemoves => {
                assert!(
                    agent_manager
                        .get_pending_permission(session_id, &permission_id)
                        .is_some(),
                    "Permission should exist before cleanup"
                );
                agent_manager.cleanup_session(session_id);
                assert!(
                    agent_manager
                        .get_pending_permission(session_id, &permission_id)
                        .is_none(),
                    "Permission should be removed after cleanup"
                );
            }
        }
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

    #[derive(Clone, Copy)]
    enum ListPendingScenario {
        PopulatedSession,
        UnknownSession,
    }

    #[test_case(ListPendingScenario::PopulatedSession; "list_pending_permissions_returns_all")]
    #[test_case(ListPendingScenario::UnknownSession; "list_pending_permissions_empty_for_unknown_session")]
    #[tokio::test]
    async fn list_pending_permissions_outcomes(scenario: ListPendingScenario) {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let session_id = "test-session";

        let expected_ids: Vec<String> = match scenario {
            ListPendingScenario::PopulatedSession => {
                let request1 = PermRequest::bash(["npm", "install"]);
                let request2 = PermRequest::write(["src", "main.rs"]);
                let request3 =
                    PermRequest::tool("delete", serde_json::json!({"path": "/tmp/file"}));

                let (id1, _rx1) = agent_manager.await_permission(session_id, request1);
                let (id2, _rx2) = agent_manager.await_permission(session_id, request2);
                let (id3, _rx3) = agent_manager.await_permission(session_id, request3);

                vec![id1, id2, id3]
            }
            ListPendingScenario::UnknownSession => vec![],
        };

        let pending = if matches!(scenario, ListPendingScenario::UnknownSession) {
            agent_manager.list_pending_permissions("unknown-session")
        } else {
            agent_manager.list_pending_permissions(session_id)
        };

        assert_eq!(
            pending.len(),
            expected_ids.len(),
            "pending count should match"
        );
        for expected in &expected_ids {
            let ids: Vec<_> = pending.iter().map(|(id, _)| id.clone()).collect();
            assert!(ids.contains(expected), "Should contain permission {expected}");
        }
        if matches!(scenario, ListPendingScenario::UnknownSession) {
            assert!(
                pending.is_empty(),
                "Should return empty list for unknown session"
            );
        }
    }

    #[tokio::test]
    async fn list_all_pending_permissions_aggregates_across_sessions() {
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let agent_manager = create_test_agent_manager(session_manager);

        let (id1, _rx1) =
            agent_manager.await_permission("session-a", PermRequest::bash(["cargo", "test"]));
        let (id2, _rx2) =
            agent_manager.await_permission("session-b", PermRequest::bash(["ls"]));

        let all = agent_manager.list_all_pending_permissions();
        assert_eq!(all.len(), 2, "Should aggregate both sessions");

        let by_session: Vec<_> = all
            .iter()
            .map(|(sid, pid, _)| (sid.as_str(), pid.clone()))
            .collect();
        assert!(by_session.contains(&("session-a", id1)));
        assert!(by_session.contains(&("session-b", id2)));

        // Responding removes the entry from the aggregate view.
        let (sid, pid, _) = all[0].clone();
        agent_manager
            .respond_to_permission(&sid, &pid, PermResponse::allow())
            .expect("respond should succeed");
        assert_eq!(agent_manager.list_all_pending_permissions().len(), 1);
    }

    #[derive(Clone, Copy)]
    enum SwitchScenario {
        CrossProvider,
        UnprefixedSameProvider,
        UnknownProviderPrefix,
        CrossProviderInvalidatesCache,
    }

    #[test_case(SwitchScenario::CrossProvider; "switch_model_cross_provider")]
    #[test_case(SwitchScenario::UnprefixedSameProvider; "switch_model_unprefixed_same_provider")]
    #[test_case(SwitchScenario::UnknownProviderPrefix; "switch_model_unknown_provider_prefix")]
    #[test_case(SwitchScenario::CrossProviderInvalidatesCache; "switch_model_cross_provider_invalidates_cache")]
    #[tokio::test]
    async fn switch_model_outcomes(scenario: SwitchScenario) {
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
            "ollama".to_string(),
            LlmProviderConfig::builder(BackendType::Ollama)
                .endpoint("http://localhost:11434")
                .build(),
        );

        let switch_input = match scenario {
            SwitchScenario::CrossProvider | SwitchScenario::CrossProviderInvalidatesCache => {
                providers.insert(
                    "zai".to_string(),
                    LlmProviderConfig::builder(BackendType::Anthropic)
                        .endpoint("https://api.zaiforge.com/v1")
                        .build(),
                );
                "zai/claude-sonnet-4"
            }
            SwitchScenario::UnprefixedSameProvider => "llama3.3",
            SwitchScenario::UnknownProviderPrefix => "unknown/model",
        };

        let llm_config = LlmConfig {
            default: Some("ollama".to_string()),
            providers,
            models: Default::default(),
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

        agent_manager
            .switch_model(&session.id, switch_input, None)
            .await
            .unwrap();

        let updated = session_manager.get_session(&session.id).unwrap();
        let agent = updated.agent.as_ref().unwrap();

        match scenario {
            SwitchScenario::CrossProvider | SwitchScenario::CrossProviderInvalidatesCache => {
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
            SwitchScenario::UnprefixedSameProvider => {
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
            SwitchScenario::UnknownProviderPrefix => {
                assert_eq!(
                    agent.model, "unknown/model",
                    "Model should be set to full string"
                );
                assert_eq!(
                    agent.provider, before_provider,
                    "Provider should remain unchanged"
                );
            }
        }

        if matches!(scenario, SwitchScenario::CrossProviderInvalidatesCache) {
            assert!(
                !agent_manager.agent_cache.contains_key(&session.id),
                "Cache should be invalidated after cross-provider switch"
            );
        }
    }
}

mod resolve_agent_profile_tests {
    use crate::acp::discovery::default_agent_profiles;
    use crucible_core::config::components::{
        acp::AgentProfile,
        permissions::{PermissionConfig, PermissionMode},
    };
    use std::collections::HashMap;

    use crate::agent_manager::resolve_agent_profile;
    use test_case::test_case;

    fn make_profile_with_permissions(mode: PermissionMode) -> AgentProfile {
        let perms = PermissionConfig {
            default: mode,
            ..Default::default()
        };
        AgentProfile {
            permissions: Some(perms),
            ..Default::default()
        }
    }

    #[derive(Clone, Copy)]
    enum ProfileScenario {
        MergesPermissions,
        NoPermissionsReturnsNone,
    }

    #[test_case(ProfileScenario::MergesPermissions; "resolve_agent_profile_merges_permissions")]
    #[test_case(ProfileScenario::NoPermissionsReturnsNone; "resolve_agent_profile_no_permissions_returns_none")]
    fn resolve_agent_profile_outcomes(scenario: ProfileScenario) {
        let mut configured = HashMap::new();
        let (name, expected_some) = match scenario {
            ProfileScenario::MergesPermissions => {
                configured.insert(
                    "my-claude".to_string(),
                    make_profile_with_permissions(PermissionMode::Ask),
                );
                ("my-claude", true)
            }
            ProfileScenario::NoPermissionsReturnsNone => {
                let p = AgentProfile {
                    extends: Some("opencode".to_string()),
                    ..Default::default()
                };
                configured.insert("my-opencode".to_string(), p);
                ("my-opencode", false)
            }
        };
        let available = default_agent_profiles();

        let resolved =
            resolve_agent_profile(name, &configured, &available).expect("should resolve");

        if expected_some {
            let perms = resolved.permissions.expect("should have permissions");
            assert_eq!(perms.default, PermissionMode::Ask);
        } else {
            assert!(resolved.permissions.is_none());
        }
    }
}
