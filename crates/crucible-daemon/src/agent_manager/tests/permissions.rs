use super::*;
use crate::test_support::{kiln_name, temp_session_manager};

mod is_safe_tests {
    use super::*;
    use test_case::test_case;

    #[test_case(
        &[
            "read_file", "glob", "grep", "read_note", "read_metadata",
            "grep_notes", "property_search", "semantic_search",
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
        "write_file", serde_json::json!({"path": "/tmp/a.md"}), "/tmp/a.md";
        "extracts_path_field"
    )]
    #[test_case(
        "write_file", serde_json::json!({"file": "/tmp/b.md"}), "/tmp/b.md";
        "extracts_file_field"
    )]
    #[test_case(
        "bash", serde_json::json!({"command": "echo hello"}), "echo hello";
        "extracts_command_field"
    )]
    #[test_case(
        "create_note", serde_json::json!({"name": "my-note"}), "my-note";
        "extracts_name_field"
    )]
    // Previously "": the old key list gave up on an unrecognised shape and
    // showed the user nothing. Any string beats a bare tool name.
    #[test_case(
        "mystery", serde_json::json!({"other": "value"}), "value";
        "falls_back_to_any_string"
    )]
    #[test_case(
        "noop", serde_json::json!({}), "";
        "returns_empty_when_there_is_nothing_to_show"
    )]
    fn brief_extracts_known_field(tool: &str, args: serde_json::Value, expected: &str) {
        assert_eq!(
            AgentManager::brief_resource_description(tool, &args),
            expected
        );
    }

    #[test]
    fn truncates_long_commands() {
        let long_cmd = "a".repeat(100);
        let args = serde_json::json!({"command": long_cmd});
        let result = AgentManager::brief_resource_description("bash", &args);
        assert!(result.ends_with('…'), "got: {result}");
        assert_eq!(result.chars().count(), 51, "50 chars plus the ellipsis");
    }

    #[test]
    fn path_takes_precedence_over_other_fields() {
        let args = serde_json::json!({
            "path": "/path/to/file",
            "name": "some name"
        });
        assert_eq!(
            AgentManager::brief_resource_description("write_file", &args),
            "/path/to/file"
        );
    }

    /// A shell call's command outranks a path argument — the command IS the
    /// thing being approved, and `cd /tmp` is not usefully described as
    /// "/tmp".
    #[test]
    fn a_shell_call_is_described_by_its_command() {
        let args = serde_json::json!({"command": "rm -rf build", "path": "/repo"});
        assert_eq!(
            AgentManager::brief_resource_description("bash", &args),
            "rm -rf build"
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

    #[test_case("bash", "cargo build", "cargo build --release", true; "store_pattern_adds_bash_pattern")]
    #[test_case("write_file", "src/", "src/main.rs", true; "store_pattern_adds_file_pattern")]
    #[test_case("custom_tool", "custom_tool", "custom_tool", true; "store_pattern_adds_tool_pattern")]
    #[test_case("bash", "*", "", false; "store_pattern_rejects_star_pattern")]
    fn store_pattern_outcomes(kind: &str, pattern: &str, sample: &str, should_succeed: bool) {
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
        let session_manager = temp_session_manager();
        let agent_manager = create_test_agent_manager(session_manager);

        // NonexistentSession has no awaited permission; the other three await one.
        let awaited = match scenario {
            RespondScenario::NonexistentSession => None,
            RespondScenario::Allow | RespondScenario::WrongPermissionId => Some(
                agent_manager
                    .await_permission("test-session", PermRequest::bash(["npm", "install"])),
            ),
            RespondScenario::Deny => Some(
                agent_manager
                    .await_permission("test-session", PermRequest::bash(["rm", "-rf", "/"])),
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
            RespondScenario::NonexistentSession => (
                "nonexistent-session",
                "nonexistent-perm",
                PermResponse::allow(),
            ),
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

    #[tokio::test]
    async fn await_permission_creates_pending_request() {
        let session_manager = temp_session_manager();
        let agent_manager = create_test_agent_manager(session_manager);

        let session_id = "test-session";
        let request = PermRequest::bash(["npm", "install"]);

        let (permission_id, _rx) = agent_manager.await_permission(session_id, request);
        assert!(
            permission_id.starts_with("perm-"),
            "Permission ID should have perm- prefix"
        );
        assert!(
            agent_manager
                .get_pending_permission(session_id, &permission_id)
                .is_some(),
            "Pending permission should exist"
        );
    }

    /// Ending a session must unblock whoever is waiting on its prompts, not
    /// merely forget them.
    ///
    /// The map-emptiness half of this is now covered by
    /// `cleanup_session_leaves_no_per_session_residue`. What is left here is the
    /// behaviour: dropping the `oneshot::Sender` is what makes a caller parked
    /// inside the permission gate return, and a teardown that freed the memory
    /// without dropping the sender would leave that caller waiting out the full
    /// 300 s timeout on a session that no longer exists.
    #[tokio::test]
    async fn cleanup_session_unblocks_a_waiting_permission_prompt() {
        let session_manager = temp_session_manager();
        let agent_manager = create_test_agent_manager(session_manager);

        let session_id = "cleanup-unblocks-prompt";
        let (_permission_id, rx) =
            agent_manager.await_permission(session_id, PermRequest::bash(["npm", "install"]));

        agent_manager.cleanup_session(session_id);

        assert!(
            rx.await.is_err(),
            "the waiter must be released, not left to time out"
        );
    }

    #[tokio::test]
    async fn channel_drop_results_in_recv_error() {
        let session_manager = temp_session_manager();
        let agent_manager = create_test_agent_manager(session_manager);

        let session_id = "test-session";
        let request = PermRequest::bash(["npm", "install"]);

        let (permission_id, rx) = agent_manager.await_permission(session_id, request);

        // Remove the pending permission without responding (simulates cleanup/drop)
        agent_manager.slot(session_id).drop_permissions();

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
        let session_manager = temp_session_manager();
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
        let session_manager = temp_session_manager();
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
            assert!(
                ids.contains(expected),
                "Should contain permission {expected}"
            );
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
        let session_manager = temp_session_manager();
        let agent_manager = create_test_agent_manager(session_manager);

        let (id1, _rx1) =
            agent_manager.await_permission("session-a", PermRequest::bash(["cargo", "test"]));
        let (id2, _rx2) = agent_manager.await_permission("session-b", PermRequest::bash(["ls"]));

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

        let _tmp = TempDir::new().unwrap();
        let session_manager = temp_session_manager();

        let session = session_manager
            .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
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
                !agent_manager.has_cached_agent(&session.id),
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

/// The gates outside the agent dispatch path resolve the *session's* rules,
/// not just the daemon-global ones.
mod session_permission_config_tests {
    use super::*;
    use crucible_core::config::components::{
        acp::{AcpConfig, AgentProfile},
        permissions::{PermissionConfig, PermissionMode},
    };
    use crucible_core::session::SessionType;
    use std::collections::HashMap;

    /// An `AgentManager` whose global rules are `default = allow` and whose
    /// `my-claude` profile is the stricter `default = deny`.
    fn manager_with_strict_profile(session_manager: Arc<SessionManager>) -> AgentManager {
        let mut agents = HashMap::new();
        agents.insert(
            "my-claude".to_string(),
            AgentProfile {
                permissions: Some(PermissionConfig {
                    default: PermissionMode::Deny,
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        let (event_tx, _) = broadcast::channel(16);
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
        AgentManager::new(AgentManagerParams {
            kiln_manager: Arc::new(KilnManager::new()),
            session_manager,
            background_manager,
            mcp_gateway: None,
            llm_config: None,
            acp_config: Some(AcpConfig {
                agents,
                ..Default::default()
            }),
            context_config: None,
            permission_config: Some(PermissionConfig {
                default: PermissionMode::Allow,
                ..Default::default()
            }),
            plugin_loader: None,
        })
    }

    /// Register a session whose agent names `profile`, and return its id.
    fn session_naming_profile(session_manager: &SessionManager, profile: Option<&str>) -> String {
        let mut agent = test_agent();
        agent.agent_name = profile.map(str::to_string);
        let mut session = crucible_core::session::Session::new(SessionType::Chat, Vec::new());
        session.agent = Some(agent);
        let id = session.id.to_string();
        session_manager.register_transient(session);
        id
    }

    /// A session whose agent card carries its own `[permissions]` block is
    /// stricter than the daemon global, and the gate must honour that. Reading
    /// only `permission_config()` handed such a session the permissive global
    /// rules — the opposite of what the operator wrote.
    #[test]
    fn a_session_profile_overrides_the_global_config() {
        let session_manager = temp_session_manager();
        let manager = manager_with_strict_profile(session_manager.clone());
        let session_id = session_naming_profile(&session_manager, Some("my-claude"));

        let resolved = manager
            .session_permission_config(&session_id)
            .expect("a config is configured");

        assert_eq!(
            resolved.default,
            PermissionMode::Deny,
            "the session's own profile must outrank the daemon-global default"
        );
    }

    /// ...and a session with no profile of its own still gets the global
    /// rules, so honouring the profile did not detach the gate from config.
    #[test]
    fn a_session_without_a_profile_falls_back_to_the_global_config() {
        let session_manager = temp_session_manager();
        let manager = manager_with_strict_profile(session_manager.clone());
        let session_id = session_naming_profile(&session_manager, None);

        let resolved = manager
            .session_permission_config(&session_id)
            .expect("a config is configured");

        assert_eq!(resolved.default, PermissionMode::Allow);
    }

    /// A session id nothing knows about resolves to the global rules rather
    /// than to nothing — `None` here would mean an empty engine, which is the
    /// permissive answer, and an unknown session is not a reason to relax.
    #[test]
    fn an_unknown_session_falls_back_to_the_global_config() {
        let session_manager = temp_session_manager();
        let manager = manager_with_strict_profile(session_manager.clone());

        let resolved = manager
            .session_permission_config("no-such-session")
            .expect("a config is configured");

        assert_eq!(resolved.default, PermissionMode::Allow);
    }
}

/// A reply is routed by which registry holds its id, not by its own shape.
///
/// `server/session/messaging.rs` matched on the RESPONSE's kind: a
/// `Permission` payload went to the permission registry and everything else to
/// the interaction registry. That is wrong whenever the two disagree, and they
/// disagree in production: when Crucible runs as an ACP agent and the host
/// cancels the permission dialog — or `request_permission` errors —
/// `commands/acp/agent.rs` sends `InteractionResponse::Cancelled` for a
/// `perm-…` id. Routing by kind sent it to the interactions map, missed, and
/// logged at debug. The permission waiter was never released, so the turn
/// stalled the full 300 s and then denied.
///
/// The TUI dodged it only by convention (Esc maps to `PermResponse::deny()`,
/// never `Cancelled`), so nothing in the suite noticed.
mod reply_routing_tests {
    use super::*;
    use crucible_core::interaction::{InteractionResponse, PermRequest};

    /// Cancelling a permission prompt must release its waiter, as a deny.
    #[tokio::test]
    async fn a_cancelled_reply_to_a_permission_id_denies_instead_of_stalling() {
        let session_manager = temp_session_manager();
        let agent_manager = create_test_agent_manager(session_manager);

        let (permission_id, response_rx) = agent_manager
            .await_permission("test-session", PermRequest::bash(["rm", "-rf", "/"]));

        agent_manager
            .deliver_client_reply(
                "test-session",
                &permission_id,
                InteractionResponse::Cancelled,
            )
            .expect("a cancelled permission must be deliverable");

        let response = response_rx
            .await
            .expect("the waiter must be released, not left parked for the timeout");
        assert!(
            !response.allowed,
            "cancelling a permission prompt is a refusal, not an approval"
        );
    }

    /// ...and a permission id still takes an ordinary permission answer, so
    /// routing by ownership did not break the path that already worked.
    #[tokio::test]
    async fn a_permission_reply_to_a_permission_id_still_arrives() {
        let session_manager = temp_session_manager();
        let agent_manager = create_test_agent_manager(session_manager);

        let (permission_id, response_rx) =
            agent_manager.await_permission("test-session", PermRequest::bash(["ls"]));

        agent_manager
            .deliver_client_reply(
                "test-session",
                &permission_id,
                InteractionResponse::Permission(PermResponse::allow()),
            )
            .expect("deliverable");

        let response = response_rx.await.expect("released");
        assert!(response.allowed);
    }
}

/// `cru.ui.permission` — a plugin asking for a decision, not the agent gate.
///
/// It registers in the INTERACTION registry (`ix-…`), because a plugin's
/// question resolves to `cancelled` on silence while the agent's gate must
/// resolve to `deny`. But the answer comes back shaped as
/// `InteractionResponse::Permission`, and routing by the reply's kind sent it
/// to the permission registry, which has no such id. The user clicked Allow,
/// `interaction_completed` fired so the modal closed, and the plugin waited
/// out its full timeout and was told `cancelled`.
mod plugin_permission_tests {
    use super::*;
    use crucible_core::interaction::{InteractionRequest, InteractionResponse, PermRequest};

    #[tokio::test]
    async fn a_plugin_permission_request_can_actually_be_answered() {
        let session_manager = temp_session_manager();
        // A real session: `request_interaction` checks existence before minting
        // an id, so a request nothing could ever answer is an error instead.
        let session = crucible_core::session::Session::new(
            crucible_core::session::SessionType::Chat,
            Vec::new(),
        );
        let session_id = session.id.to_string();
        session_manager.register_transient(session);
        let agent_manager = create_test_agent_manager(session_manager);
        let (event_tx, mut event_rx) = broadcast::channel(16);

        let request = InteractionRequest::Permission(PermRequest::bash(["ls"]));
        let am = Arc::new(agent_manager);
        let asked = tokio::spawn({
            let am = Arc::clone(&am);
            let sid = session_id.clone();
            async move {
                am.request_interaction(
                    &sid,
                    request,
                    &event_tx,
                    std::time::Duration::from_secs(5),
                )
                .await
            }
        });

        // Take the id off the wire exactly as a client would.
        let request_id = loop {
            let msg = event_rx.recv().await.expect("interaction_requested");
            if msg.event == "interaction_requested" {
                break msg.data["request_id"].as_str().expect("request_id").to_string();
            }
        };
        assert!(
            request_id.starts_with("ix-"),
            "a plugin request lives in the interaction registry: {request_id}"
        );

        am
            .deliver_client_reply(
                &session_id,
                &request_id,
                InteractionResponse::Permission(PermResponse::allow()),
            )
            .expect("a permission answer must reach the plugin that asked");

        let answer = asked.await.expect("join").expect("interaction resolved");
        match answer {
            InteractionResponse::Permission(p) => assert!(p.allowed),
            other => panic!("the plugin was told {other:?}, not its answer"),
        }
    }
}
