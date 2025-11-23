//! Tests for ACP module

#[cfg(test)]
mod agent_tests {
    use super::super::*;

    #[tokio::test]
    async fn test_is_agent_available_ls() {
        // ls should be available and supports --version
        let result = is_agent_available("ls").await;
        assert!(result.is_ok());
        // Note: ls may or may not support --version depending on system
        // Just verify the function runs without panic
    }

    #[tokio::test]
    async fn test_is_agent_available_nonexistent() {
        let result = is_agent_available("definitely-not-a-real-command-xyz123").await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn test_agent_info_creation() {
        let agent = AgentInfo {
            name: "test-agent".to_string(),
            command: "test-cmd".to_string(),
        };

        assert_eq!(agent.name, "test-agent");
        assert_eq!(agent.command, "test-cmd");
    }

    #[test]
    fn test_agent_info_clone() {
        let agent = AgentInfo {
            name: "test-agent".to_string(),
            command: "test-cmd".to_string(),
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
        };

        let debug_str = format!("{:?}", agent);
        assert!(debug_str.contains("test-agent"));
        assert!(debug_str.contains("test-cmd"));
    }

    #[tokio::test]
    async fn test_discover_agent_with_nonexistent_preferred() {
        // When preferred agent doesn't exist, should fall back to known agents
        let result = discover_agent(Some("nonexistent-agent-xyz")).await;

        // This will succeed if ANY known agent is available on the system
        // or fail if no agents are found at all
        match result {
            Ok(agent) => {
                // Fallback succeeded - should be one of the known agents
                assert!(
                    agent.name == "claude-code" || agent.name == "gemini" || agent.name == "codex",
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
        // Should try to find any available agent
        let result = discover_agent(None).await;

        // Will fail if no compatible agents are installed
        // This is expected in most test environments
        if result.is_err() {
            let error_msg = result.unwrap_err().to_string();
            assert!(error_msg.contains("No compatible ACP agent found"));
        }
    }

    #[tokio::test]
    async fn test_is_agent_available_with_multiple_calls() {
        // Test that multiple calls work (caching behavior)
        let result1 = is_agent_available("ls").await;
        let result2 = is_agent_available("ls").await;

        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }

    #[tokio::test]
    async fn test_is_agent_available_empty_string() {
        let result = is_agent_available("").await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_is_agent_available_with_spaces() {
        let result = is_agent_available("command with spaces").await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn test_is_agent_available_with_path() {
        // Test with absolute path to command
        let result = is_agent_available("/bin/ls").await;
        assert!(result.is_ok());
        // May or may not be available depending on system
    }
}

#[cfg(test)]
mod context_tests {
    use super::super::*;

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
    use crucible_acp::{ChatConfig, HistoryConfig, ContextConfig, StreamConfig};

    #[test]
    fn test_client_creation() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
        };

        let client = CrucibleAcpClient::new(agent, true);
        assert!(!client.is_connected(), "New client should not be connected");
        assert!(client.session_id().is_none(), "New client should have no session ID");
    }

    #[test]
    fn test_client_creation_read_only() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
        };

        let client = CrucibleAcpClient::new(agent, true);
        assert!(!client.is_connected());
    }

    #[test]
    fn test_client_creation_write_enabled() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
        };

        let client = CrucibleAcpClient::new(agent, false);
        assert!(!client.is_connected());
    }

    #[test]
    fn test_client_with_custom_config() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
        };

        let config = ChatConfig {
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
        };

        let client = CrucibleAcpClient::new(agent, false);
        assert!(!client.is_connected());
        assert!(client.get_stats().is_none(), "Unconnected client should have no stats");
    }

    #[test]
    fn test_client_stats_before_connection() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
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
        };

        let client = CrucibleAcpClient::new(agent, false);
        assert!(client.session_id().is_none(), "Should have no session ID before connection");
    }

    #[test]
    fn test_client_clear_history_before_connection() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
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
        };

        let mut client = CrucibleAcpClient::new(agent, false);
        let result = client.send_message("test").await;

        assert!(result.is_err(), "Should error when sending message before connection");
        assert!(result.unwrap_err().to_string().contains("not running"),
            "Error should indicate agent is not running");
    }

    #[tokio::test]
    async fn test_client_shutdown_before_connection() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
        };

        let mut client = CrucibleAcpClient::new(agent, false);
        let result = client.shutdown().await;

        // Should succeed (no-op) when shutting down unconnected client
        assert!(result.is_ok(), "Should not error when shutting down unconnected client");
    }

    // Note: Testing spawn() requires an actual agent binary (claude-code, etc.)
    // These would be integration tests run only when the agent is available.
    // For unit tests, we test the wrapper logic without actually spawning.
}
