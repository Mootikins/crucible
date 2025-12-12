//! Integration tests for agent factory
//!
//! Tests the unified agent initialization for both ACP and internal agents.

use crucible_cli::factories::{AgentInitParams, AgentType};
use crucible_config::CliAppConfig;

#[test]
fn test_agent_init_params_builder() {
    let params = AgentInitParams::new()
        .with_type(AgentType::Internal)
        .with_provider("local".to_string())
        .with_read_only(false)
        .with_max_context_tokens(8192);

    assert_eq!(params.agent_type, Some(AgentType::Internal));
    assert_eq!(params.provider_key, Some("local".to_string()));
    assert!(!params.read_only);
    assert_eq!(params.max_context_tokens, Some(8192));
}

#[test]
fn test_agent_init_params_default() {
    let params = AgentInitParams::default();
    assert_eq!(params.agent_type, None);
    assert_eq!(params.agent_name, None);
    assert_eq!(params.provider_key, None);
    assert!(params.read_only);
    assert_eq!(params.max_context_tokens, None);
}

#[test]
fn test_agent_type_default() {
    assert_eq!(AgentType::default(), AgentType::Acp);
}

#[test]
fn test_agent_types_equality() {
    assert_eq!(AgentType::Acp, AgentType::Acp);
    assert_eq!(AgentType::Internal, AgentType::Internal);
    assert_ne!(AgentType::Acp, AgentType::Internal);
}

#[tokio::test]
async fn test_create_internal_agent_with_invalid_provider() {
    use crucible_cli::factories::create_internal_agent;

    let config = CliAppConfig::default();
    let params = AgentInitParams::new()
        .with_provider("nonexistent_provider");

    let result = create_internal_agent(&config, params).await;

    // Should fail with descriptive error about missing provider
    assert!(result.is_err());
    if let Err(err) = result {
        let err_msg = err.to_string();
        assert!(err_msg.contains("nonexistent_provider") || err_msg.contains("not found"));
    }
}

#[test]
fn test_agent_init_params_with_agent_name() {
    let params = AgentInitParams::new()
        .with_type(AgentType::Acp)
        .with_agent_name("claude-code");

    assert_eq!(params.agent_type, Some(AgentType::Acp));
    assert_eq!(params.agent_name, Some("claude-code".to_string()));
}
