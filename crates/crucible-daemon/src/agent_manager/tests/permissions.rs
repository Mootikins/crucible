use super::*;
use crate::test_support::{kiln_name, temp_session_manager};
use crucible_core::types::CanonicalToolCall;

/// The one tool policy, driven through the daemon's own turn loop.
///
/// The unit tests of `decide_permission` pin the decision. These pin that the
/// daemon's tool path acts on it — a call that `decide_permission` refuses
/// must never dispatch, and a call it approves must carry the marker that
/// says which layer granted it.
mod the_tool_gate_in_a_turn {
    use super::*;

    /// Run one tool call through a turn; return the `tool_result` payload the
    /// model reads.
    async fn dispatch(h: &mut ReactorTestHarness, tool: &str) -> serde_json::Value {
        h.inject_streaming_agent(vec![
            script::tool_call("call-1", tool, serde_json::json!({})),
            script::text("done"),
            script::done(),
        ]);
        h.send("run tool").await;
        let result = h.wait_for("tool_result").await;
        assert_eq!(result.data["tool"], tool, "wrong tool reported");
        h.wait_for("message_complete").await;
        result.data["result"].clone()
    }

    /// An operator `deny` refuses a read-only tool, which never reaches the
    /// prompt at all.
    ///
    /// `get_kiln_info` is on the built-in safe list, so it takes the
    /// read-only exemption and skips the gate. The operator's rule is the
    /// only thing left that can refuse it, and it must.
    #[tokio::test]
    async fn an_operator_deny_refuses_a_read_only_tool() {
        let config = PermissionConfig {
            deny: vec!["get_kiln_info:*".to_string()],
            ..Default::default()
        };
        let mut h = ReactorTestHarness::with_permissions(Some(config)).await;

        let payload = dispatch(&mut h, "get_kiln_info").await;
        let error = payload["error"].as_str().unwrap_or_default();
        assert!(
            error.contains("denied by permissions config"),
            "`deny = [\"get_kiln_info:*\"]` must refuse a read-only tool, got: {payload:?}"
        );

        // The counterfactual: with no rule the same call answers. Without it
        // this test would pass on a tool that simply never worked.
        let mut h = ReactorTestHarness::new().await;
        let payload = dispatch(&mut h, "get_kiln_info").await;
        assert!(
            payload.get("error").is_none(),
            "the same call must answer with no rule against it, got: {payload:?}"
        );
    }

    /// A card `allow` runs the tool and says so.
    ///
    /// The marker is the user's only sight of a grant they never saw made, so
    /// it rides on the `tool_call` event rather than arriving later.
    #[tokio::test]
    async fn a_card_allow_marks_the_call_it_granted() {
        let mut h = ReactorTestHarness::new().await;
        let mut agent = test_agent();
        agent.tool_policy = Some(HashMap::from([(
            "get_kiln_info".to_string(),
            crucible_core::agent::ToolPolicy::Allow,
        )]));
        h.reconfigure(agent).await;

        h.inject_streaming_agent(vec![
            script::tool_call("call-1", "get_kiln_info", serde_json::json!({})),
            script::text("done"),
            script::done(),
        ]);
        h.send("run tool").await;
        let call = h.wait_for("tool_call").await;
        assert_eq!(
            call.data["auto_approved"], "agent card policy",
            "a card grant must name itself on the call it granted"
        );
        h.wait_for("message_complete").await;
    }

    /// A read-only tool nobody granted anything to carries no marker.
    ///
    /// Nothing was granted, because nothing was needed. A marker here would
    /// tell the user a permission decision happened when none did.
    #[tokio::test]
    async fn a_read_only_tool_carries_no_marker() {
        let mut h = ReactorTestHarness::new().await;

        h.inject_streaming_agent(vec![
            script::tool_call("call-1", "get_kiln_info", serde_json::json!({})),
            script::text("done"),
            script::done(),
        ]);
        h.send("run tool").await;
        let call = h.wait_for("tool_call").await;
        assert!(
            call.data["auto_approved"].is_null(),
            "a read-only tool was granted nothing, got: {:?}",
            call.data
        );
        h.wait_for("message_complete").await;
    }

    /// Run one call to a tool that no rule answers, with these turn
    /// settings. Return the error of each `tool_result` event and the error
    /// that the conversation tree records for the model.
    async fn deny_through_the_prompt_gate(
        is_interactive: bool,
        permission_override: Option<PermissionMode>,
    ) -> (Vec<String>, Option<String>) {
        let mut h = ReactorTestHarness::new().await;
        h.inject_streaming_agent(vec![
            script::tool_call("call-1", "write", serde_json::json!({ "path": "a" })),
            script::text("done"),
            script::done(),
        ]);
        h.agent_manager
            .send_message(
                &h.session_id,
                "run tool".to_string(),
                &h.event_tx,
                is_interactive,
                permission_override,
            )
            .await
            .unwrap();
        let mut events = Vec::new();
        loop {
            let event = h
                .wait_for_first_of(&["tool_result", "message_complete"])
                .await;
            if event.event == "message_complete" {
                break;
            }
            events.push(event.data["result"]["error"].as_str().unwrap().to_string());
        }
        let tree = h.agent_manager.get_session_tree(&h.session_id).unwrap();
        let tree = tree.lock().await;
        let model_error = tree.iter().find_map(|(_, node)| match &node.content {
            crucible_core::turn::NodeContent::ToolResult { error, .. } => error.clone(),
            _ => None,
        });
        (events, model_error)
    }

    /// A denial in the prompt gate gives the views ONE `tool_result`, and
    /// the model reads the same reason.
    #[tokio::test]
    async fn a_prompt_gate_denial_emits_one_result_with_the_reason_of_the_model() {
        for (interactive, permission_override, reason) in [
            (
                true,
                Some(PermissionMode::Deny),
                "Tool call denied by permission override",
            ),
            (false, None, "this session runs non-interactively"),
        ] {
            let (events, model_error) =
                deny_through_the_prompt_gate(interactive, permission_override).await;
            assert_eq!(events.len(), 1, "one tool_result, got: {events:?}");
            assert!(events[0].contains(reason), "got: {events:?}");
            assert_eq!(model_error.as_deref(), Some(events[0].as_str()));
        }
    }
}

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

mod pattern_matching_tests {
    use super::*;
    use test_case::test_case;

    /// The "Allowlist" answer must outlive the run that took it.
    ///
    /// The daemon is the only writer of a grant: it stores the pattern the
    /// modal suggested under the whitelists directory, and the next run
    /// loads both stores from there and skips the prompt. So the suggestion
    /// and the store have to speak one pattern language. A suggestion the
    /// store cannot match reads to the user as a saved rule and grants
    /// nothing, and no restart ever reveals it.
    ///
    /// This walks the whole chain the click walks, minus the socket: the
    /// modal builds the response, the daemon stores it, and a store loaded
    /// fresh from disk answers the same call.
    // The daemon sends a shell call as `PermRequest::tool("bash", …)`, so
    // that is the request the first two cases build.
    #[test_case(
        PermRequest::tool("bash", serde_json::json!({"command": "cargo build --release"})),
        "bash",
        serde_json::json!({"command": "cargo build --release"}),
        crucible_core::interaction::PermissionScope::Project;
        "a_bash_grant_for_this_project"
    )]
    #[test_case(
        PermRequest::bash(["cargo", "build", "--release"]),
        "bash",
        serde_json::json!({"command": "cargo build --release"}),
        crucible_core::interaction::PermissionScope::User;
        "a_bash_grant_for_this_user"
    )]
    #[test_case(
        PermRequest::write(["src", "main.rs"]),
        "write_file",
        serde_json::json!({"path": "src/main.rs"}),
        crucible_core::interaction::PermissionScope::Project;
        "a_file_grant_for_this_project"
    )]
    #[test_case(
        PermRequest::tool("fs_read_file", serde_json::json!({})),
        "fs_read_file",
        serde_json::json!({}),
        crucible_core::interaction::PermissionScope::User;
        "a_tool_grant_for_this_user"
    )]
    fn an_allowlist_grant_still_skips_the_prompt_after_a_restart(
        request: crucible_core::interaction::PermRequest,
        tool_name: &str,
        args: serde_json::Value,
        scope: crucible_core::interaction::PermissionScope,
    ) {
        use crucible_core::interaction::PermResponse;

        let tmp = TempDir::new().unwrap();
        let whitelists_dir = tmp.path().join("whitelists.d");
        let project_path = "/some/project";

        // What the modal sends when the user picks "Allowlist".
        let response = PermResponse::allow_pattern(
            request.suggested_pattern().expect("the call has a grant"),
            scope,
        );
        let pattern = response.pattern.clone().expect("the modal sends a pattern");

        // What the daemon does with it, on the run that asked.
        let file = PatternStore::store_file_in(&whitelists_dir, response.scope, project_path)
            .expect("a persisted scope has a store file");
        AgentManager::store_pattern_to(
            &file,
            &CanonicalToolCall::crucible_tool(tool_name, &args),
            &pattern,
        )
        .expect("the grant is stored");

        // The next run: nothing in memory, both stores read from disk, the
        // way `decide_permission` reads them.
        let store = PatternStore::load_sync_in(&whitelists_dir, project_path)
            .unwrap_or_default()
            .merge(&PatternStore::load_user_sync_in(&whitelists_dir).unwrap_or_default());

        assert!(
            AgentManager::check_pattern_match(
                &CanonicalToolCall::crucible_tool(tool_name, &args),
                &store
            ),
            "the grant of {pattern:?} must still skip the prompt after a restart",
        );
    }

    /// A grant reaches the command the user approved and no other.
    ///
    /// The same chain as the test above, asked the opposite question. One
    /// "Allowlist" click on `rm build/tmp.o` used to store `rm *`, which
    /// answered for every later `rm` on the machine — on every project, with
    /// no second prompt and no way for the user to see it. The suggestion is
    /// the command the modal displayed, so the stored grant covers that
    /// command alone.
    #[test_case("rm -rf /home/user/project"; "another_rm_with_another_target")]
    #[test_case("rm -rf ~/work"; "another_rm_with_a_home_target")]
    #[test_case("rm build/tmp.o /home/user/project"; "the_same_rm_with_an_extra_target")]
    #[test_case("bash -c 'rm -rf /home/user/project'"; "the_same_rm_behind_a_shell")]
    fn an_allowlist_grant_does_not_reach_a_wider_command(escalation: &str) {
        use crucible_core::interaction::{PermResponse, PermissionScope};

        let tmp = TempDir::new().unwrap();
        let whitelists_dir = tmp.path().join("whitelists.d");
        let project_path = "/some/project";
        let approved = serde_json::json!({"command": "rm build/tmp.o"});

        let request = PermRequest::tool("bash", approved.clone());
        let response = PermResponse::allow_pattern(
            request.suggested_pattern().unwrap(),
            PermissionScope::User,
        );
        let pattern = response.pattern.clone().expect("the modal sends a pattern");
        let file = PatternStore::store_file_in(&whitelists_dir, response.scope, project_path)
            .expect("a persisted scope has a store file");
        AgentManager::store_pattern_to(
            &file,
            &CanonicalToolCall::crucible_tool("bash", &approved),
            &pattern,
        )
        .expect("the grant is stored");

        let store = PatternStore::load_sync_in(&whitelists_dir, project_path)
            .unwrap_or_default()
            .merge(&PatternStore::load_user_sync_in(&whitelists_dir).unwrap_or_default());

        assert!(
            AgentManager::check_pattern_match(
                &CanonicalToolCall::crucible_tool("bash", &approved),
                &store
            ),
            "the grant of {pattern:?} must skip the prompt for the approved command",
        );
        assert!(
            !AgentManager::check_pattern_match(
                &CanonicalToolCall::crucible_tool(
                    "bash",
                    &serde_json::json!({ "command": escalation })
                ),
                &store
            ),
            "the grant of {pattern:?} must still prompt for {escalation:?}",
        );
    }

    #[test_case(
        "bash",
        serde_json::json!({"command": "npm install lodash"}),
        Some(("bash", "npm install *")),
        true;
        "bash_command_matches_prefix"
    )]
    #[test_case(
        "bash",
        serde_json::json!({"command": "rm -rf /"}),
        Some(("bash", "npm install *")),
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
            AgentManager::check_pattern_match(
                &CanonicalToolCall::crucible_tool(tool, &args),
                &store
            ),
            expected,
        );
    }

    #[test]
    fn file_operations_check_file_patterns() {
        let mut store = PatternStore::new();
        store.add_file_pattern("notes/").unwrap();

        let args = serde_json::json!({"name": "notes/my-note.md"});

        assert!(AgentManager::check_pattern_match(
            &CanonicalToolCall::crucible_tool("create_note", &args),
            &store
        ));
        assert!(AgentManager::check_pattern_match(
            &CanonicalToolCall::crucible_tool("update_note", &args),
            &store
        ));
        assert!(AgentManager::check_pattern_match(
            &CanonicalToolCall::crucible_tool("delete_note", &args),
            &store
        ));
    }

    #[test]
    fn empty_store_matches_nothing() {
        let store = PatternStore::new();

        let bash_args = serde_json::json!({"command": "npm install"});
        assert!(!AgentManager::check_pattern_match(
            &CanonicalToolCall::crucible_tool("bash", &bash_args),
            &store
        ));

        let file_args = serde_json::json!({"path": "src/lib.rs"});
        assert!(!AgentManager::check_pattern_match(
            &CanonicalToolCall::crucible_tool("write", &file_args),
            &store
        ));

        let tool_args = serde_json::json!({});
        assert!(!AgentManager::check_pattern_match(
            &CanonicalToolCall::crucible_tool("custom_tool", &tool_args),
            &store
        ));
    }

    /// A `User` grant lands in the user-wide store, not in a project store.
    #[test]
    fn store_pattern_persists_user_scope_grant() {
        let tmp = TempDir::new().unwrap();
        let whitelists_dir = tmp.path().join("whitelists.d");
        let project_path = "/some/project";

        let file = PatternStore::store_file_in(
            &whitelists_dir,
            crucible_core::interaction::PermissionScope::User,
            project_path,
        )
        .expect("User scope has a store file");
        assert_eq!(file, whitelists_dir.join("user.toml"));

        AgentManager::store_pattern_to(
            &file,
            &CanonicalToolCall::crucible_tool(
                "bash",
                &serde_json::json!({"command": "cargo build"}),
            ),
            "cargo build",
        )
        .unwrap();

        let user_store = PatternStore::load_file(&file).unwrap();
        assert!(user_store.matches_bash("cargo build"));

        let project_file = PatternStore::store_file_in(
            &whitelists_dir,
            crucible_core::interaction::PermissionScope::Project,
            project_path,
        )
        .unwrap();
        assert!(
            !project_file.exists(),
            "a User grant must not touch the project store"
        );
        assert!(PatternStore::store_file_in(
            &whitelists_dir,
            crucible_core::interaction::PermissionScope::Once,
            project_path,
        )
        .is_none());
    }

    /// Two sessions that grant to the same store at the same time must both
    /// land. `user.toml` is shared by every session on the machine.
    #[test]
    fn concurrent_store_pattern_calls_keep_every_grant() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("whitelists.d").join("user.toml");
        let writers = 8;
        let rounds = 10;

        for round in 0..rounds {
            let barrier = Arc::new(std::sync::Barrier::new(writers));
            let handles: Vec<_> = (0..writers)
                .map(|i| {
                    let file = file.clone();
                    let barrier = barrier.clone();
                    std::thread::spawn(move || {
                        barrier.wait();
                        // `tool{n} *` is a command name plus a wildcard, which
                        // the loader refuses. The grant under test is the
                        // round trip, so the rule carries an argument.
                        AgentManager::store_pattern_to(
                            &file,
                            &CanonicalToolCall::crucible_tool(
                                "bash",
                                &serde_json::json!({"command": "tool run now"}),
                            ),
                            &format!("tool{round}_{i} run *"),
                        )
                    })
                })
                .collect();
            for h in handles {
                h.join().unwrap().unwrap();
            }
        }

        let store = PatternStore::load_file(&file).unwrap();
        for round in 0..rounds {
            for i in 0..writers {
                let sample = format!("tool{round}_{i} run now");
                assert!(store.matches_bash(&sample), "grant lost: {sample:?}");
            }
        }
    }

    // The arguments decide which table a grant lands in, so each case carries
    // the call it was granted for, not only the tool name.
    #[test_case("bash", serde_json::json!({"command": "cargo build --release"}), "cargo build *", "cargo build --release", true; "store_pattern_adds_bash_pattern")]
    #[test_case("write_file", serde_json::json!({"path": "src/main.rs"}), "src/", "src/main.rs", true; "store_pattern_adds_file_pattern")]
    #[test_case("custom_tool", serde_json::json!({}), "custom_tool", "custom_tool", true; "store_pattern_adds_tool_pattern")]
    #[test_case("bash", serde_json::json!({"command": "ls"}), "*", "", false; "store_pattern_rejects_star_pattern")]
    fn store_pattern_outcomes(
        kind: &str,
        args: serde_json::Value,
        pattern: &str,
        sample: &str,
        should_succeed: bool,
    ) {
        let tmp = TempDir::new().unwrap();
        let file = PatternStore::project_file_in(&tmp.path().join("whitelists.d"), "/project");

        let result = AgentManager::store_pattern_to(
            &file,
            &CanonicalToolCall::crucible_tool(kind, &args),
            pattern,
        );

        if should_succeed {
            result.unwrap();
            let store = PatternStore::load_file(&file).unwrap();
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
    use tokio::sync::oneshot;

    /// Park a prompt in the session's registry through the one production
    /// path, `SessionSlot::register_permission`.
    pub(super) fn register_permission(
        agent_manager: &AgentManager,
        session_id: &str,
        request: PermRequest,
    ) -> (PermissionId, oneshot::Receiver<PermResponse>) {
        agent_manager.slot(session_id).register_permission(request)
    }

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
            RespondScenario::Allow | RespondScenario::WrongPermissionId => {
                Some(register_permission(
                    &agent_manager,
                    "test-session",
                    PermRequest::bash(["npm", "install"]),
                ))
            }
            RespondScenario::Deny => Some(register_permission(
                &agent_manager,
                "test-session",
                PermRequest::bash(["rm", "-rf", "/"]),
            )),
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
        let (_permission_id, rx) = register_permission(
            &agent_manager,
            session_id,
            PermRequest::bash(["npm", "install"]),
        );

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

        let (permission_id, rx) = register_permission(&agent_manager, session_id, request);

        // Remove the pending permission without responding (simulates cleanup/drop)
        agent_manager.slot(session_id).drop_permissions();

        // Verify the permission was removed
        assert!(
            !agent_manager
                .slot(session_id)
                .holds_permission(&permission_id),
            "Pending permission should be removed"
        );

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

        let (id1, _rx1) = register_permission(&agent_manager, session1, request1);
        let (id2, _rx2) = register_permission(&agent_manager, session2, request2);

        // Each session should only see its own permissions
        let pending1 = agent_manager.slot(session1).list_permissions();
        let pending2 = agent_manager.slot(session2).list_permissions();

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

        let pending1_after = agent_manager.slot(session1).list_permissions();
        let pending2_after = agent_manager.slot(session2).list_permissions();

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
    async fn a_slot_lists_every_prompt_it_holds() {
        let session_manager = temp_session_manager();
        let agent_manager = create_test_agent_manager(session_manager);

        let session_id = "test-session";
        let request1 = PermRequest::bash(["npm", "install"]);
        let request2 = PermRequest::write(["src", "main.rs"]);
        let request3 = PermRequest::tool("delete", serde_json::json!({"path": "/tmp/file"}));

        let (id1, _rx1) = register_permission(&agent_manager, session_id, request1);
        let (id2, _rx2) = register_permission(&agent_manager, session_id, request2);
        let (id3, _rx3) = register_permission(&agent_manager, session_id, request3);

        let pending = agent_manager.slot(session_id).list_permissions();
        assert_eq!(pending.len(), 3, "pending count should match");
        let ids: Vec<_> = pending.iter().map(|(id, _)| id.clone()).collect();
        for expected in [id1, id2, id3] {
            assert!(
                ids.contains(&expected),
                "Should contain permission {expected}"
            );
        }
    }

    #[tokio::test]
    async fn list_all_pending_permissions_aggregates_across_sessions() {
        let session_manager = temp_session_manager();
        let agent_manager = create_test_agent_manager(session_manager);

        let (id1, _rx1) = register_permission(
            &agent_manager,
            "session-a",
            PermRequest::bash(["cargo", "test"]),
        );
        let (id2, _rx2) =
            register_permission(&agent_manager, "session-b", PermRequest::bash(["ls"]));

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

        let mut providers = std::collections::BTreeMap::new();
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
            create_test_agent_manager_with_llm_config(session_manager.clone(), llm_config);

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

mod agent_profile_permission_tests {
    use crate::acp::discovery::profile;
    use crucible_core::config::components::{
        acp::{AcpConfig, AgentProfile},
        permissions::{PermissionConfig, PermissionMode},
    };
    use test_case::test_case;

    #[derive(Clone, Copy)]
    enum ProfileScenario {
        ConfiguredPermissions,
        NoPermissions,
    }

    /// A profile carries its own `[permissions]` block through to the caller,
    /// and a profile without one carries nothing.
    #[test_case(ProfileScenario::ConfiguredPermissions; "a_configured_profile_keeps_its_permissions")]
    #[test_case(ProfileScenario::NoPermissions; "a_profile_without_permissions_has_none")]
    fn agent_profile_permission_outcomes(scenario: ProfileScenario) {
        let permissions = match scenario {
            ProfileScenario::ConfiguredPermissions => Some(PermissionConfig {
                default: PermissionMode::Ask,
                ..Default::default()
            }),
            ProfileScenario::NoPermissions => None,
        };
        let mut agents = std::collections::BTreeMap::new();
        agents.insert(
            "claude".to_string(),
            AgentProfile {
                permissions: permissions.clone(),
                ..Default::default()
            },
        );
        let config = AcpConfig {
            agents,
            ..Default::default()
        };

        let resolved = profile("claude", &config)
            .expect("a built-in overlay resolves")
            .expect("claude is a built-in");

        match permissions {
            Some(_) => assert_eq!(
                resolved
                    .permissions
                    .expect("should have permissions")
                    .default,
                PermissionMode::Ask
            ),
            None => assert!(resolved.permissions.is_none()),
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

    /// An `AgentManager` whose global rules are `default = allow` and whose
    /// `my-claude` profile is the stricter `default = deny`.
    fn manager_with_strict_profile(session_manager: Arc<SessionManager>) -> AgentManager {
        let mut agents = std::collections::BTreeMap::new();
        agents.insert(
            "my-claude".to_string(),
            AgentProfile {
                // `my-claude` is not a built-in, so it must say what to run.
                command: Some("my-claude".to_string()),
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
            card_roots: Default::default(),
            review_snapshot_root: crate::test_support::scratch_snapshot_root(),
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
    use super::permission_channel_tests::register_permission;
    use super::*;
    use crucible_core::interaction::{InteractionResponse, PermRequest};

    /// Cancelling a permission prompt must release its waiter, as a deny.
    #[tokio::test]
    async fn a_cancelled_reply_to_a_permission_id_denies_instead_of_stalling() {
        let session_manager = temp_session_manager();
        let agent_manager = create_test_agent_manager(session_manager);

        let (permission_id, response_rx) = register_permission(
            &agent_manager,
            "test-session",
            PermRequest::bash(["rm", "-rf", "/"]),
        );

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
            register_permission(&agent_manager, "test-session", PermRequest::bash(["ls"]));

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
                am.request_interaction(&sid, request, &event_tx, std::time::Duration::from_secs(5))
                    .await
            }
        });

        // Take the id off the wire exactly as a client would.
        let request_id = loop {
            let msg = event_rx.recv().await.expect("interaction_requested");
            if msg.event == "interaction_requested" {
                break msg.data["request_id"]
                    .as_str()
                    .expect("request_id")
                    .to_string();
            }
        };
        assert!(
            request_id.starts_with("ix-"),
            "a plugin request lives in the interaction registry: {request_id}"
        );

        am.deliver_client_reply(
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

/// "Always allow" must work for every tool the display layer calls a command,
/// not only for the one named exactly `bash`.
///
/// The suggestion and the routing are two halves of one click. The suggestion
/// asks [`CanonicalToolCall`], which calls `shell`, `Bash` and `myserver__bash`
/// commands; the routing compared the name to the literal `"bash"`. So a
/// click on any other command tool filed a shell command line as a tool-NAME
/// rule, which `matches_tool` can never answer: the user was prompted again
/// on the very next identical call, for ever.
///
/// The names come from the classifier itself, so a name added there is
/// covered here without an edit.
mod always_allow_covers_every_command_tool {
    use super::*;
    use crucible_core::types::CanonicalToolCall;

    /// Every spelling of `base` that `CanonicalToolCall` classifies as a command.
    fn spellings(base: &str) -> Vec<String> {
        vec![
            base.to_string(),
            base.to_ascii_uppercase(),
            format!("myserver__{base}"),
        ]
    }

    /// A grant that the user gives for an agent's command permits the next
    /// call of that command, from the agent and from Crucible's `bash`.
    #[tokio::test]
    async fn a_grant_for_an_acp_command_permits_the_same_command_from_both_agents() {
        // The `shell` case of `tool_frames/codex-ts.jsonl`: no name.
        let acp = || {
            crucible_core::types::classify_acp(
                serde_json::from_value(serde_json::json!({
                    "toolCallId": "call_shell1",
                    "kind": "execute",
                    "title": "Run command",
                    "rawInput": { "command": "cargo test", "cwd": "/home/user/project" },
                }))
                .expect("a raw tool call"),
                &[],
            )
        };
        let bash = |command: &str| {
            CanonicalToolCall::crucible_tool("bash", &serde_json::json!({ "command": command }))
        };

        // The prompt that the ACP gate puts to the user. The user answers
        // "always allow" with the pattern that the prompt suggests.
        let args = serde_json::json!({ "command": "cargo test" });
        let pattern =
            crate::agent_manager::messaging::permission::acp_prompt_request(&acp(), &args)
                .suggested_pattern()
                .expect("a command has a grant");

        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("whitelists.d").join("user.toml");
        AgentManager::store_pattern_to(&file, &acp(), &pattern).expect("the grant is stored");
        let store = PatternStore::load_file(&file).expect("the store reloads");

        assert!(
            AgentManager::check_pattern_match(&acp(), &store),
            "the grant {pattern:?} must permit the next ACP call"
        );
        assert!(
            AgentManager::check_pattern_match(&bash("cargo test"), &store),
            "the grant {pattern:?} must permit the same command from Crucible's bash"
        );
        assert!(
            !AgentManager::check_pattern_match(&bash("rm -rf /home/user/project"), &store),
            "the grant {pattern:?} must not permit another command"
        );
    }

    #[test]
    fn a_grant_permits_the_same_call_again() {
        let args = serde_json::json!({"command": "ls -la"});

        for base in CanonicalToolCall::COMMAND_TOOL_NAMES {
            for name in spellings(base) {
                assert_eq!(
                    CanonicalToolCall::crucible_tool(&name, &args).kind,
                    "command",
                    "{name} must be a command for this test to mean anything",
                );

                // What the modal offers, and what the daemon does with it.
                let pattern = PermRequest::tool(name.clone(), args.clone())
                    .suggested_pattern()
                    .expect("a command has a grant");
                let tmp = TempDir::new().unwrap();
                let file = tmp.path().join("whitelists.d").join("user.toml");
                AgentManager::store_pattern_to(
                    &file,
                    &CanonicalToolCall::crucible_tool(&name, &args),
                    &pattern,
                )
                .expect("the grant is stored");

                // The next identical call, reading the store back from disk.
                let store = PatternStore::load_file(&file).expect("the store reloads");
                assert!(
                    AgentManager::check_pattern_match(
                        &CanonicalToolCall::crucible_tool(&name, &args),
                        &store
                    ),
                    "allowlisting {name} as {pattern:?} must permit the same call again",
                );

                // And the grant is the command, not the tool: a wider command
                // through the same tool still prompts.
                assert!(
                    !AgentManager::check_pattern_match(
                        &CanonicalToolCall::crucible_tool(
                            &name,
                            &serde_json::json!({"command": "rm -rf /home/user/project"})
                        ),
                        &store
                    ),
                    "allowlisting {name} as {pattern:?} must not permit another command",
                );
            }
        }
    }
}
