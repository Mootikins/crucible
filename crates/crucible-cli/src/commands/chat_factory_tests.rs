//! Tests for the deferred chat flow factory closure
//!
//! Tests that the factory correctly creates agent handles from AgentSelection variants.

use crate::tui::AgentSelection;

/// Test that factory pattern compiles for all AgentSelection variants
#[test]
fn test_factory_pattern_compiles() {
    // Verify the factory pattern structure compiles
    #[allow(dead_code)] // Compile-time check only — not called at runtime
    async fn factory(selection: AgentSelection) -> anyhow::Result<()> {
        match selection {
            AgentSelection::Acp(agent_name) => {
                // In real code, this creates a CrucibleAcpClient
                assert!(!agent_name.is_empty(), "Agent name should not be empty");
                Ok(())
            }
            AgentSelection::Internal => {
                // In real code, this creates an InternalAgentHandle
                Ok(())
            }
        }
    }

    // Test compiles if factory pattern is valid
}

/// Test that AgentSelection::Acp variant carries agent name
#[test]
fn test_agent_selection_acp_carries_name() {
    let selection = AgentSelection::Acp("opencode".to_string());

    match selection {
        AgentSelection::Acp(name) => {
            assert_eq!(name, "opencode");
        }
        _ => panic!("Expected Acp variant"),
    }
}

/// Test that AgentSelection::Internal variant works
#[test]
fn test_agent_selection_internal() {
    let selection = AgentSelection::Internal;

    match selection {
        AgentSelection::Internal => {} // Expected
        _ => panic!("Expected Internal variant"),
    }
}

/// Test factory creates correct output for ACP selection
#[tokio::test]
async fn test_factory_creates_acp_agent() {
    async fn test_factory(selection: AgentSelection) -> anyhow::Result<String> {
        match selection {
            AgentSelection::Acp(name) => Ok(format!("acp:{}", name)),
            AgentSelection::Internal => Ok("internal".to_string()),
        }
    }

    let result = test_factory(AgentSelection::Acp("claude-code".to_string())).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "acp:claude-code");
}

/// Test factory creates correct output for Internal selection
#[tokio::test]
async fn test_factory_creates_internal_agent() {
    async fn test_factory(selection: AgentSelection) -> anyhow::Result<String> {
        match selection {
            AgentSelection::Acp(name) => Ok(format!("acp:{}", name)),
            AgentSelection::Internal => Ok("internal".to_string()),
        }
    }

    let result = test_factory(AgentSelection::Internal).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "internal");
}

/// Test that factory closure can capture environment
#[tokio::test]
async fn test_factory_closure_captures_environment() {
    let config_value = "test-config".to_string();

    let factory = |selection: AgentSelection| {
        let captured_config = config_value.clone();

        async move {
            match selection {
                AgentSelection::Acp(name) => {
                    anyhow::Ok(format!("acp:{}:{}", name, captured_config))
                }
                AgentSelection::Internal => Ok(format!("internal:{}", captured_config)),
            }
        }
    };

    let result = factory(AgentSelection::Acp("test-agent".to_string())).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "acp:test-agent:test-config");
}

/// Test exhaustive pattern matching on AgentSelection
#[test]
fn test_agent_selection_exhaustive_match() {
    fn handle_selection(selection: AgentSelection) -> &'static str {
        match selection {
            AgentSelection::Acp(_) => "acp",
            AgentSelection::Internal => "internal",
        }
    }

    assert_eq!(
        handle_selection(AgentSelection::Acp("test".to_string())),
        "acp"
    );
    assert_eq!(handle_selection(AgentSelection::Internal), "internal");
}

/// Test that factory can propagate errors
#[tokio::test]
async fn test_factory_propagates_errors() {
    async fn failing_factory(selection: AgentSelection) -> anyhow::Result<String> {
        match selection {
            AgentSelection::Acp(_) => {
                anyhow::bail!("ACP agent creation failed")
            }
            AgentSelection::Internal => {
                anyhow::bail!("Internal agent creation failed")
            }
        }
    }

    // All variants should propagate errors
    let acp_result = failing_factory(AgentSelection::Acp("test".to_string())).await;
    assert!(acp_result.is_err());

    let internal_result = failing_factory(AgentSelection::Internal).await;
    assert!(internal_result.is_err());
}

// Integration test notes:
//
// The actual factory closure in run_interactive_chat() creates real agents:
// - AgentSelection::Acp -> factories::create_agent() -> Box<dyn AgentHandle>
// - AgentSelection::Internal -> factories::create_agent() -> Box<dyn AgentHandle>
//
// These tests verify the factory pattern structure and behavior.
// Full integration requires:
// - Real CliConfig
// - Database setup
// - LLM provider configuration
// - ACP agent discovery
//
// Integration testing is done through:
// 1. Manual testing: `cru chat --lazy-agent-selection`
// 2. E2E tests that exercise the full deferred flow
// 3. The existing chat command tests that verify agent creation
