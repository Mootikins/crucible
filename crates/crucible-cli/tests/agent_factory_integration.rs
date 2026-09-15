//! Integration tests for agent factory

#![allow(clippy::field_reassign_with_default)]

//!
//! Tests the unified agent initialization for both ACP and internal agents.

use crucible_cli::factories::{AgentInitParams, AgentType};

#[test]
fn test_agent_init_params_builder() {
    let params = AgentInitParams::new()
        .with_type(AgentType::Internal)
        .with_provider_opt(Some("local".to_string()));

    assert_eq!(params.agent_type, Some(AgentType::Internal));
    assert_eq!(params.provider_key, Some("local".to_string()));
}

#[test]
fn test_agent_init_params_with_agent_name() {
    let params = AgentInitParams::new()
        .with_type(AgentType::Acp)
        .with_agent_name("claude-code");

    assert_eq!(params.agent_type, Some(AgentType::Acp));
    assert_eq!(params.agent_name, Some("claude-code".to_string()));
}

// Configuration edge case tests

#[test]
fn test_builder_chaining_all_options() {
    let params = AgentInitParams::new()
        .with_type(AgentType::Internal)
        .with_agent_name("test-agent")
        .with_provider_opt(Some("ollama".to_string()));

    assert_eq!(params.agent_type, Some(AgentType::Internal));
    assert_eq!(params.agent_name, Some("test-agent".to_string()));
    assert_eq!(params.provider_key, Some("ollama".to_string()));
}

#[test]
fn test_builder_override_values() {
    let params = AgentInitParams::new()
        .with_type(AgentType::Acp)
        .with_type(AgentType::Internal) // Override
        .with_provider_opt(Some("openai".to_string()))
        .with_provider_opt(Some("ollama".to_string())); // Override

    // Last value should win
    assert_eq!(params.agent_type, Some(AgentType::Internal));
    assert_eq!(params.provider_key, Some("ollama".to_string()));
}

#[test]
fn test_optional_helper_methods() {
    let params = AgentInitParams::new()
        .with_agent_name_opt(Some("test".to_string()))
        .with_provider_opt(None);

    assert_eq!(params.agent_name, Some("test".to_string()));
    assert_eq!(params.provider_key, None);
}

#[test]
fn test_optional_helper_with_none() {
    let params = AgentInitParams::new()
        .with_agent_name("initial")
        .with_agent_name_opt(None); // Should override to None

    assert_eq!(params.agent_name, None);
}

#[test]
fn test_empty_string_agent_name() {
    let params = AgentInitParams::new().with_agent_name("");

    assert_eq!(params.agent_name, Some("".to_string()));
}

#[test]
fn test_empty_string_provider() {
    let params = AgentInitParams::new().with_provider_opt(Some(String::new()));

    assert_eq!(params.provider_key, Some("".to_string()));
}

#[test]
fn test_agent_type_copy_trait() {
    let agent_type = AgentType::Internal;
    let copied = agent_type;

    // Both should still be valid (Copy trait)
    assert_eq!(agent_type, AgentType::Internal);
    assert_eq!(copied, AgentType::Internal);
}

#[test]
fn test_agent_type_debug_format() {
    let internal = AgentType::Internal;
    let acp = AgentType::Acp;

    // Debug format should include type name
    let internal_debug = format!("{:?}", internal);
    let acp_debug = format!("{:?}", acp);

    assert!(internal_debug.contains("Internal"));
    assert!(acp_debug.contains("Acp"));
}
