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

    #[tokio::test]
    async fn test_discover_agent_with_nonexistent_preferred() {
        // Should fail when preferred agent doesn't exist and no fallbacks available
        let result = discover_agent(Some("nonexistent-agent-xyz")).await;

        // This should return an error with helpful message
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("No compatible ACP agent found"));
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

    #[test]
    fn test_client_creation() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
        };

        let client = CrucibleAcpClient::new(agent, true);
        // Client should be created successfully
        // Further tests would require mocking the subprocess
    }

    #[test]
    fn test_client_read_only_flag() {
        let agent = AgentInfo {
            name: "test".to_string(),
            command: "test-cmd".to_string(),
        };

        let client_readonly = CrucibleAcpClient::new(agent.clone(), true);
        let client_write = CrucibleAcpClient::new(agent, false);

        // Both should be created successfully with different permissions
        // Cannot easily test the internal read_only field without exposing it
        // or adding a getter method
    }
}
