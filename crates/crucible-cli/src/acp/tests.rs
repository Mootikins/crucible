//! Tests for ACP module
#![allow(deprecated)]

#[cfg(test)]
mod agent_tests {
    use super::super::*;

    #[tokio::test]
    async fn test_is_agent_available_ls() {
        // ls should be available and supports --version
        let _result = is_agent_available("ls").await;
        // Note: ls may or may not support --version depending on system
        // Just verify the function runs without panic
    }

    #[tokio::test]
    async fn test_is_agent_available_nonexistent() {
        let result = is_agent_available("definitely-not-a-real-command-xyz123").await;
        assert!(!result);
    }

    #[test]
    fn test_agent_info_creation() {
        let agent = AgentInfo {
            name: "test-agent".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        assert_eq!(agent.name, "test-agent");
        assert_eq!(agent.command, "test-cmd");
    }

    #[test]
    fn test_agent_info_clone() {
        let agent = AgentInfo {
            name: "test-agent".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        let cloned = agent.clone();
        assert_eq!(agent.name, cloned.name);
        assert_eq!(agent.command, cloned.command);
    }

    #[test]
    fn test_agent_info_debug() {
        let agent = AgentInfo {
            name: "test-agent".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        let debug_str = format!("{:?}", agent);
        assert!(debug_str.contains("test-agent"));
        assert!(debug_str.contains("test-cmd"));
    }

    #[tokio::test]
    async fn test_discover_agent_with_nonexistent_preferred() {
        // Note: cache is only checked when preferred=None, so no need to clear it here
        // When preferred agent doesn't exist, should fall back to known agents
        let result = discover_agent(Some("nonexistent-agent-xyz")).await;

        // This will succeed if ANY known agent is available on the system
        // or fail if no agents are found at all
        match result {
            Ok(agent) => {
                // Fallback succeeded - should be one of the known agents
                // List matches KNOWN_AGENTS in crucible-acp/src/discovery.rs
                assert!(
                    agent.name == "opencode"
                        || agent.name == "claude"
                        || agent.name == "gemini"
                        || agent.name == "codex"
                        || agent.name == "cursor",
                    "Should fall back to a known agent, got: {}",
                    agent.name
                );
            }
            Err(e) => {
                // No agents available at all - expected in isolated test environments
                let error_msg = e.to_string();
                assert!(error_msg.contains("No compatible ACP agent found"));
            }
        }
    }

    #[tokio::test]
    async fn test_discover_agent_no_preferred() {
        // Note: This may use a cached agent from other tests - that's acceptable
        // The test verifies discover_agent works, not specific agent discovery behavior

        // Should try to find any available agent (or use cache if populated)
        let result = discover_agent(None).await;

        // Will fail if no compatible agents are installed
        // This is expected in most test environments
        if let Err(e) = result {
            let error_msg = e.to_string();
            assert!(error_msg.contains("No compatible ACP agent found"));
        }
    }

    #[tokio::test]
    async fn test_is_agent_available_with_multiple_calls() {
        // Test that multiple calls work
        let _result1 = is_agent_available("ls").await;
        let _result2 = is_agent_available("ls").await;
        // Just verify it doesn't panic on repeated calls
    }

    #[tokio::test]
    async fn test_is_agent_available_empty_string() {
        let result = is_agent_available("").await;
        assert!(!result);
    }

    #[tokio::test]
    async fn test_is_agent_available_with_spaces() {
        let result = is_agent_available("command with spaces").await;
        assert!(!result);
    }

    #[tokio::test]
    async fn test_is_agent_available_with_path() {
        // Test with absolute path to command
        let _result = is_agent_available("/bin/ls").await;
        // May or may not be available depending on system
    }
}

#[cfg(test)]
mod context_tests {

    // Note: Full context enrichment tests would require a test database
    // These are structural tests

    #[test]
    fn test_context_enricher_default_size() {
        // This test just validates the structure compiles
        // Real tests would need a mock facade
    }
}

#[cfg(test)]
mod client_tests {
    use super::super::*;
    use crucible_acp::{ChatSessionConfig, ContextConfig, HistoryConfig, StreamConfig};

    #[test]
    fn test_client_creation() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        let client = CrucibleAcpClient::new(agent, true);
        assert!(!client.is_connected(), "New client should not be connected");
        assert!(
            client.session_id().is_none(),
            "New client should have no session ID"
        );
    }

    #[test]
    fn test_client_creation_read_only() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        let client = CrucibleAcpClient::new(agent, true);
        assert!(!client.is_connected());
    }

    #[test]
    fn test_client_creation_write_enabled() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        let client = CrucibleAcpClient::new(agent, false);
        assert!(!client.is_connected());
    }

    #[test]
    fn test_client_with_custom_config() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        let config = ChatSessionConfig {
            history: HistoryConfig {
                max_messages: 100,
                max_tokens: 50000,
                enable_persistence: false,
            },
            context: ContextConfig {
                enabled: true,
                context_size: 10,
                use_reranking: true,
                rerank_candidates: Some(30),
                enable_cache: true,
                cache_ttl_secs: 600,
                inject_context: false,
            },
            streaming: StreamConfig::default(),
            auto_prune: false,
            enrich_prompts: false,
        };

        let client = CrucibleAcpClient::with_config(agent, true, config);
        assert!(!client.is_connected(), "New client should not be connected");
    }

    #[test]
    fn test_client_default_config() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        let client = CrucibleAcpClient::new(agent, false);
        assert!(!client.is_connected());
        assert!(
            client.get_stats().is_none(),
            "Unconnected client should have no stats"
        );
    }

    #[test]
    fn test_client_stats_before_connection() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        let client = CrucibleAcpClient::new(agent, false);
        let stats = client.get_stats();
        assert!(stats.is_none(), "Should have no stats before connection");
    }

    #[test]
    fn test_client_session_id_before_connection() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        let client = CrucibleAcpClient::new(agent, false);
        assert!(
            client.session_id().is_none(),
            "Should have no session ID before connection"
        );
    }

    #[test]
    fn test_client_clear_history_before_connection() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        let mut client = CrucibleAcpClient::new(agent, false);
        // Should not panic when clearing history on unconnected client
        client.clear_history();
    }

    #[test]
    fn test_client_set_context_enrichment() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        let mut client = CrucibleAcpClient::new(agent, false);

        // Should not panic when toggling enrichment on unconnected client
        client.set_context_enrichment(false);
        client.set_context_enrichment(true);
    }

    #[tokio::test]
    async fn test_client_send_message_before_connection() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        let mut client = CrucibleAcpClient::new(agent, false);
        let result = client.send_message_acp("test").await;

        assert!(
            result.is_err(),
            "Should error when sending message before connection"
        );
        assert!(
            result.unwrap_err().to_string().contains("not running"),
            "Error should indicate agent is not running"
        );
    }

    #[tokio::test]
    async fn test_client_shutdown_before_connection() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        let mut client = CrucibleAcpClient::new(agent, false);
        let result = client.shutdown().await;

        // Should succeed (no-op) when shutting down unconnected client
        assert!(
            result.is_ok(),
            "Should not error when shutting down unconnected client"
        );
    }

    // Note: Testing spawn() requires an actual agent binary (claude-code, etc.)
    // These would be integration tests run only when the agent is available.
    // For unit tests, we test the wrapper logic without actually spawning.

    #[test]
    fn test_client_mode_tracking() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        let client = CrucibleAcpClient::new(agent, true);
        // Should track current mode (starts in Plan for read_only=true)
        assert_eq!(client.mode_id(), "plan");

        let client2 = CrucibleAcpClient::new(
            AgentInfo {
                name: "test".to_string(),
                command: "test-cmd".to_string(),
                args: vec![],
                env_vars: std::collections::HashMap::new(),
            },
            false,
        );
        // Write-enabled should start in Normal mode (default)
        assert_eq!(client2.mode_id(), "normal");
    }

    #[tokio::test]
    async fn test_client_set_mode() {
        use crate::chat::AgentHandle;

        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        let mut client = CrucibleAcpClient::new(agent, false);
        assert_eq!(client.mode_id(), "normal");

        // Should be able to change mode
        client.set_mode_str("plan").await.unwrap();
        assert_eq!(client.mode_id(), "plan");

        client.set_mode_str("auto").await.unwrap();
        assert_eq!(client.mode_id(), "auto");

        client.set_mode_str("normal").await.unwrap();
        assert_eq!(client.mode_id(), "normal");
    }

    #[tokio::test]
    async fn test_client_set_mode_idempotent() {
        use crate::chat::AgentHandle;

        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        let mut client = CrucibleAcpClient::new(agent, false);

        // Setting same mode multiple times should be fine
        client.set_mode_str("plan").await.unwrap();
        assert_eq!(client.mode_id(), "plan");

        client.set_mode_str("plan").await.unwrap();
        assert_eq!(client.mode_id(), "plan");
    }

    #[tokio::test]
    async fn test_chat_agent_trait_impl() {
        use crate::chat::AgentHandle;

        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };

        let mut client = CrucibleAcpClient::new(agent, false);

        // Test AgentHandle trait methods
        assert!(!client.is_connected());

        // Mode should work through trait
        assert_eq!(client.mode_id(), "normal");

        // Should be able to call set_mode through trait
        client.set_mode_str("plan").await.unwrap();
        assert_eq!(client.mode_id(), "plan");

        // send_message should fail (not connected) but be callable through trait
        let result = AgentHandle::send_message(&mut client, "test").await;
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod env_passthrough_tests {
    use super::super::*;
    use std::collections::HashMap;

    #[test]
    fn test_client_config_includes_env_vars_from_agent() {
        // Given an agent with env_vars
        let mut env_vars = HashMap::new();
        env_vars.insert(
            "LOCAL_ENDPOINT".to_string(),
            "http://localhost:11434".to_string(),
        );
        env_vars.insert("OPENAI_API_KEY".to_string(), "test-key".to_string());

        let agent = AgentInfo {
            name: "test-agent".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars,
        };

        // When we build the client config
        let client = CrucibleAcpClient::new(agent, false);
        let config = client.build_client_config();

        // Then the config should include the env vars
        let env_vec = config.env_vars.expect("env_vars should be Some");
        assert_eq!(env_vec.len(), 2);

        // Verify both env vars are present (order doesn't matter)
        let has_endpoint = env_vec
            .iter()
            .any(|(k, v)| k == "LOCAL_ENDPOINT" && v == "http://localhost:11434");
        let has_api_key = env_vec
            .iter()
            .any(|(k, v)| k == "OPENAI_API_KEY" && v == "test-key");

        assert!(has_endpoint, "Should include LOCAL_ENDPOINT");
        assert!(has_api_key, "Should include OPENAI_API_KEY");
    }

    #[test]
    fn test_client_config_with_empty_env_vars() {
        // Given an agent with no env_vars
        let agent = AgentInfo {
            name: "test-agent".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: HashMap::new(),
        };

        // When we build the client config
        let client = CrucibleAcpClient::new(agent, false);
        let config = client.build_client_config();

        // Then env_vars should be None (not empty Vec)
        assert!(config.env_vars.is_none(), "Empty env_vars should be None");
    }
}

#[cfg(test)]
mod working_dir_tests {
    use super::super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Documents current behavior: working_dir is None by default.
    /// This causes fallback to std::env::current_dir() at connect time.
    #[test]
    fn test_client_config_working_dir_is_none_by_default() {
        let agent = AgentInfo {
            name: "test-agent".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: HashMap::new(),
        };

        let client = CrucibleAcpClient::new(agent, false);
        let config = client.build_client_config();

        // Current behavior: working_dir is always None
        assert!(
            config.working_dir.is_none(),
            "Default working_dir should be None"
        );
    }

    /// TDD: Working directory should be capturable at construction time.
    ///
    /// This test will FAIL until we add `with_working_dir()` method.
    /// The working directory should be passed through to the ClientConfig
    /// so the agent operates in the correct directory.
    #[test]
    fn test_client_with_explicit_working_dir() {
        let agent = AgentInfo {
            name: "test-agent".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: HashMap::new(),
        };

        let workspace = PathBuf::from("/home/user/my-project");

        // This should set the working directory for the agent
        let client = CrucibleAcpClient::new(agent, false).with_working_dir(workspace.clone());

        let config = client.build_client_config();

        // The working_dir should be passed through to ClientConfig
        assert_eq!(
            config.working_dir,
            Some(workspace),
            "ClientConfig should contain the explicit working_dir"
        );
    }

    /// TDD: AgentInitParams should capture working_dir for ACP agents.
    ///
    /// When an agent is initialized, the working directory should be captured
    /// and passed to the ACP client.
    #[test]
    fn test_agent_init_params_has_working_dir() {
        use crate::factories::agent::AgentInitParams;

        let workspace = PathBuf::from("/home/user/workspace");

        let params = AgentInitParams::new().with_working_dir(workspace.clone());

        assert_eq!(
            params.working_dir,
            Some(workspace),
            "AgentInitParams should store working_dir"
        );
    }
}

#[cfg(test)]
mod agent_handle_parity_tests {
    use super::super::*;
    use crate::chat::AgentHandle;

    fn make_client(read_only: bool) -> CrucibleAcpClient {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
            args: vec![],
            env_vars: std::collections::HashMap::new(),
        };
        CrucibleAcpClient::new(agent, read_only)
    }

    #[tokio::test]
    async fn temperature_round_trips() {
        let mut client = make_client(false);
        assert_eq!(client.get_temperature(), None);

        client.set_temperature(0.7).await.unwrap();
        assert_eq!(client.get_temperature(), Some(0.7));

        client.set_temperature(0.0).await.unwrap();
        assert_eq!(client.get_temperature(), Some(0.0));
    }

    #[tokio::test]
    async fn thinking_budget_round_trips() {
        let mut client = make_client(false);
        assert_eq!(client.get_thinking_budget(), None);

        client.set_thinking_budget(4096).await.unwrap();
        assert_eq!(client.get_thinking_budget(), Some(4096));

        client.set_thinking_budget(-1).await.unwrap();
        assert_eq!(client.get_thinking_budget(), Some(-1));

        client.set_thinking_budget(0).await.unwrap();
        assert_eq!(client.get_thinking_budget(), Some(0));
    }

    #[tokio::test]
    async fn max_tokens_round_trips() {
        let mut client = make_client(false);
        assert_eq!(client.get_max_tokens(), None);

        client.set_max_tokens(Some(2048)).await.unwrap();
        assert_eq!(client.get_max_tokens(), Some(2048));

        client.set_max_tokens(None).await.unwrap();
        assert_eq!(client.get_max_tokens(), None);
    }

    #[tokio::test]
    async fn switch_model_returns_not_supported() {
        let mut client = make_client(false);

        let err = client.switch_model("gpt-4").await.unwrap_err();
        match err {
            crate::chat::ChatError::NotSupported(msg) => {
                assert!(
                    msg.contains("gpt-4"),
                    "Error should mention the model: {msg}"
                );
            }
            other => panic!("Expected NotSupported, got: {other:?}"),
        }
    }

    #[test]
    fn read_only_starts_in_plan_mode() {
        let client = make_client(true);
        assert_eq!(client.get_mode_id(), "plan");
    }

    #[test]
    fn write_enabled_starts_in_normal_mode() {
        let client = make_client(false);
        assert_eq!(client.get_mode_id(), "normal");
    }

    #[tokio::test]
    async fn mode_change_updates_local_state_without_session() {
        let mut client = make_client(false);
        client.set_mode_str("auto").await.unwrap();
        assert_eq!(client.get_mode_id(), "auto");
    }

    #[test]
    fn cached_fields_default_to_none() {
        let client = make_client(false);
        assert_eq!(client.get_temperature(), None);
        assert_eq!(client.get_thinking_budget(), None);
        assert_eq!(client.get_max_tokens(), None);
    }
}
